# The OHOS backend (ArkUI)

`crates/nsh-platform-ohos` renders NSH apps on HarmonyOS / OpenHarmony (API 20+, `arm64-v8a`) with ArkUI custom nodes
and lays text out with native `OH_Drawing_Typography`. Each `NshSurface` owns one `NodeContent`, one engine thread and
one QuickJS VM. User-facing page: [website/docs/ohos.md](../../website/docs/ohos.md).

## Building and checking

Building requires the OpenHarmony NDK (discovered from `OHOS_NDK_HOME`, `DEVECO_SDK_HOME` or DevEco Studio's default
location on macOS, Windows and Linux) and the `aarch64-unknown-linux-ohos` Rust target. Off `target_env = "ohos"` the
crate is a host stub, so check the real backend with `xtask`:

```sh
cargo xtask ohos                         # target/ohos/debug
cargo xtask ohos --release               # target/ohos/release
cargo xtask ohos --dev                   # target/ohos/debug-dev
cargo xtask ohos --features api-23       # target/ohos/debug-api-23
cargo xtask ohos --features api-26       # target/ohos/debug-api-26
cargo xtask clippy-ohos                  # clippy across default, dev, api-23 and api-26
```

`cargo xtask ohos` builds `libnsh_platform_ohos.so`, generates `index.d.ts`, and stages the companion `@nsh/ohos`
ArkTS package under `target/ohos/arkts-deps/nsh/`.

Features:
- Default / `dev`: API 20 baseline (`OH_ArkUI_*` custom nodes, `OH_Drawing_Typography`, `NativeVSync`, `AVPlayer`,
  system `TextInput` / `TextArea`, and `ArkWeb`).
- `api-23`: native accessibility providers, state descriptions and reduced-motion observation.
- `api-26`: includes `api-23` plus named custom accessibility actions.

## Architecture

- **One `NshSurface` per embedded view** (`src/app/`): the host creates `new NshSurface(this.getUIContext())` and places
  `surface.content` (`NodeContent`) inside a standard ArkUI `ContentSlot`. `@nsh/ohos` binds each window session
  explicitly from the provided `UIContext` (`UIAbilityContext` and `Window`) in a standard `UIAbility` without a custom
  base class or process-global context guessing.
- **Commits and frames** (`src/render/`): the engine submits each `MountBatch` through a N-API threadsafe function to
  the ArkUI UI thread. Per-surface `NativeVSync` callbacks drive animations on the UI thread; idle surfaces request no
  frames.
- **Shaders** (`src/render/shader/`): wgpu runs on GLES only, which waits for the GPU on the calling thread, so each
  shader view renders into a native surface of its own (an XComponent first among its children) from a thread of the
  surface's (`nsh-shader`), paced by the surface's `NativeVSync`, skipping views out of sight; nothing is read back
  (`docs/dev/shaders.md`).
- **Typography** (`src/text/`): the engine thread shapes and measures paragraphs with `ohos-drawing-binding`
  (`FontCollection::shared()`, `Typography`) and hands owned RGBA bitmaps and selection geometry to the UI thread.
- **Controls and media** (`src/input/`, `src/scroll/`, `src/media/`, `src/interop/`): `Input` uses the system
  `TextInput` / `TextArea` BuilderNode; `Scroll` and `List` use native ArkUI `Scroll` (with on-demand `Refresh`);
  `WebView` uses `ArkWeb`; `Video` uses a C `XComponent` surface and an `AVPlayer` worker.
- **Navigation** (`src/navigation/`): OHOS is a native-header platform, as iOS and Android: each `Screen` draws its own
  bar (back button, title) and places only the `headerLeft` / `headerTitle` / `headerRight` items in it; a screen's
  `header` slot (the bar on platforms without a native header) is never shown, as UIKit's and the Android toolbar's
  bars never place it. Screens and modals report `onInsets` as the other backends do: a screen's top inset is 0 below
  a visible, opaque header, else the safe area's top.
- **Hit areas**: `hitSlop` and overflowing children widen `NODE_RESPONSE_REGION`, whose width and height are
  percentages of the node's size (`100` is the node itself), not ratios.
