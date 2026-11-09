# Share an application's Tokio runtime with mobile views

A Rust-linked mobile host can use the same Tokio runtime for its service core and
NSH's async tasks. Each view keeps a cloned `Handle`; the application owns the
`Runtime`. Keep it running until all views using it have stopped. A multi-thread
runtime with IO and time enabled is suitable for a process-lifetime service core:

```rust
let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?;
```

Use the application's existing runtime when it already has one. The snippets below
receive a reference to it rather than create another pool. A current-thread runtime
also works when the application keeps driving it on another thread.

## iOS: Rust `AppView`

Call on the UIKit main thread, using the same Rust-linked `nsh-platform-ios` crate
as the rest of the host:

```rust
use nsh_engine::Bundle;
use nsh_platform_ios::{AppView, SurfaceOptions};
use objc2::MainThreadMarker;
use objc2_core_foundation::CGRect;
use tokio::runtime::Runtime;

fn make_view(
    mtm: MainThreadMarker,
    frame: CGRect,
    runtime: &Runtime,
    bundle: Bundle,
) -> AppView {
    let mut options = SurfaceOptions::default();
    options.runtime = Some(runtime.handle().clone());
    let view = AppView::new(mtm, frame, options);
    view.load(bundle);
    view
}
```

Add `view.view()` to the UIKit hierarchy and retain `view` for as long as that
screen is shown. Subsequent loads reuse the configured handle. Dropping `view`
stops its engine; the application can still spawn work on `runtime` and can create
another view with it. `Surface::new` accepts the same `SurfaceOptions` directly.

## Android: Rust-linked `NshView`

Link `nsh-platform-android` into the application's custom `libnsh.so` alongside its
Rust service core. A JNI method in that library can read a live `NshView`'s native
handle and configure just that view. This helper takes the application's runtime:

```rust
use nsh::{jni, set_runtime};
use jni::{JNIEnv, objects::JObject};
use tokio::runtime::Runtime;

/// # Safety
/// Call on the view's UI thread with a live NshView from this linked library.
unsafe fn configure_view(
    env: &mut JNIEnv<'_>,
    view: &JObject<'_>,
    runtime: &Runtime,
) -> jni::errors::Result<()> {
    let handle = env.get_field(view, "handle", "J")?.j()?;
    // SAFETY: the caller keeps this view alive on its UI thread.
    unsafe { set_runtime(handle, Some(runtime.handle().clone())) };
    Ok(())
}
```

Your app's JNI entry point calls this helper with the service core's runtime;
report any JNI error to the Java caller. The Java/Kotlin side calls that entry
point after constructing the view, before its first load or dev-server connection:

```java
// The app implements this JNI method in its Rust-linked library.
private static native void configureRuntime(NshView view);

NshView view = new NshView(context);
configureRuntime(view);
view.loadAssetBundle("nsh");
```

Each view has its own setting. Set it before a load; an app waiting for the first
non-empty size uses the setting at engine startup. Later changes affect the next
load, while the running engine keeps its current handle. Call
`unsafe { set_runtime(handle, None) }` to select NSH's shared default for subsequent
loads. Destroying the view stops its engine and releases its handles while the
application's runtime stays usable. A recreated view starts with the default;
configure it again to share the application's runtime.

These hooks keep Rust handles inside the custom-linked Rust host. The stock Swift
and Java/Kotlin SDKs select NSH's process-wide default runtime.

## Regression coverage

`cargo test -p nsh-engine --test engine runtime::` checks that component tasks and
Tokio workers see the supplied runtime's ID, that separate engines select their
own handles, that an application's runtime remains usable after engine disposal
and recreation, and that engines given `None` share the existing NSH default.
The mobile backend builds check the forwarding code for their respective targets;
UI lifecycle validation runs on an iOS simulator or Android device.
