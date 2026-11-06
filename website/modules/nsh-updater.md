---
title: nsh/updater
description: "A packaged app updates itself: check the source nsh.json names, download the archive with progress, replace the app and restart."
order: 12.7
---

# nsh/updater

`nsh/updater` checks for a newer version of a packaged app, downloads it and replaces the app. Where updates come
from is set in [`nsh.json`'s `"updater"`](#nshjson), never by the code that calls the module, and every step is checked
in Rust: the manifest's signature, the app id, a newer version, the archive's size and SHA-256. The UI is yours: the
module returns state and does the work.
[App Updates](../docs/app-updates.md) walks through the whole flow, from setting the source to publishing a release.

```tsx
import { checkForUpdate, downloadUpdate, installAndRestart } from "nsh/updater";
```

The module can be imported on every platform:

| Platform | `checkForUpdate` | `downloadUpdate`, `installAndRestart` |
| --- | --- | --- |
| macOS, Windows, Linux (packaged) | Checks the source | Download and replace the app |
| iOS, Android | Checks the source; `url` is the store link (`nsh update-manifest --link`) | Reject: send the user to `url` |
| Web, development runs, apps without `"updater"` | Resolves `null` | Reject |

It needs no capability: `capabilities.network` does not apply, since the app's code cannot choose where these requests
go. Its requests use the platform's own network stack, as `fetch` does.

## checkForUpdate

```ts
function checkForUpdate(): Promise<AppUpdate | null>;

interface AppUpdate {
  version: string;
  notes?: string | null;
  url: string;
  canInstall: boolean;
}
```

Fetches the manifest from the source in `nsh.json` and resolves the newer version it describes, or `null` when the app
is up to date, has no `"updater"`, or runs where apps do not update (the web, `nsh run`).

| Field | Meaning |
| --- | --- |
| `version` | The new version, semver |
| `notes` | What changed, as the manifest's `--notes` file says it |
| `url` | Where the user can get it by hand: the archive, or the store page on iOS and Android. Pass it to [`openUrl`](./nsh.md#openurl) when `canInstall` is false |
| `canInstall` | Whether `downloadUpdate` and `installAndRestart` work in this run: a packaged desktop app installed where the user can write. False on iOS and Android, and under a directory only the system writes (a system package) |

It rejects when the source cannot be reached or its manifest is not acceptable: a bad or missing signature when
`publicKey` is set, another app's id, a URL that is not HTTPS. A manifest of the same or an older version is not an
update, so a stale manifest cannot downgrade the app.

## downloadUpdate

```ts
function downloadUpdate(onProgress?: ((progress: UpdateProgress) => void) | null): Promise<void>;

interface UpdateProgress {
  received: number;
  total?: number | null;
}
```

Downloads the archive of the update the last `checkForUpdate` found into the app's data directory, calling
`onProgress` as bytes arrive (`total` is missing when the server does not say), then checks its size and SHA-256
against the manifest. It rejects with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` when no update was found or `canInstall` is
false, and when the download fails or does not match.

```tsx
const progress = signal(0);
await downloadUpdate(({ received, total }) => progress.set(total ? received / total : 0));
```

## installAndRestart

```ts
function installAndRestart(): Promise<void>;
```

Unpacks the downloaded archive next to the app, checks the new app (its id and executable, and on macOS its code
signature), swaps it in, starts it and exits this run. The promise settles only on failure, which leaves the installed
app as it was: it rejects when nothing was downloaded or the app cannot be replaced. Save what the user would lose
before calling it.

On Linux and macOS the whole directory or `.app` is replaced, keeping the name the user gave it; on Windows the files
are replaced one by one and put back if one fails.

## Calls in progress

One check, download or install runs at a time: calling any of them while another is in progress rejects. Disable the
button while one runs.

```tsx
import { openUrl, signal } from "nsh";
import { checkForUpdate, downloadUpdate, installAndRestart, type AppUpdate } from "nsh/updater";

const update = signal<AppUpdate | null>(null);
const busy = signal(false);

async function run<T>(work: () => Promise<T>) {
  busy.set(true);
  try {
    return await work();
  } finally {
    busy.set(false);
  }
}

export const check = () => run(async () => update.set(await checkForUpdate()));

export const install = () =>
  run(async () => {
    const found = update();
    if (!found) return;
    if (!found.canInstall) return openUrl(found.url);
    await downloadUpdate();
    await installAndRestart();
  });
```

## nsh.json

`"updater"` names one source: `github`, an `owner/repo` whose latest GitHub Release holds `latest.json`, or `url`, the
manifest on a host of your own, which requires `publicKey`. `publicKey` is the contents of an `nsh keygen` `.pub`
file; with it, unsigned manifests are refused.

```json
{
  "id": "com.example.notes",
  "version": "1.2.0",
  "updater": { "github": "example/notes", "publicKey": "<contents of keys/update.pub>" }
}
```

| Key | Type | Meaning |
| --- | --- | --- |
| `github` | string | `owner/repo`: the manifest is the asset `latest.json` of its latest release. Drafts and prereleases are skipped |
| `url` | string | The manifest's HTTPS URL on your own host; archives' relative URLs resolve next to it |
| `publicKey` | string | The Ed25519 key that signs the manifests (`latest.json.sig`). Required with `url` |

`nsh package` checks the key and adds it to the package, so the source is fixed when the app is built. `version` must be
semver. `nsh update-manifest` writes `latest.json` and its signature for one release's archives ([CLI](../docs/cli.md#nsh-update-manifest)).

## Rust

Rust apps (`nsh.json` `"root"`) use the `nsh-updater` crate directly with an HTTP client of their own:
`Config::check` verifies the manifest, `Download` the archive, and `Installation::install_and_launch` replaces the app
and starts it, rolling back if the launch fails ([App Updates](../docs/app-updates.md#rust-apps)).
