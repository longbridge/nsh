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
handle and configure just that view.

### Keep the JNI names in minified builds

The distributed `nsh.aar` and `nsh-dev.aar` already carry a consumer rule preserving
`com.longbridge.nsh` and all its members, including the package-private
`NshView.handle` field. A custom AAR built from the Java sources or `classes.jar`
needs the same rule in its `consumer-rules.pro`. The example's application-owned
bridge also needs its class and native method names preserved:

```proguard
-keep class com.longbridge.nsh.** { *; }
-keep class com.example.app.RuntimeBridge { *; }
```

For an Android library built with Gradle, publish these rules with the custom AAR
using its `build.gradle.kts`:

```kotlin
android {
    defaultConfig {
        consumerProguardFiles("consumer-rules.pro")
    }
}
```

For an app consuming the distributed AAR, put only the `RuntimeBridge` rule in the
app's `proguard-rules.pro`, enabled by its minified build configuration; the AAR
supplies the NSH rule. A host that packages `classes.jar` directly puts both rules
in that app file. Keeping only public members would leave `handle` unprotected.
Keep the bridge's package and class names in sync with the exported JNI symbol
below when adapting this example.

### Rust initialization and JNI entry point

Call `install_runtime` once from the application's existing Rust service
initialization, before Java configures any view. This stores only a cloned handle;
the service core continues owning and driving its `Runtime`. The app-level slot is
for this example's process-lifetime service, not a global NSH runtime setting:
each call still configures only the view it receives.

```rust
use std::sync::OnceLock;

use jni::{
    JNIEnv,
    objects::{JClass, JObject},
};
use nsh::{jni, set_runtime};
use tokio::runtime::{Handle, Runtime};

static APP_RUNTIME: OnceLock<Handle> = OnceLock::new();

/// Registers the existing service runtime; returns the handle on a second call.
pub fn install_runtime(runtime: &Runtime) -> Result<(), Handle> {
    APP_RUNTIME.set(runtime.handle().clone())
}

#[unsafe(no_mangle)]
extern "system" fn Java_com_example_app_RuntimeBridge_configureRuntime(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    view: JObject<'_>,
) {
    let Some(runtime) = APP_RUNTIME.get() else {
        let _ = env.throw_new(
            "java/lang/IllegalStateException",
            "initialize the application's Rust runtime before configuring NSH",
        );
        return;
    };
    let handle = match env
        .get_field(&view, "handle", "J")
        .and_then(|value| value.j())
    {
        Ok(0) => {
            let _ = env.throw_new("java/lang/IllegalStateException", "NshView was destroyed");
            return;
        }
        Ok(handle) => handle,
        Err(error) => {
            // Preserve NoSuchFieldError and other exceptions raised by JNI.
            if let Ok(false) = env.exception_check() {
                let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
            }
            return;
        }
    };
    // SAFETY: RuntimeBridge.configure checks the UI thread and passes a live
    // NshView from this linked library; its nonzero handle cannot be destroyed
    // concurrently on that same thread.
    unsafe { set_runtime(handle, Some(runtime.clone())) };
}
```

### Java bridge and call site

Define the matching bridge in `com/example/app/RuntimeBridge.java`. It loads the
custom-linked library and checks the UI thread before entering Rust. A JNI lookup
failure or an uninitialized runtime propagates to the Java caller as an exception.

```java
package com.example.app;

import android.os.Looper;
import com.longbridge.nsh.NshView;
import java.util.Objects;

public final class RuntimeBridge {
    static {
        System.loadLibrary("nsh");
    }

    private RuntimeBridge() {}

    private static native void configureRuntime(NshView view);

    public static void configure(NshView view) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            throw new IllegalStateException("configure NSH on the UI thread");
        }
        configureRuntime(Objects.requireNonNull(view));
    }
}
```

After initializing the Rust service runtime, construct and configure the view on
the UI thread, before its first load or dev-server connection:

```java
NshView view = new NshView(context);
RuntimeBridge.configure(view);
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
