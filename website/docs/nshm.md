---
title: nshm
description: "nshm (NSH modules): Rust crates of components, modules and classes that any Native Shell app installs with nsh add and uses from Rust and TSX alike, how to write and publish one, versions and the lock file."
order: 5.7
group: start
---

# nshm

An **nshm** (NSH module) is a Rust crate that gives Native Shell apps components, modules and classes, published on crates.io or Git. An app installs one with `nsh add` and uses it from Rust and from TSX: the same implementation, native on every platform.

[nsh-omarchy](https://github.com/huacnlee/nsh-omarchy), for example, gives an app [Omarchy](https://omarchy.org)'s look: buttons, fields, menus, dialogs, tables and charts that follow the system's Omarchy theme as it changes.

```sh
nsh add nsh-omarchy                  # from crates.io
nsh add github:huacnlee/nsh-omarchy  # or from GitHub
```

Then use it from the language the app is written in, Rust or TSX. **In a Rust app**, its components are functions of the crate:

```rust
// src/lib.rs: the app's root component
use nsh_core::prelude::*;
use nsh_omarchy::tokens;

#[component]
pub fn App() -> impl IntoElement {
    // Draw with the system's Omarchy theme, and follow it as it changes.
    nsh_omarchy::init();
    let notify = signal(true);
    let saved = signal(0_u32);
    view()
        .flex_1()
        .p(px(tokens::SPACE_6))
        .gap(px(tokens::SPACE_4))
        .bg(move || tokens::background())
        .child(
            nsh_omarchy::panel("Settings")
                .child(
                    nsh_omarchy::switch("Notifications")
                        .checked(move || notify.get())
                        .on_change(move |on: &bool| notify.set(*on)),
                )
                .child(nsh_omarchy::button("Save").primary().on_press(move || {
                    saved.update(|n| *n += 1);
                    nsh_omarchy::toast(nsh_omarchy::Toast::new("Settings saved"));
                })),
        )
        .child(nsh_omarchy::body(move || format!("Saved {} times", saved.get())))
        .child(nsh_omarchy::toaster())
}
```

**In a TSX app**, an nshm that exports components, modules or classes to TSX is ready to use once `nsh add` installs it: import them by its specifier, here `nsh-omarchy`. The app needs no newer NSH and no Rust code of its own, since `nsh` builds the app's hosts with the crate.

```tsx
// App.tsx
import { mount, signal, View } from "nsh";
import { Body, Button, Omarchy, Panel, Switch, Toaster, toast } from "nsh-omarchy";

function App() {
  // Draw with the system's Omarchy theme, and follow it as it changes.
  Omarchy.followSystem();
  const notify = signal(true);
  const saved = signal(0);
  const save = () => {
    saved.set(saved() + 1);
    toast({ title: "Settings saved" });
  };
  return (
    <View class="flex-1 gap-4 p-6 bg-background">
      <Panel title="Settings">
        <Switch label="Notifications" checked={notify()} onChange={({ checked }) => notify.set(checked)} />
        <Button label="Save" variant="primary" onPress={save} />
      </Panel>
      <Body>{`Saved ${saved()} times`}</Body>
      <Toaster />
    </View>
  );
}

mount(() => <App />);
```

An nshm is how a feature reaches the apps that need it without being part of NSH: it ships on its own schedule, and only the apps that install it pay its size, build time and dependencies.

## Using an nshm

`nsh add` installs an nshm from crates.io, GitHub, any Git URL or a local directory:

```sh
nsh add nsh-omarchy                                             # crates.io: the newest version
nsh add github:huacnlee/nsh-omarchy                             # GitHub: the default branch, or @<tag>
nsh add https://github.com/huacnlee/nsh-omarchy.git --rev main  # any Git URL
nsh add --path ../nsh-omarchy                                   # a local directory, as it is
```

It lists the nshm in `nsh.json` `"modules"`, pins the version or commit it resolved to in `nsh.lock` (commit both with the app), and adds the crate to the `[dependencies]` of the app's own crate when it has one. Every command builds with the locked version; running `nsh add` again updates it. A pure TSX app gets hosts built with its nshm, as an app with a crate of its own does, so its development machine and CI need the Rust toolchain ([Building with Rust](./rust.md)).

```json
{
  "modules": ["nsh-omarchy", { "path": "../my-gauge" }]
}
```

A crate that holds several modules keeps each behind a Cargo feature, so an app turns on only those it uses and pays
only their size, permissions and store declarations. `--features` turns them on, and the entry takes the object form:

```sh
nsh add nsh-extra --features camera
```

```json
{
  "modules": [{ "crate": "nsh-extra", "features": ["camera"] }]
}
```

[CLI: nsh add](./cli.md#nsh-add) covers every source form, version requirements and how the lock file is kept.

## nsh-extra

`nsh-extra` holds Native Shell's official modules: features that only some apps need, each a Cargo feature of the
crate, imported from `nsh-extra` in TSX and called as `nsh_extra::<module>` in Rust.

| Feature | TSX | What it does |
|---|---|---|
| `camera` | `Camera` | Takes a photo with the system's camera on iOS and Android |
| `push` | `Push` | Receives remote notifications from any push service on iOS |

```tsx
import { Camera } from "nsh-extra";

if (await Camera.available()) {
  // The photo comes as `Picker.pickImages` gives an image: a JPEG, upright, scaled natively.
  const photo = await Camera.takePhoto({ maxDimension: 2048 });
  if (photo) upload(photo.file);
}
```

`takePhoto` resolves with `null` when the user cancels and rejects with the code `EACCES` when the user does not let
the app use the camera. `available()` is `false` where there is no camera to show (the desktops, the web, a simulator),
and there `takePhoto` rejects with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`. The feature adds what the packaged app needs:
the camera's usage description on iOS (an app sets its own wording with `ios.infoPlist`'s
`NSCameraUsageDescription`) and the query of camera apps on Android.

### Push

`Push` registers the device with the system's push service and hands the app its token, which the app sends to the
push service it uses. Notifications then arrive as events:

```sh
nsh add nsh-extra --features push
```

```tsx
import { Notifications } from "nsh";
import { Push } from "nsh-extra";

if (await Push.available()) {
  // Showing notifications needs the user's permission; background deliveries do not.
  await Notifications.requestPermission();
  // The token can come later than this call (offline): hear it as an event, don't wait on it.
  Push.on("token", ({ token }) => sendTokenToServer(token));
  Push.register().catch((error) => console.warn("not registered", error));
}

Push.on("press", ({ payload }) => openChat(payload));
Push.on("notification", ({ threadId }) => refreshInbox(threadId));
Push.on("background", async ({ payload }) => {
  const changed = await syncInbox(payload);
  return changed ? "newData" : "noData";
});

// While a conversation is open, its notifications show no banner and play no sound.
function openConversation(threadId: string) {
  Push.setForegroundPresentation({ banner: false, sound: false }, { threadId });
}
function closeConversation(threadId: string) {
  Push.setForegroundPresentation(null, { threadId });
}
```

- `register()` resolves with `{ token }`, the string the push service takes (hexadecimal on iOS), and rejects with the
  system's reason when the device is not registered. The system answers once it can reach its servers, so offline the
  promise stays pending: `on("token")` is the dependable path, and it reports a token that changed. Register at every
  launch. `token()` reads the last one.
- `press` reports the notification the user pressed, including the one that launched the app, which waits for the
  first listener. `notification` reports one that arrived while the app is in front. Both carry its `id`, its
  `threadId` and the `payload` the push service sent. A payload is untrusted input.
- `background` reports a delivery that woke the app or reached it in front (on iOS one with `content-available`; a
  visible one with `content-available` arriving in front is reported to `notification` too). The system waits for its
  listeners: return a promise for the work and what it gave, within 25 seconds of the delivery. One that came while
  nothing listened waits for a listener until 5 seconds after it came.
- `setForegroundPresentation(presentation)` decides how notifications show while the app is in front, and
  `setForegroundPresentation(presentation, { threadId })` how one thread's show; `null` takes either back. They are
  decided ahead of the arrival, as the system asks at once, and hold for every surface of the app.
- `closeThread(threadId)` removes the delivered notifications of a thread, but those `Notifications.show` showed;
  `Notifications.close(id)` removes one.
- `available()` is `true` in a standalone iOS app. Elsewhere it is `false`, and `register` and `closeThread` reject
  with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`.

The feature adds the `remote-notification` background mode to the packaged app. The `aps-environment` entitlement comes
from the provisioning profile the app is signed with ([iOS devices](./packaging.md#ios-devices)). Rust code, such as
a push service's SDK binding, uses `nsh_extra::push`: `on_event` hears the same events with the token's raw bytes,
`set_foreground_presentation_decider` decides each notification's presentation natively, and the launch options come
from `nsh_platform_ios::on_application_event` ([Application events](./ios.md#application-events)).

## Writing an nshm

An nshm is an ordinary Cargo library with a `#[native_module("<specifier>")]` entry at its root, the same as an app's own crate: the specifier is what TSX imports, and the function registers what it exports ([Extending a Rust app with TSX](./rust.md#extending-a-rust-app-with-tsx)). A small one:

```toml
# Cargo.toml
[package]
name = "my-gauge"
version = "0.1.0"
edition = "2024"
keywords = ["nshm"]

[dependencies]
nsh-core = "0.6"
```

```rust
// src/lib.rs
use nsh_core::prelude::*;

#[native_module("my-gauge")]
fn native(registry: &mut Registry) {
    registry.register::<Gauge>();
}

#[derive(Clone, PartialEq, Props)]
pub struct GaugeProps {
    /// The value shown, from 0 to 1.
    pub value: Prop<f32>,
}

/// A horizontal gauge.
#[component]
pub fn Gauge(props: GaugeProps) -> impl IntoElement {
    view()
        .h(px(8.0))
        .rounded_full()
        .bg(rgb(0xe5e7eb))
        .child(
            view()
                .h_full()
                .rounded_full()
                .bg(rgb(0x2563eb))
                .w(move || relative(props.value.get())),
        )
}
```

The NSH versions an nshm builds with are its own `nsh-core` requirement (`nsh-core = "0.6"`), which `nsh add` checks against the running `nsh` before anything compiles. Its TypeScript declarations are generated from its Rust types, so apps check their TSX against it as they do against their own crate.

[nsh-omarchy's `src/lib.rs`](https://github.com/huacnlee/nsh-omarchy/blob/main/src/lib.rs) is a complete one: its components, exported to TSX by `#[native_module("nsh-omarchy")]`, with the `Omarchy` theme module.

### What the packaged app needs

An nshm that uses a system feature declares what the packaged app needs in its `Cargo.toml`, under
`[package.metadata.nsh]`, or under a feature's `[package.metadata.nsh.features.<feature>]` when only that feature
needs it. `nsh` adds the entries of every crate the app links, with the features Cargo turns on, to the packages it builds,
and the `Info.plist` keys and Android entries to its development hosts:

```toml
[package.metadata.nsh.features.scanner.ios.infoPlist]
NSCameraUsageDescription = "Scans the codes you point the camera at."

[package.metadata.nsh.features.scanner.android]
permissions = ["CAMERA"]
queries = ['<intent><action android:name="android.media.action.IMAGE_CAPTURE"/></intent>']
application = ['<service android:name="com.example.scanner.Sync" android:exported="false"/>']
```

| Key | Adds |
|---|---|
| `ios.infoPlist` | Keys of the iOS app's `Info.plist`, as `nsh.json`'s `ios.infoPlist` |
| `ios.entitlements` | The iOS app's entitlements |
| `android.permissions` | `<uses-permission>` elements, named as in `nsh.json` (`CAMERA` is `android.permission.CAMERA`) |
| `android.queries` | `<intent>`, `<package>` and `<provider>` elements of the manifest's `<queries>` |
| `android.application` | `<service>`, `<receiver>`, `<provider>`, `<activity>` and `<meta-data>` elements of `<application>`; `${applicationId}` is the app's package |

The app's own `nsh.json` wins: a key it sets keeps its value, and arrays (such as `UIBackgroundModes`) are joined. When
two crates set one key to different values (two usage descriptions of the camera), the app chooses by setting the key
in `ios.infoPlist` or `ios.entitlements`; until it does, packaging for iOS stops with an error naming both crates. Keys NSH
sets itself (the bundle's identifier, version and the like) are not the crates'.

Develop it beside an app with `nsh add --path ../my-gauge`, which reads the directory as it is. Publish it as any crate (`cargo publish`), with the keyword `nshm` so that it can be found on [crates.io](https://crates.io/keywords/nshm), or push it to Git and share its URL.

## Security

An nshm's build scripts and procedural macros run when the app builds, and its code runs with the app's permissions. Any crate in the app's dependency graph can also add system permissions, manifest components and entitlements to the packaged app through `[package.metadata.nsh]`. Review its source and the changes to `nsh.lock` as you review the app's own code.
