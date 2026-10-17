---
title: Getting started
description: "Write a module with nsh init, run it with hot reload on iOS, Android, macOS, Windows or Linux, host it in your app, and ship updates as signed packages; or package it as a standalone app."
order: 4
group: start
demo: guide-getting-started
---

# Getting started

NSH's main use is hosting modules inside a native app (see [Introduction](./index.md)). This guide follows that path: write a module, develop it with hot reload on the platforms you target, show it in your app, and ship an update as a signed package. It assumes `nsh` is installed and on your `PATH` ([Installation](./installation.md)); in a checkout, use `./target/debug/nsh` instead.

> [!TIP]
> No app project at hand? The first two sections need none: `nsh run` shows the module in NSH's own development host: in a window on your computer, in the iOS simulator or on an Android device. You can stop there, or [package it as a standalone app](#or-ship-it-as-a-standalone-app).

## Create a module

```sh
nsh init my-app
```

A module is what a host app loads: one or more `.tsx` files with an entry. `nsh init` creates the directory if it is missing and writes:

| File | What it is |
|---|---|
| `App.tsx` | The entry module: renders the app with `mount` |
| `nsh.json` | The app's name, id, version and build number ([Project structure](./project-structure.md#nshjson)) |
| `tsconfig.json` | TypeScript settings that resolve `nsh` to the declarations in `.nsh/types` |
| `.gitignore` | Ignores `.nsh/` and `out/` |
| `.nsh/types/` | Generated declarations of every NSH module, for your editor |

The display name defaults to the directory's name; pass `--name "My App"` to choose another. The id becomes `com.example.<name>`: change it in `nsh.json` before you ship. Files that already exist are kept, so you can run `nsh init` in a directory that has some of them.

### The entry module

```tsx
import { mount, signal, Text, View } from "nsh";
import { SafeAreaView } from "nsh/ui/mobile";

function App() {
  const count = signal(0);
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <Text style={{ fontSize: 28, fontWeight: "600" }}>{count()}</Text>
      <View
        onPress={() => count.update((c) => c + 1)}
        activeOpacity={0.6}
        accessibilityRole="button"
        style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 10, backgroundColor: "#0969da" }}
      >
        <Text style={{ color: "white", fontSize: 17 }}>Tap</Text>
      </View>
    </SafeAreaView>
  );
}

mount(App);
```

Line by line:

- **`import { mount, signal, Text, View } from "nsh"`**: `nsh` is the core module: [signals](./reactivity.md), `mount`, the other reactive primitives, and the host elements `View` and `Text`. It is served by the native runtime, not bundled.
- **`import { SafeAreaView } from "nsh/ui/mobile"`**: the mobile UI set. [`SafeAreaView`](../components/safe-area-view.md) pads its content so it stays clear of the notch, the status bar and the home indicator; on desktops, where the safe area is 0, it adds no padding.
- **`function App()`** is a component: a function that returns JSX. It runs **once**. `signal(0)` makes a value that changes over time.
- **`{count()}`** reads the signal inside JSX. The compiler sees a call and makes this text follow the signal: when `count` changes, the runtime updates this one text, nothing else.
- **`<View onPress>`** is a native view that reacts to presses; `activeOpacity` fades it while pressed, natively. `View`, `Text`, `Image`, `Scroll`, `Input` and the other [host elements](../elements/index.md) are imported from NSH's modules, like any component; only `If` and `Each` need no import.
- **`style={{ … }}`** takes an object of camelCase style properties, with numbers in points. Layout is flexbox, and children stack in a column by default ([Layout](./layout.md)).
- **`mount(App)`** renders the component into the app's surface.

> [!IMPORTANT]
> Text must be inside `<Text>`. A bare string in a `<View>` (`<View>Hello</View>`) is not displayed.

## Develop it

### Run it

`nsh run` starts a dev server, builds the app, launches a development host that loads it, and keeps rebuilding while you edit. Logs from the app (`console.log`, errors) appear in the terminal.

Without `--target`, `nsh run` uses the machine you are on: a Win32 window on Windows, a GTK window on Linux, and on macOS a Mac Catalyst window that shows the app as an iPhone would. Pick another platform with `--target`:

