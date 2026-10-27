# The terminal backend (ratatui)

`crates/nsh-platform-tui` renders an NSH surface into a terminal grid with ratatui. Its `AppView` lives on the
embedding app's thread; the app owns the terminal, its event loop and the area given to the surface.
`crates/nsh-tui-host` is the Development host: it fills the terminal with one surface and restores the terminal
when it exits. It loads source bundles directly or connects to the CLI's development server.

```sh
cargo build -p nsh-cli -p nsh-tui-host
./target/debug/nsh run examples/tui-gallery --target tui
./target/debug/nsh run examples/counter --target tui --points-per-cell 8x16
```

The CLI serves and watches the app in its own process. It builds `nsh-tui-host` in a checkout, using the CLI's
profile, and starts it with inherited stdin/stdout. CLI diagnostics, the host's stderr and the host logger append
to the app's `.nsh/tui.log`, including verbose tool output. The host alone writes the terminal UI. Ctrl+C exits;
Unix stop signals and Windows console breaks ask the host loop to exit so its terminal guard can restore the
screen and raw mode before the CLI reaps it.

`C-h e` toggles the logs and errors panel. Escape, `C-g` or `q` closes it; arrows and PageUp/PageDown scroll,
Home/End select the start/end, and `r` reloads the latest build. Errors open the panel automatically and the next
update closes it. Ordinary builds and connection messages stay in the log. With the panel closed, the app owns
every row. The panel reads at most 64 KiB from the log tail every 250 ms while open; its recent error history keeps
at most 256 lines of 4096 characters each. Control characters are removed before drawing diagnostics.

A source bundle can also be loaded directly:

```sh
./target/debug/nsh build examples/counter --out .nsh/tui-counter
./target/debug/nsh-tui-host --bundle .nsh/tui-counter/bundle.js --log-file .nsh/tui-counter/host.log
./target/debug/nsh-tui-host --dev-server ws://127.0.0.1:8790/ws
```

`--bundle` reads the adjacent source map and capability metadata. Bytecode files require the trusted-resource
or verified-package path, owned by the packaging work. Direct hosts without `--log-file` keep diagnostics in
`nsh-tui-host-<pid>.log` in the system temporary directory, visible through the same panel. `--trace` includes
the engine's timing logs; app `console.log` output is recorded with or without it. `--data-dir` selects persistent
app data. A source-bundle failure exits after terminal restoration; a development session stays open for a fix.

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
including the embedding area's origin. Bounds follow rounded parent origins, scrolling, clipping and concealed
screens. The agent drives press/release, motion, wheel events, individual keys and bracketed paste. Their behavior
is the platform's current input implementation: this adapter does not emulate missing input features. Field
editing, richer pointer/scroll behavior and system dialogs are supplied by their respective platform tickets.
`setViewport` currently returns an error; select the initial viewport with `open(entry, { width, height })`.

`app.screenshot(path?)` returns bytes and optionally writes them on the test machine, through the existing
base64 screenshot transport. On the terminal target those bytes are UTF-8 JSON (use a `.json` path):

```json
{"format":"nsh-tui-cells","version":1,"width":1,"height":1,"rows":[[{"symbol":"A","fg":"#123456","bg":"reset","underline":"reset","modifiers":["bold"]}]]}
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

`examples/tui-gallery` is the reference for how an app designed for the Terminal idiom looks; the platform
restyles nothing, so the look is the app's own, after Emacs's dired, magit and ibuffer:

- Dense and text first: sizes in whole Cells (8 points across, 16 down), at most one Cell of padding, one blank
  row between sections.
- Section headings are bold text, not bordered cards; a header line names the page, a rule of `─` below it.
- Buttons are text, `[ OK ]`; switches `[on ]` / `[off]`, checkboxes `[x]`, radios `(*)`, levels `━━━──`.
- One accent color marks what is selected or active (a selected row also gets a `▸` marker); green and red only
  carry a value's meaning.
- Separators are drawn with line characters (`─`, `│`, borders); there are no filled blocks. The pages set no
  background, so the terminal's own shows through, and the palette follows the light or dark appearance. The only
  fill is a dialog's, which has to hide the rows under it.

```sh
./target/debug/nsh build examples/tui-gallery --out .nsh/tui-gallery
./target/debug/nsh-tui-host --bundle .nsh/tui-gallery/bundle.js
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
  and pointer policy. Fills and borders use `Block`; the text engine prepares styled `Paragraph`s, which are
  reused on frames. Clipped text uses `Paragraph::scroll`; a clipped border drops the sides outside the clip.
