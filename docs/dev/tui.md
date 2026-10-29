# The terminal backend (ratatui)

`crates/nsh-platform-tui` renders an NSH surface into a terminal grid with ratatui. Its `AppView` lives on the
embedding app's thread; the app owns the terminal, its event loop and the area given to the surface.
`crates/nsh-tui-host` is the Development host: it fills the terminal with one surface and restores the terminal
when it exits. It loads source bundles directly or connects to the CLI's development server.

```sh
cargo build -p nsh-cli -p nsh-tui-host
./target/debug/nsh run examples/gallery --target tui
./target/debug/nsh run examples/counter --target tui --points-per-cell 8x16
```

The CLI serves and watches the app in its own process. It builds `nsh-tui-host` in a checkout, using the CLI's
profile, and starts it with inherited stdin/stdout. CLI diagnostics, the host's stderr and the host logger append
to the app's `.nsh/tui.log`, including verbose tool output. The host alone writes the terminal UI. Ctrl+C exits;
Unix stop signals and Windows console breaks ask the host loop to exit so its terminal guard can restore the
screen and raw mode before the CLI reaps it.

The host enables [alternate scroll mode (DECSET 1007)](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)
with `ESC[?1007h` on entering the alternate screen and disables it with `ESC[?1007l` during normal, error and panic
restoration. When mouse capture is off, terminals supporting this mode can send Up/Down keys for the wheel;
those keys follow the terminal Key scheme. This prepares the uncaptured-wheel path for the mouse-capture
setting in ticket 09; captured wheel events keep the three-row/six-column behavior below.

`C-h e` toggles the logs and errors panel. Escape, `C-g` or `q` closes it; arrows and PageUp/PageDown scroll,
Home/End select the start/end, and `r` reloads the latest build. Errors open the panel automatically and the next
update closes it. Ordinary builds and connection messages stay in the log. With both diagnostics and the performance line closed, the app owns
every row. The panel reads at most 64 KiB from the log tail every 250 ms while open; its recent error history keeps
at most 256 lines of 4096 characters each. Control characters are removed before drawing diagnostics.

A source bundle can also be loaded directly:

```sh
./target/debug/nsh build examples/counter --out .nsh/tui-counter
./target/debug/nsh-tui-host --bundle .nsh/tui-counter/bundle.js --log-file .nsh/tui-counter/host.log
./target/debug/nsh-tui-host --dev-server ws://127.0.0.1:8790/ws
```

`--bundle` reads the adjacent source map and capability metadata. Its `asset:/` resources resolve inside the
adjacent `assets/` directory; connected to a development server (`nsh run`), they are read from the server's
`/assets/`, as on the desktop hosts. Bundled fonts load successfully while text uses the terminal font. An embedding
app can supply its own `AppOptions::assets` reader, which takes precedence over both. Bytecode files require the trusted-resource
or verified-package path, owned by the packaging work. Direct hosts without `--log-file` keep diagnostics in
`nsh-tui-host-<pid>.log` in the system temporary directory, visible through the same panel. `--trace` includes
the engine's timing logs; app `console.log` output is recorded with or without it. `--data-dir` selects persistent
app data. A source-bundle failure exits after terminal restoration; a development session stays open for a fix.

## Performance monitor and frame budget

```sh
./target/debug/nsh run examples/gallery --target tui --fps
cargo test --release -p nsh-tui-host --test host bench_terminal_scroll -- --ignored --nocapture --test-threads=1
```

`--fps` enables a single line at the top right; `C-h p` toggles it beside `C-h e`. These prefixes apply outside
editable fields and inside diagnostics; a field keeps C-h as Backspace. The line shows active FPS (`idle` after
a second with no app frames), mean frame time and p95, mean apply / draw / flush milliseconds, and mean bytes
and changed Cells per frame. It logs the same line through `nsh::perf`, visible in `C-h e`. Long lines clip to
the terminal width. The readings update once a second while active, with one final idle update; HUD-only
refreshes never count as app frames and idle monitoring schedules no recurring draws.

