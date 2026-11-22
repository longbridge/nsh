---
title: nshm
description: "nshm (NSH modules): packages of components, modules and classes that any Native Shell app installs with nsh add, as Rust crates or pure TSX; versions, the lock file, the shared installation, platform nshm, and how to write and publish one."
order: 5.7
group: start
---

# nshm

An **nshm** (NSH module) is a package that gives Native Shell apps components, modules and classes, as a crate gives Rust programs code. It comes in two kinds:

- A **Rust nshm** is a Rust crate, published on crates.io or Git. An app uses it from Rust and from TSX: the same implementation, native on every platform.
- A **TSX nshm** is TSX published as a Git repository with version tags. The app's bundle includes it, so an app that uses only TSX nshm needs no Rust, and the nshm ships with the app's packages and over-the-air updates.

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

It lists the nshm in `nsh.json` `"dependencies"` by its package name, as `Cargo.toml` does: a version requirement for one from crates.io (`nsh add nsh-omarchy` writes `"^<the version it resolved to>"`, `nsh add nsh-omarchy@0.3` keeps `"0.3"`), and an object with `"github"`, `"git"` or `"path"` for one from elsewhere. A Git nshm's versions are its tags (`v1.4.0` or `1.4.0`), so it takes a version requirement too; `--tag` or `--rev` pins one tag or commit instead.

```json
{
  "dependencies": {
    "nsh-omarchy": "^0.3.2",
    "acme-ui": { "github": "acme/ui", "version": "^1.4" },
    "my-gauge": { "path": "../my-gauge" }
  }
}
```

NSH resolves one version of each nshm the app reaches, including the nshm its nshm depend on: the newest version every requirement accepts. It pins them in `nsh.lock` beside `nsh.json` (commit both with the app), and every command builds with the locked versions, so the app builds the same on every machine until you change them:

```sh
nsh upgrade                # every nshm to the newest version its requirement accepts
nsh upgrade nsh-omarchy    # only this one
nsh remove nsh-omarchy     # out of nsh.json, nsh.lock and the app's crate
```

Each version is installed once per machine, in `~/.nsh/nshms` (`NSH_HOME` moves `~/.nsh`), and every app that uses it shares that copy: `nsh-omarchy/0.3.2/` from crates.io, `github.com/acme/ui/1.4.0/` from a tag, `github.com/acme/ui/<commit>/` from a commit. A directory added with `--path` is read where it is. An app whose Rust comes from nshm only builds them once, optimized, in `~/.nsh/builds`, shared by every app with the same nshm: after the first `nsh run`, the next start skips compiling until an nshm or nsh changes. Builds no app used for 30 days are removed.

A Rust nshm is also added to the `[dependencies]` of the app's own crate when it has one, so the app's Rust uses it directly. An app whose nshm include a Rust one gets hosts built with it, as an app with a crate of its own does, so its development machine and CI need the Rust toolchain ([Building with Rust](./rust.md)). An app whose nshm are all TSX builds with `nsh` alone.

A crate that holds several modules keeps each behind a Cargo feature, so an app turns on only those it uses and pays
only their size, permissions and store declarations. `--features` turns them on, and the entry takes the object form:

```sh
nsh add nsh-extra --features camera
```

```json
{
  "dependencies": { "nsh-extra": { "version": "^0.6.0", "features": ["camera"] } }
}
```

An app listing them in the older `"modules"` array still builds; the next `nsh add` rewrites it as `"dependencies"`, keeping each requirement (a bare name becomes `"*"`) and what `nsh.lock` pinned.