| Target | Command | Runs from |
|---|---|---|
| iOS simulator | `nsh run my-app/App.tsx --target ios` | macOS with Xcode |
| Android | `nsh run my-app/App.tsx --target android` | macOS, Windows, Linux |
| macOS (AppKit) | `nsh run my-app/App.tsx --target macos` | macOS |
| Windows | `nsh run my-app/App.tsx` | Windows (the default) |
| Linux (GTK 4) | `nsh run my-app/App.tsx` | Linux (the default) |
| Mac Catalyst | `nsh run my-app/App.tsx` | macOS (the default) |

#### iOS simulator

```sh
nsh run my-app/App.tsx --target ios
nsh run my-app/App.tsx --target ios --device "iPhone 16 Pro"
```

`nsh` uses a booted simulator, or boots the newest iPhone if none is running, and installs the development host into it. `--device` picks a simulator by name or UDID.

#### Android

```sh
nsh run my-app/App.tsx --target android
```

Start an emulator or connect a device first (`adb devices` lists them; NSH uses the first one). `nsh` installs a development host APK (rebuilt only when the runtime changes) and forwards the dev server's port to the device with `adb reverse`, so it works over USB without a shared network.

#### macOS (AppKit)

```sh
nsh run my-app/App.tsx --target macos
```

The app opens as a desktop Mac app (AppKit, in a resizable window), with mouse, trackpad and keyboard input instead of
touches. It needs macOS 14 or later; see [macOS](./macos.md).

#### Windows (default on Windows)

```sh
nsh run my-app/App.tsx
```

The app opens in a desktop window (1200×800 points), with mouse, touchpad, touch and keyboard input. It needs Windows
10 1809 or later; see [Windows](./windows.md).

#### Linux (default on Linux)

```sh
nsh run my-app/App.tsx
```

The app opens in a GTK window (390×844 points). It needs GTK 4.14 or later; see [Linux](./linux.md).

#### Mac Catalyst (default on macOS)

```sh
nsh run my-app/App.tsx
```

The app opens in an iPhone-sized window (390×844 points) on your Mac. This is the quickest way to see a module as a phone shows it: no simulator to boot. It uses UIKit like iOS, so what you see is what an iPhone shows, apart from Mac-specific behavior such as scrolling with a mouse wheel. For the Mac app itself, use `--target macos`.

### Edit with hot reload

Leave `nsh run` running and change the label in `App.tsx`:

```tsx
<Text style={{ color: "white", fontSize: 17 }}>Add one</Text>
```

Save. The CLI rebuilds and sends a patch; the host runs the changed module again and renders again only the components whose code changed. Here that is `App`, which owns the `count` signal, so the count starts over. State survives when you edit a component that does not own it. Move the button into a component of its own:

```tsx
function TapButton(props: { onPress: () => void }) {
  return (
    <View onPress={props.onPress} activeOpacity={0.6} style={{ padding: 12, borderRadius: 10, backgroundColor: "#0969da" }}>
      <Text style={{ color: "white", fontSize: 17 }}>Tap</Text>
    </View>
  );
}
```

Use `<TapButton onPress={() => count.update((c) => c + 1)} />` in `App`, tap a few times, then change `TapButton`'s color: the button updates and the count stays.

Hot replacement works per top-level component: a function with a capitalized name whose body contains JSX. A component renders again when its own code changes, or when the module's code outside components changes (a constant it uses, a helper). If an edit cannot be applied in place (for example, it imports an NSH module the running app did not import yet, or changes `nsh.json`'s capabilities), the host reloads the whole app.

#### Errors

A build error is printed in the terminal and shown on the device as a red overlay with the file, line and column; the app keeps running the last good build. The overlay disappears with the next successful build (tap it to dismiss it earlier). An exception thrown at run time is shown the same way, with its stack mapped back to your `.tsx` source.

## Host it in your app

Your app shows a module in a view of its own: `NshAppView` (a `UIView`) on iOS, `NshView` (an Android `View`) on Android, `AppView` on macOS, Windows and Linux. Place it like any other view, on any screen, as many times as you need. During development, point it at the dev server; in a release, load a built bundle or a downloaded package.

### iOS

Link `NativeShellDev.xcframework` in Debug and `NativeShell.xcframework` in Release ([Embedding in iOS apps](./embedding-ios.md)), then:

```swift
import NativeShell

let app = NshAppView()
app.onError = { message in print("nsh error: \(message)") }
view.addSubview(app)   // and constrain it like any view

#if DEBUG
try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)   // `nsh dev my-app` on your Mac
#else
try app.loadBundle(at: Bundle.main.url(forResource: "bundle", withExtension: "qbc", subdirectory: "nsh")!)
#endif
```

`nsh dev my-app/App.tsx` serves the module to the app and pushes every save, as `nsh run` does with its own host. For the release build, `nsh build my-app/App.tsx --release --out MyApp/nsh` writes the bundle into a folder you add to the app's resources.

### Android

Add `nsh-dev.aar` for debug builds and `nsh.aar` for release builds ([Embedding in Android apps](./embedding-android.md)), then:

```kotlin
val nsh = NshView(this)
nsh.setErrorListener { message -> Log.e("nsh", message) }
setContentView(nsh)   // or add it to any layout

if (BuildConfig.DEBUG) {
    nsh.connectDevServer("ws://127.0.0.1:8790/ws")   // with `adb reverse tcp:8790 tcp:8790`
} else {
    nsh.loadAssetBundle("nsh")   // assets/nsh, from `nsh build my-app/App.tsx --release --out app/src/main/assets/nsh`
}
```

### macOS, Windows and Linux

Desktop apps written in Rust embed modules with `AppView`, from the `nsh-platform-macos`, `nsh-platform-windows`, `nsh-platform-linux` and `nsh-platform-gpui` crates: an `NSView`, a child window, a GTK widget or a GPUI view that loads a built bundle. See [macOS](./macos.md#embedding-in-a-mac-app), [Windows](./windows.md#embedding-in-a-win32-app), [Linux](./linux.md#embedding-in-a-gtk-app) and [GPUI](./gpui.md), and [Platforms](./platforms.md#embed-in-a-native-app) for every host side by side.

### What the module may do

On iOS and Android, the host decides before loading what the module may do: `grantCapabilities` sets the network hosts and other capabilities it may reach, and limits cap its memory, call time and node count ([Sandbox and capabilities](./sandbox-and-capabilities.md)). To give modules your own native views (a map, a chart, a design-system component), register them as [native components](./native-components.md).

## Ship updates as signed packages

A module the app downloads after release comes as a signed package. Create a root key once and keep it offline, certify a release key with it, and publish:

```sh
nsh keygen keys/root && nsh keygen keys/release
nsh certify --root keys/root.secret --key keys/release.pub --apps 'com.example.*' -o keys/release.cert
nsh publish my-app/App.tsx --key keys/release.secret --cert keys/release.cert --build 2
```

The app trusts the root's public key and loads what it downloads; NSH verifies the signature, the certificate and the revocation list before running anything:

```swift
let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
let trust = try NshTrust(stateDirectory: support.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)   // the contents of keys/root.pub
let info = try await app.loadPackage(at: downloadedFile, trust: trust)
```

Android has the same flow with `NshTrust` and `NshPackage`. Hosting, delivery and revocation in detail: [Signed packages](./signed-packages.md).

## Or ship it as a standalone app

The same module can also be a whole app. `nsh package` builds an installable app around it from the prebuilt runtime, with no Xcode project or Gradle build of your own:

```sh
nsh package my-app --target ios-simulator --run
nsh package my-app --target android
nsh package my-app --target macos
nsh package my-app --target linux
nsh package my-app --target windows
```

This is a supported use on every platform, and the quickest way to put a module on a device or a desktop, but NSH is designed around hosting modules in native apps. Options, signing and icons: [Packaging](./packaging.md).

## Try it in the terminal

`nsh trace` runs an app headless (no device) and prints the native view operations it produces and their timings, then taps the first pressable view twice:

```sh
nsh trace my-app/App.tsx
```

It is a quick way to check that an app starts without errors, and to see how much work an interaction causes ([Testing and trace](./testing-and-trace.md)).

## Next steps

- [Platforms](./platforms.md): what each platform supports, and its own page ([iOS](./embedding-ios.md), [Android](./embedding-android.md), [macOS](./macos.md), [Windows](./windows.md), [Linux](./linux.md)).
- [Signed packages](./signed-packages.md) and [Sandbox and capabilities](./sandbox-and-capabilities.md): delivery and what modules may do.
- [JSX and components](./jsx-and-components.md) and [Reactivity](./reactivity.md): writing modules.
- [Project structure](./project-structure.md): what goes in `nsh.json`, assets, files.
