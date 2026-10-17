---
title: Android
description: Run NSH apps on Android with Android views, on an emulator or a device, package them as an .apk, and embed them in an existing Android app with nsh.aar and NshView.
order: 12
group: platforms
---

# Android

NSH apps run on Android with ordinary Android views and `StaticLayout` text, from Android 9 (API 28), on `arm64-v8a`
and `x86_64`. An existing app embeds modules in an `NshView` and loads signed packages it downloads, under limits and
capabilities it sets: that is the main use. Running and packaging whole apps work too, as on the other platforms. On Android, `Platform.OS` is
`"android"` and the `android={{…}}` props apply.

## Requirements

- **The Android SDK** (with `platforms;android-35`, build tools and `platform-tools`) and **a JDK**, on macOS, Windows
  or Linux. See [Installation](./installation.md#android).
- An emulator, or a device connected over `adb`.

## Running an app

```sh
nsh run my-app --target android
```

Start an emulator or connect a device first (`adb devices` lists them; NSH uses the first one). `nsh` installs a
development host APK (rebuilt only when the runtime changes) and forwards the dev server's port with `adb reverse`, so
it works over USB without a shared network; the app updates as you save, with logs in the terminal.

## Packaging

```sh
nsh package my-app --target android --run     # out/<Name>.apk, installed and launched
```

The APK is built with the SDK's build tools, without Gradle, and signed with the debug key unless you pass a release
keystore. Signing for release and icons: [Packaging](./packaging.md#android).

## Embedding in an Android app

An existing Android app can show NSH apps inside its own UI. The runtime ships as an Android library, `nsh.aar`: you
create a `com.longbridge.nsh.NshView` (a `FrameLayout`), add it to any layout and load an app into it. The NSH app
renders with ordinary Android views next to your own.

Each `NshView` is one independent engine with its own JS VM; there are no global singletons, so several can run at
once.

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/host-app-light.svg" alt="One host app with two nsh views (NshAppView on iOS, NshView on Android): each is its own engine with its own thread, VM, limits and capabilities; the host grants capabilities, sets limits and registers native components such as a map.">
<img class="diagram diagram-dark" src="/diagrams/host-app-dark.svg" alt="One host app with two nsh views (NshAppView on iOS, NshView on Android): each is its own engine with its own thread, VM, limits and capabilities; the host grants capabilities, sets limits and registers native components such as a map.">
</div>

## Get the library

The distribution contains both variants:

```text
dist/nsh/embed/android/nsh.aar       release: bundles and signed packages
dist/nsh/embed/android/nsh-dev.aar   also connects to a `nsh dev` server
```

The AAR holds the Java runtime and `libnsh.so` for `arm64-v8a` and `x86_64` (64-bit only). It declares the `INTERNET`
permission and needs **API 28** (Android 9) or newer.

Add it to the app module, for example from a `libs` folder:

```kotlin
// app/build.gradle.kts
dependencies {
    debugImplementation(files("libs/nsh-dev.aar"))
    releaseImplementation(files("libs/nsh.aar"))
}
```

## Ship a bundle with the app

Build the NSH app into the app's assets:

```sh
nsh build App.tsx --release --out app/src/main/assets/nsh
```

This writes `bundle.nshc` (bytecode), `bundle.js` (source), `bundle.js.map`, `capabilities.json` (when `nsh.json`
declares [capabilities](./sandbox-and-capabilities.md)) and `assets/` (the images and fonts the app imports).

## Show the app

```java
import android.app.Activity;
import android.os.Bundle;
import android.util.Log;
import com.longbridge.nsh.NshView;

public final class MainActivity extends Activity {
    private NshView nsh;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        nsh = new NshView(this);
        nsh.setErrorListener(message -> Log.e("nsh", message));
        setContentView(nsh);
        nsh.loadAssetBundle("nsh");   // runs assets/nsh/bundle.nshc
    }

    @Override
    protected void onDestroy() {
        nsh.destroy();
        super.onDestroy();
    }
}
```

In Kotlin:

```kotlin
class MainActivity : Activity() {
    private lateinit var nsh: NshView

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        nsh = NshView(this)
        nsh.setErrorListener { message -> Log.e("nsh", message) }
        setContentView(nsh)
        nsh.loadAssetBundle("nsh")
    }

    override fun onDestroy() {
        nsh.destroy()
        super.onDestroy()
    }
}
```

`NshView` can also sit inside any layout, below your own toolbar for example:

```java
LinearLayout column = new LinearLayout(this);
column.setOrientation(LinearLayout.VERTICAL);
column.addView(toolbar);
column.addView(nsh, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1));
setContentView(column);
```

The view's size is the app's window: NSH lays the app out at the view's size and again whenever it changes. Loading
again replaces the running app. The complete sample is
`examples/embed-android/src/com/longbridge/nsh/embeddemo/MainActivity.java`.

## `NshView`

| Method | Description |
|---|---|
| `NshView(Context context)`, `NshView(Context context, boolean trace)` | Creates an empty view. `trace` logs the time of every commit. |
| `setErrorListener(ErrorListener listener)` | Receives fatal app errors (load failures, uncaught JS exceptions, a VM stopped by a limit) on the UI thread. They are always logged. |
| `setLimits(int memoryMb, int stackKb, int loadMs, int callMs, int maxNodes)` | VM limits of the apps loaded next; 0 keeps a default. |
| `setGrantedCapabilities(String json)` | The most the apps loaded next may do, as JSON in the form of `nsh.json`'s `capabilities`; `null` grants what each app declares. Returns `false` (and is ignored) for an invalid grant. |
| `loadAssetBundle(String dir)` | Runs `dir/bundle.nshc` from the APK's assets; app assets come from `dir/assets/`. Returns `false` if it cannot be read (also reported to the error listener). |
| `loadBundleFile(File file)` | Runs a `bundle.js` file; assets come from `assets/` next to it. Refuses `.nshc` files. |
| `runPackage(NshPackage pkg)` | Runs a verified signed package, replacing the running app. A package runs once. |
| `registerComponent(String name, NshComponent.Factory factory)` | Registers a [native component](./native-components.md). |
| `connectDevServer(String url)` | Development: loads from a `nsh dev` server and reloads on every change. Returns `false` with the release AAR. |
| `handleBack()` | Lets the app handle a back press (closes modals, pops screens, runs `BackHandler` listeners); returns whether it did. |
| `destroy()` | Stops the app. The view can no longer be used. |

> [!IMPORTANT]
> Compiled bytecode (`.nshc`) only loads from the APK's own assets or from a signed package. Files on storage can be replaced,
> and QuickJS does not validate bytecode, so `loadBundleFile` takes JS source only. Code the app downloads should come
> as a [signed package](./signed-packages.md).

`setLimits` and `setGrantedCapabilities` apply from the next load: call them before `loadAssetBundle` and friends.

## Limits

```java
// 128 MB heap, default stack, 10 s to load, 1 s per call, 50 000 nodes
nsh.setLimits(128, 0, 10_000, 1_000, 50_000);
```

The defaults are a 256 MB heap, a 4096 KB stack, 10 s to load, 2 s per call and 200 000 nodes. What each limit does
when it is hit: [Sandbox and capabilities](./sandbox-and-capabilities.md).

## Capabilities

```java
nsh.setGrantedCapabilities(
    "{\"network\": {\"hosts\": [\"api.example.com\"]}, \"webview\": {\"hosts\": [\"help.example.com\"]}}");
nsh.loadAssetBundle("nsh");
```

A request must be allowed both by what the app declares in its `nsh.json` and by the grant. See
[Sandbox and capabilities](./sandbox-and-capabilities.md).

## Signed packages

Verify downloaded packages off the UI thread, then run them on it:

```java
NshTrust trust = new NshTrust(new File(getFilesDir(), "nsh"));
trust.addRoot(rootPublicKey);   // the contents of root.pub from `nsh keygen`

executor.execute(() -> {
    try {
        NshPackage pkg = trust.openPackage(downloadedFile);
        runOnUiThread(() -> nsh.runPackage(pkg));
    } catch (NshPackage.RefusedException e) {
        Log.w("nsh", "refused: " + e.getMessage());   // the running app stays
    }
});
```

| API | Description |
|---|---|
| `new NshTrust(File stateDir)` | Opens or creates the trust store. Its state (newest revocation list, highest build accepted per app) persists there and packages' assets are extracted under it. Trusts no key until `addRoot`. Usable from any thread; `close()` when done. |
| `boolean addRoot(String publicKey)` | Trusts a root public key (base64, the contents of a `.pub` file). `false` for an invalid key. |
| `boolean updateRevocations(String json)` | Applies a revocation list from `nsh revoke`, signed by a trusted root and newer than the store's. |
| `NshPackage openPackage(File file) throws NshPackage.RefusedException` | Verifies, extracts the assets and accepts the package (older builds of the app are refused from then on). Slow for large packages: call it off the UI thread. |
| `NshPackage` | `appId`, `appName`, `version`, `build`; `close()` if it will not run. |

How keys, certificates and revocation work: [Signed packages](./signed-packages.md).

## Development

With `nsh-dev.aar`:

```java
if (BuildConfig.DEBUG) {
    nsh.connectDevServer("ws://127.0.0.1:8790/ws");
} else {
    nsh.loadAssetBundle("nsh");
}
```

Run `nsh dev App.tsx` on your computer. For an emulator or a USB device, forward the port so the device's own
`127.0.0.1` reaches the server:

```sh
adb reverse tcp:8790 tcp:8790
```

Otherwise use the LAN address `nsh dev` prints. Assets come from the dev server over plain HTTP, so `nsh-dev.aar`
declares `android:usesCleartextTraffic="true"`, which the manifest merger applies to the app (a network security
config of the app may override it). The release `nsh.aar` does not.

## Activity setup

- **Back button.** On Android 13 and newer, `NshView` registers its own back callback while the app wants back presses
  (an open modal, a pushed screen, a `BackHandler` listener). On Android 12 and older, forward the press:

  ```java
  @Override
  public void onBackPressed() {
      if (!nsh.handleBack()) super.onBackPressed();
  }
  ```

  For predictive back on Android 13+, set `android:enableOnBackInvokedCallback="true"` on the `<application>`, as
  packaged NSH apps do.
- **Keyboard.** `NshView` scrolls focused inputs clear of the keyboard itself. Leave `android:windowSoftInputMode` unset
  or use `adjustResize`; with `adjustPan` the system moves the whole window instead.
- **Safe area.** The view reports as insets only the parts of the system bars and the keyboard that overlap it, so an
  `NshView` below a toolbar gets no top inset. For edge-to-edge content, draw behind the system bars and let the app use
  `SafeAreaView` / `useSafeAreaInsets()`.
- **Configuration changes.** An activity recreated on rotation or a theme change destroys the view, and the app starts
  again from scratch. To keep it running, declare the changes the activity handles itself; packaged NSH apps use:

  ```xml
  <activity android:name=".MainActivity"
      android:configChanges="orientation|screenSize|screenLayout|smallestScreenSize|keyboard|keyboardHidden|navigation|uiMode|density|fontScale|locale|layoutDirection"
      android:windowSoftInputMode="adjustResize" />
  ```

  `NshView` follows the new size, color scheme, font scale and layout direction itself.
- **Lifecycle.** Call `destroy()` when the hosting activity or fragment goes away.

## See also

- [Embedding in iOS apps](./embedding-ios.md)
- [Native components](./native-components.md)
- [Signed packages](./signed-packages.md)
- [Packaging](./packaging.md): a standalone APK without writing any Java
