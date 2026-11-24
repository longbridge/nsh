---
title: Publishing
description: "How an NSH app gets from nsh package to its users on every platform: app stores, installers, web hosting, credentials, release CI and what each platform supports today."
order: 18.5
group: publishing
---

# Publishing

Publishing is everything after the app works: building a release, signing it, getting it into an app store or onto a
download page, and keeping it up to date. [Packaging](./packaging.md) covers the first part, `nsh package`, for every
platform; the pages in this group cover the rest, one platform each:

- [iOS](./publish-ios.md): App Store Connect and TestFlight
- [Android](./publish-android.md): Google Play
- [macOS](./publish-macos.md): Developer ID, notarization and direct download
- [Windows](./publish-windows.md): installer, code signing and direct download
- [Linux](./publish-linux.md): archive, desktop entry and direct download
- [Web](./publish-web.md): a static site on any host

## What each platform supports

| Platform | What `nsh package` writes | Where it goes | What `nsh publish` does |
|---|---|---|---|
| iOS | A signed `.ipa` | App Store Connect: TestFlight, then the App Store | Uploads the IPA for TestFlight (`nsh publish ios`) |
| Android | A signed `.aab` (or `.apk`) | Google Play | Uploads the bundle and releases it to the internal testing track (`nsh publish android`) |
| macOS | A signed `.app` | Direct download, notarized with Apple's tools | Nothing yet: notarize and upload it yourself |
| Windows | A directory, `.zip` and an Inno Setup installer | Direct download | Nothing yet: sign and upload it yourself |
| Linux | A directory and `.tar.gz` with a desktop entry | Direct download | Nothing yet: upload it yourself |
| Web | A static site | Any web server or CDN | Nothing: copy the directory to the host |

What the table says "yourself" is a step outside NSH, described on the platform's page with the commands it takes.
Store review submissions, phased rollouts and production releases are made in each store's console; no `nsh` command
makes them.

Apart from the stores, three ways to deliver an app that is already installed work on every native platform:

| Delivers | How | Page |
|---|---|---|
| A new version of the whole app (macOS, Windows, Linux) | `nsh/updater`, from GitHub Releases or your host | [App Updates](./app-updates.md) |
| New TSX for an installed app, without a store release | Signed packages the app downloads | [OTA Updates](./ota-updates.md) |
| NSH screens inside another app | Signed packages a host app verifies | [Signed packages](./signed-packages.md) |

`nsh publish <ENTRY> --key … --cert …`, without a platform, makes such a signed package; `nsh publish ios` and
`nsh publish android` are the store uploads described here.

## One release, step by step

1. **Set the version in `nsh.json`.** `version` is what users see (`1.4.0`); `build` is a whole number that must grow
   with every upload to a store, on every platform (`CFBundleVersion` on Apple platforms, `versionCode` on Android).
   Both can also be given on the command line (`--app-version`, `--build`), which is handy when CI numbers builds.
2. **Package on each platform.** iOS and macOS need a Mac with Xcode; Windows packages are built on Windows (only
   Windows can embed the icon); Linux packages on Linux; Android and the web anywhere. See
   [Packaging](./packaging.md).
3. **Sign.** iOS, Android and macOS sign as they package when given an identity or keystore; a Windows executable is
   signed afterwards with `signtool`. Each platform's page has the details.
4. **Publish.** Upload to the store, or attach the files to a release.
5. **Follow it up.** Processing, review and the move from testing to production happen in the store's console.

```json
{
  "name": "Notes",
  "id": "com.example.notes",
  "version": "1.4.0",
  "build": 42,
  "icon": "icon.png"
}
```

The `id` is the bundle identifier on Apple platforms and the package name on Android; the store records must use the
same one. Once an app is in a store under an id, it cannot change.

## Credentials

Store credentials (App Store Connect API keys, Google Play service account keys, signing keystores and certificates,
passwords) never go into `nsh.json` or the repository. `nsh` reads them from flags or environment variables, which a
release CI fills from its secrets:

| Platform | What it needs | How `nsh` gets it |
|---|---|---|
| iOS | Apple Distribution certificate and an App Store provisioning profile | `nsh package --sign … --profile …`; the certificate in the keychain |
| iOS | App Store Connect team API key: issuer ID, key ID, `.p8` file | `NSH_APP_STORE_ISSUER_ID`, `NSH_APP_STORE_KEY_ID`, `NSH_APP_STORE_PRIVATE_KEY` |
| Android | Upload keystore and its password | `nsh package --keystore … --key-alias …`, `NSH_KEYSTORE_PASSWORD` |
| Android | Google Cloud service account JSON key | `NSH_PLAY_CREDENTIALS` or `--credentials` |
| macOS | Developer ID Application certificate; an App Store Connect API key or app-specific password for notarization | `nsh package --sign …`; `xcrun notarytool` |
| Windows | A code signing certificate | `signtool` |

## Automate it

Store uploads are plain commands, so a release CI runs them like any other step: a tag triggers the workflow, each
platform packages on a runner of its own system, and the uploads run last. A sketch for GitHub Actions:

```yaml
on:
  push:
    tags: ["v*"]

jobs:
  android:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      # Install nsh, the Android SDK and a JDK, and write the keystore from a secret, then:
      - run: nsh package --target android --format aab --keystore release.jks --key-alias upload --build ${{ github.run_number }}
        env:
          NSH_KEYSTORE_PASSWORD: ${{ secrets.KEYSTORE_PASSWORD }}
      - run: nsh publish android .nsh/out/Notes.aab
        env:
          NSH_PLAY_CREDENTIALS: ${{ runner.temp }}/play.json

  ios:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      # Install nsh, import the certificate into a keychain and write the profile and .p8 key from secrets, then:
      - run: nsh package --target ios --sign "Apple Distribution: Example Inc (ABCDE12345)" --profile AppStore.mobileprovision --build ${{ github.run_number }}
      - run: nsh publish ios .nsh/out/Notes.ipa
        env:
          NSH_APP_STORE_APP_ID: ${{ vars.APP_STORE_APP_ID }}
          NSH_APP_STORE_ISSUER_ID: ${{ secrets.APP_STORE_ISSUER_ID }}
          NSH_APP_STORE_KEY_ID: ${{ secrets.APP_STORE_KEY_ID }}
          NSH_APP_STORE_PRIVATE_KEY: ${{ runner.temp }}/AuthKey.p8
```

Using the run number as `build` keeps it growing without editing `nsh.json` for each release. Pin the NSH version the
workflow installs, so a release builds with the same NSH every time.

## See also

- [Packaging](./packaging.md): `nsh package` for every platform
- [CLI](./cli.md): every command and flag
- [App Updates](./app-updates.md): apps that update themselves
- [OTA Updates](./ota-updates.md): new TSX without a store release
