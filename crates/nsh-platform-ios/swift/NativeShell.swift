// NativeShell: embedding nsh apps in iOS apps.
//
//     let app = NshAppView(frame: container.bounds)
//     container.addSubview(app)
//     app.onError = { message in print(message) }
//     try app.loadBundle(at: Bundle.main.url(forResource: "bundle", withExtension: "qbc", subdirectory: "nsh")!)
//
// Signed packages (`nsh publish`), e.g. downloaded updates:
//
//     let trust = try NshTrust(stateDirectory: applicationSupport.appendingPathComponent("nsh"))
//     try trust.addRoot("<root.pub>")
//     try await app.loadPackage(at: downloaded, trust: trust)
//
// Built over the C ABI of `src/embed.rs` (`include/nsh.h`), imported
// privately: apps only see this module.

import UIKit
internal import NativeShellC

/// Why nsh refused something: an invalid bundle, grant, key or revocation
/// list, a refused package, a missing development build.
public struct NshError: Error, LocalizedError, CustomStringConvertible, Sendable {
    /// What went wrong.
    public let message: String

    /// The message.
    public var description: String { message }
    /// The message.
    public var errorDescription: String? { message }
}

/// Throws the error string `error` returned, if any (freeing it).
private func check(_ error: UnsafeMutablePointer<CChar>?) throws {
    guard let error else { return }
    let message = String(cString: error)
    nsh_string_free(error)
    throw NshError(message: message)
}

/// The VM limits of an app (`nil`: the default).
public struct NshLimits: Sendable, Equatable {
    /// JS heap in MB (default 256, for an app embedded in another). Running out stops the app.
    public var memoryMB: UInt32?
    /// Interpreter stack in KB (default 4096, also the maximum).
    public var stackKB: UInt32?
    /// Time to evaluate the bundle, in seconds (default 10).
    public var loadBudget: TimeInterval?
    /// Time of any other call into JS (one event, timer or frame), in
    /// seconds (default 2). An overrun call is cut off.
    public var callBudget: TimeInterval?
    /// Nodes the app may keep (default 200 000).
    public var maxNodes: UInt32?

    /// Limits; `nil` keeps a default.
    public init(
        memoryMB: UInt32? = nil,
        stackKB: UInt32? = nil,
        loadBudget: TimeInterval? = nil,
        callBudget: TimeInterval? = nil,
        maxNodes: UInt32? = nil
    ) {
        self.memoryMB = memoryMB
        self.stackKB = stackKB
        self.loadBudget = loadBudget
        self.callBudget = callBudget
        self.maxNodes = maxNodes
    }
}

/// Holds an error handler for the C callback.
private final class ErrorHandlerBox {
    let handler: (String) -> Void
    init(_ handler: @escaping (String) -> Void) { self.handler = handler }
}

/// A view running a nsh app. Add it to any hierarchy, then load a bundle,
/// a signed package or (with `NativeShellDev.xcframework`) a dev server.
/// Loading again replaces the running app; the app stops when the view is
/// deallocated.
@MainActor
public final class NshAppView: UIView {
    private let app: OpaquePointer

    /// Creates the view. `trace` logs per-commit timings.
    public init(frame: CGRect = .zero, trace: Bool = false) {
        app = nsh_app_create(frame, trace)
        super.init(frame: frame)
        attach()
    }

    /// Creates the view from a storyboard or nib.
    public required init?(coder: NSCoder) {
        app = nsh_app_create(.zero, false)
        super.init(coder: coder)
        attach()
    }

    private func attach() {
        let content = nsh_app_view(app)
        content.frame = bounds
        content.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        addSubview(content)
    }

    deinit {
        // UIKit releases views on the main thread.
        nsh_app_destroy(app)
    }

    /// Receives fatal app errors (load failures, uncaught JS exceptions) on
    /// the main thread. They are always logged; development builds also
    /// show them over the app.
    public var onError: ((String) -> Void)? {
        didSet {
            guard let onError else {
                nsh_app_set_error_handler(app, nil, nil, nil)
                return
            }
            let box = Unmanaged.passRetained(ErrorHandlerBox(onError)).toOpaque()
            nsh_app_set_error_handler(
                app,
                { context, message in
                    let box = Unmanaged<ErrorHandlerBox>.fromOpaque(context!).takeUnretainedValue()
                    box.handler(String(cString: message))
                },
                box,
                { context in Unmanaged<ErrorHandlerBox>.fromOpaque(context!).release() }
            )
        }
    }

    /// The VM limits of the apps loaded from now on.
    public var limits = NshLimits() {
        didSet {
            let ms = { (seconds: TimeInterval?) -> UInt32 in
                seconds.map { UInt32(clamping: Int(($0 * 1000).rounded())) } ?? 0
            }
            nsh_app_set_limits(
                app, limits.memoryMB ?? 0, limits.stackKB ?? 0, ms(limits.loadBudget),
                ms(limits.callBudget), limits.maxNodes ?? 0)
        }
    }

