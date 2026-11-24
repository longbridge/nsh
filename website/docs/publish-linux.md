---
title: Publishing on Linux
description: Ship a Linux NSH app as an AppImage, a Flatpak or on Flathub, with metadata from nsh.json, nsh publish linux, status and doctor.
order: 21
group: delivery
---

# Publishing on Linux

A Linux app reaches users three ways:

| Channel | What you ship | How users get it |
|---|---|---|
| Archive | `<Name>.tar.gz` from `nsh package --target linux` | Download, extract, run `<Name>` |
| AppImage | `<Name>-<arch>.AppImage` | Download one file, make it executable, run it |
| Flathub | A manifest in the `flathub/<id>` repository | GNOME Software, KDE Discover, `flatpak install flathub <id>` |

All three are built from the same package and read the same `nsh.json`. Flathub builds and signs the app itself:
NSH prepares the manifest, you open the pull request.

## Before you start

- The app packages on Linux: `nsh package --target linux` works ([Linux](./linux.md#packaging)).
- `nsh.json` has an `id` in reverse-DNS form. On Flathub it must be a domain you control, or
  `io.github.<user>.<App>` for a GitHub project.
- A square icon in `nsh.json` `icon`, ideally 256×256 or larger: NSH writes it at 256×256 and `doctor` warns when it has to scale it up.
- For AppImage: [appimagetool](https://github.com/AppImage/appimagetool/releases) on `PATH`, or its path in
  `NSH_APPIMAGETOOL`. Without FUSE, set `APPIMAGE_EXTRACT_AND_RUN=1`.
- For a local Flatpak build: `flatpak` and `flatpak-builder`, and the GNOME runtime and SDK
  (`flatpak install flathub org.gnome.Platform//49 org.gnome.Sdk//49`).
- Optional, for `doctor`: `appstreamcli` and `desktop-file-validate` (packages `appstream` and `desktop-file-utils`).

`nsh publish linux doctor` reports each of these.

## Configuration

```json
{
  "name": "Counter",
  "id": "io.github.example.Counter",
  "version": "1.2.0",
  "icon": "icon.png",
  "store": {
    "summary": "Count things up and down",
    "description": "A small counter.\n\nParagraphs are separated by blank lines.",
    "homepage": "https://example.com",
    "license": "MIT",
    "developer": "Example",
    "categories": ["Utility"],
    "keywords": ["count", "tally"],
    "screenshots": ["https://example.com/counter.png"]
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
| `store.summary` | The one-line summary. Flathub: at most 35 characters, no trailing period |
| `store.description` | The long description; blank lines separate paragraphs |
| `store.homepage` | The metainfo `homepage` link |
| `store.license` | `project_license`, an SPDX expression |
| `store.developer` | The developer name shown in software centres |
| `store.categories` | freedesktop categories for the desktop entry and metainfo. Default `Utility` |
| `store.keywords` | Search keywords |
| `store.screenshots` | Image URLs; the first is the default screenshot. Flathub requires at least one |
| `linux.flatpak.runtimeVersion` | The GNOME runtime version. Default: the one NSH is tested with |
| `linux.flatpak.finishArgs` | Replaces the sandbox permissions. Default: Wayland with X11 fallback, IPC, the GPU and the network |

`store` is shared by every store `nsh publish` targets; fill it once.

## AppImage

```sh
nsh package --target linux --format appimage
```

Writes `.nsh/out/<Name>.AppDir` and `.nsh/out/<Name>-<arch>.AppImage`, besides the directory and archive. The
AppImage carries the desktop entry, icon and metainfo, and runs on the system's GTK 4 and WebKitGTK like the archive,
so the target system needs them installed. Attach it to your release; users run:

```sh
chmod +x Counter-x86_64.AppImage
./Counter-x86_64.AppImage
```

## Flatpak

```sh
nsh package --target linux --format flatpak
```

Writes `.nsh/out/flatpak/<id>.yml`, a manifest that installs the package's archive on the GNOME runtime, which
provides GTK 4 and WebKitGTK. When `flatpak-builder` is installed it also builds it and writes
`.nsh/out/<Name>.flatpak`, a single-file bundle to test or hand out:

```sh
flatpak install --user .nsh/out/Counter.flatpak
flatpak run io.github.example.Counter
```

## Flathub

Flathub takes no uploads. Each app is a GitHub repository, `flathub/<id>`, whose manifest names the release archive by
URL and SHA-256; Flathub builds and publishes every change merged there.

### First release

1. `nsh publish linux doctor` until it passes.
2. `nsh package --target linux` and upload `.nsh/out/<Name>.tar.gz` to a release with a stable URL, such as a GitHub
   release asset.
3. Write the manifest:

   ```sh
   nsh publish linux .nsh/out/Counter.tar.gz \
     --url https://github.com/example/counter/releases/download/v1.2.0/Counter.tar.gz
   ```

   This writes `.nsh/out/flathub/<id>/<id>.yml`.
4. Submit it as described in [Flathub's submission guide](https://docs.flathub.org/docs/for-app-authors/submission): a
   pull request to `flathub/flathub` with the manifest. Flathub reviews it and creates `flathub/<id>`.

### Later releases

Bump `version`, package, upload the new archive, then point `--out` at your checkout of `flathub/<id>`:

```sh
nsh publish linux .nsh/out/Counter.tar.gz \
  --url https://github.com/example/counter/releases/download/v1.3.0/Counter.tar.gz \
  --out ../flathub-io.github.example.Counter
```

When the manifest exists, only the archive source marked `# nsh: release archive` is updated (`url` and `sha256`); the
rest stays as you or Flathub's reviewers edited it. Commit and open a pull request on `flathub/<id>`. NSH never opens
pull requests for you.

### Status

```sh
nsh publish linux status [--json]
```

Prints the version Flathub lists for the app and exits 1 until it equals `nsh.json`'s `version`, so CI can wait on a
release going live. An app not on Flathub yet is reported as such.

### Doctor

```sh
nsh publish linux doctor
```

Checks the `id`, the `store` fields Flathub requires (summary length, license, screenshots), that the icon is square (warning when it is under 256×256), validates
the generated metainfo with `appstreamcli` and the desktop entry with `desktop-file-validate` when installed, and finds
`appimagetool` and `flatpak-builder`. It exits non-zero while something Flathub requires is missing.

## Commands

```sh
nsh package --target linux [--format appimage|flatpak]
nsh publish linux <ARCHIVE.tar.gz> --url <URL> [--entry <DIR>] [--out <DIR>]
nsh publish linux status [--entry <DIR>] [--json]
nsh publish linux doctor [--entry <DIR>]
```

| Flag | Description |
|---|---|
| `--url <URL>` | Where the archive is downloaded from (HTTPS) |
| `--entry <DIR>` | The app's directory or entry. Default `.` |
| `--out <DIR>` | Where to write `<id>.yml`. Default `.nsh/out/flathub/<id>` |
| `--json` | `status` only: machine-readable output |

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `appimagetool not found` | Install it on `PATH` or set `NSH_APPIMAGETOOL` |
| AppImage fails with `fuse: failed to open /dev/fuse` | Set `APPIMAGE_EXTRACT_AND_RUN=1`, or install `libfuse2` |
| AppImage starts but no window appears | GTK 4 or WebKitGTK 6.0 is missing on that system; use the Flatpak, which brings them |
| `--format flatpak` writes only the manifest | `flatpak-builder` is not installed; install it, or let Flathub build the manifest |
| `doctor`: summary too long | Flathub allows 35 characters; shorten `store.summary` |
| `doctor`: no screenshots | Add at least one URL to `store.screenshots` |
| Flathub build fails on architecture | The archive is built for this machine's processor (`only-arches`); build and add an archive per architecture |
| `status` says not on Flathub | The first submission isn't merged yet, or the `id` differs from the Flathub repository name |

## What is not covered

- No `.deb`, `.rpm`, Snap or AUR packages.
- No upload to Flathub or opening pull requests: Flathub builds from the manifest you submit.
- AppImage bundles NSH but not GTK 4 or WebKitGTK.
