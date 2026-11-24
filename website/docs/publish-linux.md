---
title: Publish for Linux
description: "Ship an NSH app to Linux users: the directory and tar.gz from nsh package, the desktop entry and icon, runtime dependencies, an install script, self-updates and common errors."
order: 19.6
group: publishing
---

# Publish for Linux

`nsh package --target linux` writes the app as a directory and a `.tar.gz` of it, with a desktop entry and the icon.
You upload the archive to your site or a GitHub Release, and users unpack it and install the desktop entry;
[App Updates](./app-updates.md) keeps installed copies up to date.

`nsh publish` has no Linux store upload, and `nsh package` does not write Flatpak, AppImage, `.deb` or `.rpm`
packages yet.

## Before the first release

Package on Linux: the runner is built for the machine it is built on, so package on the processor you publish for
(`x86_64` or `aarch64`), on a distribution old enough for the users you support, as a newer glibc than theirs stops the
app from starting. Ubuntu 24.04 is a good base for current desktops. The build needs the development packages of
GTK 4 (4.14 or later), Pango and WebKitGTK 6.0 ([Linux: Requirements](./linux.md#requirements)).

## Configure the app

| Key | In the package | Notes |
|---|---|---|
| `id` | `<id>.desktop`, `StartupWMClass`, the GTK application id, `<id>.png` | Desktops match the app's windows to its launcher by it; reverse-DNS, as `com.example.notes`. Keep it stable. |
| `name` | The executable `<Name>`, the directory, and the desktop entry's `Name` | |
| `version` | Self-update archives | Must be semver and grow for [self-updates](./app-updates.md). |
| `icon` (or `icon.linux`) | `<id>.png`, named by the desktop entry's `Icon` | A square PNG that fills its canvas ([Icons](./packaging.md#icons)). |

## Build

```sh
nsh package --target linux
```

```text
.nsh/out/<Name>/
  <Name>          the executable
  nsh/            the app's code, assets and app.json
  <id>.desktop    the desktop entry
  <id>.png        the icon
.nsh/out/<Name>.tar.gz
```

[Packaging: Linux](./packaging.md#linux) has the details.

## Distribute it

Upload `<Name>.tar.gz` to your site or a GitHub Release, and tell users which libraries to install. The runtime needs
GTK 4, Pango and WebKitGTK 6.0:

| Distribution | Install |
|---|---|
| Debian, Ubuntu | `sudo apt install libgtk-4-1 libwebkitgtk-6.0-4` |
| Fedora | `sudo dnf install gtk4 webkitgtk6.0` |
| Arch | `sudo pacman -S gtk4 webkitgtk-6.0` |

Installing for one user takes four steps, which an install script can run: unpack the directory somewhere stable, link
the executable onto the `PATH`, and copy the desktop entry and the icon where desktops find them.

```sh
mkdir -p ~/.local/opt ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/256x256/apps
tar -xzf Notes.tar.gz -C ~/.local/opt
ln -sf ~/.local/opt/Notes/Notes ~/.local/bin/Notes
cp ~/.local/opt/Notes/com.example.notes.desktop ~/.local/share/applications/
cp ~/.local/opt/Notes/com.example.notes.png ~/.local/share/icons/hicolor/256x256/apps/
```

The executable finds its `nsh/` directory through the link. Check the desktop entry with
`desktop-file-validate ~/.local/share/applications/com.example.notes.desktop` before you ship it.

With [`"updater"`](./app-updates.md#set-the-source) in `nsh.json`, `nsh package` also writes
`<Name>-<version>-linux-<arch>.tar.gz` for self-updates. Upload it with the manifest
([App Updates](./app-updates.md#publish-a-release)):

```sh
nsh update-manifest *.tar.gz *.zip --notes CHANGES.md --key keys/update.secret
```

## Common errors

| Problem | What to do |
|---|---|
| `error while loading shared libraries: libgtk-4.so.1` (or `libwebkitgtk-6.0.so.4`) | Install the runtime libraries above. |
| `version 'GLIBC_2.39' not found` | The app was packaged on a newer distribution than the user's: package on an older one. |
| The app does not show in the launcher | The desktop entry is not in `~/.local/share/applications`, or its `Exec` does not find the executable: link it onto the `PATH`. |
| The launcher shows a generic icon, or a second icon while the app runs | The icon is not in an icon theme directory, or the desktop entry was renamed: keep the file name `<id>.desktop`, which `StartupWMClass` matches. |
| `exec format error` | The archive was built for another processor: package on `x86_64` and `aarch64` separately. |

## See also

- [Publishing](./publishing.md): every platform, credentials and release CI
- [Packaging: Linux](./packaging.md#linux)
- [Linux](./linux.md): running and embedding on Linux
- [App Updates](./app-updates.md): self-updating apps