[CLI: nsh add](./cli.md#nsh-add) covers every source form; [nsh upgrade](./cli.md#nsh-upgrade) and [nsh remove](./cli.md#nsh-remove) the others.

## nsh-extra

`nsh-extra` holds Native Shell's official modules: features that only some apps need, each a Cargo feature of the
crate, imported from `nsh-extra` in TSX and called as `nsh_extra::<module>` in Rust.

| Feature | TSX | What it does |
|---|---|---|
| `camera` | `Camera` | Takes a photo with the system's camera on iOS and Android |
| `push` | `Push` | Receives remote notifications from any push service on iOS, and on Android through the service's binding |

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

`Push` registers the device with a push service and hands the app its token, which the app sends to its server.
Notifications then arrive as events. On iOS the service is the system's; Android has none that every device has, so
there the app links a push service's binding, a crate built on `nsh_extra::push` that wraps the service's SDK:

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
  service's reason when the device is not registered. The service answers once it can be reached, so offline the
  promise stays pending: `on("token")` is the dependable path, and it reports a token that changed. Register at every
  launch. `token()` reads the last one.
- `press` reports the notification the user pressed, including the one that launched the app, which waits for the
  first listener. `notification` reports one that arrived while the app is in front. Both carry its `id`, its
  `threadId` and the `payload` the push service sent. A payload is untrusted input.
- `background` reports a delivery that woke the app or reached it in front (on iOS one with `content-available`; a
  visible one with `content-available` arriving in front is reported to `notification` too; on Android one the
  binding hands over). The system waits for its listeners: return a promise for the work and what it gave, within 25
  seconds of the delivery. One that came while nothing listened waits for a listener until 5 seconds after it came.
- `setForegroundPresentation(presentation)` decides how notifications show while the app is in front, and
  `setForegroundPresentation(presentation, { threadId })` how one thread's show; `null` takes either back. They are
  decided ahead of the arrival, as the system asks at once, and hold for every surface of the app. On Android the
  binding posts a notification as they say.
- `closeThread(threadId)` removes the delivered notifications of a thread (on Android a notification group), but those
  `Notifications.show` showed; `Notifications.close(id)` removes one.
- `available()` is `true` in a standalone iOS app and wherever the app links a push service's binding. Elsewhere it is
  `false`, and `register` rejects with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`; so does `closeThread`, except on Android.
- An app that declares `"capabilities"` grants `Push` with
  [`"notifications": true`](./sandbox-and-capabilities.md#notifications), the capability `Notifications` uses. The
  token, the deliveries and how they show belong to the whole app, so every function but `available()` asks for it
  first.

The feature adds the `remote-notification` background mode to the packaged iOS app. The `aps-environment` entitlement
comes from the provisioning profile the app is signed with ([iOS devices](./packaging.md#ios-devices)). On Android it
adds nothing: the push service's SDK brings what its manifest needs. Rust code, such as a push service's SDK binding,
uses `nsh_extra::push`: `on_event` hears the same events with the token's raw bytes,
`set_foreground_presentation_decider` decides each notification's presentation natively, and the launch options on iOS
come from `nsh_platform_ios::on_application_event` ([Application events](./ios.md#application-events)). A binding
implements `PushProvider` (it registers the device with its service), sets it with `set_provider` from its
`#[native_module]` entry, and reports what the service delivers with `provide_token`,
`provide_registration_failure`, `provide_notification` and `provide_background`. A press of a notification the binding
posted opens the app with the payload as the intent's extras, and reaches `press` with them as its `payload` (Android
does not say which notification it was, so it has no `id` or `threadId`).

## An app's own Rust: `ext/`

A TSX app that needs Rust of its own (a scanner, a parser, a database, a native view) keeps it in `ext/`, beside `src/`: `src/` holds the app's TSX, `ext/` its Rust, one nshm crate per directory. The app lists each in `nsh.json` `"dependencies"` by path and imports it from TSX like any other nshm. `nsh init --template mixed` starts an app this way.

```text
my-app/
├── App.tsx                    the entry
├── src/                       the app's TSX modules
├── ext/                       the app's Rust
│   └── my-app-native/
│       ├── Cargo.toml         nsh-core and the crate's own dependencies
│       └── src/lib.rs         #[native_module("my-app-native")]
├── nsh.json                   "dependencies": { "my-app-native": { "path": "ext/my-app-native" } }
└── nsh.lock
```

