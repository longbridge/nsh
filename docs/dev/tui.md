# The terminal backend (ratatui)

`crates/nsh-platform-tui` renders an NSH surface into a terminal grid with ratatui. Its `AppView` lives on the
embedding app's thread; the app owns the terminal, its event loop and the area given to the surface.
`crates/nsh-tui-host` is the Development host: it loads a prebuilt source bundle, fills the terminal with one
surface and restores the terminal when it exits. It draws views, text and clipped scroll content, and handles
mouse presses.

```sh
cargo build -p nsh-cli -p nsh-tui-host
./target/debug/nsh build examples/counter --out .nsh/tui-counter
./target/debug/nsh-tui-host --bundle .nsh/tui-counter/bundle.js
```

Click `+1` and `−1` to change the value; Ctrl+C exits. The host uses the source bundle reader, which also reads
its adjacent source map and capability metadata. Bytecode files require the trusted-resource or verified-package
path, owned by the packaging work. The host keeps a bounded diagnostic tail and prints it after restoring the
terminal. `--trace` includes the engine's timing logs, `--data-dir` selects its persistent data directory.

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
component ticket. Dev-server connection, the CLI target, test agent, terminal idiom, animation and the Release
runner are separate slices. The current environment uses desktop behavior and the machine's real OS.

## Points per Cell

One **Cell** is **8 × 16 points** by default. These are layout units, independent of the terminal font's physical
pixel dimensions. `CellSize::new` accepts finite, positive typed dimensions; the same value is used for the text
engine, viewport, drawing and input. CLI and app-file selection of the scale belongs to the terminal-idiom work.

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

Modals cover the surface and paint above the page. A batch's new modals follow the completed tree's order (core
sibling insertions may arrive in reverse); a modal presented in a later batch goes above the existing ones.
The newest modal owns keyboard focus. A stack inside it gets back first; otherwise it receives RequestClose with
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
is none. These user-driven offsets keep the applied count unchanged; only core ScrollBy and ScrollTo increment
it. Wheel scrolling, indicators, sticky cells, gestures and scrolling animations remain separate work.

## The terminal Key scheme

`key_scheme.rs` maps keystrokes to actions; `keyboard.rs` applies focus, activation and scrolling, and
`navigation.rs` handles back. This keeps a future vim scheme independent of the actions, with one scheme selected
today and no mode switch. The app's `nsh/keymap` table is matched first, including its editing and repeat policy.
The standalone host retains its Ctrl+C exit handling.

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
focus styles apply. The platform adds no highlight: `AppView::render` puts the terminal cursor on the focused
view's first visible Cell. A buffer-only Widget cannot position the terminal cursor. Removing a focused view or
changing its focus scope clears focus; unfocused surfaces ignore keys.

The host preserves Ctrl, Alt, Shift and known releases; crossterm also decodes an ESC-prefixed character as Alt.
`Input::KeyRepeat` preserves a repeat identified by the terminal, so a nonrepeating app shortcut consumes it
without firing again. Navigation activation completes a press in one action; a later release does not press
again. Legacy terminal presses are independent actions: Ctrl+I/Tab, Ctrl+M/Enter and Ctrl+[/Esc can be
indistinguishable, and a terminal may consume Ctrl+S/Ctrl+Q for flow control. Kitty negotiation, field editing and
the search Minibuffer are separate work; this slice adds navigation over fields, not editing their text.

## Degradation in this slice

| Feature | Terminal representation |
|---|---|
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
The host's unit checks cover crossterm conversion; its two ignored lifecycle checks require a real terminal.

```sh
cargo test -p nsh-platform-tui --features run-app --test tui
cargo test -p nsh-tui-host
cargo test -p nsh-tui-host --lib terminal::tests::normal_exit_restores_raw_mode -- --ignored --nocapture --test-threads=1
cargo test -p nsh-tui-host --lib terminal::tests::panic_restores_raw_mode -- --ignored --nocapture --test-threads=1
```

After the lifecycle checks, verify that the shell cursor is visible, the main screen is back and typed input is
echoed. The panic check deliberately emits a panic and catches the unwind. These terminal checks, counter demo,
Linux/Windows checks and performance measurements require runtime validation; `cargo check` proves compilation
only. The CLI's `nsh test --target tui` seam arrives with the test-agent ticket.
