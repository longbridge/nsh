---
title: Publish for macOS
description: "Ship an NSH app to Mac users outside the App Store: Developer ID signing with nsh package, notarization and stapling with Apple's tools, a download archive, self-updates and common errors."
order: 19.4
group: publishing
---

# Publish for macOS

A Mac app is distributed outside the Mac App Store as a signed and notarized `.app`, downloaded from your site or a
GitHub Release. `nsh package --target macos --sign …` builds the app and signs it with your Developer ID;
notarization, stapling and uploading are done with Apple's command line tools and your release host.
[App Updates](./app-updates.md) then keeps installed copies up to date.

`nsh publish` has no macOS store upload: NSH does not package for the Mac App Store yet.

## Before the first release

Once per developer account:

1. **Join the Apple Developer Program.**
2. **Create a Developer ID Application certificate** (Certificates, Identifiers & Profiles → Certificates; only the
   account holder can create one) and install it in the login keychain of the Mac (or CI runner) that packages the
   app. `security find-identity -v -p codesigning` lists it as
   `Developer ID Application: Example Inc (ABCDE12345)`.
3. **Set up notarization credentials** for `notarytool`, stored once in the keychain under a profile name. With an App
   Store Connect team API key (the same kind [iOS uploads](./publish-ios.md#before-the-first-release) use):

   ```sh
   xcrun notarytool store-credentials notes-notary \
     --key /secure/AuthKey_ABC123DEFG.p8 --key-id ABC123DEFG --issuer 69a6de70-0000-0000-0000-000000000000
   ```

   or with an Apple ID and an app-specific password: `xcrun notarytool store-credentials notes-notary --apple-id
   you@example.com --team-id ABCDE12345`.

## Configure the app

| Key | In the build | Notes |
|---|---|---|
| `id` | `CFBundleIdentifier` | Also the app's [storage](../modules/nsh.md#storage) location; keep it stable. |
| `name` | The `.app` name and `CFBundleName` | |
| `version`, `build` | `CFBundleShortVersionString`, `CFBundleVersion` | `version` must be semver and grow for [self-updates](./app-updates.md). |
| `icon` (or `icon.macos`) | `AppIcon.icns` | macOS artwork sits inset on a transparent canvas ([Icons](./packaging.md#icons)). |
| `macos.infoPlist` | `Info.plist` keys | Usage descriptions and other keys ([Info.plist keys](./packaging.md#infoplist-keys-and-entitlements)). |
| `macos.entitlements` | The app's entitlements | Signed into the app with Hardened Runtime; add the exceptions it needs, such as camera access. |
| `LSMinimumSystemVersion` in `macos.infoPlist` | The lowest macOS that opens the app | Default macOS 14 ([Minimum system version](./packaging.md#minimum-system-version)). |
| `macos.extensions` | `Contents/PlugIns/` | Prebuilt `.appex` bundles, signed inside-out ([macOS app extensions](./packaging.md#macos-app-extensions)). |

## Build and sign

```sh
nsh package --target macos --arch universal \
  --sign "Developer ID Application: Example Inc (ABCDE12345)"
```

This writes `.nsh/out/<Name>.app`. With a certificate, NSH signs nested frameworks and extensions first, then the app,
with Hardened Runtime (`--options runtime`) and a secure timestamp (`--timestamp`) as notarization requires, and
verifies the result with `codesign --verify --deep --strict`. `--arch universal` runs on both Apple silicon and Intel
Macs (it needs `rustup target add x86_64-apple-darwin` for a Rust app); leave it out for this Mac's processor only.
Without `--sign` the app is signed ad hoc, which runs on the Mac that built it but is stopped by Gatekeeper elsewhere.

## Notarize and staple

Apple scans a notarized app for malicious content and records it, so Gatekeeper opens it without a warning. Submit a
zip of the app, wait for the result, then staple the ticket to the app so it opens offline too:

```sh
cd .nsh/out
ditto -c -k --keepParent Notes.app Notes-notarize.zip
xcrun notarytool submit Notes-notarize.zip --keychain-profile notes-notary --wait
xcrun stapler staple Notes.app
spctl --assess --type execute --verbose Notes.app   # "accepted, source=Notarized Developer ID"
```

If the submission is `Invalid`, `xcrun notarytool log <submission-id> --keychain-profile notes-notary` lists every
problem.

## Distribute it

Archive the stapled app for download. A zip made with `ditto` keeps its signature and extended attributes:

```sh
ditto -c -k --keepParent Notes.app Notes-1.4.0-macos.zip
```

With [`"updater"`](./app-updates.md#set-the-source) in `nsh.json`, `nsh package` also writes
`<Name>-<version>-macos-<arch>.tar.gz` for self-updates. It holds the app as packaged, before notarization: archive the
stapled app again under the same name, then write the manifest, as [App Updates](./app-updates.md#publish-a-release)
describes:

```sh
tar -czf Notes-1.4.0-macos-aarch64.tar.gz Notes.app
nsh update-manifest *.tar.gz --notes CHANGES.md --key keys/update.secret
```

Upload the archives, `latest.json` and its signature to the GitHub Release or your host. Installed copies check the
manifest, download the archive, verify it and the app's code signature, and replace themselves.

## Common errors

| Problem | What to do |
|---|---|
| `codesign` cannot find the identity | The certificate is not in the keychain the build uses, or its private key is missing: import the `.p12` exported with the key. |
| `errSecInternalComponent` while signing on CI | The keychain is locked: unlock it (`security unlock-keychain`) and allow `codesign` to use the key (`security set-key-partition-list`). |
| Notarization `Invalid`: not signed with a valid Developer ID certificate | The app was signed ad hoc or with an Apple Development certificate: package with `--sign "Developer ID Application: …"`. |
| Notarization `Invalid`: the signature does not include a secure timestamp | Sign through `nsh package --sign`, which adds it; do not re-sign by hand without `--timestamp`. |
| "Notes is damaged and can't be opened" after download | The archive lost the signature (zipped with a tool that drops attributes, or modified after signing): archive with `ditto` or `tar` after stapling. |
| Gatekeeper warns although notarization succeeded | The ticket was not stapled and the Mac is offline, or the downloaded app is not the one notarized: staple, then archive again. |
| The app cannot use the camera, microphone or other protected resources | Add the usage description to `macos.infoPlist` and the Hardened Runtime exception to `macos.entitlements`. |

## See also

- [Publishing](./publishing.md): every platform, credentials and release CI
- [Packaging: macOS](./packaging.md#macos)
- [macOS](./macos.md): running and embedding on macOS
- [App Updates](./app-updates.md): self-updating apps