`perf.rs` retains 120 application frames. Apply includes draining pending work, callbacks, scroll reports and native animation evaluation;
draw includes surface painting and focus reconciliation; flush includes ratatui's diff, encoding and the
writer's flush. HUD formatting is outside those phases. Cells count the backend's actual diff iterator, including
wide-character handling; bytes count successful writer output, including ANSI escapes. Output means include
the HUD when it is part of an app frame. An uninstrumented transport displays `-` for its unknown counters.
`PerfMonitor` supplies the active FPS and engine timings (its four cold frames are skipped for FPS); the
terminal phase history includes every app frame. The budget and baseline are in [Performance](performance.md#terminal-frame-budget).
Changes to terminal rendering, input or layout include before / after benchmark numbers and trace counts.

For a full-terminal embedder, enable `AppOptions::fps` or call `AppView::set_fps_monitor(bool)`, wrap the
writer with `view.measure_writer(writer)` and the backend with `view.measure_backend(backend)`, then draw with
`view.draw_terminal(&mut terminal)` (as `run_app` does). The adapters remain pass-through while the monitor is
hidden: no timers, clock reads, history, diff scans or output counting are added on that path. `frame_metrics()`
returns the last completed frame's `FrameMetrics`; `perf_monitor()` exposes the shared engine monitor for tools.
Disabling drops the monitor and detaches it from the engine; enabling starts a fresh history. Ordinary `render`
and Widget embedding keep the same layout API; full transport timings require `draw_terminal`.

## Terminal lifetime and idle waits

On Unix, SIGHUP, SIGTERM and SIGINT request shutdown. The host checks the flag before and after input waits
and while waiting for a full output queue. A disconnected stdin/stdout (`POLLHUP`, `POLLERR`, `POLLNVAL`),
loss of the input TTY, or a read/write error ends the run loop. On macOS, select readiness plus an empty
input queue also detects EOF on PTYs whose poll does not report HUP. The check consumes no input. Interactive runs require a live terminal on
stdin; `--headless` continues to accept redirected or closed input.

The Unix session saves the descriptor flags and enables nonblocking input/output. It uses crossterm 0.29's
`use-dev-tty` reader: its EOF/error path returns to the bounded poll, whereas the default mio reader can keep
reading zero bytes or EIO forever, never returning to the host's signal check. The event parsing and conversion
remain crossterm's. On non-macOS Unix, the reader's millisecond poll truncation can spin over a fractional
millisecond; decoding is capped at 50 microseconds and `run_app` sleeps the remainder of its 16 ms wait.
On macOS the reader uses select with microsecond waits. Every Unix decode poll gets at least 50 microseconds,
including when another frame is already due: crossterm's `use-dev-tty` reader skips its input and parser queue
entirely at zero timeout. Continuous frames therefore retain an input opportunity between draws. Neither path
retries a failed host poll.

Terminal and OSC 52 writes share the cancellable output path, with buffered frame encoding. Output backpressure
waits for writability in 16 ms slices and checks shutdown between waits; errors propagate. Once the loop ends,
writer destructors stop writing. The session attempts mouse/alternate-screen/cursor restoration with nonblocking
writes, discards undeliverable queued output if that write would block (macOS can otherwise wait in process
teardown), restores raw-mode settings, then restores the original descriptor flags. Cleanup does not wait for a
hung-up or full output queue. Signals do only an atomic store; cleanup runs on the host thread.

`run_app` honors the remainder of a poll that returns `Idle` early, capped by the next frame deadline, so
spurious wakeups do not turn into a busy loop. Engine wakeups coalesce until rendering, animation deadlines are
consumed by a frame, and the performance HUD stops scheduling after its idle update. Pending work still has an
immediate deadline. The maximum sleep remains 16 ms; headless hosts use the same loop and sleep contract.

The host integration binary has private-PTY regressions for each stop signal, partial input, hangup with and
without a controlling terminal, closed stdin, and shutdown with a full output queue. The default parallel
runner starts each lifecycle scenario in a separate copy of that test binary, selecting only that scenario.
Each worker has a new session, no controlling terminal, null input and a private log. Each host then gets its
own new session and PTY; inherited signal masks and ignored termination/job-control signals are reset before
exec. The test process never changes its own session, terminal or signal handlers.

The worker opens PTYs and launches hosts sequentially within its scenario, so parallel scenarios share neither
PTY descriptors nor fork-time descriptor setup. The retained probe opens the slave independently, with
`O_NOCTTY | O_NONBLOCK`: host restoration cannot change the probe's blocking mode. Reads drain a bounded batch,
fill writes have a one-second deadline, and cleanup closes the master, kills the owned child and polls for
reaping with a one-second bound rather than blocking in `wait()`.

Exit is still required within one second of a signal or hangup. Startup has a separate ten-second deadline.
A 45-second worker watchdog covers the whole scenario, kills and reaps its registered hosts, then ends the
worker if the scenario stops making progress. The parallel runner also enforces a 50-second worker limit.
Deliberately blocked and panicking scenarios verify that cleanup leaves no live host. Fixtures and captured
stderr belong to the scenario's private directory and are removed even when its worker times out.

Restoration is inspected on a non-controlling PTY, which remains queryable after child exit; kernel `PENDIN`
bookkeeping is excluded from the termios comparison. Workers validate session/process-group identity and retain
host stderr in failure messages. Their successful output, including benchmark readings, is forwarded to the
outer test so `--nocapture` retains its usual meaning.

Use each worktree's own default `target` directory. The suite itself uses the default parallel runner:

```sh
cargo test -p nsh-tui-host
cargo test --release -p nsh-tui-host --test host bench_terminal_ -- --ignored --nocapture --test-threads=1
```

See [idle CPU measurement](performance.md#terminal-idle-cpu) for the sample window and pending results.

## Development sessions

Feature `dev` adds `AppView::connect_dev_server`. The shared `nsh-dev::DevClient` reports `HostPlatform::Tui`,
bytecode compatibility and the linked UI sets, reconnects on disconnect and queues ordered events for the
embedding thread. Rendering processes those events. Full bundles carry their source map, app ID, declared
capabilities, development policy and VM limits. A patch applies only to its `(session, base)` through
`EngineControl::hot_update`; its following full bundle becomes the reload target without restarting the app.
The existing hot-runtime rules determine which signals and component instances survive an edit (see
[tooling](tooling.md#dev-server-nsh-dev--nsh-run)). A rejected patch reloads its matching full bundle, whether
that bundle arrived before or after the rejection. Build failures, incompatible UI sets and structured app
errors appear in the dismissible panel. Public `load` and `stop` disconnect the dev client; internal development
reloads replace only the engine and retained tree.

Apps with their own Rust UI sets get a generated `tui-host` in `.nsh/rust`, using `main_with_options` to pass
registries and an optional Rust root. Rust roots use the development server's `Root` message; Rust edits use the
CLI's existing rebuild/restart path. Embedders may select a diagnostic file with `AppView::set_dev_log_path`.
The library neither installs a logger nor takes over terminal streams.

## Platform tests and Cell screenshots

`nsh test --target tui` runs the existing `nsh/test` files through the Development host. The CLI starts it with
`--headless`, selecting ratatui's `TestBackend` before any terminal session is initialized. It uses the same
`run_app`, `AppView`, engine, renderer and input path as an interactive run. Standard input may be closed; raw mode,
alternate-screen setup, terminal queries and crossterm polling are never entered on this path. The host's
`--window-size WIDTHxHEIGHT` is in points, rounded up to whole Cells. `open` supplies 390 × 844 points by default
(49 × 53 Cells at 8 × 16); a directly started headless host defaults to 100 × 40 Cells. Tests use the app's
`nsh.json` `tui.pointsPerCell` setting when present.

Feature `dev` links `nsh_testing::agent`. Its `Tester` consumes test protocol events on the dev-client thread.
Model attachment and bundles share one ordered inbox: `Model::set_up_engine` runs before startup and
`Model::engine_started` receives the resulting control. Completed mount batches and app errors feed the Model;
storage, network routes, clipboard, environment pins and manual clocks use the shared agent's setup.

The private `TuiNative` posts requests to the view and wakes it. Rendering applies complete batches, draws, then
services requests; a screenshot serializes that completed buffer. Input requests call `AppView::handle`, exactly
as the host's terminal input does. Pointer coordinates convert from content-relative points to absolute Cells,
including the embedding area's origin. `Native::bounds` uses the renderer's unclipped Cell geometry, including
absolute edge rounding, transforms and scroll offsets. Mounted controls outside a Scroll keep their bounds,
including negative coordinates above its viewport, so the shared agent can scroll them into view before pressing.
Concealed or hidden views have no rendered geometry. Pointer input still uses clipped hit regions, and text-span
bounds describe visible characters. The agent drives press/release, motion, wheel events, individual keys and
bracketed paste. Scroll requests are quantized to the same three-row/six-column wheel steps as real input. Their behavior
is the platform's current input implementation: this adapter does not emulate missing input features. Richer pointer/scroll behavior and system dialogs are supplied by their respective platform tickets.
Fields use the same editor for real keys, synthetic keys and bracketed paste.
`setViewport` currently returns an error; select the initial viewport with `open(entry, { width, height })`.

`app.screenshot(path?)` returns bytes and optionally writes them on the test machine, through the existing
base64 screenshot transport. On the terminal target those bytes are UTF-8 JSON (use a `.json` path):

```json
{"format":"nsh-tui-cells","version":1,"width":1,"height":1,"rows":[[{"symbol":"A","fg":"blue","bg":"reset","underline":"reset","modifiers":["bold"]}]]}
```

`rows[y][x]` is one Cell, including blank Cells and the trailing Cells of wide symbols. Width and height count
Cells in the rendered surface. `symbol` preserves the Cell's entire string. Colors are `reset`, lowercase
ratatui named colors (`black`, `darkgray`, `lightred`, etc.), `#rrggbb`, or `indexed:N`. Modifiers are ordered:
`bold`, `dim`, `italic`, `underlined`, `slow_blink`, `rapid_blink`, `reversed`, `hidden`, `crossed_out`, with only
active entries present. Screenshots contain no timing or terminal escape framing, so identical renders compare
byte for byte. Graphical targets continue to return PNG bytes.

```ts
const picture = JSON.parse(new TextDecoder().decode(await app.screenshot()));
expect(picture.rows[0][0].symbol).toBe("A");
```

## The example app

The terminal's demo is the official `examples/gallery`, the same code every platform runs, so a page can be
compared side by side with the others. Its layout and colors' meanings are the gallery's own
(`examples/nsh-gallery-ui`); the terminal backend draws them in the terminal's look (below).

```sh
./target/debug/nsh run examples/gallery --target tui
./target/debug/nsh build examples/gallery --out .nsh/gallery
./target/debug/nsh-tui-host --bundle .nsh/gallery/bundle.js
```

## Architecture

- **The core lays out in points.** Taffy places each view. The backend projects its frame to Cells using the
  rounding rule below; child frames, content boxes, drawing and hit testing share that projection. Text is measured
  on the grid on the engine thread. Changing the area passed to `render` sends the new viewport and environment
  to the engine.
- **One engine per surface.** `app.rs` owns the public lifecycle and configuration. `surface.rs` carries complete
  `MountBatch` messages, structured errors and frame requests over a channel. Only rendering drains the inbox;
  `mount.rs` applies a whole batch before callbacks, drawing or input can observe it. Replacing or stopping an
  app drops its engine and inbox, so messages from the previous app cannot reach the next one.
- **One retained view per node.** `view.rs` stores tree links, point frames, styles, props, events and laid-out
  text, indexed by node id. `render.rs` walks children in paint order, propagating clipping, visibility, opacity
  and pointer policy. Borders use `Block`; the text engine prepares styled `Paragraph`s, which are reused on
  frames, one per appearance. Clipped text uses `Paragraph::scroll`; a clipped border drops the sides outside the clip.
- **Input belongs to the embedder.** `Input` is independent of a terminal backend. Mouse positions are absolute
  terminal Cells; `handle` localizes them against the last rendered area, follows the painted hit regions and
  uses the core's `input::Press` to deliver PressDown, PressUp and Press. Releasing outside the area cancels the
  press and returns `Handled::Ignored`. Unused keys and pastes are ignored. `set_focused` lets an embedder remove
  keyboard focus and cancel a pending press.
- **Scheduling is passive.** The wake callback only schedules a render on the embedding thread. Wakeups coalesce
  until rendering begins. `next_frame_deadline` is immediate for pending messages, timed for an engine display
  frame request or native animation, and `None` while idle. Error and appearance callbacks run during render on the embedding thread.
  `Widget for &mut AppView` follows the same batch and drawing path for buffer-only callers.
- **The terminal transport stays in the host.** The library depends on `ratatui-core` 0.1 and `ratatui-widgets`
  0.3, the crates behind ratatui 0.30. Feature `run-app` adds a generic loop over `Terminal<B>` and a polling
  callback; it uses only public `AppView` methods. The host supplies `ratatui-crossterm`, event conversion, raw
  mode, the alternate screen and mouse capture. Since crossterm polling cannot be woken by the engine, the loop
  polls at most 16 ms at a time; idle polls draw no frames. An RAII guard restores terminal state on normal and
  error returns; a panic hook restores it before printing the panic. Forced termination cannot run that guard.

`AppOptions` supplies the point scale, appearance/background, limits, UI sets, cache, storage, assets, clipboard
and host channel. `TerminalCapabilities` starts with the terminal background; capability fields grow with their
renderers. Embedding defaults to bounded limits; the Development host explicitly selects standalone limits.
`register_component` creates per-node widgets with ordered property updates, commands and buffer rendering;
reserved `nsh/` names are rejected. Native component input, sizing and richer context belong to the native
component ticket. The terminal idiom and the Release runner are separate slices. The current environment uses desktop behavior and reports the machine's operating system (macOS, Windows or Linux), as the desktop development hosts do, so `fs/promises` and `process` run under the app's declared capabilities and `platform.os` names the real system. `nsh/desktop`'s `isAvailable` follows the OS, so an app that draws its own window chrome there (the gallery's `TitleBar`) draws it in the terminal too, as the app's own content. This is the real-OS part of the Terminal idiom ticket; the idiom itself (`Idiom::Terminal`) is still that ticket's, and the surface reports the desktop idiom until then.

## Points per Cell

One **Cell** is **8 × 16 points** by default. These are layout units, independent of the terminal font's physical
pixel dimensions. `CellSize::new` accepts finite, positive typed dimensions; the same value is used for the text
engine, viewport, drawing and input. `nsh run --target tui --points-per-cell 10x20` overrides the app setting:

```json
{ "tui": { "pointsPerCell": "10x20" } }
```

The CLI option wins over `nsh.json`; the default is `8x16`. Both dimensions must be finite and positive.
The development host takes the same `--points-per-cell WIDTHxHEIGHT` option. The selection is made when the
session starts; restart the run after changing this setting.

Every geometric projection uses **round-half-up**: `floor(points / points_per_cell + 0.5)`. A tie goes toward
positive coordinates, including negative positions (`-0.5` becomes `0`). The embedding area's integer Cell origin
is added after conversion. `geometry::CellBounds` is the common boundary for view frames and content boxes;
input content boxes follow the same rule as text content boxes.

A positive width or height occupies at least one Cell; a zero extent stays zero. This rendering rule keeps thin
progress rails, slider thumbs and partial indicators visible and operable: the official gallery's 6-point rail
occupies one Cell row at the default 16-point Cell height. It changes only the Cell projection; layout, app styles
and the control's numeric value remain the core's.

Each edge rounds from its **absolute, unrounded point coordinate**, after composing ancestor transforms and
scroll offsets. Parent rounding is never added to a child's point position. The right and bottom edges are
exclusive. Equal whole-Cell gaps stay equal and nested padding accumulates before rounding; a moving view's Cell
width or height can vary by one Cell. For example, a box at y = 216pt with height 40pt covers rows 14 and 15;
its bottom at 256pt rounds to edge 16. A text row after 12pt of padding begins at 228pt, on row 14.
Alignment follows from these rules, and the alignment tests check it: text starting at the same point x starts
in the same Cell column on every row (a list's labels, a section's left edge, values under their headings), and
right-aligned text whose right edge is at the same point x ends in the same column (prices, sizes, totals), since
the text engine measures whole Cells. Siblings centered on one row share it. Brackets, the borders and fills the
terminal look leaves out and the palette's grounds never move content between Cells. Views at different point
x keep their own rounding: two cards at different phases can place an icon and its title one Cell closer in one
than in the other, and a 34-point tile is 2 or 3 rows tall depending on where it starts.
Consecutive siblings of equal height stacked in one column at an equal point pitch (a list's rows) keep one Cell
pitch and one Cell height: the first keeps its rounded row, each next one starts the pitch's rounded rows below
the one before, on a Cell edge, and its content moves with it. The gallery's 36-point sidebar rows 2 points apart
are 2 rows apart each, where absolute edges alone would space them 2, 3, 2, 3 rows apart. Other views keep their
absolute edges.
A positive point gap between sibling one-line texts reserves at least one Cell of separation, including reverse rows;
a zero gap keeps the texts adjacent. Canvas text calls apply the same minimum gap as whole runs, independently
of draw order. Existing ancestor and canvas clipping still apply.
Text measurement still counts the whole Cells that fit within a point width, separately from positioning edges.
A compact canvas or rounded control (less than one and a half Cells tall) occupies its center row, so a centered icon and a
one-line label share a row even when their point heights differ.

Selection accents keep their authored dimensions. The gallery's sidebar accent is 3 × 18 points; its watchlist
accent is 3 × 36 points (a 64-point row with 14 points above and below). Their different Cell heights follow
those dimensions and edge rounding. Inspect Cell modifiers as well as symbols when checking these markers; a
reversed Cell can have a space as its symbol.

A drawn border owns one Cell on each side. Child boxes fit inside those reserved Cells, with any displacement
carried into their contents. When reservation shortens a child box, its vertical point offsets contract into
that box while text keeps terminal glyphs; vertical text stacks reserve one Cell per measured line,
so contraction reduces quantized gaps without collapsing two labels onto one row. A following inset
starts below already-painted sibling text. Ordinary descendants remain clipped by inherited border limits. Scroll content
keeps its offset and clips rather than moving back into the viewport. A view's own text, field and canvas use
its inner Cell rect. `overflow: hidden` clips descendants to that rect and masks rounded corners. A border is
drawn whole or not at all: each drawn side owns a Cell, and a box holding content keeps one row and one column
between them, so a one- or two-row box with content (a chip, a compact button) draws no border and its content
keeps the Cells the layout gave it; an empty two-row box keeps its closed border when no sibling text or content
is drawn through it. Borders are straight single lines whatever their radii; radii shape
only fill coverage and `overflow: hidden` clips. A button draws no border (the terminal look below).
A childless plain View fill thinner than half a Cell draws `─` or `│` in its fill's palette color (the
Terminal look below). A fill at exactly half a Cell is a solid leaf. When both dimensions are thin, the longer
normalized Cell extent chooses the line direction. Hairlines use absolute edges; rounded controls and compact
canvases keep their center-row rule.
A sub-Cell fill preserves already-painted glyphs that it does not overlap in point space, even when both
quantize to the same Cell; fills that overlap text in points keep their normal paint order.
Rounded fills sample the Cell centers against circular corner arcs; continuous curves use the same coarse
approximation. Positive tiny fills still occupy a Cell.
Text painting, search highlights, selection and the test agent share the projected text content rectangle.
Anchored overlays use the resolved side and anchor from `ViewProps::overlay_anchor`. Their nearest edge moves
out to the anchor's Cell boundary when needed, including anchors smaller than one Cell. The core's placement,
flip decisions and point-layout events remain unchanged.

`CellTextEngine` counts Unicode display width: wide characters take two Cells and grapheme clusters stay whole.
Ordinary text occupies one row at any font size. Inline attachments reserve their width in whole Cells
and grow the line to hold their height; baseline, text-top, text-bottom and middle alignment use Cell font metrics.
Their child views draw and receive input in the reserved space, and attachments removed by truncation stay hidden.
Wrapping prefers spaces after content, then breaks inside a word. Leading indentation stays with the first part
of the word instead of becoming a separate whitespace-only row. Explicit newlines
preserve empty rows. A grapheme wider than the available line remains whole and is clipped by drawing.
`numberOfLines` limits the rows; tail, head and middle ellipses reserve one Cell, while clip adds no marker.
Alignment becomes a whole-Cell indent. Line geometry reports UTF-16 offsets and point dimensions, as the core's
`TextEngine` contract requires. Custom font registration succeeds with the terminal font remaining in use.

## The terminal look

The terminal backend follows the terminal's own look, as system UI keeps a platform's look: `palette.rs` maps
every app color onto the terminal's 16-color meanings, so the user's terminal theme decides the actual colors.
A color is read against its ground: the opaque fill of the nearest view the app drew it on (what the app
designed it against: a button label on its button's fill, text on its card), else the page of the app's
appearance (`ColorScheme`: the terminal's background from OSC 11, dark when unknown, or the app's own choice),
black when dark and white when light. Text, fields and canvases sit on their own view's fill; fills and borders
on what is under their view. A color's alpha is blended over the ground before it is read. View opacity never
changes a color, and the ground decides only the ink, never where anything is drawn.

| App color | Terminal ink |
|---|---|
| Saturated: chroma (largest minus smallest channel) at least 20% of 255 | The accent of its hue band, at normal intensity: red [335°, 15°), yellow [15°, 75°) (orange, amber), green [75°, 165°) (lime, emerald), cyan [165°, 200°) (teal), blue [200°, 265°) (violet), magenta [265°, 335°) (purple, pink) |
| Near-neutral, CIE L* at least 75 from the ground's | The default foreground |
| Near-neutral, closer to the ground (secondary text, placeholders, separators, borders) | The default foreground, dim |
| Fully transparent | Hidden |

Text keeps its attributes: weight 600 and above is bold, and italic, underline and strikethrough stay. The text
engine lays text out once on the engine thread; the styled `Paragraph` for a ground is made when the text is
first drawn on it, and the last few are kept. Borders, hairlines, canvas dots and text, field text and placeholders and animated colors use
the same mapping.

No fill color is drawn; every Cell shows the terminal's background. A view's opaque fill, or a translucent one
of at least 50% alpha, still clears the Cells it covers, so a popup, a menu, a sticky header or a floating bar
hides what lies under it. Lighter fills (scrims, tints) draw nothing. A childless View whose opaque fill is its
content (a progress indicator, a slider thumb, a status
dot) draws its Cells reversed in the fill's palette color; one thinner than half a Cell draws the hairline glyphs
above. Gradients are left out.

Views whose accessibility state is selected or checked (`checked: true`; mixed is not), other than checkboxes,
radios and switches (below), and the focused control,
draw their content reversed over their Cell box, after their descendants. Cells a canvas drew stay as drawn,
in their palette color on the terminal background (a sparkline in a selected row), unless a later fill covered
them. A focused text field shows the terminal cursor at its caret instead. Outlines, including the app's focus outlines, are not drawn: focus is the
reversed box. A view whose opacity, multiplied through its ancestors, is below 3% draws nothing and keeps its hit
regions; any other opacity is drawn as opaque.

Decorations are plain, and each is a documented Decorative feature:

- **Borders** are straight single lines (`┌─┐`, never `╭╮`) in their palette color: neutral border colors read
  dim, accent ones (an invalid field's red) keep their accent.
- **Buttons** read `[ label ]`. A button is a pressable view (role `button` or `imagebutton`, or no role and press
  handlers) that the app drew as a box, with a border or an opaque fill unlike the fill it sits on, on itself or
  on the plain View it wraps all its content in (a press handler around a styled View), and whose text sits on
  one Cell row. List items without a box (sidebar and watchlist rows) and selected or checked views,
  which are reversed, have no brackets; other roles (link, tab, checkbox, radio, switch, menu item, ...) keep
  their own look, and a pressable with several rows of text or other content (a field, an image, a labeled canvas)
  keeps the app's box. The label stays in the Cells the layout gave it; the brackets take the button's own Cells
  with the same gap on both sides, one Cell when the box has room, else none (`[Sell]`), both or neither: a label
  flush with its box stays bare. The button draws no border or fill.
- **Icons and decorative images** are not drawn: canvases with role `image` or without an accessibility label, and
  image placeholders narrower than 3 Cells or shorter than 2. A button whose content is only icons shows its
  accessibility label as `[ label ]` on its box's middle row, cut with `…` to the box (`[L…]`).
- **Checkboxes, radios and switches** (roles `checkbox`, `radio`, `switch`) show their state with terminal glyphs
  instead of reverse: a checkbox `[x]`, `[ ]` or `[-]` (mixed), a radio `(•)` or `( )`, a switch `[on ]` or
  `[off]`. The glyph takes the place of the indicator, the first view inside the control that the app drew as a
  box (a border or an opaque fill, not a hairline rule) holding no text; the indicator and everything in it (its
  border, fill, mark or thumb) are not drawn. The glyph starts at the indicator's left column on its middle row,
  so equal controls in a list put their glyphs in one column, and takes the palette ink of the indicator's fill,
  else its border. When the indicator is narrower, the glyph extends right only into blank Cells; without room
  it shortens (`x`, `-`, `•`, `on`, `off`), and without room for that it is left out. A control without an
  indicator draws its glyph in the blank Cells before its label, one Cell from it, or its short form, or
  nothing. Keyboard focus still reverses the control.
- **Shadows, gradients and opacity** are ignored (opacity below 3% still hides).

## Navigation stacks

Each stack keeps the core's `navigation::Stack`. Inserts, moves and removals mark the affected stack; after the
whole mount batch, `Stack::reconcile` selects its top screen. Every other screen is concealed: drawing and hit
testing skip its subtree. The app can remove the top screen to reveal the previous one. Pushes and app-driven
removals settle immediately, with no transition frames. Stacks and screens clip their children to their bounds
and draw the app's own styles, with no placeholder box.

A screen keeps its `ScreenConfig`. Of its header items, only the node named by `header_view` is visible; the
native left, right and title slots stay concealed. The selected header is kept above the app's content, including
content inserted later. Header items draw their children directly, without a placeholder.

Before drawing, attached, visible screens report their stack's point size and zero insets. A screen's
`navigation::Reported::update` suppresses unchanged reports; a change sends `set_root_size`, and an inset change
also sends the core's Insets event. A hidden screen, including one inside a concealed outer screen, is not
reported until it becomes visible.

`Esc` and `C-g` use `navigation::back_target`. Without a modal, the app's `onBackPress` hears the request first;
a declined request returns as `Command::Back`, which proceeds directly to navigation. Otherwise the innermost
stack takes it: its top screen receives BackRequest when listened for, `prevent_back` keeps it in place, and a
native pop sends Dismiss immediately. The shared Stack remembers native pops until the app removes those screens.
Each screen remembers its last focused control when another screen covers it. On back or an app-driven pop,
that control regains focus if it still exists, is focusable and shows; otherwise the screen's first focusable
control is selected and revealed. Covered screens stay mounted, including their Scroll states, so returning to
a valid saved control keeps both focus and scroll offsets. Only a fallback that needs revealing moves a Scroll.
Modal focus restoration takes precedence over page history and retains its separate invalid-opener policy.

Modals cover the surface and paint above the page. A batch's new modals follow the completed tree's order (core
sibling insertions may arrive in reverse); a modal presented in a later batch goes above the existing ones.
Presenting a modal saves the previous focus and moves focus to its first focusable view in tree order; an
empty modal has no focus. Keyboard traversal stays inside the newest modal, including when it has no controls.
Closing a modal, natively or through an app update, restores the saved view only if it still exists, is focusable
and has visible Cells. Otherwise focus is cleared, rather than moved to another page control. Nested modals keep
this history per presentation, so closing several at once restores the page's focus.

Mouse hit testing is also scoped to the newest modal, even when its container uses `pointerEvents: "box-none"` or
`"none"`. A completed press on its own backdrop sends RequestClose with reason `backdrop`; the modal's
`close_on_backdrop_press` also closes it natively and sends Dismiss. The release is consumed even if the modal
closed in between, so it cannot press the underlying page. App-rendered backdrop controls keep their own handlers.

A stack inside the newest modal gets back first; otherwise the modal receives RequestClose with
reason `cancel`. `close_on_escape` also closes it natively and sends Dismiss. Modal sizes and zero insets use the
same Reported contract, and first presentation sends Show. Transitions settle immediately.

## Scroll content

A Scroll draws its children within its viewport, using the current content offset; it starts at zero. The view's
own fill and border stay fixed. Children are translated by the offset (with content insets from the core's scroll
behavior), then projected to Cells and clipped to the Scroll and its ancestors. Text that starts outside the
viewport uses the same Paragraph scrolling path as other clipped text. The hit regions use these translated,
clipped rectangles, so only visible content receives presses.

The retained view owns the core's `scroll::ScrollState`. `ScrollConfig`, `ScrollContent` and `ScrollBehavior`
update its axis, extent and behavior data. `ScrollTo` from the core, including an initial `contentOffset` and
resolved `scrollToEnd`, moves immediately within the content limits in this reduced-motion slice. `ScrollBy`
applies the core's anchoring delta without clamping it. After applying pending batches, the surface reports each
changed scroll view's final offset and applied-command count to the engine, which delivers scroll events and
updates its view of the content. Keyboard movement reveals the focused control in its surrounding Scroll,
innermost first. A reveal leaves a fully visible target in place. When vertical scrolling is needed, it centers
the target in the viewport, clamped to the content limits, keeping it away from app overlays at the edges.
Horizontal reveals and targets too large to center retain the core's reveal margin (16 points, capped at a
quarter of the viewport) and favor the target's start when it does not fit.
Paging and endpoint keys act on the innermost Scroll around focus, or around the search point
when a plain-text match holds it. With neither in a Scroll, they use the first Scroll in the active focus scope.
Paging moves focus to the visible control nearest its old relative row in the viewport (relative column
for horizontal scrolling). Home/End and Alt+< / Alt+> choose the Scroll's first/last focusable control and reveal
it, even when nonfocusable content precedes or follows the controls. With no focusable controls, these keys only
scroll, to the content endpoints for Home/End. Page selection uses the next completed frame's geometry, and Enter
never presses an off-screen control. These user-driven offsets keep the applied count unchanged; only core
ScrollBy and ScrollTo increment it.

Each wheel step moves three rows vertically, or six columns horizontally (three columns times two). Shift sends
vertical wheel steps horizontally. The core's
`input::scroll_target` selects the innermost Scroll under the pointer that can move; at an edge the same-axis
parent can take the event. This search stays inside the newest modal. A vertical wheel uses a horizontal view
only when no surrounding Scroll takes the vertical axis. Disabled scroll views do not take the wheel.

Wheel scrolling keeps the current keyboard focus while any of its Cells remain visible. If it leaves sight,
focus moves to the nearest visible focusable view in the current focus scope, measured from its previous Cell
position. A wheel event with no keyboard focus creates none. Focus selection uses the completed scrolled frame,
so the next activation cannot target content that has left the viewport. Indicators, sticky cells, gestures and
scrolling animations remain separate work.

## Native animations

The retained tree owns one core `animation::Animator`. Every mount item feeds it, including deletion and binding
changes. `AppView::render` and its buffer-only Widget path tick it with monotonic seconds, write `AnimValues` and
canvas slots, then send its finished runs and value reads through `EngineControl::animations_finished`. Timing,
springs, decay, composites and native keyframes use the core's drivers. Replacing or stopping an app clears the
graph and its deadline. Replacing bindings clears their old overrides, so ordinary styles become visible again.

Active drivers request the next render in 16 ms through `next_frame_deadline`; they have their own deadline,
separate from the engine's display-frame request. Embedders arm a timer for that deadline after rendering. The
standalone loop already uses this contract. Once the graph finishes or is stopped, it schedules no further
frames. The test agent reports the graph's actual animation state when settling input.

Animated opacity multiplies through descendants. Background, border and text colors override their style values
and are drawn in the palette like any other color; text fields keep their placeholder color. Canvas numeric and
color slots are evaluated on the same frame, including the slots used by drawing transforms.

`animation.rs` composes two-dimensional view transforms in CSS order around `transformOrigin`, including bound
steps. Translation snaps to Cells; scale, rotation and skew project each view and content box to its axis-aligned
Cell extent. Descendants inherit the transform, and clipping, focus geometry and hit regions use those projected
boxes. Text keeps upright, unscaled terminal glyphs inside its projected box; borders remain Cell-aligned.
Canvas paths receive the composed affine transform on the braille grid, including after a canvas `setTransform`.
Depth rotation and perspective are Decorative features left out silently.

The environment reports reduced motion. Navigation, modal transitions, layout animations and style transitions
take their final state immediately. Motions honor reduced motion through the core; explicit `Animated` and
animated-value drivers run as written, so entrance fades such as the gallery's `Appear` finish visible and
completion callbacks and promises resolve normally.

## Canvases

`canvas.rs` replays a canvas's `Drawing` against a small 2D context onto a grid of braille dots, 2 across and 4
down per Cell (4 × 4 points at the default 8 × 16), and ratatui's `canvas::Canvas` with `Marker::Braille` turns
them into symbols. Only the canvas's visible Cells are replayed onto, so a canvas partly scrolled or clipped keeps
its dots in place. The widget resets the background of its area, so it draws into a scratch buffer and only its
symbols and palette inks are copied: what lies under it stays. A canvas with role `image` or without an
accessibility label is an icon and is not drawn (the terminal look above); label a chart to draw it.

- **State**: the affine transform (`translate`, `scale`, `rotate`, `transform`, `setTransform`), `save` /
  `restore`, `fillStyle` / `strokeStyle`, `globalAlpha`, the line dash and its offset, `font` (its weight and
  italic only), `textAlign` and `textBaseline`, and the clip.
- **Paths** (`canvas/path.rs`) are flattened to polylines in the view's points, within half a point: lines,
  quadratic and cubic curves, `arc`, `arcTo`, `ellipse`, `rect` and `roundRect`, under the transform at the time
  each part was added.
- **Fills** set the dots whose centers the fill rule (nonzero or even-odd) puts inside the path, every subpath
  closed; `fillRect` fills a rectangle without touching the current path. **Strokes** set the dots each segment
  passes through, one dot wide; `strokeRect` strokes a rectangle. `clearRect` empties dots.
- **Clips** are masks of dots, each new one intersected with those in effect; fills, strokes and text stay inside.
- **Dashes** are sampled at each dot's center along its subpath, in user-space lengths scaled by the transform; an
  odd pattern repeats, and every subpath starts the pattern again.
- **Colors**: a gradient draws its middle stop, and an animated color (`Paint::Slot`) its current value from the
  canvas arguments the native animator writes (transparent until it has one; numbers read 0). A paint's alpha
  times `globalAlpha` is part of its color, read by the palette; the view's opacity is not. A Cell shows one
  ink: that of its most recently painted dot.
- **Text** (`fillText` / `strokeText`) is printed on whole Cells above the dots, one row tall: `textAlign` places
  it by its width in Cells, and `textBaseline` puts its row's top at `y` (`top`, `hanging`), half a row above
  (`middle`), three quarters of a row above (`alphabetic`) or a whole row above (`ideographic`, `bottom`). A weight
  of 600 or more is bold; `maxWidth` cuts it to the Cells that fit.

A drawing's nodes (`Drawing::nodes`) are pressed as a pointer at their center presses: the agent's model finds the
node, the canvas reports its Cells like any view, and the press goes to whatever takes presses under that Cell, the
canvas itself included.

## The terminal Key scheme

`key_scheme.rs` maps keystrokes to actions; `keyboard.rs` applies focus, activation and scrolling, and
`navigation.rs` handles back. This keeps a future vim scheme independent of the actions, with one scheme selected
today and no mode switch. The app's `nsh/keymap` table is matched first, including its editing and repeat policy.
The standalone host reserves Ctrl+C for exit. Outside fields, C-h e opens diagnostics; inside fields,
C-h is Backspace (leave the field with Esc or C-g to open diagnostics). While the diagnostics panel is open,
it owns keys and the app's field caret is hidden; closing it restores the caret when an editable field has focus.

| Keys | Action |
|---|---|
| Ctrl+N, Down, Tab | Next focusable view in tree order |
| Ctrl+P, Up, Shift+Tab | Previous focusable view in tree order |
| Ctrl+F, Right / Ctrl+B, Left | Nearest focusable view to the right / left on the same row |
| Enter | Press the focused view, reporting PressDown, PressUp and Press in order, at its middle |
| Esc, Ctrl+G | Leave the focused field; otherwise go back through the app handler, stack or newest modal |
| Ctrl+V, PageDown / Alt+V, PageUp | Scroll one viewport down / up in the innermost Scroll around focus or the search point |
| Alt+<, Home / Alt+>, End | In that Scroll, focus the first / last control in tree order; center it when a vertical reveal must scroll, or scroll to the endpoint when there are no controls |
| Ctrl+S / Ctrl+R | Search the text on screen forward / backward in the Minibuffer |
| Ctrl+C | Quit |

Focus visits pressable and editable views, honoring `focusable` and disabled state, in depth-first document
order within the focus scope, as NSH's other platforms do. A sidebar's stops stay together before the next
panel's stops, regardless of their Cell positions. Traversal wraps at the ends; horizontal movement stays
spatial, choosing the nearest control on overlapping row ranges. Traversal centers a control vertically when
revealing it requires scrolling, and leaves fully visible controls in place (see Scroll content).
The sink opts into `MountItem::TreeOrder`: the core supplies logical document order in the same batch as
structural changes. `Insert` retains native paint order, including `zIndex`; raising a sidebar changes its
stacking while its focus stops retain their document order. Ordinary text and style updates keep the stored
order. The backend filters that order through the completed frame's geometry and current focus scope.
Concealed screens supply no candidates. When a modal is open, candidates come only from the newest modal.
Focus and focus-within states are reported through TrackStates and `EngineControl::set_states`, so the app's
focus styles apply. Pressable views have no platform focus marker: their app-defined focus styles are the
indicator, and their terminal cursor is hidden. `AppView::render` reserves the terminal cursor for a focused
editable field's caret and respects `caret={false}`. A buffer-only Widget cannot position the terminal cursor.
Removing an ordinary focused view clears focus. Modal presentation and closing use the focus history described
above; unfocused surfaces ignore keys.

Focus changes clear the old focused state before setting the new one, so the core's derived `focus-within`
state stays on a shared ancestor when moving backward between its controls. The focused control is drawn
reversed over its Cell box, transformed with the control before Cell rounding and ancestor clipping; the app's
outlines and its other focus styles' fills are not drawn (the terminal look above).

The host preserves Ctrl, Alt, Shift and known releases; crossterm also decodes an ESC-prefixed character as Alt.
`Input::KeyRepeat` preserves a repeat identified by the terminal, so a nonrepeating app shortcut consumes it
without firing again. Navigation activation completes a press in one action; a later release does not press
again. Legacy terminal presses are independent actions: Ctrl+I/Tab, Ctrl+M/Enter and Ctrl+[/Esc can be
indistinguishable, and a terminal may consume Ctrl+S/Ctrl+Q for flow control (the host's raw mode turns
the tty's own flow control off). Kitty negotiation is separate work. Editing uses the keys below while a field
is focused.

## The Minibuffer

`search.rs` searches the text on screen incrementally, as readline's and Emacs's incremental searches do:
`C-s` forward, `C-r` backward. The app's shortcuts are matched first, so an app that binds either key keeps it;
inside a field both still search. While searching, the Minibuffer owns the keys:

| Keys while searching | Action |
|---|---|
| Characters, bracketed paste | Extend the query; the current match stays while it still matches, else the next one in the search's direction is taken |
| Ctrl+S / Ctrl+R | Next / previous match; with an empty query, search for the previous search's query |
| Backspace, Ctrl+H | Return to the query and match before the last key |
| Enter | End the search at the match |
| Esc, Ctrl+G | Cancel: restore the focus, the search point and the Scroll offsets from before the search |
| Other keys, mouse input, losing focus | End the search at the match; the key or mouse input then acts as usual |

The search reads the rows the Cell text engine laid out for the Text views of the current focus scope (the
newest modal, else the page), including text scrolled out of a Scroll's viewport; concealed screens and hidden
views supply none. Field contents and canvas text are not searched. A match lies within one displayed row and is
compared grapheme by grapheme; a query without capitals ignores case (smart case). Matches are ordered by Cell
position, top to bottom and left to right, and the order wraps at both ends. A search starts at the focused
control (inclusive), at the search point, or at the first / last match when there is neither.

A match moves keyboard focus to the nearest focusable view around it (a button's label focuses the button) and
reveals the match's row in its surrounding Scrolls, innermost first. As with focus traversal, a vertical reveal
that must scroll centers the row in the viewport, within the content limits; an already fully visible row stays
in place. Horizontal reveals retain the margin described under Scroll content. A match in other text clears
focus and becomes the search point: Next / Previous (Ctrl+N, Ctrl+P, Tab, Shift+Tab) then continue from it to the nearest
control after or before it in tree order, so a match in a page panel moves within that panel rather than to a
sidebar beside it on the same rows. Paging and endpoint keys use the innermost Scroll around that search point,
including after the Minibuffer closes.

While searching, the platform draws the Minibuffer over the bottom row of the surface's area, in the terminal's
default colors: `/` (forward) or `?` (backward), after less and vim, then as much of the query's end as fits,
with the terminal cursor after it. A query with no match is crossed out. The current match's Cells are reversed, or underlined inside a box
already reversed (the focused control it moved to, a selected row), so the match stays visible.
The prompt is a symbol because the platform draws no built-in strings. The app's layout never changes for the
Minibuffer, and once the search ends nothing of it, or of the match highlight, is drawn. A match on the bottom
row lies under the Minibuffer until the search ends. Positions come from the last completed frame.

### Base controls

The official gallery's Controls demo uses the same `nsh/base` components in the terminal. Button, Checkbox,
Radio/RadioGroup, Switch and Tab/Tabs take Enter after focus traversal; disabled controls are skipped. The app
owns their values and the content of Collapsible. Checked, mixed, selected and disabled styles, including the
projected states of Checkbox indicators and Switch tracks/thumbs, are resolved by the core and painted through
ordinary mount batches. The backend supplies interaction states and never substitutes a terminal control.

A focused Slider thumb installs its component's keys in the existing app keymap: arrows change by a step,
PageUp/PageDown by a page, and Home/End move to its limits. Those bindings run before terminal navigation and
scrolling. Tab/Shift+Tab leave the thumb; Ctrl+N/Ctrl+P remain traversal keys. Range thumbs stay within each
other and disabled thumbs are skipped. Progress draws the app's indicator as a solid leaf or hairline in its
palette color, including indeterminate indicators sized by the app; its track is a container and draws no fill.
Selected Tabs are reversed over their whole Cell box; Checkboxes, Radios and Switches show their state with
terminal glyphs (the terminal look above).

Mouse presses use the same hit regions as other views when the host captures the mouse. The controls
acceptance covers keyboard and press behavior; Slider dragging and hover feedback require the joint pointer
acceptance described below. Decorative shadows (including Tailwind rings) and 3D transforms retain
their documented degradation below; focus is the reversed box, not the gallery's outline-based focus styles.

## Mouse and text selection

The standalone host leaves mouse capture off so the terminal emulator owns ordinary selection. Start
`nsh-tui-host --mouse --bundle <bundle.js>` (or `--mouse --dev-server <url>`) to capture clicks, motion and wheel
reports. An embedder owns this transport choice and forwards `Input::Mouse`; AppView never changes terminal
modes. The test agent forwards the same input independently of terminal capture.

`input.rs` picks the last painted, clipped hit regions inside the newest modal. Hover follows the ancestor
chain, reports `HOVERED`, HoverIn/Out/Move and rechecks the pointer after rendering a changed tree. Leaving the
surface or losing focus clears hover. Pressed states clear on release, cancellation or a drag. Pointer coordinates
are absolute Cells at the input seam and Cell centers in points in events, including an embedded area's offset.

A primary-button drag retains its initial target outside the surface. The core's Grab, Gate and Candidate
helpers choose native pans or content scrolling; Touch start/move/end/cancel events feed the responder system,
including `panActivation`. A blocking JS responder prevents native gesture takeover. Native pans write the
surface's Animator and report translation, velocity and values at release; the animation renderer consumes that
same graph. The terminal deliberately admits mouse drags as scroll candidates, so dragging unclaimed scroll
content moves it directly within its bounds. Selecting text takes precedence over content scrolling.

`selection.rs` adapts the core SelectionModel to the visible tree's text layouts. `<View selectable>` groups its
texts (nested groups belong to the outer group); `userSelect: "text"` selects a standalone Text and
`userSelect: "none"` excludes it. Dragging highlights whole graphemes with reversed Cells, Shift+click extends,
and Ctrl+Shift+C, Ctrl+Insert, Alt+W or Cmd+C copies through the existing clipboard backend (OSC 52 in the
standalone host). Ctrl+C also copies at the AppView seam; the standalone host reserves it for quitting.
Ctrl+A / Cmd+A selects the area's texts and Esc / Ctrl+G clears the selection. Clicking elsewhere or losing
focus clears it. A click on selectable pressable text still presses; a drag past the shared press slop selects.
The layout retains each painted grapheme's UTF-16 range, including wide characters and ellipsis gaps; copying
uses original text and joins paragraphs with newlines. Highlights stay within clipped text Cells and below
covering views. Removed text endpoints clear the selection through the shared model.

## Syntax highlighting

The terminal runtime enables `nsh-base`'s built-in highlighting, as the desktop backends do.
Editor, DiffView and TextView code blocks draw their language token colors in the palette in both appearances.

## Text fields

`text_input.rs` keeps one `nsh_core::text_edit::Model` per Input. The core model owns the text, byte selection,
character/word boundaries, replacement, deletion and undo history. The backend maps the terminal Key scheme to
those operations and projects text to Cells. `InputValue` updates are accepted only when their edit count is at
least the local count; an equal value preserves the selection and undo history. Every local edit reports one
`ChangeText` with the new count, including when the app has no change listener. Selection events use UTF-16
positions; Cell columns and model byte offsets are never used as event offsets. Typed insertions and Backspace
report `KeyPress`; Enter reports it before `SubmitEditing`. Focus changes use the shared keyboard Focus/Blur and
tracked-state path. `autoFocus`, `focus()`, `blur()` and `setSelection` use the same field state.

Tab / Shift+Tab enter and leave fields. C-n/C-p and Up/Down traverse controls in single-line fields; in multiline
fields they move between visual lines, retaining the desired Cell column across short lines. A mouse press in a
field positions its caret; dragging or Shift+click extends the selection. The last painted, clipped hit regions
respect modal scope and `pointerEvents`. Esc / C-g clears field focus before a later cancellation reaches the app.
App shortcuts run before editing, with the existing `When::Editing` policy.

| Keys in a field | Action |
|---|---|
| C-a / C-e, Home / End | Start / end of the logical line |
| C-f / C-b, Right / Left | Forward / back one core character boundary |
| M-f / M-b | End of the next / start of the previous alphanumeric word (readline) |
| Ctrl+Right / Ctrl+Left | Core desktop word boundaries, including trailing / leading whitespace |
| Shift+arrows, Shift+Home/End | Extend the selection |
| C-n / C-p, Down / Up (multiline) | Next / previous visual line |
| C-d / Delete, C-h / Backspace | Delete forward / back |
| C-k / C-u | Kill to line end / start; C-k at line end also kills the newline |
| C-w / M-d | Kill back to the whitespace boundary / forward to the end of the next alphanumeric word |
| C-y | Insert the last killed text, shared between fields of this surface |
| C-t | Transpose the characters beside the caret, or the last two at the end |
| Ctrl+Shift+C / Ctrl+Shift+X, Cmd+C / Cmd+X | Copy / cut the selection |
| Ctrl+Insert / Shift+Delete, M-w | Copy / cut; M-w copies |
| Ctrl+V, Cmd+V, Shift+Insert | Paste the clipboard text |
| Ctrl+Shift+A, Cmd+A | Select all; the test agent's clear step uses Ctrl+Shift+A |
| Ctrl+Z / Ctrl+Shift+Z, Cmd+Z / Cmd+Shift+Z | Undo / redo |
| Ctrl+Home / Ctrl+End, M-< / M-> | Start / end of the text |
| Enter | `InputConfig::submit()`: newline, submit, or submit and blur |
| Shift+Enter / Alt+Enter (multiline) | Insert a newline even in a field configured to submit |
| Esc / C-g | Leave the field |

The readline word policies follow [GNU Readline's killing commands](https://www.gnu.org/software/bash/manual/html_node/Commands-For-Killing.html);
C-w treats punctuation within a whitespace-delimited word as part of that word. They choose a range using the
core's character boundaries, then use the core Model for selection and deletion.

The app's `InputContent` box defines padding and border offsets. Text is a Paragraph with the app's text style;
empty fields show the placeholder in its palette color. Single-line fields center their text and caret vertically
in that content box; drawing, selection and pointer placement use the same row. Multiline fields begin at its top.
Graphemes stay whole; wide graphemes occupy their display width,
multiline fields wrap at Cell boundaries, and single-line fields scroll horizontally. The field scrolls its caret
into sight vertically and horizontally. Selected glyphs are reversed. Masked fields show one `•` per grapheme and
do not export their selection through copy/cut. `AppView::render` places the terminal cursor at the caret;
`caret={false}` hides it. Input-method composition belongs to the terminal emulator; committed characters enter
through the ordinary key/paste path.

Bracketed paste is one model insertion and one ChangeText event; it is clipped by the core's
UTF-16 `maxLength`. The core's `pasted` helper normalizes CRLF and single-line line breaks. `Paste` is reported
before insertion, and `pasteBehavior="report"` reports without insertion.

Fields share the exact `ClipboardBackend` passed to their engine, including the test agent's isolated clipboard.
An embedder without one gets a memory clipboard. The standalone host supplies OSC 52: writes are base64 encoded,
queued from engine calls and flushed between terminal frames so escape sequences cannot interleave. Its copied
text is retained in process for reads, including over SSH. Before a local copy, it tries the available desktop
reader (`pbpaste`, PowerShell's `Get-Clipboard`, `wl-paste` or `xclip`). Bracketed paste is the terminal emulator's
clipboard delivery and does not require a remote read. OSC 52 delivery requires the user's terminal/multiplexer to
permit clipboard writes; `nsh test` uses memory and never writes to the machine's clipboard.

## Degradation in this slice

| Feature | Terminal representation |
|---|---|
| Performance monitor | One corner line, toggled by `C-h p` or enabled by `--fps`; idle draws stop and readings also go to `nsh::perf` |
| Development logs and connection/build status | Append-only log file, visible in the on-demand `C-h e` panel |
| Test screenshots | Versioned UTF-8 JSON of Cell symbols, foreground/background/underline colors and modifiers |
| Development errors | Dismissible panel, with the mapped message and stack; cleared by the next update |
| Colors | The terminal's 16-color meanings by `palette.rs`: hue-band accents, the default foreground or dim, read against the opaque fill the app drew them on or the appearance's page |
| View fills | No color: opaque fills and fills of at least 50% alpha clear the Cells they cover to the terminal background; childless solid leaves reversed in their palette color; lighter fills nothing |
| Opacity | Ignored, except that a view below 3% draws nothing |
| Selected, checked and focused views | Reversed over their Cell box; focused fields show the terminal cursor |
| Borders | Decorative: straight single lines in per-side palette colors over a reserved inner Cell rect, drawn whole or not at all; boxes without room for a whole border around their content draw none |
| Buttons | Decorative: `[ label ]` in the button's own Cells around its one row of text, or its accessibility label when it shows only icons; no border or fill |
| Icons and decorative images | Decorative: canvases with role `image` or no accessibility label and image placeholders under 3 × 2 Cells are not drawn |
| Outlines, including control focus outlines | Left out; focus is the reversed box |
| Anchored overlays | Snapped outside the anchor's Cell rect on the core-resolved placement side |
| Gaps between texts | Positive point gaps between one-line sibling texts or canvas text runs retain at least one Cell; zero gaps keep adjacent text |
| Childless hairline fills | Below half a Cell, horizontal or vertical line glyphs in the fill's palette color |
| Positive sub-Cell extents | At least one Cell; zero-width progress indicators remain empty |
| Base control value states and projected part states | Core-resolved checked, mixed, selected and disabled styles; selected tabs reversed |
| Checkbox, radio and switch states | Decorative: `[x]` / `[ ]` / `[-]`, `(•)` / `( )`, `[on ]` / `[off]` in place of the indicator (short forms `x`, `-`, `•`, `on`, `off`), or before the label without one; only focus reverses them |
| Slider keyboard input | Component-owned arrows, page and endpoint keys while its thumb has focus |
| Slider dragging and control hover | Ticket-09 pointer follow-up |
| Border width, dashes, dots, corner radii | Decorative: solid single-Cell strokes with square corners; radii shape fill coverage only; widths still participate in core layout |
| Text | Styled Paragraphs: palette foreground, bold at weight 600 and above, italic, underline and strikethrough |
| Hover and pressed states | App-defined state styles and hover events on the Cell hit regions |
| Mouse drags | Native pan values, responder moves and bounded content scrolling |
| Selectable text | Reversed grapheme Cells; copies use the shared clipboard and OSC 52 |
| Pointer precision | One terminal Cell, converted to its center in points |
| Input | Editable Paragraph, reversed selection and terminal caret; single-line text and caret centered in the content box; masked text is dots |
| Searching the screen | The Minibuffer over the bottom row and the match's Cells reversed (underlined inside a reversed box), only while searching |
| Keyboard paging and endpoints | The innermost Scroll around focus or a plain-text search point; otherwise the first Scroll in the active scope |
| Focus and search reveals | Fully visible targets stay in place; vertical scrolling centers the target within the content limits, with core margins for horizontal or oversized targets |
| Input-method composition | Terminal emulator composition, committed text delivered to the field |
| Input accessories (`InputAccessoryView`) | Left unmounted, as on the desktops: no software keyboard to show them over |
| Clipboard | OSC 52 writes, desktop reads when available and an in-process copy |
| Syntax highlighting | The built-in Editor, DiffView and TextView token colors in the palette |
| Inline attachments, mentions and inline math | Measured child content placed in whole Cells, wrapping with the surrounding text |
| Font size, family, line height and letter spacing | Terminal font, display width and one row per line; imported fonts load from adjacent `assets/` or the development server |
| Shadows, blur, materials, 3D transforms and blend modes | Decorative features left out silently in this slice |
| View gradients (`backgroundImage`: linear, radial and conic) | Decorative: left out, as every fill color |
| Native animated values | Host-frame opacity, background/border/text colors, transforms and canvas slots; completion and cancellation reported |
| 2D view transforms | Composed positions and axis-aligned Cell extents; upright terminal glyphs and Cell-aligned borders |
| Canvas transforms | Affine paths on the braille grid, with animated numeric and color slots |
| Navigation, modal, layout and style transitions | Final state applied immediately under reduced motion |
| Scroll | Children translated by the current offset and clipped to the viewport |
| Stack, Screen and HeaderItem | The top screen and its selected header, clipped and styled by the app |
| Modal | A surface-sized root above the page, with its own focus scope |
| Canvas | Braille dots (2 × 4 per Cell) for fills and strokes, text on whole Cells above them |
| Canvas line widths, caps, joins and miters | Decorative features left out: every stroke is one dot wide |
| Canvas shadows, compositing (`globalCompositeOperation`) and images (`drawImage`) | Decorative features left out silently |
| Canvas gradients | The gradient's middle stop as a flat color |
| Canvas colors within a Cell | One palette ink per Cell: the most recently painted dot's |
| Other view kinds and unregistered native components | Named Blocks, reported once per kind/name, while their Content feature ticket supplies its renderer |

The trunk applies View/Text geometry, styles, props, text, events, appearance, component registration, core scroll
positions, top screens and their headers, modal cancellation, terminal navigation, search and editing keys, InputValue/InputContent, field commands and app exit. Other mount operations are reported once per operation kind per loaded app; they are not implemented by
an empty success path. The full Content feature reporting and degradation policy grows with those renderers.
Safe areas are zero, reduced motion is true and `screen_reader` is false.

## Checks

The single integration test binary exercises the public `TextEngine` and `AppView` seams. It uses ratatui's
`TestBackend`, real engines and the counter example compiled by NSH's single-module compiler. It checks visible
cells, mouse-driven changes, area ownership, resize, callbacks and idle scheduling; canvases are checked by their
braille Cells, clipping, fill rules and a press at a node's center, and the library's unit tests cover path
flattening, fill-rule spans and dash patterns. The host's integration test
bundles the counter into a temporary directory, loads it through `Bundle::read`, and drives `run_app` with
crossterm events passed through `convert`. Its scripted user locates `+1` in the flushed cells, clicks it and
waits through idle polls for the value and history to change. It needs no prebuilt CLI, existing bundle or TTY.
The development tests use a real local `DevServer` and `TestBackend` to check complete loads, hot updates retaining
clicked state, rejected-patch reloads, errors and the log panel. CLI subprocess tests select a temporary host via
`NSH_RUNTIME` to check inherited input/output, file diagnostics (including `-v`) and Cell scale precedence and
validation. These are additional ticket-02 seams. The test-agent acceptance seam is `nsh test --target tui`: the existing
counter test and `crates/nsh-tui-host/tests/fixtures/agent` (styled Cell snapshots after a press, and startup errors).
The pointer tests exercise hover, responder capture, pan values, drag scrolling and cross-text copying through
AppView; the pointer fixture imports the unchanged Physics, Gestures and Resizable demos. Their runtime acceptance
and pan animation rendering must be checked together with the animation ticket.
The text-input fixture checks typing, readline navigation, one large paste, selection, copy/cut through the
engine's isolated clipboard, masked Cells, multiline paste and report-only paste. Its Form entry, a sign-up form of
its own, checks invalid fields and successful account creation.
The `controls` fixture imports `examples/gallery/demos/Controls.tsx` unchanged, traverses and operates its
controls with terminal keys, exercises ordinary mouse presses, and reads Cell screenshots. AppView controls
tests cover reversed focus without app outlines, backward focus-within transitions, projected value/disabled
styles, sub-Cell progress indicators and focused Slider keys. The palette module checks each color rule, both
appearances, attributes, fills and reversed states. These tests do not exercise hover or Slider drags yet.
The search fixture searches forward and backward through a Scroll's rows and checks the Minibuffer row before,
during and after the search.
The animation module at the AppView seam checks completion, cancellation, active-frame and idle scheduling,
app replacement, binding removal, animated fills, transform geometry and presses, and canvas slots. The
`animation` host fixture checks final Cells and callbacks through `nsh test --target tui`.
The host's unit checks cover crossterm conversion and busy-frame input. The latter runs the real `run_app`,
parser and AppView in an isolated process on a private PTY. Each backend flush requests an app update and waits
for its engine wakeup, forcing a due frame at every input poll without timing sleeps. Queued Ctrl+N keys must
focus four controls in order and Enter must activate the fourth. The two ignored lifecycle checks require a
real terminal; the ignored input worker is launched automatically by its supervising test.

```sh
cargo test -p nsh-platform-tui --features dev,run-app --test tui
cargo test -p nsh-platform-tui --lib canvas
cargo test -p nsh-tui-host
cargo build -p nsh-cli -p nsh-tui-host
./target/debug/nsh test examples/counter --target tui
./target/debug/nsh test crates/nsh-tui-host/tests/fixtures/text-input --target tui < /dev/null
./target/debug/nsh test crates/nsh-tui-host/tests/fixtures/pointer --target tui < /dev/null
./target/debug/nsh test crates/nsh-tui-host/tests/fixtures/agent --target tui < /dev/null
./target/debug/nsh test crates/nsh-tui-host/tests/fixtures/controls --target tui < /dev/null
./target/debug/nsh test crates/nsh-tui-host/tests/fixtures/search --target tui < /dev/null
./target/debug/nsh test crates/nsh-tui-host/tests/fixtures/animation --target tui < /dev/null
cargo test -p nsh-cli --test cli
cargo test -p nsh-tui-host --lib terminal::tests::normal_exit_restores_raw_mode -- --ignored --nocapture --test-threads=1
cargo test -p nsh-tui-host --lib terminal::tests::panic_restores_raw_mode -- --ignored --nocapture --test-threads=1
```

After the lifecycle checks, verify that the shell cursor is visible, the main screen is back and typed input is
echoed. The panic check deliberately emits a panic and catches the unwind. These terminal checks, counter demo,
Linux/Windows checks and performance measurements require runtime validation; `cargo check` proves compilation
only. The CLI acceptance commands above check the complete handshake, in-memory rendering and screenshot
transport, including execution with standard input redirected.
