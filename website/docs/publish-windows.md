---
title: Publish for Windows
description: "Ship an NSH app to Windows users: the per-user Inno Setup installer and zip from nsh package, code signing with signtool, SmartScreen, download hosting, self-updates and common errors."
order: 19.5
group: publishing
---

# Publish for Windows

`nsh package --target windows` writes everything a Windows release needs for direct download: the app's directory, a
`.zip` of it, and a per-user installer built with Inno Setup. You sign the executables with your code signing
certificate and upload them to your site or a GitHub Release; [App Updates](./app-updates.md) keeps installed copies
up to date.

`nsh publish` has no Windows store upload: NSH does not package MSIX for the Microsoft Store yet.

## Before the first release

On the Windows machine (or CI runner) that packages the app:

1. **Install the tools.** A TSX app needs none; a Rust app's runner is built with Visual Studio Build Tools. For the
   installer, install Inno Setup 6: `winget install JRSoftware.InnoSetup`.
2. **Get a code signing certificate** from a certificate authority, or use a cloud signing service such as Azure
   Trusted Signing. Unsigned executables still run, but SmartScreen warns about an unknown publisher, and some
   antivirus products treat them with suspicion.
3. **Install the Windows SDK** for `signtool.exe` (it comes with the Visual Studio installer's Windows SDK component).

## Configure the app

| Key | In the build | Notes |
|---|---|---|
| `id` | The installer's AppId, the Start menu shortcut's AppUserModelID | Groups the app's windows in the taskbar and identifies it for notifications; a newer installer with the same id upgrades the installed app. Keep it stable. |
| `name` | `<Name>.exe`, the install directory and the Start menu entry | |
| `version` | The installer's version and the Installed apps entry | Must be semver and grow for [self-updates](./app-updates.md). |
| `icon` (or `icon.windows`) | Embedded in `<Name>.exe`, written as `nsh\icon.ico` | Package on Windows: only Windows can embed the icon ([Icons](./packaging.md#icons)). |

## Build

```powershell
nsh package --target windows
```

```text
.nsh\out\<Name>\           the app: <Name>.exe and nsh\
.nsh\out\<Name>.zip        the same, as an archive
.nsh\out\<Name>.iss        the Inno Setup script
.nsh\out\<Name>-setup.exe  the installer, when Inno Setup 6 is installed
```

The installer needs no administrator rights: it installs for the current user in `%LOCALAPPDATA%\Programs\<Name>`,
adds a Start menu shortcut and an Installed apps entry that uninstalls it, and upgrades an installed copy in place,
closing the app first. The app's data is never touched, by upgrades or by the uninstaller.
[Packaging: Windows](./packaging.md#windows) has the details.

## Sign

Sign the app's executable before the installer is built, so the installed app is signed too, then sign the installer.
With a certificate in the Windows certificate store:

```powershell
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /n "Example Inc" .nsh\out\Notes\Notes.exe
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" .nsh\out\Notes.iss
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /n "Example Inc" .nsh\out\Notes-setup.exe
signtool verify /pa .nsh\out\Notes-setup.exe
```

`ISCC.exe` compiles the installer again from the script `nsh package` wrote, now with the signed executable; its path
depends on where Inno Setup is installed (`C:\Program Files (x86)\Inno Setup 6\ISCC.exe` for a machine-wide install).
Make the `.zip` again from the signed directory too if you publish it. Always timestamp (`/tr`): the signature then
stays valid after the certificate expires.

## Distribute it

Upload `Notes-setup.exe` (and the `.zip`, for people who prefer a portable copy) to your site or a GitHub Release.

With [`"updater"`](./app-updates.md#set-the-source) in `nsh.json`, `nsh package` also writes
`<Name>-<version>-windows-<arch>.zip` for self-updates. Put the signed app in it, then write the manifest with the other
platforms' archives ([App Updates](./app-updates.md#publish-a-release)):

```powershell
nsh update-manifest *.zip *.tar.gz --notes CHANGES.md --key keys\update.secret
```

## SmartScreen

SmartScreen decides by reputation, which grows as people download and run files signed with the same certificate. A
new certificate, or a new kind of certificate, can show the "Windows protected your PC" warning for a while even when
the file is signed. Keep signing every release with the same certificate so the reputation carries over.

## Common errors

| Problem | What to do |
|---|---|
| No `<Name>-setup.exe` in `.nsh\out` | Inno Setup 6 is not installed, or not where `nsh package` looks: install it with `winget install JRSoftware.InnoSetup`, or compile `.nsh\out\<Name>.iss` with `ISCC.exe`. |
| The executable shows the default icon | It was packaged on macOS or Linux: package on Windows to embed the icon. |
| `signtool` is not recognized | Run it from a Developer PowerShell, or add the Windows SDK's `bin\<version>\x64` directory to `PATH`. |
| `SignTool Error: No certificates were found that met all the given criteria` | The certificate is not in the store `signtool` searches, or `/n` does not match its subject: list them with `certutil -user -store My`. |
| "Windows protected your PC" on a signed installer | SmartScreen reputation is still building for the certificate (above). |
| The installer asks to close the app | It upgrades in place and closes the running copy first; this is expected. |

## See also

- [Publishing](./publishing.md): every platform, credentials and release CI
- [Packaging: Windows](./packaging.md#windows)
- [Windows](./windows.md): running and embedding on Windows
- [App Updates](./app-updates.md): self-updating apps
