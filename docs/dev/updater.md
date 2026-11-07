# App updates

`nsh/updater` lets a packaged app replace itself. The rules are in `crates/nsh-updater` (no network, no UI), the JS face
in `crates/nsh-core/src/runtime/updater.rs`, the toolchain's side in `crates/nsh-cli/src/updater.rs`; the user guide is
`website/docs/app-updates.md`, the API reference `website/modules/nsh-updater.md`.

## Flow

1. `nsh.json`'s `"updater"` (`github` or `url`, `publicKey`) is checked by `nsh package`, which adds it with the app's
   id and version to the package's assets as `nsh-updater.json`, and leaves `<name>-<version>-<os>-<arch>.<ext>`.
2. `nsh update-manifest` writes `latest.json` and `latest.json.sig` for one version's archives.
3. `checkForUpdate` reads the asset, gets the manifest, checks the signature, the id and that the version is newer.
4. `downloadUpdate` streams the asset into `<data dir>/updates`, checking its size and SHA-256.
5. `installAndRestart` unpacks next to the app, checks the new app, swaps it in, then restarts the app as
   `Application.restart()` does (`DesktopCommand::Restart`): this run quits, and the new version starts once it has
   exited (`nsh_core::restart`).

HTTP is the platform's `Network`, as `fetch`'s, through `UpdaterHost`, which the engine gives `System`: no second HTTP
or TLS stack, and it works on iOS and Android, where `ureq` is not linked. These requests are not subject to the app's
`capabilities.network`: the app's code cannot choose where they go.

## Decisions

- **The source comes from the package, not the caller.** Otherwise app code, or a plugin, could point the update at
  anything.
- **A plugin cannot replace its host.** `Installation::current` needs the executable beside an `app.json` of the id and
  a `nsh-updater.json` with the same bytes the module read. A plugin has other assets, and if it had the same bytes it
  would only update from the host's own source.
- **Signing.** `SHA-256` in the manifest does not help if the host is compromised, since the manifest comes from the
  same place. A custom host therefore needs `publicKey`; GitHub may have one. The signature has its own context, so a
  signature over a manifest is not valid as anything else signed by a `nsh keygen` key, or the other way around.
- **`releases/latest/download/`, not the GitHub API.** No 60 requests per hour limit, and it skips drafts and
  prereleases. The archives' relative URLs resolve next to it.
- **Swapping.** Linux and macOS rename the whole directory or `.app`, which keeps a user's own name for it. Windows
  cannot rename a directory holding the running executable, but can rename the executable: `swap_entries` swaps entry
  by entry, undoes on failure, and the next check removes what was still in use.
- **Restarting** is `Application.restart()`'s path, the one way an NSH app starts again: the app quits as the user's
  quit does (storage written, windows closed), and the new version starts only after this process has exited, so the
  two never run side by side. Everything that can refuse the new app (its id, its executable, its signature) is
  checked before the swap; after it the app is the new one, whether or not it starts.
- **macOS** checks the new app with `codesign --verify --deep --strict` and starts it with `open -n`. Whether Gatekeeper
  accepts it is the developer's signing and notarization.
- **No Cargo feature.** `zip`, `tar`, `semver` and `nsh-updater` add about 125 KB to a 51 MB Linux runner (`nm -S`), and
  `sha2`, `ed25519-dalek` and `flate2` are linked already. `nsh-updater` is not a dependency in WebAssembly, where
  `checkForUpdate` resolves `null`.
- **Rust apps** (`nsh.json` `"root"`) have no JS modules: they use `nsh-updater` directly and bring their own HTTP.
  After verifying the archive, they call `Installation::install`, then `nsh_core::element::desktop::restart()` as
  `installAndRestart` does.
- **Concurrent calls** are refused while a check, download or install is in progress. Downloads use exclusively
  created unique files, so a retry cannot truncate a previously verified archive.
- **Public records** (`Config`, `Manifest`, `Asset`, `Available`) are non-exhaustive and implement `Default`, so later
  fields do not require applications to change exhaustive struct literals.

## Limits

- iOS and Android only check and report a link (`--link`): stores do not allow replacing the binary, and installing an
  APK needs a permission an SDK must not declare for its hosts.
- Downgrades are refused by the version check; a signed manifest of an older version is therefore harmless.
- `nsh#914`: desktop TLS uses the webpki roots, so a custom host on a certificate chain they lack fails to connect.
