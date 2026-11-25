---
title: App Updates
description: "Let a packaged app update itself: from GitHub Releases or your own host, with signed manifests and verified downloads."
order: 20.5
group: delivery
---

# App Updates

`nsh/updater` lets a packaged app check for a newer version, download it and replace itself. You build the UI. The
module gives you the state and the actions.

This page covers updating the whole app. To update only a packaged app's TSX, see [OTA Updates](/docs/ota-updates).
To deliver only NSH screens to a host app, see [Signed Packages](/docs/signed-packages).

| Platform | What it does |
| --- | --- |
| macOS, Windows, Linux | Checks, downloads, replaces the app and restarts it |
| iOS, Android | Checks, and gives you the link to send the user to |
| Web | Nothing to do. A deploy updates the app, and `checkForUpdate` resolves `null` |
| Every native platform | Runs a newer TSX release as a signed package ([OTA updates](#ota-updates)) |

## Set the source

From GitHub Releases, in `nsh.json`:

```json
{ "version": "1.2.0", "updater": { "github": "owner/repo" } }
```

From your own host, you also need a signing key. Without one, whoever controls the host controls what your app runs.
Create a key with `nsh keygen keys/update`, and keep the `.secret` file out of your repository:

```json
{
  "version": "1.2.0",
  "updater": {
    "url": "https://dl.example.com/app/latest.json",
    "publicKey": "<contents of keys/update.pub>"
  }
}
```

`publicKey` also works with `github`. The app then refuses unsigned manifests.

To offer a beta to users who opt in, add a channel. A channel is a second manifest with a name. Your code passes the
user's choice as that name (`checkForUpdate({ channel: "beta" })`), never as an address. So the package, not the
code, still decides where updates come from ([channels](../modules/nsh-updater.md#channels)).

```json
"updater": {
  "url": "https://dl.example.com/app/latest.json",
  "channels": { "beta": "https://dl.example.com/app/beta/latest.json" },
  "publicKey": "<contents of keys/update.pub>"
}
```

## Use it

```tsx
import { checkForUpdate, downloadUpdate, installAndRestart } from "nsh/updater";
import { openUrl } from "nsh";

const update = await checkForUpdate(); // null when up to date
if (update?.canInstall) {
  await downloadUpdate(({ received, total }) => setProgress(received / (total ?? 1)));
  await installAndRestart();
} else if (update) {
  await openUrl(update.url);
}
```

`update` has `version`, `notes`, `url` and `canInstall`. `canInstall` is false on iOS and Android. It is also false
on the desktop when the user cannot write to the app's install location, for example after a system package install.
In a development run, `checkForUpdate` resolves `null`.

`installAndRestart` restarts the app the same way `Application.restart()` does. The current run quits as if the user
quit it, and the new version starts after it exits. If the install fails, the app stays as it was.

To install without interrupting the user, call `installOnQuit()` instead. NSH verifies the update now and replaces the
app when it quits. The next start opens the new version. The
[nsh/updater reference](../modules/nsh-updater.md) has every function, its errors and the `nsh.json` keys.

## Publish a release

`version` in `nsh.json` must be [semver](https://semver.org) and must increase with each release. When `"updater"` is
set, `nsh package` also writes an archive named after its platform. Package on each platform, then write the manifest:

```sh
nsh package --target linux   # My-App-1.3.0-linux-x86_64.tar.gz
nsh package --target macos   # My-App-1.3.0-macos-aarch64.tar.gz
nsh package --target windows # My-App-1.3.0-windows-x86_64.zip

nsh update-manifest *.tar.gz *.zip \
  --notes CHANGES.md \
  --key keys/update.secret \
  --link ios=https://apps.apple.com/app/id000000000
```

Upload the archives, `latest.json` and `latest.json.sig` (written when you pass `--key`). Put them on the GitHub
Release as assets, or in one folder on your host. The updater skips GitHub releases marked as draft or prerelease.

For notes in more than one language, add one `--notes <tag>=<file>` per language, using a BCP 47 tag, for example
`--notes zh-TW=CHANGES.zh-TW.md`. Each file is written as `notes.<tag>.md` beside `latest.json`, and `latest.json`
records its SHA-256. Upload these files too. When you call `checkForUpdate({ locale })`, `update.notes` holds the
notes for that language, downloaded on their own. If there are none, it holds the untagged notes. Apps on older
runtimes keep showing the untagged notes.

### Smaller downloads

Pass the previous release's archives with `--previous`. `nsh update-manifest` then writes one zstd patch per
platform, from that version to this one, beside the manifest. Upload the patches too.

An app on the previous version downloads the patch instead of the archive. It rebuilds the new app from its installed
files and installs it as usual. It falls back to the whole archive when:

- its installed files differ from the released ones;
- the patch fails;
- the result does not match the SHA-256 in the signed manifest.

Apps on older versions or older runtimes download the full archive as before.

```sh
gh release download v1.2.0 -p 'My-App-1.2.0-*' -D previous
nsh update-manifest *.tar.gz *.zip --previous previous/* --notes CHANGES.md --key keys/update.secret
```

On NSH's own release packages, a patch between adjacent versions is 24–37% of the archive's size. Making one takes
about a minute and up to 0.7 GB of memory per platform, so do it in release CI. Applying one takes well under a second.

## OTA updates

A release that changes only the app's TSX needs no new app. [OTA updates](/docs/ota-updates) deliver it as a signed
package, and installed apps run it from their next start. This works on every native platform, including iOS and
Android. Prefer OTA updates. Ship a new app only when the change needs a newer NSH runtime or native code. One
manifest can carry both, and the app checks for a package first:

```tsx
import { checkForPackage, checkForUpdate, downloadPackage } from "nsh/updater";

if (await checkForPackage()) {
  await downloadPackage(); // runs from the next start
} else {
  const update = await checkForUpdate();
  // …as above
}
```

[OTA Updates](/docs/ota-updates) covers the keys, the `"packages"` setting, releases from CI and what happens on the
device.

## What is checked

- The manifest's signature, when there is a `publicKey`.
- The version: only a newer one is offered, so an old manifest cannot downgrade the app.
- URLs: HTTPS only (plain HTTP only to this machine, for local tests).
- The archive's size and SHA-256.
- The new app: it must be one runnable app, with its executable and, on macOS, its code signature. Its id and
  executable name may differ from the installed app's, so an update can rename the app or move it to a new id. The
  manifest's signature is what ties an update to the app. Apps that share a signing key install whatever their
  `"updater"` source serves.
- That the current run is the packaged app itself. An NSH module running inside another app cannot replace that app.
- An OTA package: its signature against `packages.roots`, the app id, a build newer than the running one and than
  any build the app ran before, the runtime it needs, its size and SHA-256.

On macOS, `nsh package` signs ad hoc. That is fine for testing, but Gatekeeper may block the app on other Macs. To
distribute, sign the `.app` with your Developer ID and notarize it. Then archive it again under the same name
(`tar -czf <archive> <Name>.app`) before you run `nsh update-manifest`.

## Rust apps

A native Rust app uses `nsh-updater` directly and supplies its own HTTP transport.
Use `Config::check` to verify the manifest and `Download` to verify the archive.
Then call `Installation::install`. When it succeeds, call
`nsh_core::element::desktop::restart()`. The app quits the same way as with
`Application.restart()`, and the new version starts after this process exits.
If `install` fails, the app stays as it was.

The `Config`, `Manifest`, `Asset` and `Available` records are non-exhaustive.
Construct them with `Default::default()` and assign the fields you need.
