# JS modules

Two layers. `nsh` and `nsh/<module>` are the primitives and the environment that every UI set builds on. `nsh/ui/<set>`
is a whole UI for one form factor: `nsh/ui/mobile` has everything that is mobile UI or interaction (navigation stacks,
modals, the status bar, system dialogs, and its components, one look on every platform, written in Rust in
`crates/nsh-ui-mobile`); a desktop set is planned as a separate one (`nsh/ui/desktop`), not an abstraction over both,
and does not exist yet.
Shared vs one form factor decides the layer, not JS vs native.

Every module is native: `nsh-js` serves it (`modules.rs`: NSH's own modules by `nsh_core::js::NATIVE_MODULES`, then the
registries: the runtime's `nsh_core::runtime::registries()` and the UI sets a host serves), and the bundler keeps
imports of them as imports (`plugin.rs`; how apps are compiled: `native-pipeline.md`). A module can be both: the
hand-written exports of `nsh/android` and `nsh/accessibility` are joined by their registry's. There is no JS file of
NSH's and no internal module apps could reach.

Declarations: each export of a module of NSH's own carries its TypeScript declaration in its table (`modules::Item`:
`signal`'s signatures next to `reactive::signal`), each module its own interfaces (`types`); a registry's components
and modules are declared from their specs (`nsh_core::ui::declarations`, `docs/dev/components.md`), the styles, the
host elements' props and events, the easings and the canvas methods from the core's tables. `nsh-js`'s
`declarations.rs` assembles one file per module (`nsh/list.d.ts`, `nsh/ui/mobile.d.ts`), `nsh/types.d.ts` (the
element and JSX types `nsh` exports), `nsh/jsx-runtime.d.ts` and `nsh/globals.d.ts`, and tests check every module's
exports against its declarations; the CLI writes them into `.nsh/types` (`tooling.md`).
`nsh` itself only has the reactive API, `mount`, the primitives every UI is made of (`View`, `Text`, `Scroll`,
`Input`) and the types; there is no catch-all module, and moved APIs get no compatibility re-exports. Each primitive
has one home, which is where apps import its tag from (`native-pipeline.md`): the table below. `If` and `Each` are
globals.

| Module | Exports | Implemented in (`crates/nsh-js/src`) |
|---|---|---|
| `nsh` | `signal`, `computed`, `effect`, `batch`, `untracked`, `cleanup`, `context`, `split`, `mount`; `View`, `Text`, `Scroll`, `Input` (host elements as components: `dom::host_component`); `YNode`, `Accessor`, `Signal`, `Context`, `Component`, the types of styles, element props, events and `JSX` | `reactive.rs`, `dom.rs` |
| `nsh:rt` | what compiled JSX calls (`element`, `insert`, `spread`, `cond`, `each`, `list`, `font`, ...); apps never import it | `dom.rs`, `list.rs` |
| (globals) | `If`, `Each` (control flow; their tags compile to `cond` / `each`) | `dom.rs` |
| `nsh/animated` | `createNativeValue`, `NativeValue`, `NativeNode`, `Animated`, `Easing`, `LayoutAnimation` | `animated.rs` |
| `nsh/device` | `windowSize()`, `colorScheme()`, `fontScale()`, `Platform`, `PixelRatio`, `I18nManager` | `env.rs` |
| `nsh/accessibility` | `AccessibilityInfo`, `screenReaderEnabled()`, `reduceMotion()` | `env.rs` + its registry (`nsh_core::runtime`) |
| `nsh/list` | `List` | `list.rs` |
| `nsh/canvas` | `Canvas`, `Path2D`, `CanvasRenderingContext2D`, `CanvasGradient` | `canvas.rs` |
| `nsh/image` | `Image`: the image element's component, with `Image.getSize`, `Image.prefetch` (promises the core resolves from the root's `ImageInfo` event) | `modules.rs` + its registry (`components.rs`) |
| `nsh/gesture` | `PanResponder` | `touch.rs` |
| `nsh/clipboard` | `Clipboard.setString(text)`, `Clipboard.getString()` / `hasString()` (promises the core resolves from the root's `Clipboard` event; `hasString` does not read the text, so iOS shows no paste notice); UIPasteboard, ClipboardManager, `navigator.clipboard`; the `clipboard` capability | its registry (`nsh_core::runtime`) |
| `nsh/keymap` | `keymap.bind(keystroke, handler, { when, repeat })` / `keymap.bind({ keystroke: handler })`: keyboard shortcuts. Keystrokes are parsed and checked in Rust (`nsh_core::keymap`: syntax, `mod`, reserved keystrokes, which ones text inputs keep); the whole table goes to the platform as the root's `Prop::Keymap` → `MountItem::Keymap`, which matches keys on the spot and reports the root's `KeyCommand` event (the binding's id). Bindings made under an owner go with it; a module's top-level ones are undone when hot module replacement runs it again (`hot::undo_on_rerun`) | `keymap.rs` |
| `nsh/webview`, `nsh/video` | `WebView`, `Video` | `platform_modules.rs` |
| `nsh/native` | `nativeComponent(name, { props, events })`: a component for a native component the embedding app registered | `platform_modules.rs` |
| `nsh/ios` | `isAvailable`, `InputAccessoryView` | `platform_modules.rs` |
| `nsh/android` | `isAvailable`, `Toast.show(message, "short" \| "long")`, `BackHandler` (`addEventListener` native, `exitApp` Rust), `setStatusBarBackgroundColor` (the status bar's base layer) | `platform_modules.rs` + its registry |
| `nsh/desktop` | `isAvailable`, `titleBar()` (the environment's `title_bar`), `Menu.setApplicationMenu` / `Menu.popup` (decoded natively into `nsh_core::desktop::MenuItem`s, the items' functions kept by id, called on the root's `onMenu`), `Window` (Rust: `setTitle`, `setTitleBar`, `minimize`, `toggleMaximize`, `toggleFullScreen`, `close`); all of it is `Command::Desktop` on the root, which mobile platforms ignore | `desktop.rs` + its registry |
| `nsh/ui/mobile` | `Stack`, `Screen`, `createNavigator`, `Modal`; the Rust components and modules of `nsh-ui-mobile`: `SafeAreaView`, `KeyboardAvoidingView`, `StatusBar` (and `StatusBar.setBarStyle` / `setHidden`), `Alert.alert` and `ActionSheet.show` (promises of the index chosen; a list dialog on Android), `Keyboard.dismiss`, `Button`, `Card`, `Group`, `ListView`, `Stepper`, `TextField`, `Sheet`, `SelectList`; re-exported from the runtime: `safeArea()`, `keyboard()`, `useSafeAreaInsets()` | `navigation.rs`; the registry (`components.rs`), `env.rs` |

`nsh:hot` (hot module replacement, `hot.rs`) is only imported by development bundles. The same loader also compiles
the chunks of a split bundle (`Screen.js`: a name that is no native module, with code the host gave; `chunks.rs`,
`native-pipeline.md`), which `import()` loads through the global `__nsh_import`.

- **Platform modules** are importable on both platforms (one bundle runs on both); on the other one their calls do
  nothing and their components render nothing. `isAvailable` tells which.
- **Platform-only props** are grouped: `ios={{…}}` / `android={{…}}` / `web={{…}}` / `desktop={{…}}` (macOS, Windows, Linux, and browsers in the desktop idiom) on
  elements and components (`IOSProps` / `AndroidProps` / `DesktopProps` in `types.d.ts`, extended per element; the host
  props in them are the generated `IOS…HostProps` / `Android…HostProps`). The renderer
  (`platform_props` in `dom.rs`) sends only the current platform's object, each key as a prop of its own (keys removed
  from the object are cleared); the core knows them by those names. Android: `ripple`, `elevation`,
  `accessibilityLiveRegion`, Modal `statusBarTranslucent`, StatusBar `backgroundColor` (a Rust component's `android`
  prop). Alert's `cancelable` is an option of both platforms (iOS alerts are never dismissed by a tap outside). iOS:
  input `inputAccessoryViewID` / `textContentType` / `keyboardAppearance`, Screen `headerLargeTitle` / `headerBackTitle`, Modal
  `presentationStyle` / `supportedOrientations`, WebView `allowsBackForwardNavigationGestures`. Desktop: Screen
  `headerLargeTitle` / `headerBackTitle`, Modal `presentationStyle`, WebView `allowsBackForwardNavigationGestures`
  (macOS' trackpad swipes, Windows' touchscreen swipes). Web: Modal `presentationStyle` (a prop of several platforms names each:
  `#[prop(ios = "x", desktop = "x")]`).
- A module's exports are made when a bundle first imports it; what must always happen (the root's direction, the
  themes) is the renderer's (`dom.rs`, `setup`), not a module's.

## Where new things go

- A primitive or environment both UI sets need: a module of NSH's own (`nsh/<module>`), natives in `crates/nsh-js`
  listed in `modules.rs` and `nsh_core::js::NATIVE_MODULES`, each export with its declaration (`Ts`) and the
  module's interfaces in its `types`. Functions that only ask the platform (a command, a request answered by an event)
  are a Rust `#[module]` in `crates/nsh-core/src/runtime/`, registered in `runtime::registries()` under the module's
  specifier: their JS face and declarations come from the spec, with nothing written per function.
- Mobile UI or interaction: `nsh/ui/mobile`; its components are Rust, in `crates/nsh-ui-mobile` (see its README).
- Something only one platform has: `nsh/ios` / `nsh/android`, or a prop in `ios={{…}}` / `android={{…}}` /
  `desktop={{…}}`.
- Never a compatibility re-export when something moves, never a React Native compatibility wrapper.