[Disktree](https://github.com/huacnlee/disktree), a disk usage analyzer, is built this way: its UI is TSX in `App.tsx` and `src/`, and its file system scanner, an SQLite index and file removal are Rust in `ext/disktree-native`:

```json
// nsh.json
{
  "name": "disktree",
  "dependencies": {
    "disktree-native": { "path": "ext/disktree-native" }
  }
}
```

```rust
// ext/disktree-native/src/lib.rs (abridged)
mod scanner;

use nsh_core::prelude::*;

/// Scans, browses and removes entries using the native disk index.
#[module]
pub struct DiskTree;

#[module]
impl DiskTree {
    /// Runs a filesystem operation with a JSON payload and returns its JSON result.
    pub async fn request(route: String, body: String) -> Result<String, String> {
        // An `async fn` returns a Promise to TSX; `blocking` keeps the
        // file system work off the UI thread.
        blocking(move || scanner::dispatch(&route, &body)).await
    }
}

#[native_module("disktree-native")]
fn native(registry: &mut Registry) {
    registry.module::<DiskTree>();
}
```

```ts
// src/runtime.ts
import { DiskTree } from "disktree-native";

const result = JSON.parse(await DiskTree.request("/scan", JSON.stringify({ path })));
```

Why `ext/` rather than a crate at the app's root (`"rust": "."`):

- **The same way as every other nshm.** The app's own Rust and the nshm it installs are listed, locked and built the same way, and moving a crate from `ext/` to crates.io or its own repository later only changes its `nsh.json` entry.
- **Each crate on its own.** It has its own `Cargo.toml`, dependencies and tests (`cargo test` in its directory), and builds once, optimized, in `~/.nsh/builds` until it changes. The app's root stays a TSX project.
- **Small interfaces.** TSX reaches the Rust only through what `#[native_module]` registers, with declarations generated from the Rust signatures (`.nsh/types/<crate>.d.ts`), so the boundary is typed and easy to see in review.

Add another crate with `cargo new --lib ext/<name>`, give it a `#[native_module("<name>")]`, and run `nsh add --path ext/<name>`. Keep UI in TSX and put in Rust what needs native speed, the platform, or the crates.io ecosystem; a crate exports components too when a view must be native ([A Rust nshm](#a-rust-nshm)).

## Writing an nshm

### A Rust nshm

A Rust nshm is an ordinary Cargo library with a `#[native_module("<specifier>")]` entry at its root, the same as an app's own crate: the specifier is what TSX imports, and the function registers what it exports ([Extending a Rust app with TSX](./rust.md#extending-a-rust-app-with-tsx)). A small one:

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
| `ios.infoPlistStrings` | The text of those keys in other languages, by language, as `nsh.json`'s `ios.infoPlistStrings` ([Packaging](./packaging.md#infoplist-text-in-other-languages)) |
| `ios.entitlements` | The iOS app's entitlements |
| `android.permissions` | `<uses-permission>` elements, named as in `nsh.json` (`CAMERA` is `android.permission.CAMERA`) |
| `android.queries` | `<intent>`, `<package>` and `<provider>` elements of the manifest's `<queries>` |
| `android.application` | `<service>`, `<receiver>`, `<provider>`, `<activity>` and `<meta-data>` elements of `<application>`; `${applicationId}` is the app's package |

The app's own `nsh.json` wins: a key it sets keeps its value, and arrays (such as `UIBackgroundModes`) are joined. When
two crates set one key to different values (two usage descriptions of the camera), the app chooses by setting the key
in `ios.infoPlist` or `ios.entitlements`; until it does, packaging for iOS stops with an error naming both crates. Keys NSH
sets itself (the bundle's identifier, version and the like) are not the crates'.

A crate translates the keys the crates set in `ios.infoPlist`, as they word them; a translation of a key no crate sets
with the app's features (one of a feature that is off) is left out with a warning. The app takes the crates'
translations for the languages it has only, those of its `locales` or its own `ios.infoPlistStrings` (an empty `{}`
names a language without text of its own), so a crate may carry many languages without adding any to the app. A key
the app sets itself takes none of the crates' translations, which would say something else than the app's text. Two
crates translating a key differently are settled by the app as above, in `ios.infoPlistStrings`.

Develop it beside an app with `nsh add --path ../my-gauge`, which reads the directory as it is. Publish it as any crate (`cargo publish`), with the keyword `nshm` so that it can be found on [crates.io](https://crates.io/keywords/nshm), or push it to Git and tag its versions.

### A TSX nshm

A TSX nshm is a Git repository whose `nsh.json` describes it: its package name and version, the NSH versions it works with, and what it exports, each specifier with its TSX entry file:

```json
{
  "name": "acme-badge",
  "version": "0.1.0",
  "nsh": ">=0.6",
  "exports": { "acme-badge": "src/index.tsx" },
  "dependencies": { "acme-icons": { "github": "acme/icons", "version": "^1" } }
}
```

```tsx
// src/index.tsx
import { Text, View } from "nsh";

/** A rounded label. */
export function Badge(props: { label: string }) {
  return (
    <View class="rounded-full bg-primary px-2 py-0.5">
      <Text class="text-xs text-primary-foreground">{props.label}</Text>
    </View>
  );
}
```

Its exports are its specifier or start with it (`acme-badge`, `acme-badge/icons`), and its files stay inside it. Apps import it as they import their own modules, and `nsh check` checks their TSX against its source. Publish a version by tagging it (`git tag v0.1.0 && git push --tags`); the tag's version must be the one its `nsh.json` says.

A Rust nshm can export TSX as well, by the same `"exports"` in an `nsh.json` beside its `Cargo.toml`: TSX helpers written on top of its native module. Its name and version are then its crate's.

### Dependencies

An nshm lists the nshm it needs in its `nsh.json` `"dependencies"`, in the app's form: from crates.io or Git, with a version requirement as wide as it works with, so that apps and the other nshm they use can agree on one version. A published nshm cannot depend on a directory. The app's own entry for a package decides where it comes from: an app can take an nshm its nshm depend on from a fork or a local directory while their version requirements still apply.

## Platform nshm

An nshm can provide a target of its own, such as a platform NSH does not build for, with `"platform"` in its `nsh.json` and a binary named `nsh-platform-<name>` in its crate. The crate needs no `#[native_module]` when it gives the app nothing but the target; without one, the app is not linked with it and does not become a Rust app:

```json
{ "name": "nsh-harmony", "version": "0.1.0", "platform": "harmony" }
```

After `nsh add nsh-harmony`, the app's `nsh run`, `nsh dev`, `nsh test` and `nsh package` with `--target harmony` build that binary (in the app's `.nsh/platforms/harmony`, leaving the shared installation as it is) and run it as an [external target](./cli.md#external-targets). A platform installed on `PATH` as `nsh-platform-<name>` serves every app without being an nshm; the app's nshm comes first. [Building a platform](./platform-extensions.md) covers what a platform implements.

## Security

An nshm's build scripts and procedural macros run when the app builds, and its code, Rust or TSX, runs with the app's permissions. Any crate in the app's dependency graph can also add system permissions, manifest components and entitlements to the packaged app through `[package.metadata.nsh]`. Review its source and the changes to `nsh.lock` as you review the app's own code.