    /// Grants the apps loaded from now on at most these capabilities: JSON in
    /// the form of `nsh.json`'s `capabilities` (`nil`: what each app
    /// declares). A request must be allowed by both.
    public func grantCapabilities(_ json: String?) throws {
        try check(nsh_app_set_granted_capabilities(app, json))
    }

    /// Runs a bundle written by `nsh build`; assets come from `assets` next
    /// to it. Bytecode (`bundle.nshc`) loads only from inside the app bundle;
    /// elsewhere load `bundle.js`, or use signed packages.
    public func loadBundle(at url: URL) throws {
        try check(nsh_app_load_bundle(app, url.path))
    }

    /// Runs a verified package (``NshTrust/openPackage(at:)``).
    public func run(_ package: NshPackage) {
        guard let handle = package.take() else {
            onError?("the package already ran")
            return
        }
        nsh_app_run_package(app, handle)
    }

    /// Verifies the signed package at `url` with `trust` (off the main
    /// thread) and runs it. Throws why it was refused; the running app stays.
    @discardableResult
    public func loadPackage(at url: URL, trust: NshTrust) async throws -> NshPackage.Info {
        let package = try await trust.openPackage(at: url)
        run(package)
        return package.info
    }

    /// Development: loads from a `nsh dev` server (`ws://host:port/ws`) and
    /// reloads on every change. Throws without development support
    /// (`NativeShell.xcframework` rather than `NativeShellDev`).
    public func connectDevServer(_ url: URL) throws {
        try check(nsh_app_connect_dev_server(app, url.absoluteString))
    }

    /// The status bar style the running apps asked for: return it from the
    /// host view controller's `preferredStatusBarStyle`.
    public static var preferredStatusBarStyle: UIStatusBarStyle { nsh_status_bar_style() }

    /// Whether the running apps hid the status bar (`prefersStatusBarHidden`).
    public static var prefersStatusBarHidden: Bool { nsh_status_bar_hidden() }

    /// How status bar changes animate (`preferredStatusBarUpdateAnimation`).
    public static var preferredStatusBarUpdateAnimation: UIStatusBarAnimation {
        nsh_status_bar_animation()
    }
}

/// The root keys, revocations and accepted builds that signed packages are
/// verified against. The newest revocation list and the highest build
/// accepted per app persist in its state directory, and packages' assets are
/// extracted under it. Thread-safe.
public final class NshTrust: @unchecked Sendable {
    private let handle: OpaquePointer

    /// Opens (or creates) the store in `stateDirectory`, e.g. under
    /// Application Support. It trusts no key until `addRoot`.
    public init(stateDirectory: URL) throws {
        var error: UnsafeMutablePointer<CChar>?
        guard let handle = nsh_trust_create(stateDirectory.path, &error) else {
            try check(error)
            throw NshError(message: "cannot open the trust store")
        }
        self.handle = handle
    }

    deinit {
        nsh_trust_destroy(handle)
    }

    /// Trusts a root public key (base64, `<name>.pub` from `nsh keygen`):
    /// packages whose release key it certified load.
    public func addRoot(_ publicKey: String) throws {
        try check(nsh_trust_add_root(handle, publicKey.trimmingCharacters(in: .whitespacesAndNewlines)))
    }

    /// Applies a revocation list (the JSON `nsh revoke` writes) signed by a
    /// trusted root; it must be newer than the one the store has.
    public func updateRevocations(_ json: String) throws {
        try check(nsh_trust_update_revocations(handle, json))
    }

    /// Verifies the signed package at `url` (`.nshpkg`, from `nsh publish`)
    /// off the calling thread: its release key must be certified by a trusted
    /// root for its app id, not revoked, and its build not older than one
    /// accepted before. Extracts its assets and accepts it (older builds of
    /// the app are refused from now on). Run it with `NshAppView.run`.
    public func openPackage(at url: URL) async throws -> NshPackage {
        let path = url.path
        return try await Task.detached(priority: .userInitiated) {
            var error: UnsafeMutablePointer<CChar>?
            guard let package = nsh_trust_open_package(self.handle, path, &error) else {
                try check(error)
                throw NshError(message: "refused package")
            }
            return NshPackage(package)
        }.value
    }
}

/// A verified signed package, ready to run once (`NshAppView.run`).
public final class NshPackage: @unchecked Sendable {
    /// The app a package holds.
    public struct Info: Sendable, Equatable {
        /// Its id (`com.example.app`).
        public let id: String
        /// Its display name.
        public let name: String
        /// Its version string.
        public let version: String
        /// Its build number.
        public let build: UInt64
    }

    /// The app it holds.
    public let info: Info
    private var handle: OpaquePointer?
    private let lock = NSLock()

