---
title: Publishing
description: Publish NSH apps to app stores. Linux: release on Flathub with metadata from nsh.json, nsh publish linux, status and doctor.
order: 21
group: delivery
---

# Publishing

`nsh publish <platform>` releases an app on that platform's app store, with the app's identity and listing from
`nsh.json`. Each store has its own section below. Making the files you ship (archives, installers, AppImage, the web
build) is [Packaging](./packaging.md); platforms without a store, such as the web, are covered there only.

## Linux

On Linux the store is Flathub: users install from GNOME Software, KDE Discover or `flatpak install flathub <id>`.
Flathub builds and signs the app itself from a manifest; NSH prepares the manifest from the release archive, you open
the pull request. Archives, AppImage and local Flatpak bundles are packages, not store releases: see
[Packaging](./packaging.md#appimage-and-flatpak).

### Before you start

- The app packages on Linux: `nsh package --target linux` works ([Linux](./linux.md#packaging)).
- `nsh.json` has an `id` in reverse-DNS form. On Flathub it must be a domain you control, or
  `io.github.<user>.<App>` for a GitHub project.
- A square icon in `nsh.json` `icon`, ideally 256×256 or larger: NSH writes it at 256×256 and `doctor` warns when it has to scale it up.
- Optional, for `doctor`: `appstreamcli` and `desktop-file-validate` (packages `appstream` and `desktop-file-utils`).

`nsh publish doctor linux` reports each of these.

### Configuration

```json
{
  "name": "Counter",
  "id": "io.github.example.Counter",
  "version": "1.2.0",
  "icon": "icon.png",
  "store": {
    "locales": {
      "en-US": {
        "subtitle": "Count things up and down",
        "description": "A small counter.\n\nParagraphs are separated by blank lines.",
        "keywords": "count,tally",
        "marketingUrl": "https://example.com"
      }
    },
    "linux": {
      "license": "MIT",
      "developer": "Example",
      "categories": ["Utility"],
      "screenshots": ["https://example.com/counter.png"],
      "archiveUrl": "https://github.com/example/counter/releases/download/v{version}/Counter.tar.gz"
    }
  },
  "linux": {
    "flatpak": {
      "runtimeVersion": "49",
      "finishArgs": ["--share=ipc", "--socket=wayland", "--socket=fallback-x11", "--device=dri"]
    }
  }
}
```

| Field | Used for |
|---|---|
| `name` | The executable, the desktop entry's `Name`, the file names |
| `id` | The desktop entry, metainfo and manifest file names (`<id>.desktop`, `<id>.metainfo.xml`, `<id>.yml`) and `StartupWMClass` |
| `version` | The metainfo release, and what `status` compares with Flathub |
| `icon` | `<id>.png` at 256×256 |
| `store.locales.en-US.subtitle` | The one-line AppStream summary. Flathub: at most 35 characters, no trailing period |
| `store.locales.en-US.description` | The long description; blank lines separate paragraphs |
| `store.locales.en-US.keywords` | Search keywords, comma separated (a list works too) |
| `store.locales.en-US.marketingUrl` | The metainfo `homepage` link, unless `store.linux.homepage` is set |
| `store.linux.license` | `project_license`, an SPDX expression |
| `store.linux.developer` | The developer name shown in software centres |
| `store.linux.categories` | freedesktop categories for the desktop entry and metainfo. Default `Utility` |
| `store.linux.screenshots` | Image URLs; the first is the default screenshot. Flathub requires at least one |
| `store.linux.archiveUrl` | Where each release archive is downloaded from; `{version}` is `nsh.json` `version` |
| `linux.flatpak.runtimeVersion` | The GNOME runtime version. Default: the one NSH is tested with |
| `linux.flatpak.finishArgs` | Replaces the sandbox permissions. Default: Wayland with X11 fallback, IPC, the GPU and the network |

`store` is shared by every store `nsh publish` targets; fill it once ([Store listing](./packaging.md#store-listing)).
The listing texts come from the `en-US` locale (else the first one), and may also be files in `store/en-US/`
(`subtitle.txt`, `description.txt`, `keywords.txt`). Flathub names the app by `id`.

### Flathub

Flathub takes no uploads. Each app is a GitHub repository, `flathub/<id>`, whose manifest names the release archive by
URL and SHA-256; Flathub builds and publishes every change merged there.

#### First release

1. `nsh publish doctor linux` until it passes.
2. `nsh package --target linux` and upload `.nsh/out/<Name>.tar.gz` to a release with a stable URL, such as a GitHub
   release asset.
3. Write the manifest:

   ```sh
   nsh publish linux .nsh/out/Counter.tar.gz
   ```

   The archive's URL is `store.linux.archiveUrl` with the version filled in (or `--url`). This writes `.nsh/out/flathub/<id>/<id>.yml`.
4. Submit it as described in [Flathub's submission guide](https://docs.flathub.org/docs/for-app-authors/submission): a
   pull request to `flathub/flathub` with the manifest. Flathub reviews it and creates `flathub/<id>`.

#### Later releases

Bump `version`, package, upload the new archive, then point `--out` at your checkout of `flathub/<id>`:

```sh
nsh publish linux .nsh/out/Counter.tar.gz --out ../flathub-io.github.example.Counter
```

When the manifest exists, only the archive source marked `# nsh: release archive` is updated (`url` and `sha256`); the
rest stays as you or Flathub's reviewers edited it. Commit and open a pull request on `flathub/<id>`. NSH never opens
pull requests for you.

#### Status

```sh
nsh publish status linux [--json]
```

Prints the version Flathub lists for the app and exits 1 until it equals `nsh.json`'s `version`, so CI can wait on a
release going live. An app not on Flathub yet is reported as such.

#### Doctor

```sh
nsh publish doctor linux
```

Checks the `id`, the store fields Flathub requires (summary length, license, screenshots), that the icon is square (warning when it is under 256×256), validates
the generated metainfo with `appstreamcli` and the desktop entry with `desktop-file-validate` when installed. It exits non-zero while something Flathub requires is missing.

### Commands

```sh
nsh publish linux <ARCHIVE.tar.gz> [--url <URL>] [--entry <DIR>] [--out <DIR>]
nsh publish status linux [--entry <DIR>] [--json]
nsh publish doctor linux [--entry <DIR>]
```

| Flag | Description |
|---|---|
| `--url <URL>` | Where the archive is downloaded from (HTTPS). Default: `store.linux.archiveUrl` |
| `--entry <DIR>` | The app's directory or entry. Default `.` |
| `--out <DIR>` | Where to write `<id>.yml`. Default `.nsh/out/flathub/<id>` |
| `--json` | `status` only: machine-readable output |

### Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `doctor`: summary too long | Flathub allows 35 characters; shorten the `en-US` listing's `subtitle` |
| `doctor`: no screenshots | Add at least one URL to `store.linux.screenshots` |
| Flathub build fails on architecture | The archive is built for this machine's processor (`only-arches`); build and add an archive per architecture |
| `nsh publish linux` asks for `--url` | Set `store.linux.archiveUrl` |
| `status` says not on Flathub | The first submission isn't merged yet, or the `id` differs from the Flathub repository name |

### What is not covered

- No Snap Store or AUR releases.
- No upload to Flathub or opening pull requests: Flathub builds from the manifest you submit.
