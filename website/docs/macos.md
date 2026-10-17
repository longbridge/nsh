---
title: macOS
description: Run NSH apps on macOS in AppKit windows, package them as .app bundles, embed them in a Mac app with AppView, and what differs on macOS.
order: 13
group: platforms
---

# macOS

NSH apps also run as native Mac apps. The macOS backend renders them with AppKit views and Core Animation and lays out
text with CoreText; the core, the reactive graph, the renderer, Taffy layout and the `nsh/ui/mobile` components are the
same as on iOS and Android, so one bundle runs everywhere. On macOS, `Platform.OS` is `"macos"` and the
`desktop={{…}}` props apply. The window's title, a title bar of the app's own, the menu bar and context menus come from
[`nsh/desktop`](../modules/nsh-desktop.md).

The macOS backend behaves like a desktop app: mouse, trackpad and keyboard input, resizable windows, centered sheets
and a menu bar.

## Requirements

- **macOS 14** or later, where the app runs.
- Xcode's command line tools (`xcode-select --install`), to build NSH's macOS runtime from a checkout and to package
  apps (`codesign`, `sips`). The distribution made with `cargo xtask dist` on a Mac ships the runtime in
  `runtime/macos/`.

## Running an app

```sh
nsh run my-app --target macos
```

The app opens in an AppKit window (1200×800 by default), with hot reload, logs in the terminal and the error overlay as
on the other targets; Cmd+R reloads the latest build. The menu bar and the window show the app's name (nsh.json
`"name"`, else its directory's). In a checkout, `nsh run` builds the development host
(`nsh-macos-host`) first. Stopping `nsh run` closes the window, and closing the window ends `nsh run`. The host also
runs a built bundle on its own:

```sh
nsh build my-app --release -o build
dist/nsh/runtime/macos/nsh-macos-host --bundle build/bundle.nshc --size 1200x800   # in a checkout: target/debug/nsh-macos-host
```

`--size` is the window's initial size; the window can be resized.

### Performance monitor

`--fps` (`nsh run my-app --target macos --fps`, the host's `--fps`, a packaged app's `open -a MyApp --args --fps`, or
`AppOptions { fps: true, .. }` / `AppView::set_fps_monitor(true)` when embedding) shows a panel at the top right of the
app that updates twice a second:

| Row | Measures |
|---|---|
| `FPS` | Frames shown over the last second: display refreshes in which the app changed (an idle app reads low; that is not slowness). `max` is what the main thread could keep up with, then the display's rate (120 Hz on ProMotion screens). |
| `TICK` | The gaps between display refreshes as they arrived (mean, p95) and `gap`, the p95 gap between shown frames: one refresh (8.3 ms at 120 Hz) when nothing is late. |
| `MAIN` | Main-thread time per frame (applying the engine's changes, drawing text and canvases, animations, input), its 95th percentile, and `drop`: the share of frames that took longer than one refresh. |
| `JS` / `CORE` | Per commit: your code (handlers, timers, list rows) and NSH's core (layout, text, diffing); commits per second, native view changes per commit. |
| `HOP` | How long changes wait between the engine thread and the main thread; how busy the engine thread is. |
| `CPU` / `GPU` / `MEM` | The process: CPU (100% is one core), its GPU share, its memory footprint (Activity Monitor's figure). |

The bars below are the last 120 frames' main-thread time against one refresh (the line): green within half of it, amber
within it, red over. With `--trace` too, the host also logs the readings once a second (milliseconds as
mean/p50/p95/max). Measure release builds; the monitor costs nothing while it is off.

## Packaging

```sh
nsh package my-app --target macos [--run]
```

Writes an app bundle:

```text
out/<Name>.app/Contents/
  Info.plist
  MacOS/<Name>               the executable (nsh's release runner)
  Resources/nsh/bundle.nshc  your app, compiled
  Resources/nsh/assets/…     the assets it imports
  Resources/nsh/app.json     the window title and the app's appearance
  Resources/AppIcon.icns     the icon, when nsh.json names one (converted with sips)
```

The `.app` is signed ad hoc, so it runs on the Mac that built it. To distribute it, sign it with a Developer ID and
notarize it. `--run` opens the app after packaging it.

## Embedding in a Mac app

The `nsh-platform-macos` crate (from the NSH repository) gives a Rust Mac app `AppView`: one NSH engine as an `NSView`.
Each `AppView` is independent; several can run at once.

```rust
use std::rc::Rc;
use nsh_platform_macos::{
    objc2::MainThreadMarker, objc2_app_kit::NSWindow, AppOptions, AppView, AssetBase, Bundle,
};

fn show(window: &NSWindow) -> Rc<AppView> {
    let mtm = MainThreadMarker::new().expect("main thread");
    let view = AppView::new(mtm, AppOptions {
        asset_base: Some(AssetBase::Directory("app/assets".into())),
        ..AppOptions::default()
    });
    // register native components here, before loading
    let code = std::fs::read_to_string("app/bundle.js").expect("bundle");
    view.load(Bundle::source(code).into());
    window.setContentView(Some(&view.view()));
    // Keep the view while the window shows it; `stop` it when the window closes.
    view
}
```

`AppView::load` replaces whatever ran; `stop` ends the app. `asset_base` tells it where `asset:/` URIs resolve
(`AssetBase::Directory` or `AssetBase::Http`). Bytecode bundles (`bundle.nshc`) load with
`unsafe { Bundle::bytecode(bytes) }`: bytecode is trusted input, so only load it from your own app's files.
For an app that is only an NSH app, `run_app(source, options)` opens one `AppView` in a window with a standard menu
(About, Hide, Quit, Edit, Minimize, Zoom, Enter Full Screen, Close), which the app can replace with
`Menu.setApplicationMenu` ([`nsh/desktop`](../modules/nsh-desktop.md)), and runs until the window closes.

Native components are registered with `AppView::register_component`; see
[Native components](./native-components.md#registering-on-macos-rust).

## What differs on macOS

The environment is a desktop's: the safe area and the keyboard height are 0, `Platform.constants.interfaceIdiom` is
`"desktop"`, `Platform.Version` is the macOS version, and dark mode follows the system's appearance live. See
[Device and environment](./device-and-environment.md).

Input is a desktop's: the mouse, the trackpad (scrolling with momentum, pinch and rotation) and the keyboard.
`selectable` text is selected by dragging, double- and triple-clicking, and copied with Cmd+C. Escape, Option+Left,
Cmd+[ and the mouse's back button go back in a stack or ask a modal to close (Escape in a focused field or web page
only leaves it; a page gets these keys first, and keeps those it uses); page and form sheets are cards centered
in the window over a dimmed backdrop. A click anywhere in an `Input`, its padding too, puts the caret at the nearest
point. `Alert` and `ActionSheet` show an `NSAlert` sheet on the window: up to two alert buttons in a row (the cancel
button leftmost, the last regular one the default for Return), more buttons and an action sheet's options stacked in
their order with the cancel button last and no default; Escape presses the cancel button.

### Keyboard

| Keys | Do |
|---|---|
| Return | In an `Input`, what `submitBehavior` says: submit (`onSubmitEditing`), submit and leave the field, or a new line (multiline inputs) |
| Shift+Return, Option+Return, Ctrl+Return | A new line in a multiline input whose return submits |
| Tab, Shift+Tab (Ctrl too) | The next / previous field in screen order, within the newest modal; Option+Tab types a tab in a multiline input |
| Escape | Leaves the focused field; else clears a text selection, then goes back or asks the newest modal to close |
| Option+Left, Cmd+[, mouse back button | Go back (not while a field has the focus: there Option+Left moves the caret) |
| Option / Cmd + arrows, with Shift; Option+Delete, Cmd+Delete; Ctrl+A / E / K | Move, select and delete by word and line in a field, as in every Mac app |
| Cmd+Z, Shift+Cmd+Z, Cmd+X / C / V / A | Undo, redo, cut, copy, paste, select all (a secure field refuses cut and copy); Cmd+A / Cmd+C select and copy `selectable` text |
| Arrows; Page Up / Down, Space, Shift+Space, Option+Up / Down; Home / End, Cmd+Up / Down | Scroll the scroll view around the focused view or under the mouse by a line, a page, to the start / end (not while a field has the focus) |
| Cmd+W, Cmd+M, Ctrl+Cmd+F, Cmd+H, Option+Cmd+H, Cmd+Q | Close, minimize, full screen, hide, hide others, quit (`nsh package` and `nsh run` apps' menus) |
| Cmd+R | Reloads the latest build (`nsh run`) |

The app's own shortcuts ([`nsh/keymap`](../modules/nsh-keymap.md)) come before all of these, except Quit, Hide,
Minimize and Full Screen, which cannot be bound.

Shortcuts work with any keyboard layout (on a Cyrillic or Greek one, Cmd+C is the key that types C on a US layout).
A return that commits an input method's composition (Chinese, Japanese, ...) never submits. `onKeyPress` reports
what React Native reports: the typed text, `"Enter"`, `"Backspace"`, and `"Escape"`.

Not supported, or different:

- **Zooming scroll views** (`zoomTo`, the zoom props).
- **`mixBlendMode`**, and `brightness()` above 1.
- **Inputs**: content types and autofill, keyboard types, autocapitalization, letter spacing; input accessory views
  are never shown (there is no software keyboard) and `keyboardShouldPersistTaps` is ignored.
- **`WebView`'s `scrollEnabled={false}`** keeps the mouse wheel and the trackpad from scrolling the page: they scroll
  the app's scroll views around it, as on iOS. The page still scrolls with its keyboard (Space, the arrows) and its own
  scripts, and trackpad swipes between its pages (`allowsBackForwardNavigationGestures`) stop as well.
- **Accessibility**: custom `accessibilityActions`.
- **The status bar** and **haptics** (no desktop equivalent).

## See also

- [CLI](./cli.md): `nsh run --target macos`, `nsh package --target macos`
- [Packaging](./packaging.md)
- [Platform-specific code](./platform-specific.md)
