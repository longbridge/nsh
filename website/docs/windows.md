---
title: Windows
description: Run NSH apps on Windows in Win32 windows drawn with Direct2D, package them, embed them in a Win32 app with AppView, and what differs on Windows.
order: 14
group: platforms
---

# Windows

NSH apps also run on Windows. The Windows backend draws them in Win32 windows with Direct2D and lays out text with
DirectWrite; the core, the reactive graph, the renderer, Taffy layout and the `nsh/ui/mobile` components are the same
as on iOS and Android, so one bundle runs everywhere. On Windows, `Platform.OS` is `"windows"` and the
`desktop={{…}}` props apply. The window's title, a title bar of the app's own, the menu bar and context menus come from
[`nsh/desktop`](../modules/nsh-desktop.md).

## Requirements

- **Windows 10 1809** or later, or Windows 11.
- The **WebView2 Runtime** for web views. It comes with Windows 11 and with Microsoft Edge on Windows 10; without it a
  `WebView` shows a placeholder.
- **Media Foundation codecs** for the video formats your app plays (videos play through Windows' own media stack).
- To build NSH's Windows runtime (from a checkout, or `cargo xtask dist` on Windows): Visual Studio Build Tools with
  the C++ workload, the Windows SDK and LLVM (for QuickJS' bindings).

## Running an app

On Windows, `nsh run` opens the app in a window (1200×800 by default), with hot reload, logs in the terminal and the
error overlay as on the other targets:

```sh
nsh run my-app                 # --target windows is the default on Windows
```

In a checkout, `nsh run` builds the development host (`nsh-windows-host.exe`) first; the distribution made on Windows
ships it in `runtime\windows\`. Ctrl-C closes the window, and closing the window ends `nsh run`; the host never
outlives `nsh`, however it ends. The host also runs a built bundle on its own:

```sh
nsh build my-app --release -o build
dist\nsh\runtime\windows\nsh-windows-host --bundle build\bundle.nshc --size 390x844   # in a checkout: target\debug\nsh-windows-host
```

`--size` is the window's size in points (`<width>x<height>`).

## Packaging

```sh
nsh package my-app --target windows [--run]
```

Writes a directory and the same as an archive:

```text
out\<Name>\
  <Name>.exe             the executable (nsh's release runner), with the app's icon
  nsh\bundle.nshc         your app, compiled
  nsh\assets\…           the assets it imports
  nsh\app.json           the window title, the app id, the version and the appearance
out\<Name>.zip
```

The executable finds `nsh\` next to itself; copy the whole directory (or unpack the `.zip`) wherever the app should
live: nothing is installed. The icon from `nsh.json` is embedded in the executable (Explorer and the taskbar show it)
without any extra tools; packaging on another system keeps the default icon. The app id groups the app's windows in
the taskbar. `--run` launches the app after packaging it. The executable is not signed: sign it with your own
certificate (`signtool`) to distribute it.

## Embedding in a Win32 app

The `nsh-platform-windows` crate (from the NSH repository) gives a Rust Win32 app `AppView`: one NSH engine in a child
window of a window of yours. Each `AppView` is independent; several can run at once.

```rust
use std::rc::Rc;
use nsh_platform_windows::{
    windows::{self, Win32::{Foundation::HWND, System::Ole::OleInitialize, UI::WindowsAndMessaging::MoveWindow}},
    AppOptions, AppView, AssetBase, Bundle,
};

/// Called on the thread that runs your message loop, with your window.
fn embed(parent: HWND) -> windows::core::Result<Rc<AppView>> {
    // Once per thread: COM and OLE (the clipboard, image decoding).
    unsafe { OleInitialize(None)? };
    let view = AppView::new(
        Some(parent),
        AppOptions {
            asset_base: Some(AssetBase::Directory("app/assets".into())),
            app_id: "com.example.host".into(),
            ..AppOptions::default()
        },
    )?;
    // register native components here, before loading
    let code = std::fs::read_to_string("app/bundle.js").expect("bundle");
    view.load(Bundle::source(code).into());
    Ok(view)
}

// On WM_SIZE of `parent`:
// unsafe { MoveWindow(view.hwnd(), 0, 0, width, height, true) };
```

The view is a child window: your message loop runs it, and you place it with `MoveWindow` on its `hwnd()` when the
parent resizes. `AppView::load` replaces whatever ran; `stop` (or destroying the window) ends the app. `asset_base`
tells it where `asset:/` URIs resolve (`AssetBase::Directory` or `AssetBase::Http`). Bytecode bundles (`bundle.nshc`)
load with `unsafe { Bundle::bytecode(bytes) }` (or `Bundle::read_trusted(path)`, unsafe too): QuickJS bytecode is trusted input, so
only load it from your own app's files. A standalone window is `nsh_platform_windows::run_app(source, options)`.

Native components are registered with `AppView::register_component`; see
[Native components](./native-components.md#registering-on-windows-rust).

## What differs on Windows

The environment is a desktop's: the safe area and the keyboard height are 0, `Platform.constants.interfaceIdiom` is
`"desktop"` and `Platform.Version` is `"10.0"` (on Windows 11 too: Windows kept the version number). Dark mode
follows Settings > Personalization > Colors live, `fontScale()` the Text size setting, reduce motion the animation
effects setting and the locale the user's regional format. See [Device and environment](./device-and-environment.md).

### Keyboard

| Keys | Do |
|---|---|
| Return | In an `Input`, what `submitBehavior` says: submit (`onSubmitEditing`), submit and leave the field, or a new line (multiline inputs) |
| Shift+Return, Alt+Return | A new line in a multiline input whose return submits (Ctrl+Return submits) |
| Tab, Shift+Tab | The next / previous text field, within the newest modal; the field's text is selected, as in Windows' dialogs |
| Escape | Leaves the focused field; else clears a text selection, then goes back or asks the newest modal to close |
| Alt+Left, Browser Back key, mouse back button | Go back (also from a field) |
| Ctrl+arrows, Home / End, Ctrl+Home / End, with Shift; Ctrl+Backspace / Delete | Move, select and delete by word and line in a field, as in every Windows app |
| Ctrl+Z, Ctrl+Y, Ctrl+X / C / V / A; Shift+Delete, Ctrl+Insert, Shift+Insert | Undo, redo, cut, copy, paste, select all (a secure field refuses cut and copy); Ctrl+A / Ctrl+C select and copy `selectable` text |
| Arrows; Page Up / Down, Space, Shift+Space; Home / End, Ctrl+Home / End | Scroll the scroll view around the focused view or under the mouse by a line, a page, to the start / end (not while a field has the focus) |
| F11, Alt+F4 | Full screen, close the window (`nsh package` and `nsh run` apps) |
| Ctrl+R, F5 | Reload the latest build (`nsh run`) |

Shortcuts work with any keyboard layout (on a Cyrillic or Greek one, Ctrl+C is the key that types C on a US layout).
A return that commits an input method's composition (Chinese, Japanese, ...) never submits. `onKeyPress` reports
what React Native reports: the typed text, `"Enter"`, `"Backspace"`, and `"Escape"`; pasted text is not reported.

The app's own shortcuts ([`nsh/keymap`](../modules/nsh-keymap.md)) come before a focused field's keys, navigation
and scrolling; Alt+F4, F11 and Ctrl+Q cannot be bound.

Different, or not supported:

- **`WebView`**: a page inside a view with a 3D transform (perspective) is not shown, and a `backdropFilter` over a
  page does not blur it. `source.baseUrl` takes `http(s)` URLs. A native component that brings a window of its own
  stays above everything NSH draws.
- **Video**: the formats the installed codecs decode; no poster; the control bar always shows with `controls`.
- **Zooming scroll views** (`zoomTo`, the zoom props); the scroll indicator cannot be dragged.
- **`mixBlendMode` and `backdropFilter`** are drawn, but not inside a view that has a `filter` or a 3D transform;
  `borderCurve: "continuous"` draws circular corners.
- **Text**: head and middle ellipses on a single line only (more lines get a tail ellipsis); color emoji from COLR
  fonts only; small caps only where the font has them.
- **Text inputs**: keyboard types and autofill do not apply (there is no software keyboard); Tab moves between text
  fields. A right click opens the edit menu (Undo, Cut, Copy, Paste, Delete, Select All), as on selectable text (Copy,
  Select All).
- **Accessibility** (UI Automation): headers' titles and buttons, dialogs and toasts are not exposed; headings report
  level 1.
- **Input accessory views** and the **status bar** (no desktop equivalent).

## See also

- [CLI](./cli.md): `nsh run --target windows`, `nsh package --target windows`
- [Packaging](./packaging.md)
- [Platform-specific code](./platform-specific.md)