- **Input belongs to the embedder.** `Input` is independent of a terminal backend. Mouse positions are absolute
  terminal Cells; `handle` localizes them against the last rendered area, follows the painted hit regions and
  uses the core's `input::Press` to deliver PressDown, PressUp and Press. Releasing outside the area cancels the
  press and returns `Handled::Ignored`. Unused keys and pastes are ignored. `set_focused` lets an embedder remove
  keyboard focus and cancel a pending press.
- **Scheduling is passive.** The wake callback only schedules a render on the embedding thread. Wakeups coalesce
  until rendering begins. `next_frame_deadline` is immediate for pending messages, timed for an engine display
  frame request, and `None` while idle. Error and appearance callbacks run during render on the embedding thread.
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
component ticket. The terminal idiom, animation and the Release runner are separate slices. The current environment uses desktop behavior and the machine's real OS.

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
input content boxes follow the same rule when the input renderer is added.

A box first snaps its origin relative to its parent's snapped origin. Its right and bottom edges are then computed
from that snapped origin plus its point width and height, using the same rounding rule. Children and content use
this snapped origin, including when the parent is partly off screen; clipping happens afterward. Hit regions reuse
the resulting visible rectangles, with the right and bottom edges exclusive. They never convert points again.

For example, a box at y = 216pt snaps to row 14 (224pt). Its 40pt height places the bottom at 264pt, which rounds
up to row 17: rows 14, 15 and 16 draw. With 12pt padding above and below one 16pt text row, the text starts at
236pt and rounds to row 15, the middle row. Keeping an unrounded parent origin for text would instead put that
label on row 14. This projection also keeps a box's Cell dimensions stable as its origin crosses a half Cell;
it can change a sub-Cell gap, while layout remains the core's. Text measurement still counts the whole Cells that
fit within a point width; that capacity calculation is separate from positioning edges.

`CellTextEngine` counts Unicode display width: wide characters take two Cells and grapheme clusters stay whole.
Every line is one row at any font size. Wrapping prefers spaces, then breaks inside a word. Explicit newlines
preserve empty rows. A grapheme wider than the available line remains whole and is clipped by drawing.
`numberOfLines` limits the rows; tail, head and middle ellipses reserve one Cell, while clip adds no marker.
Alignment becomes a whole-Cell indent. Line geometry reports UTF-16 offsets and point dimensions, as the core's
`TextEngine` contract requires. Custom font registration succeeds with the terminal font remaining in use.

## Page backgrounds

A **Page background** is the fill of the surface root, an app's top-level view, a screen or a modal, and any
view fill of exactly the same RGBA color. These fills draw `Color::Reset`, so the terminal supplies its own
background. A matching fill resets the background even when it lies over a colored card; it does not inherit that
card's color. Other fills keep the app's colors, including colors that differ by a single channel value.

The attached tree's page colors are collected after each complete mount batch, before drawing. This makes
matching independent of paint order and follows style changes, detachments and app replacement. Transparent fills
remain transparent. The terminal Key scheme is described below; the Minibuffer and mouse-capture controls are
separate interaction work.

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
Each screen remembers its last focused control. When that screen becomes visible again after back or an
app-driven removal, the control regains focus if it still exists, is focusable and shows. A missing or hidden
control leaves focus empty. Modal focus restoration takes precedence over this page history.

