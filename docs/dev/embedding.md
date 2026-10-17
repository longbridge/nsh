# Packaging and embedding

- `nsh package <entry> --target ios|ios-simulator|android|linux|macos|windows|gpui` builds an installable app from the prebuilt runtime: metadata from
  `nsh.json` next to the entry (`name`, `id`, `version`, `build`, `icon`; `appearance` also sets `UIUserInterfaceStyle`
  and a dark window theme, so the launch screen is in it) or flags. iOS: `.app`, `.ipa` with `--sign <identity>
  --profile <file.mobileprovision>` (entitlements come from the profile; icons are compiled by `actool` into `Assets.car`).
  Android: an APK assembled without Gradle (`aapt2 link`, native libraries stored uncompressed and 16 KB aligned, `apksigner`;
  the debug key unless `--keystore`). Linux: a directory `<name>/` with the release runner (`crates/nsh-linux-app`) as
  the executable, `nsh/bundle.nshc`, `nsh/assets/` and `nsh/app.json` (the window title and the GTK application id,
  which the runner reads next to itself), `<id>.desktop` (`StartupWMClass` is the id, which GTK makes the window's
  app id) and `<id>.png`, plus the same as `<name>.tar.gz`; GTK 4, Pango and WebKitGTK 6.0 (the runner's default
  `webview` feature, `linux.md`) must be installed where it runs. macOS: `<name>.app` with the
  release runner (`crates/nsh-macos-app`) as `Contents/MacOS/<executable>`, `Contents/Resources/nsh/` holding
  `bundle.nshc`, `assets/` and `app.json` (the window title and appearance), the icon as `AppIcon.icns` (`sips`), signed
  ad hoc (`codesign -s -`); `--run` opens it.
  Windows: a directory `<name>\` with the release runner (`crates/nsh-windows-app`, a windowed program: no console)
  as `<name>.exe` (characters Windows forbids in file names dropped), the icon embedded as its icon group 1 (which the
  window class loads, and Explorer and the taskbar show) with Windows' resource update API (`crates/nsh-cli/src/windows.rs`:
  16 to 64 pixels as DIBs plus a 256-pixel PNG; packaging off Windows keeps the runner's icon, with a warning), and
  `nsh\bundle.nshc`, `nsh\assets\` and `nsh\app.json` (the window title, the app id, which groups the app's windows in
  the taskbar, the version and the appearance, which the window opens in); plus the same as `<name>.zip`. No tools
  are needed; `--run` launches the executable.
  GPUI: packaged as the native apps of the machine it runs on are (Linux's directory and `.tar.gz`, macOS' `.app`,
  Windows' directory and `.zip`), with the GPUI release runner (`crates/gpui/nsh-gpui-app`) as the executable; it finds
  `nsh/` next to itself, or `Contents/Resources/nsh` inside an app bundle (`nsh_gpui_app::resources`). The Linux
  runner links WebKitGTK 4.1 (GTK 3) and GStreamer dynamically (features `webview`, `video`): they must be installed
  where the package runs, as GTK 4 must for Linux packages.
- Embedding: iOS apps link `NativeShell.xcframework` and `import NativeShell` (Swift: `NshAppView`, a `UIView`;
  `NshTrust`, `NshPackage`, `NshLimits`, `NshError`); Android apps add `nsh.aar` and use `com.longbridge.nsh.NshView`.
  `*Dev` / `-dev` variants can also load from a dev server. See `examples/embed-*`. Rust desktop apps use the platform
  crate itself: `nsh_platform_linux::AppView` (a GTK widget), `nsh_platform_macos::AppView` (an `NSView`) or
  `nsh_platform_windows::AppView` (a child window of the app's `HWND`, run by the app's message loop; `windows.md`).
  GPUI and GPUI Kit apps use `nsh_platform_gpui::AppView`, a GPUI view (`gpui.md`): `let view = cx.new(|cx|
  AppView::new(options, window, cx));` inside a window, `view.read(cx).load(App::Js(bundle))`, then put the entity in
  any element tree (in a GPUI Kit window, as a child of its `Root`); it fills the space its parent gives it and takes
  the focus it is given (`Focusable`). The crate depends on `gpui-fast` 0.1 from crates.io (re-exported as
  `nsh_platform_gpui::gpui`); GPUI Kit's `gpui-fast` feature makes `gpui_component` use that same crate, so both share
  one GPUI. `open_app_window` opens a window showing an app in a running GPUI app.
- **Native components** (`nsh/native`; `component.rs`, `ComponentBox.java`): the embedding app registers its own native
  views by name — Swift `NshAppView.register(component:) { context in … }` (`NshComponent`: `view`, `setProp`,
  `command`), Java `NshView.registerComponent(name, factory)` (`NshComponent`), Rust `AppView::register_component`
  (`NativeComponent`) — one registry per view, no globals. JS declares one with `nativeComponent<P, E>(name, { props,
  events })`; the element is a `<native>` leaf: `component` names it, each own prop goes as `$name` (any JSON value,
  `nsh_core::JsonValue`, sent when it changes; `undefined` removes it), view props (style, classes, accessibility, `ref`)
  stay the box's. The core sends `Create` (`ViewKind::Native`, a box decorated and placed like any view), then
  `MountItem::Component { name }` and `ComponentProps` (all props, then the changed ones). The component fills the box,
  emits events (`ComponentContext::emit` → `onNativeEvent` `{ name, data }` → the `on<Name>` handler) and may report
  its own size (`set_size`; the leaf measures to it where the style leaves a dimension open, like an image's natural
  size). `ref.command(name, args)` → `Command::Native`. Unknown names render an empty box (warned once). Props and events
  cross the C ABI / JNI as JSON text (Swift `JSONSerialization`, Java `org.json` as maps and lists). Registered
  components are the embedding app's own code: capabilities do not apply to them.
  On the GPUI backend (`nsh-platform-gpui`, `native.rs`) a component renders a GPUI element
  (`NativeComponent::render(window, cx) -> Option<AnyElement>`, any GPUI / GPUI Kit element or `Entity` view): the
  surface element's prepaint lays it out at the box's window bounds and prepaints it (`prepaint_as_root`, clipped to the
  clipping ancestors), the tree paints it where it draws the box (nsh content drawn later covers it), and a `BlockMouse`
  hitbox under it keeps nsh's input away; keys go to it while focus is inside it, and one that returns its widget's
  handle from `NativeComponent::focus_handle` is a Tab stop among the text fields (in tree order, in the newest modal's
  scope). Props and commands reach it with the next frame, before `render`, with the window and `App` (`set_prop(name,
  value, window, cx)`, `command(name, args, window, cx)`), so it can update its entities. `render` is also called while
  the box is empty, so a component can size itself. Content masks are
  rectangles: rounded corners do not cut a component (videos follow them).
  `crates/gpui/nsh-platform-gpui/examples/embed.rs` runs `examples/embedded` with its badge as a GPUI element.
- **The iOS SDK** (`crates/xtask/src/ios.rs`): per platform slice (device, simulator, Catalyst) a static
  `NativeShell.framework` whose binary is `nsh-platform-ios` with feature `embed` (built by `cargo rustc --crate-type
  staticlib`, so the hosts never build or export it) merged by `libtool` with the Swift module, compiled with library
  evolution (`.swiftinterface`: apps on other Swift compilers can import it). The Swift module imports the C ABI's
  module `NativeShellC` with `internal import`, so it is neither in the interface nor shipped; its module map's `link`
  lines become autolink entries of the Swift objects (apps link nothing but the framework). Catalyst frameworks use the
  versioned macOS layout and Swift needs the SDK's `iOSSupport` paths. The C functions return errors as strings
  (`nsh_string_free`) that Swift throws as `NshError`; the error handler's context is released by a callback.

## GPUI and GPUI Kit apps

Desktop apps built with GPUI or GPUI Kit (Longbridge Pro, Allsum Desktop) embed nsh with `nsh-platform-gpui`'s
`AppView`, a GPUI view; `examples/embed-gpui` is a GPUI Kit window with a sidebar, buttons, an input and two surfaces
in a resizable split.

- **One GPUI.** nsh draws with GPUI Fast (`gpui-fast` from crates.io); GPUI Kit (`gpui-kit` / `gpui-component`) must
  use it too: its `gpui-fast` feature. `cargo tree -i gpui-fast` must show one version, else the app has two GPUIs
  and nsh's views are not the app's elements. `nsh_platform_gpui::gpui` re-exports nsh's.
- **A view.** `cx.new(|cx| AppView::new(options, window, cx))`, then `view.read(cx).load(App::Js(bundle))` (or a
  Rust app's root); the app puts the entity wherever it wants as a child element. The surface fills the space its
  parent gives it and follows it when resized (each layout reports the size to the engine). Several views run at once,
  in one window or several, each with its own engine thread; the process shares only the main-thread queue (`rt.rs`)
  and the key bindings. Dropping the view (its last entity handle) stops its engine and joins its thread, as does
  `AppView::stop`.
- **Focus.** `AppView` is `Focusable` and a tab stop (a `div` with `track_focus` around the surface element, which
  registers the stop). A press focuses it; keys reach it only while it is focused, so several surfaces and the host's
  inputs never share keys. Its key context `NshAppView` binds Tab and Shift + Tab deeper than GPUI Kit's `Root` (which
  binds them to move the window's focus): the surface's text fields come first, and past the last one (the first
  with Shift) the field is blurred and `window.focus_next()` / `focus_prev()` move on to the host's widgets. Tab from
  the host's widgets onto the view goes on to its first field (its last with Shift: a keystroke observer, since the
  host handles that Tab); in a window with no other tab stop (`run_app`) the fields wrap around, as on the other
  backends.
- **Frame rate.** Open the host window with `inactive_frame_interval: None` in its `WindowOptions`: GPUI draws
  windows without keyboard focus at 30 fps by default, which halves the surfaces' animations whenever the host window
  is not focused (nsh's own windows set it, `open_app_window`).
- **Overlays.** The surface draws only inside its bounds and takes the mouse only on its hitbox: GPUI Kit's dialogs,
  sheets, popovers and notifications (drawn by `Root` above the content) cover it and take the input, while its app
  keeps running.
- **The host's theme in nsh.** `AppView::set_theme(Some(theme), cx)` gives the surface a `nsh_core::ui::Theme`
  (re-exported as `nsh_platform_gpui::Theme`) and `set_theme_json(json, cx)` one in gpui-kit's JSON: the components of
  the UI sets render with it instead of the app's own theme (`Core::set_host_theme`, which wins until cleared with
  `None`), and the surface is drawn on its `background`. It applies at once and to every app loaded later, in their
  first frame (the surface starts engines with `Engine::start_with`, sending the theme before the start).
  `set_appearance(Appearance::Light | Dark, cx)` is the host's mode, standing in for the system's (`System` follows
  the window again); an app that sets its own appearance still wins, as over the system's. The example's
  `src/theme.rs` converts GPUI Kit's active `Theme`: both modes from its `light_theme` / `dark_theme` configs (gpui-kit
  theme JSON, unset colors null, which nsh skips), then the current mode's colors as GPUI Kit resolved them, so nsh's
  components match GPUI Kit's widgets; it runs on every change (`cx.observe_global::<Theme>`).
- **nsh's theme in the host.** An nsh theme file is a gpui-kit theme file (same keys, same `themes` / `mode` /
  `colors` structure): `ThemeRegistry::global_mut(cx).load_themes_from_str(json)` registers its entries by `name`
  (give each one), and setting `Theme::global_mut(cx).light_theme` / `dark_theme` to them and calling
  `Theme::change(mode, …)` applies them to GPUI Kit (the example's violet theme, `themes/violet.json`). Share hex
  colors and Tailwind palette names only: nsh also reads CSS colors (`rgb()`, names) GPUI Kit does not, and GPUI Kit
  reads gradients and `blue-500/50` that nsh rejects.
- **From nsh to the host.** The app talks to the embedding app through the native components the latter registers
  (`AppView::register_component`): their events and commands, as on the other platforms. A component can render GPUI
  Kit widgets in its box (`NativeComponent::render`), which take their own input: the example's "Extended hours" is a
  GPUI Kit `Switch` (`src/switch.rs`, `host.switch`) that reports its natural size (`ComponentContext::set_size`: its
  track and label, so the app's style leaves the box's size open), emits `change` to JS, and toggles on clicks and on
  Space or Enter while focused (nsh's fields do not see those keys). It returns a focus handle of its own
  (`NativeComponent::focus_handle`; a wrapper's, since the `Switch` keeps its handle internal), so it is a Tab stop
  among the app's text fields, and a click focuses that handle too. `AppView::on_error` hears
  the app's fatal errors (also logged).