    fileprivate init(_ handle: OpaquePointer) {
        self.handle = handle
        info = Info(
            id: String(cString: nsh_package_app_id(handle)),
            name: String(cString: nsh_package_app_name(handle)),
            version: String(cString: nsh_package_app_version(handle)),
            build: nsh_package_app_build(handle))
    }

    fileprivate func take() -> OpaquePointer? {
        lock.lock()
        defer { lock.unlock() }
        let taken = handle
        handle = nil
        return taken
    }

    deinit {
        nsh_package_free(handle)
    }
}

/// A native component the app registers for `nativeComponent(name, ...)` in
/// JS (`nsh/native`): a map, a player, any view of the app's own.
@MainActor
public protocol NshComponent: AnyObject {
    /// Its view; it fills the box nsh lays out (decoration, frame and
    /// touches are the box's).
    var view: UIView { get }
    /// Prop `name` changed (`nil`: removed). Values are JSON objects
    /// (`String`, `NSNumber`, `[Any]`, `[String: Any]`, `NSNull`); every prop
    /// the app set comes right after the component is made.
    func setProp(_ name: String, _ value: Any?)
    /// JS called `ref.command(name, args)`.
    func command(_ name: String, _ args: Any?)
}

extension NshComponent {
    /// Ignores commands.
    public func command(_ name: String, _ args: Any?) {}
}

/// What a component tells its app: its events and its own size.
/// Thread-safe.
public final class NshComponentContext: @unchecked Sendable {
    private let handle: OpaquePointer

    fileprivate init(_ handle: OpaquePointer) {
        self.handle = handle
    }

    deinit {
        nsh_component_context_free(handle)
    }

    /// Emits `event` with `data` (anything `JSONSerialization` takes): the JS
    /// handler `on<Event>` gets it.
    public func emit(_ event: String, _ data: Any? = nil) {
        guard let data else { return nsh_component_emit(handle, event, nil) }
        guard JSONSerialization.isValidJSONObject([data]),
            let json = try? JSONSerialization.data(withJSONObject: data, options: .fragmentsAllowed)
        else {
            return print("[nsh] event \(event) of a component: data is not JSON")
        }
        String(decoding: json, as: UTF8.self).withCString { nsh_component_emit(handle, event, $0) }
    }

    /// The component's own size (points): used where its style leaves a
    /// dimension open (e.g. content-sized views).
    public func setSize(_ size: CGSize) {
        nsh_component_set_size(handle, Double(size.width), Double(size.height))
    }
}

/// Holds a registration's factory for the C callbacks.
private final class ComponentFactoryBox {
    let make: @MainActor (NshComponentContext) -> NshComponent
    init(_ make: @escaping @MainActor (NshComponentContext) -> NshComponent) { self.make = make }
}

/// Holds a component for the C callbacks.
private final class ComponentBox {
    let component: NshComponent
    init(_ component: NshComponent) { self.component = component }
}

/// JSON from nsh as Foundation objects.
private func jsonObject(_ json: UnsafePointer<CChar>?) -> Any? {
    guard let json else { return nil }
    let data = Data(bytes: json, count: strlen(json))
    return try? JSONSerialization.jsonObject(with: data, options: .fragmentsAllowed)
}

extension NshAppView {
    /// Registers native component `name` (`nativeComponent(name, ...)` in
    /// JS): `make` makes one for every element created from now on.
    public func register(component name: String, _ make: @escaping @MainActor (NshComponentContext) -> NshComponent) {
        let factory = Unmanaged.passRetained(ComponentFactoryBox(make)).toOpaque()
        var callbacks = NshComponentCallbacks(
            create: { factory, context in
                let box = Unmanaged<ComponentFactoryBox>.fromOpaque(factory!).takeUnretainedValue()
                // nsh calls it on the main thread.
                let component = MainActor.assumeIsolated { box.make(NshComponentContext(context)) }
                return Unmanaged.passRetained(ComponentBox(component)).toOpaque()
            },
            view: { component in
                let box = Unmanaged<ComponentBox>.fromOpaque(component).takeUnretainedValue()
                return MainActor.assumeIsolated { box.component.view }
            },
            set_prop: { component, name, json in
                let box = Unmanaged<ComponentBox>.fromOpaque(component).takeUnretainedValue()
                let name = String(cString: name)
                let value = jsonObject(json)
                MainActor.assumeIsolated { box.component.setProp(name, value) }
            },
            command: { component, name, json in
                let box = Unmanaged<ComponentBox>.fromOpaque(component).takeUnretainedValue()
                let name = String(cString: name)
                let args = jsonObject(json)
                MainActor.assumeIsolated { box.component.command(name, args) }
            },
            destroy: { component in
                Unmanaged<ComponentBox>.fromOpaque(component).release()
            },
            release_factory: { factory in
                Unmanaged<ComponentFactoryBox>.fromOpaque(factory!).release()
            }
        )
        nsh_app_register_component(app, name, factory, &callbacks)
    }
}
