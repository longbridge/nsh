---
title: iOS
description: Run NSH apps on iOS with UIKit views, in the simulator or on devices, package them as .app and .ipa, and embed them in an existing iOS app with NativeShell.xcframework and NshAppView.
order: 11
group: platforms
---

# iOS

NSH apps run on iOS with UIKit views and CoreText text, on devices and in the simulator, from iOS 15.0. The main use is
embedding: an existing app shows modules in an `NshAppView` and loads signed packages it downloads, under limits and
capabilities it sets. Running and packaging whole apps work too, as on the other platforms. On iOS,
`Platform.OS` is `"ios"` and the `ios={{…}}` props apply.

## Requirements

- **Xcode** on a Mac, with an iOS simulator runtime (`xcodebuild -downloadPlatform iOS` installs one). See
  [Installation](./installation.md#ios-and-mac-catalyst).
- Apps need **iOS 15.0** or later, on iPhone and iPad.

## Running an app

```sh
nsh run my-app --target ios
nsh run my-app --target ios --device "iPhone 16 Pro"
```

`nsh` uses a booted simulator, or boots the newest iPhone, installs the development host into it and keeps the app up
to date as you save, with logs in the terminal. `--device` picks a simulator by name or UDID. For a quicker loop
without a simulator, plain `nsh run` on a Mac opens the same UIKit backend in an iPhone-sized
[Mac Catalyst](./getting-started.md#mac-catalyst-default-on-macos) window.

## Packaging

```sh
nsh package my-app --target ios-simulator --run      # out/<Name>.app, installed and launched in the simulator
nsh package my-app --target ios --sign "Apple Distribution: Example Inc (ABCDE12345)" --profile AppStore.mobileprovision
```

`--target ios` writes a signed `.ipa` for devices, TestFlight or the App Store. Signing identities, provisioning
profiles and icons: [Packaging](./packaging.md#ios-devices).

## Embedding in an iOS app

An existing iOS app can show NSH apps inside its own UI. The runtime ships as `NativeShell.xcframework`, a static
framework with a Swift module: you create an `NshAppView` (a `UIView`), add it anywhere in your view hierarchy and load
an app into it. Your native screens, navigation and the NSH app live side by side in one UIKit hierarchy.

Each `NshAppView` is one independent engine with its own JS VM; there are no global singletons, so an app can show
several NSH apps at once, for example one per tab.

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/host-app-light.svg" alt="One host app with two NshAppView regions: each is its own engine with its own thread, VM, limits and capabilities; the host grants capabilities, sets limits and registers native components such as a map.">
<img class="diagram diagram-dark" src="/diagrams/host-app-dark.svg" alt="One host app with two NshAppView regions: each is its own engine with its own thread, VM, limits and capabilities; the host grants capabilities, sets limits and registers native components such as a map.">
</div>

## Get the framework

The distribution contains both variants:

```text
dist/nsh/embed/ios/NativeShell.xcframework      release: bundles and signed packages
dist/nsh/embed/ios/NativeShellDev.xcframework   also connects to a `nsh dev` server
```

From a checkout of NSH you can build just the frameworks:

```sh
cargo xtask ios-embed    # writes dist/nsh/embed/ios/{NativeShell,NativeShellDev}.xcframework
```

Each XCFramework has three arm64 slices: iOS devices, the iOS simulator (Apple silicon) and Mac Catalyst. The minimum
deployment target is iOS 15.0. The Swift module is compiled with library evolution, so apps built with other Swift
compiler versions can import it, and the framework links everything it needs: you add nothing else.

## Add it to an Xcode project

1. Drag `NativeShell.xcframework` (or `NativeShellDev.xcframework` for development builds) into the app target's
   **Frameworks, Libraries, and Embedded Content**. It is a static framework, so choose **Do Not Embed**.
2. In Swift, `import NativeShell`.

> [!TIP]
> Link `NativeShellDev.xcframework` in the Debug configuration and `NativeShell.xcframework` in Release, so only
> development builds can connect to a dev server.

## Ship a bundle with the app

Build the NSH app into a folder and add that folder to the app's resources as a folder reference named `nsh`:

```sh
nsh build App.tsx --release --out MyApp/nsh
```

`nsh build` writes:

| File | Contents |
|---|---|
| `bundle.nshc` | The app compiled to bytecode: the fastest to load. |
| `bundle.js` | The same app as JS source. |
| `bundle.js.map` | Its source map (stack traces point at your `.tsx` files). |
| `capabilities.json` | The [capabilities](./sandbox-and-capabilities.md) the app declares, when `nsh.json` has any. |
| `assets/` | Images and fonts the app imports, at their paths relative to the entry. |

## Show the app

```swift
import NativeShell
import UIKit

final class EmbedController: UIViewController {
    private let app = NshAppView()

    override func viewDidLoad() {
        super.viewDidLoad()
        app.translatesAutoresizingMaskIntoConstraints = false
        app.onError = { message in print("nsh error: \(message)") }
        view.addSubview(app)
        NSLayoutConstraint.activate([
            app.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            app.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            app.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            app.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        guard let url = Bundle.main.url(forResource: "bundle", withExtension: "qbc", subdirectory: "nsh") else { return }
        do {
            try app.loadBundle(at: url)
        } catch {
            print("cannot load: \(error)")
        }
    }

    // Let the embedded app's StatusBar decide the status bar.
    override var preferredStatusBarStyle: UIStatusBarStyle { NshAppView.preferredStatusBarStyle }
    override var prefersStatusBarHidden: Bool { NshAppView.prefersStatusBarHidden }
    override var preferredStatusBarUpdateAnimation: UIStatusBarAnimation {
        NshAppView.preferredStatusBarUpdateAnimation
    }
}
```

The view's size is the app's window: NSH lays the app out at the view's bounds and again whenever they change. Loading
again replaces the running app; the app stops when the view is deallocated.

The complete sample, a UIKit app that shows an NSH app below its own native header, is `examples/embed-ios/main.swift`.

## `NshAppView`

`NshAppView` is a `UIView` subclass isolated to the main actor.

| Member | Description |
|---|---|
| `init(frame: CGRect = .zero, trace: Bool = false)` | Creates the view. `trace` logs the time of every commit. `init?(coder:)` supports storyboards and nibs. |
| `onError: ((String) -> Void)?` | Receives fatal app errors (a bundle that fails to load, uncaught JS exceptions, a VM stopped by a limit) on the main thread. Errors are always logged; development builds also show them over the app. |
| `limits: NshLimits` | The VM limits of the apps loaded from now on. |
| `grantCapabilities(_ json: String?) throws` | The most the apps loaded from now on may do, as JSON in the form of `nsh.json`'s `capabilities`; `nil` grants what each app declares. Throws `NshError` for invalid JSON. |
| `loadBundle(at url: URL) throws` | Runs a bundle written by `nsh build`. Assets come from `assets/` next to it; `bundle.js.map` and `capabilities.json` next to it are read too. |
| `run(_ package: NshPackage)` | Runs a verified signed package (see below). A package runs once. |
| `loadPackage(at:trust:) async throws -> NshPackage.Info` | Verifies the package at a URL off the main thread, then runs it. Throws why it was refused; the running app stays. |
| `connectDevServer(_ url: URL) throws` | Development: loads from a `nsh dev` server and reloads on every change. Throws with `NativeShell.xcframework`, which has no development support. |
| `register(component:_:)` | Registers a [native component](./native-components.md). |
| `static preferredStatusBarStyle`, `prefersStatusBarHidden`, `preferredStatusBarUpdateAnimation` | What the running apps asked for with `StatusBar`: return them from the hosting view controller's overrides. |

> [!IMPORTANT]
> `bundle.nshc` (bytecode) loads only from inside the app bundle. QuickJS does not validate bytecode, so bytecode from
> anywhere else could corrupt memory: for a file outside the app bundle, load `bundle.js` (JS source), or better, a
> [signed package](./signed-packages.md).

`limits` and `grantCapabilities` apply from the next load: set them before `loadBundle` / `run`.

## Limits

```swift
app.limits = NshLimits(memoryMB: 128, callBudget: 1, maxNodes: 50_000)
```

| `NshLimits` field | Default | Meaning |
|---|---|---|
| `memoryMB: UInt32?` | 256 | JS heap in MB. Running out stops the app. |
| `stackKB: UInt32?` | 4096 (also the maximum) | Interpreter stack in KB. Deeper recursion throws a `RangeError`. |
| `loadBudget: TimeInterval?` | 10 s | Time to evaluate the bundle. |
| `callBudget: TimeInterval?` | 2 s | Time of any other call into JS (one event, one timer, one frame). An overrun call is cut off. |
| `maxNodes: UInt32?` | 200 000 | Nodes the app may keep. |

`nil` keeps the default. What each limit does when it is hit: [Sandbox and capabilities](./sandbox-and-capabilities.md).

## Capabilities

An app declares what it may reach in its `nsh.json`; the embedding app can grant less. A request must be allowed by
both:

```swift
try app.grantCapabilities("""
{
  "network": { "hosts": ["api.example.com", "*.cdn.example.com"] },
  "webview": { "hosts": ["help.example.com"] }
}
""")
try app.loadBundle(at: url)
```

Here the app may not set the status bar (no `statusBar`), even if its `nsh.json` asks for it. Details:
[Sandbox and capabilities](./sandbox-and-capabilities.md).

## Signed packages

To run code the app downloads (updates, third-party mini apps), use [signed packages](./signed-packages.md):
`nsh publish` writes a `.nshpkg` signed by a release key that your root key certified, and the app verifies it before
running it.

```swift
let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
let trust = try NshTrust(stateDirectory: support.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)   // the contents of root.pub from `nsh keygen`

Task {
    do {
        let info = try await app.loadPackage(at: downloadedFile, trust: trust)
        print("running \(info.id) \(info.version) (build \(info.build))")
    } catch {
        print("refused: \(error)")   // the running app stays
    }
}
```

To verify first and run later, split the two steps:

```swift
let package = try await trust.openPackage(at: downloadedFile)   // verifies off the main thread
// ... decide, ask the user, ...
app.run(package)
```

| API | Description |
|---|---|
| `NshTrust(stateDirectory: URL) throws` | Opens or creates the trust store. Its state (newest revocation list, highest build accepted per app) persists there, and packages' assets are extracted under it. Trusts no key until `addRoot`. Thread-safe. |
| `addRoot(_ publicKey: String) throws` | Trusts a root public key (base64, the contents of a `.pub` file). Several roots may be trusted. |
| `updateRevocations(_ json: String) throws` | Applies a revocation list written by `nsh revoke`, signed by a trusted root and newer than the one the store has. |
| `openPackage(at: URL) async throws -> NshPackage` | Verifies, extracts assets and accepts the package (older builds of that app are refused from then on). |
| `NshPackage.info` | `id`, `name`, `version`, `build` of the app inside. |
| `NshError` | The error every throwing call throws; `message` says why. |

## Development

With `NativeShellDev.xcframework`, point the view at a dev server instead of a bundle:

```swift
#if DEBUG
try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)
#else
try app.loadBundle(at: bundledURL)
#endif
```

Run `nsh dev App.tsx` on your Mac. The simulator shares the Mac's network, so `127.0.0.1` reaches the server; a
device uses the LAN address `nsh dev` prints (`devices on this network: ws://192.168.1.20:8790/ws`). Every save
updates the app in place, logs appear in the `nsh dev` terminal, and errors show over the app. See
[Dev server and debugging](./dev-server-and-debugging.md).

> [!NOTE]
> On a device, the connection to your Mac is ordinary local network traffic over plain `ws://` and `http://` (assets).
> iOS may ask the user for local network access, and the app's App Transport Security settings must allow it.

## Notes

- **Threads.** `NshAppView` is main-actor isolated. `NshTrust`, `NshPackage` and `NshComponentContext` are thread-safe.
  The JS engine runs on its own thread; the main thread never waits for it.
- **Status bar.** The embedded app's `StatusBar` changes what `NshAppView.preferredStatusBarStyle` and friends return;
  the hosting view controller must forward them, as in the example above.
- **Safe area.** Apps use `SafeAreaView` or `useSafeAreaInsets()` from [`nsh/ui/mobile`](../components/safe-area-view.md)
  as usual. An `NshAppView` placed below your own header gets the insets that apply to it, not the window's.
- **Mac Catalyst.** The same framework works in Catalyst apps.

## See also

- [Embedding in Android apps](./embedding-android.md)
- [Native components](./native-components.md)
- [Signed packages](./signed-packages.md)
- [Sandbox and capabilities](./sandbox-and-capabilities.md)