Modals cover the surface and paint above the page. A batch's new modals follow the completed tree's order (core
sibling insertions may arrive in reverse); a modal presented in a later batch goes above the existing ones.
Presenting a modal saves the previous focus and moves focus to its first focusable view in reading order; an
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
innermost first. Paging and endpoint keys act on the nearest Scroll around focus, or the page's Scroll when there
is none. Paging moves focus to the visible control nearest its old relative row in the viewport (relative column
for horizontal scrolling); Home/End choose the first/last visible control after moving to the endpoint. Selection
uses the next completed frame's geometry, and Enter never presses an off-screen control. These user-driven
offsets keep the applied count unchanged; only core ScrollBy and ScrollTo increment it.

Each wheel step moves one Cell along the chosen axis. Shift sends vertical wheel steps horizontally. The core's
`input::scroll_target` selects the innermost Scroll under the pointer that can move; at an edge the same-axis
parent can take the event. This search stays inside the newest modal. A vertical wheel uses a horizontal view
only when no surrounding Scroll takes the vertical axis. Disabled scroll views do not take the wheel.

Wheel scrolling keeps the current keyboard focus while any of its Cells remain visible. If it leaves sight,
focus moves to the nearest visible focusable view in the current focus scope, measured from its previous Cell
position. A wheel event with no keyboard focus creates none. Focus selection uses the completed scrolled frame,
so the next activation cannot target content that has left the viewport. Indicators, sticky cells, gestures and
scrolling animations remain separate work.

## The terminal Key scheme

`key_scheme.rs` maps keystrokes to actions; `keyboard.rs` applies focus, activation and scrolling, and
`navigation.rs` handles back. This keeps a future vim scheme independent of the actions, with one scheme selected
today and no mode switch. The app's `nsh/keymap` table is matched first, including its editing and repeat policy.
The standalone host reserves Ctrl+C for exit and C-h e for diagnostics. While the diagnostics panel is open,
it owns keys and the app's field caret is hidden; closing it restores the caret when an editable field has focus.

| Keys | Action |
|---|---|
| Ctrl+N, Down, Tab | Next focusable view |
| Ctrl+P, Up, Shift+Tab | Previous focusable view |
| Ctrl+F, Right / Ctrl+B, Left | Nearest focusable view to the right / left on the same row |
| Enter | Press the focused view, reporting PressDown, PressUp and Press in order, at its middle |
| Esc, Ctrl+G | Go back through the app handler, stack or newest modal |
| Ctrl+V, PageDown / Alt+V, PageUp | Scroll one viewport down / up |
| Alt+<, Home / Alt+>, End | Scroll to the start / end |
| Ctrl+C | Quit |

Focus visits pressable and editable views, honoring `focusable` and disabled state, in top-to-bottom and
left-to-right Cell order. Traversal wraps at the ends; horizontal movement stays on overlapping row ranges.
Concealed screens supply no candidates. When a modal is open, candidates come only from the newest modal.
Focus and focus-within states are reported through TrackStates and `EngineControl::set_states`, so the app's
focus styles apply. Pressable views have no platform focus marker: their app-defined focus styles are the
indicator, and their terminal cursor is hidden. `AppView::render` reserves the terminal cursor for a focused
editable field's caret and respects `caret={false}`. A buffer-only Widget cannot position the terminal cursor.
Removing an ordinary focused view clears focus. Modal presentation and closing use the focus history described
above; unfocused surfaces ignore keys.

