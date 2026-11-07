---
title: App Updates
description: "Let a packaged app update itself: from GitHub Releases or your own host, with signed manifests and verified downloads."
order: 21
group: delivery
---

# App Updates

`nsh/updater` lets a packaged app check for a newer version, download it and replace itself. The UI is yours: the module
gives you the state and the actions. It updates the whole app; to deliver only NSH screens to a host app, see
[Signed Packages](/docs/signed-packages).

| Platform | What it does |
| --- | --- |
| macOS, Windows, Linux | Checks, downloads, replaces the app and restarts it |
| iOS, Android | Checks, and gives you the link to send the user to |
| Web | Nothing to do: a deploy updates the app, `checkForUpdate` resolves `null` |
| Every native platform | Runs a newer signed package of the app's code ([below](#update-the-apps-code)) |

## Set the source

From GitHub Releases, in `nsh.json`:

```json
{ "version": "1.2.0", "updater": { "github": "owner/repo" } }
```

From your own host, which needs a signing key, because whoever controls the host would otherwise control what your app
runs (`nsh keygen keys/update`, keep the `.secret` out of your repository):

```json
{
  "version": "1.2.0",
  "updater": {
    "url": "https://dl.example.com/app/latest.json",
    "publicKey": "<contents of keys/update.pub>"
  }
}
```

`publicKey` also works with `github`: unsigned manifests are then refused.

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

`update` has `version`, `notes`, `url` and `canInstall`. `canInstall` is false on iOS and Android, and on the desktop when
the app is installed where the user cannot write, such as by a system package. In a development run `checkForUpdate`
resolves `null`.

`installAndRestart` restarts the app as `Application.restart()` does: this run quits as when the user quits it, and
the new version starts once it has exited. If it fails, the app stays as it was. The
[nsh/updater reference](../modules/nsh-updater.md) has every function, its errors and the `nsh.json` keys.

## Publish a release

`version` in `nsh.json` must be [semver](https://semver.org) and grow with each release. With `"updater"` set,
`nsh package` also writes an archive named for its platform. Package on each platform, then write the manifest:

```sh
nsh package --target linux   # My-App-1.3.0-linux-x86_64.tar.gz
nsh package --target macos   # My-App-1.3.0-macos-aarch64.tar.gz
nsh package --target windows # My-App-1.3.0-windows-x86_64.zip

nsh update-manifest *.tar.gz *.zip \
  --notes CHANGES.md \
  --key keys/update.secret \
  --link ios=https://apps.apple.com/app/id000000000
```

Upload the archives, `latest.json` and `latest.json.sig` (with `--key`) as assets of the GitHub Release, or to your host
next to each other. A release marked as a draft or prerelease on GitHub is not offered.

## Update the app's code

A release that changes only TSX can reach installed apps as a [signed package](/docs/signed-packages) of the app's
code, on every native platform, iOS and Android included, without a new app build. A change to the NSH runtime
itself still needs a new app. Trust the root key that certifies your release keys (`nsh keygen keys/root`, then
`nsh certify`):

```json
{
  "version": "1.2.0",
  "build": 12,
  "updater": {
    "github": "owner/repo",
    "packages": { "roots": ["<contents of keys/root.pub>"] }
  }
}
```

Publish a package with a higher `build` and add it to the manifest:

```sh
nsh publish --key keys/release.secret --cert keys/release.cert --build 13
nsh update-manifest --package .nsh/out/com.example.app-13.nshpkg --key keys/update.secret
```

In the app:

```tsx
import { checkForPackage, downloadPackage, restartToPackage } from "nsh/updater";

const found = await checkForPackage(); // { version, build, notes, size } or null
if (found) {
  await downloadPackage();
  await restartToPackage().catch(() => {}); // desktop: now; iOS and Android: next start
}
```

The app runs the newest verified package from its next start. Its first run is a trial: if it fails to start or the
app crashes within 10 seconds, the app goes back to what it ran before. `runningPackage()` says which package runs.
The [nsh/updater reference](../modules/nsh-updater.md#packages) has the details.

## What is checked

- The manifest's signature, when there is a `publicKey`.
- The manifest's app id: it must be this app's.
- The version: only a newer one is offered, so an old manifest cannot downgrade the app.
- URLs: HTTPS only (plain HTTP only to this machine, for local tests).
- The archive's size and SHA-256.
- The new app: its id and executable, and on macOS its code signature.
- That this run is the package itself: an NSH module running inside another app cannot replace it.
- A package of the app's code: its signature against `packages.roots`, the app id, a build newer than the running one
  and than any the app ran before, the runtime it needs, its size and SHA-256.

On macOS, `nsh package` signs ad hoc, which is fine to test, but Gatekeeper may stop it on other Macs. To distribute, sign
the `.app` with your Developer ID, notarize it, and archive it again under the same name (`tar -czf <archive> <Name>.app`)
before `nsh update-manifest`.

## Rust apps

A native Rust app uses `nsh-updater` directly and supplies its own HTTP transport.
Use `Config::check` to verify the manifest and `Download` to verify the archive.
Then call `Installation::install` and, when it succeeds,
`nsh_core::element::desktop::restart()`: the app quits as `Application.restart()`
makes it, and the new version starts once this process has exited. If `install`
fails, the app stays as it was.

The `Config`, `Manifest`, `Asset` and `Available` records are non-exhaustive.
Construct them with `Default::default()` and assign the fields you need.
