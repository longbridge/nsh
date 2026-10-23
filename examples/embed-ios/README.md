# Embedding NSH in an iOS app

`main.swift` is an existing UIKit app that shows a NSH app below its own
native views.

1. Add `dist/nsh/embed/ios/NativeShell.xcframework` (or `NativeShellDev.xcframework` for
   development builds) to the app target (`cargo xtask ios-embed` builds both). Swift code then
   `import NativeShell`.
2. Build the NSH bundle and add the output folder to the app's resources as
   `nsh`: `nsh build ../embedded --release --out nsh`.
3. Create an `NshAppView` (a `UIView`), add it to a view hierarchy and call
   `try app.loadBundle(at:)` with the URL of `nsh/bundle.nshc`. The app stops when the view is deallocated.

Signed packages (`nsh publish`; see `docs/dev/signed-packages.md`), e.g. updates the app downloads:

```swift
let trust = try NshTrust(stateDirectory: applicationSupport.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)                 // root.pub from `nsh keygen`
let info = try await app.loadPackage(at: downloaded, trust: trust)
```

`loadPackage` verifies off the main thread and throws why a package was refused (the running app
stays). Bytecode files only load from inside the app bundle: downloaded code must come as a signed
package (or JS source).

Native components: the app's own views, used from JS with `nativeComponent` (`nsh/native`).
`main.swift` registers `embed.badge`, which `examples/embedded` shows:

```swift
app.register(component: "embed.badge") { context in Badge(context) }  // Badge: NshComponent
```

The component gets its props (`setProp`, JSON as Foundation objects) and commands, emits events with
`context.emit("tap", ["count": n])` and may report its size with `context.setSize(_:)`.

Theme overrides: "Brand" puts the host's brand over the app's theme. The tokens of a theme file
(`nsh.json`'s `"theme"` format) win over the app's of the same names, light and dark, and
classes such as `bg-primary` restyle at once; `nil` gives the app its own theme back:

```swift
try app.setThemeOverrides(##"{ "cssVars": { "light": { "primary": "#e11d48" } } }"##)
```

With `NativeShellDev.xcframework`, `try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)`
loads from `nsh dev` instead and reloads on every change.

`NshAppView` is main-actor isolated; `NshTrust` is thread-safe. The host view controller can forward
the app's status bar: `NshAppView.preferredStatusBarStyle`, `.prefersStatusBarHidden`,
`.preferredStatusBarUpdateAnimation`.