The host preserves Ctrl, Alt, Shift and known releases; crossterm also decodes an ESC-prefixed character as Alt.
`Input::KeyRepeat` preserves a repeat identified by the terminal, so a nonrepeating app shortcut consumes it
without firing again. Navigation activation completes a press in one action; a later release does not press
again. Legacy terminal presses are independent actions: Ctrl+I/Tab, Ctrl+M/Enter and Ctrl+[/Esc can be
indistinguishable, and a terminal may consume Ctrl+S/Ctrl+Q for flow control. Kitty negotiation, field editing and
the search Minibuffer are separate work; this slice adds navigation over fields, not editing their text.

## Degradation in this slice

| Feature | Terminal representation |
|---|---|
| Development logs and connection/build status | Append-only log file, visible in the on-demand `C-h e` panel |
| Test screenshots | Versioned UTF-8 JSON of Cell symbols, foreground/background/underline colors and modifiers |
| Development errors | Dismissible panel, with the mapped message and stack; cleared by the next update |
| Page background | Terminal default background (`Color::Reset`), including every fill of exactly the same RGBA color |
| Other view fills | RGB Cell backgrounds, alpha and view opacity blended over the existing background |
| Borders | One Cell per visible side; each side keeps its color; positive radius selects rounded corners |
| Border width, dashes, dots, corner curves | Solid single-Cell strokes; widths still participate in core layout |
| Text | Styled Paragraphs: RGB foreground, bold at weight 600 and above, italic, underline and strikethrough |
| Font size, family, line height and letter spacing | Terminal font, display width and one row per line |
| Shadows, blur, materials, gradients, transforms and blend modes | Decorative features left out silently in this slice |
| Scroll | Children translated by the current offset and clipped to the viewport |
| Stack, Screen and HeaderItem | The top screen and its selected header, clipped and styled by the app |
| Modal | A surface-sized root above the page, with its own focus scope |
| Other view kinds and unregistered native components | Named Blocks, reported once per kind/name, while their Content feature ticket supplies its renderer |

The trunk applies View/Text geometry, styles, props, text, events, appearance, component registration, core scroll
positions, top screens and their headers, modal cancellation, terminal navigation keys and app exit. Other mount operations are reported once per operation kind per loaded app; they are not implemented by
an empty success path. The full Content feature reporting and degradation policy grows with those renderers.
Safe areas are zero, reduced motion is true and `screen_reader` is false.

## Checks

The single integration test binary exercises the public `TextEngine` and `AppView` seams. It uses ratatui's
`TestBackend`, real engines and the counter example compiled by NSH's single-module compiler. It checks visible
cells, mouse-driven changes, area ownership, resize, callbacks and idle scheduling. The host's integration test
bundles the counter into a temporary directory, loads it through `Bundle::read`, and drives `run_app` with
crossterm events passed through `convert`. Its scripted user locates `+1` in the flushed cells, clicks it and
waits through idle polls for the value and history to change. It needs no prebuilt CLI, existing bundle or TTY.
The development tests use a real local `DevServer` and `TestBackend` to check complete loads, hot updates retaining
clicked state, rejected-patch reloads, errors and the log panel. CLI subprocess tests select a temporary host via
`NSH_RUNTIME` to check inherited input/output, file diagnostics (including `-v`) and Cell scale precedence and
validation. These are additional ticket-02 seams. The test-agent acceptance seam is `nsh test --target tui`: the existing
counter test and `crates/nsh-tui-host/tests/fixtures/agent` (styled Cell snapshots after a press, and startup errors).
The host's unit checks cover crossterm conversion; its two ignored lifecycle checks require a real terminal.

```sh
cargo test -p nsh-platform-tui --features run-app --test tui
cargo test -p nsh-tui-host
cargo build -p nsh-cli -p nsh-tui-host
./target/debug/nsh test examples/counter --target tui
./target/debug/nsh test crates/nsh-tui-host/tests/fixtures/agent --target tui < /dev/null
cargo test -p nsh-cli --test cli
cargo test -p nsh-tui-host --lib terminal::tests::normal_exit_restores_raw_mode -- --ignored --nocapture --test-threads=1
cargo test -p nsh-tui-host --lib terminal::tests::panic_restores_raw_mode -- --ignored --nocapture --test-threads=1
```

After the lifecycle checks, verify that the shell cursor is visible, the main screen is back and typed input is
echoed. The panic check deliberately emits a panic and catches the unwind. These terminal checks, counter demo,
Linux/Windows checks and performance measurements require runtime validation; `cargo check` proves compilation
only. The CLI acceptance commands above check the complete handshake, in-memory rendering and screenshot
transport, including execution with standard input redirected.
