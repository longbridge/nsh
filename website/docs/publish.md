---
title: Publishing
description: "How an NSH app gets from nsh package to its users on every platform: app stores, installers, web hosting, credentials, release CI and what each platform supports today."
order: 18.5
group: publishing
---

# Publishing

Publishing is everything after the app works: building a release, signing it, getting it into an app store or onto a
download page, and keeping it up to date. [Packaging](./packaging.md) covers the first part, `nsh package`, for every
platform; this page covers the rest, with one section per platform:

- [iOS](#publish-to-the-app-store): App Store Connect and TestFlight
- [Android](#publish-to-google-play): Google Play
- [macOS](#publish-for-macos): Developer ID, notarization and direct download
- [Windows](#publish-for-windows): installer, code signing and direct download
- [Linux](#publish-for-linux): archive, desktop entry and direct download
- [Web](#publish-on-the-web): a static site on any host

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

## Publish to the App Store

An iOS app reaches users through App Store Connect: you upload a build, testers get it in TestFlight, and once Apple
approves it, the App Store offers it. `nsh package --target ios` builds and signs the IPA; `nsh publish ios` uploads it
to App Store Connect. Everything after the upload (TestFlight groups, the store listing, review and release) is done in
App Store Connect.

Both commands run on macOS with Xcode installed, for Rust, Rust + TSX and TSX apps alike.

### Before the first release

Once per app:

1. **Join the Apple Developer Program** with the account (person or organization) that will publish the app.
2. **Register the bundle ID.** In Certificates, Identifiers & Profiles, add an App ID whose bundle ID is the app's
   `id` from `nsh.json` (`com.example.notes`), with the capabilities the app uses (push notifications, app groups…).
3. **Create a distribution certificate.** An *Apple Distribution* certificate, installed in the login keychain of the
   Mac (or CI runner) that packages the app. `security find-identity -v -p codesigning` lists it as
   `Apple Distribution: Example Inc (ABCDE12345)`.
4. **Create an App Store provisioning profile** for that App ID and certificate, and download the
   `.mobileprovision` file. Keep it outside the repository.
5. **Create the app in App Store Connect** (My Apps → New App) with the same bundle ID. Its numeric **Apple ID**, shown
   under App Information, is the app id `nsh publish ios` takes.
6. **Create a team API key** in App Store Connect under Users and Access → Integrations → App Store Connect API, with
   the Developer, App Manager or Admin role. Note its **Key ID** and the **Issuer ID** shown above the list, and
   download the `.p8` private key; Apple lets you download it only once.

### Configure the app

The keys of `nsh.json` that end up in the iOS build:

```json
{
  "name": "Notes",
  "id": "com.example.notes",
  "version": "1.4.0",
  "build": 42,
  "icon": "icon.png",
  "ios": {
    "infoPlist": {
      "NSCameraUsageDescription": "Notes uses the camera to scan documents.",
      "ITSAppUsesNonExemptEncryption": false
    },
    "entitlements": {
      "com.apple.security.application-groups": ["group.com.example.notes"]
    }
  }
}
```

| Key | In the build | Notes |
|---|---|---|
| `id` | `CFBundleIdentifier` | Must match the App ID, the profile and the App Store Connect app. |
| `name` | `CFBundleDisplayName` | The name under the icon. The store name is set in App Store Connect. |
| `version` | `CFBundleShortVersionString` | The version users see. A new version needs a new version record in App Store Connect before it can be submitted for review. |
| `build` | `CFBundleVersion` | Must be higher than every build uploaded for that version. |
| `icon` | The app icon | A square 1024×1024 PNG without transparency, as the App Store requires ([Icons](./packaging.md#icons)). |
| `ios.infoPlist` | `Info.plist` keys | Usage descriptions for every protected resource the app touches, export compliance and more ([Info.plist keys](./packaging.md#infoplist-keys-and-entitlements)). |
| `ios.entitlements` | The app's entitlements | Checked against the provisioning profile while packaging. |
| `ios.privacyManifest` | `PrivacyInfo.xcprivacy` | The privacy manifest App Store Connect checks ([Privacy manifest](./packaging.md#privacy-manifest)). |
| `ios.extensions` | Embedded app extensions | Each signed with its own profile ([App extensions](./packaging.md#app-extensions)). |

Setting `ITSAppUsesNonExemptEncryption` to `false` (when the app only uses the encryption built into the system, such as
HTTPS) answers TestFlight's export compliance question once, instead of for every build.

### Build the IPA

```sh
nsh package --target ios \
  --sign "Apple Distribution: Example Inc (ABCDE12345)" \
  --profile /secure/AppStore.mobileprovision
```

This writes `.nsh/out/<Name>.ipa`. NSH checks that the executable has an `arm64` slice, writes the Xcode and SDK
build metadata into `Info.plist`, embeds the profile, checks that the profile covers the app's `id` and its
entitlements, signs the extensions and then the app with `codesign`, and zips it as `Payload/<Name>.app`. Without
`--sign` and `--profile` it writes an unsigned `.app` that cannot be uploaded. [Packaging: iOS
devices](./packaging.md#ios-devices) has the details.

To check the result before uploading:

```sh
unzip -q -o .nsh/out/Notes.ipa -d /tmp/notes-ipa
codesign --verify --strict --verbose=2 /tmp/notes-ipa/Payload/Notes.app
codesign -d --entitlements - /tmp/notes-ipa/Payload/Notes.app
```

### Upload to TestFlight

`nsh publish ios <IPA>` uploads the IPA to App Store Connect with Apple's
[build uploads API](https://developer.apple.com/documentation/appstoreconnectapi/build-uploads):

```sh
export NSH_APP_STORE_APP_ID=1234567890
export NSH_APP_STORE_ISSUER_ID=69a6de70-0000-0000-0000-000000000000
export NSH_APP_STORE_KEY_ID=ABC123DEFG
export NSH_APP_STORE_PRIVATE_KEY=/secure/AuthKey_ABC123DEFG.p8
nsh publish ios .nsh/out/Notes.ipa
```

| Flag | Environment variable | Meaning |
|---|---|---|
| `--app-id <ID>` | `NSH_APP_STORE_APP_ID` | The numeric Apple ID of the App Store Connect app. |
| `--issuer-id <ID>` | `NSH_APP_STORE_ISSUER_ID` | The team API key's issuer ID. |
| `--key-id <ID>` | `NSH_APP_STORE_KEY_ID` | The API key's ID. |
| `--private-key <PATH>` | `NSH_APP_STORE_PRIVATE_KEY` | The `.p8` private key file. |

All four are required; a flag overrides its variable. Keep them in your shell profile or the CI's secrets, never in
`nsh.json` or the repository.

What the command does:

1. Reads the bundle ID, version and build from the IPA's `Info.plist`, and checks that the IPA targets iOS devices.
2. Signs a short-lived ES256 token with the API key, and checks that the IPA's bundle ID is the App Store Connect app's.
3. Creates a build upload, streams the IPA in the parts Apple asks for, and commits its SHA-256 checksum.
4. Prints the upload ID and the app's TestFlight page.

Success means Apple accepted the file for processing. Processing takes from a few minutes to an hour; Apple then
checks the signature, the profile, the icons and `Info.plist`, and emails the account holder about any problem.

### After the upload

In App Store Connect:

- **TestFlight**: once processed, the build is available to internal testers (members of your team) at once. External
  testers need a group and a Beta App Review of the first build of each version.
- **The App Store listing**: name, subtitle, description, keywords, screenshots for each device size, the privacy
  details (App Privacy) and the age rating. They are entered in App Store Connect.
- **Review**: add the build to the version, then Add for Review and Submit. After approval, the version is released
  manually, automatically, or on a date, as the version's release option says.

### Common errors

| Message | What to do |
|---|---|
| `nsh publish ios runs on macOS` | Upload from a Mac or a macOS CI runner. |
| `IPA must contain exactly one Payload/*.app/Info.plist` | Pass the `.ipa` that `nsh package --target ios --sign … --profile …` wrote, not the `.app` or a simulator build. |
| `IPA must target iOS devices` | The IPA is a simulator build: package with `--target ios`, not `--target ios-simulator`. |
| `IPA bundle identifier does not match --app-id` | The app id belongs to another App Store Connect app, or `nsh.json` `id` differs from the app's bundle ID. |
| `expected a PKCS#8 App Store Connect private key (.p8)` | `--private-key` names the wrong file: give the `AuthKey_<KEY_ID>.p8` downloaded from App Store Connect. |
| `App Store Connect HTTP 401: …` | The issuer ID, key ID and key do not belong together, or the key was revoked. |
| `App Store Connect HTTP 403: …` | The key's role cannot upload builds: use Developer, App Manager or Admin. |
| `App Store Connect HTTP …` saying the build already exists | A build with this version and build number was uploaded before: raise `build` in `nsh.json` and package again. |
| `upload … did not finish; check App Store Connect before retrying` | The connection dropped. Each run starts a new upload; check the printed upload ID in App Store Connect, then run it again. |
| `Apple rejected the uploaded file; check App Store Connect` | Apple refused the file once uploaded; the reason is in App Store Connect and in Apple's email. |
| The profile does not cover the app's id (while packaging) | Create the App Store profile for the app's bundle ID, or a wildcard ID that covers it. |

Problems Apple finds while processing (a missing usage description, an icon with transparency, an entitlement the
profile lacks) arrive by email and show on the build in App Store Connect; fix them in `nsh.json`, raise `build`, and
package and upload again.

## Publish to Google Play

Google Play takes Android App Bundles (`.aab`). `nsh package --target android --format aab` builds and signs one with
your upload key, without Gradle; `nsh publish android` uploads it with the Google Play Developer API and releases it
to the **internal testing** track. Promoting it to closed, open or production testing, the store listing and review
are done in Play Console.

Both commands run on macOS, Linux and Windows.

### Before the first release

Once per app:

1. **Create a Google Play developer account** and, in Play Console, **create the app** (Create app). Its package name
   is fixed by the first bundle you upload, and must be the app's `id` from `nsh.json`.
2. **Create an upload key**, which signs every bundle you upload:

   ```sh
   keytool -genkeypair -v -keystore release.jks -alias upload -keyalg RSA -keysize 2048 -validity 10000
   ```

   Keep `release.jks` and its password outside the repository, backed up. With Play App Signing (the default for new
   apps), Google signs what devices install with its own app signing key, and the upload key can be reset through
   Play Console support if it is lost.
3. **Create a service account** for uploads: in Google Cloud, pick or create a project, enable the **Google Play
   Android Developer API**, create a service account and download a JSON key for it.
4. **Give the service account access**: in Play Console, Users and permissions → Invite new users, enter the service
   account's email, and grant it the app with permission to release to testing tracks.
5. **Make the first release in Play Console.** Until the app has had one release made in Play Console, Google Play
   accepts only draft releases through the API: upload the first bundle with `nsh publish android --draft` and roll the
   draft out in Play Console.

### Configure the app

```json
{
  "name": "Notes",
  "id": "com.example.notes",
  "version": "1.4.0",
  "build": 42,
  "icon": {
    "android": {
      "foreground": "icon-foreground.png",
      "background": "#1F6FEB",
      "monochrome": "icon-monochrome.png"
    }
  },
  "android": {
    "targetSdk": 35,
    "permissions": ["android.permission.CAMERA"]
  }
}
```

| Key | In the build | Notes |
|---|---|---|
| `id` | The package name | The app's identity on Google Play; it cannot change once uploaded. |
| `name` | `android:label` | The name under the icon. The store name is set in Play Console. |
| `version` | `versionName` | The version users see. |
| `build` | `versionCode` | Must be higher than every version code uploaded for the app, on any track. |
| `icon` | The launcher icon | One PNG, or an adaptive icon with a monochrome layer for themed icons ([Icons](./packaging.md#icons)). |
| `android.targetSdk` | `targetSdkVersion` | Default 35. Google Play requires a recent level for new apps and updates ([Target API level](./packaging.md#target-api-level)). |
| `android.permissions` | `<uses-permission>` | Declared permissions; Play Console asks for a declaration for some of them ([Permissions](./packaging.md#permissions)). |

### Build the bundle

```sh
NSH_KEYSTORE_PASSWORD=… nsh package --target android --format aab \
  --keystore release.jks --key-alias upload
```

This writes `.nsh/out/<Name>.aab`, signed with `jarsigner` from the JDK. It holds native code for `arm64-v8a` and
`armeabi-v7a`; Google Play builds from it the APKs each device installs. `NSH_KEYSTORE_PASSWORD` keeps the password out
of the shell history (`--keystore-password` works too). Without `--keystore` the bundle is signed with the debug key,
which Google Play rejects. [Packaging: Android](./packaging.md#android) has the requirements (Android SDK, NDK for
Rust apps, JDK).

A release bundle carries `libnsh.so` without its symbols; the complete libraries are written beside the runtime in
`symbols/<abi>/libnsh.so`. Upload them in Play Console (the bundle's Downloads → native debug symbols) to read native
crash stacks in Android vitals.

To try a bundle on a device before uploading, use [bundletool](https://github.com/google/bundletool):
`bundletool build-apks --bundle .nsh/out/Notes.aab --output notes.apks --connected-device`, then
`bundletool install-apks --apks notes.apks`.

### Upload to the internal testing track

```sh
export NSH_PLAY_CREDENTIALS=/secure/play-service-account.json
nsh publish android .nsh/out/Notes.aab
```

```sh
nsh publish android <BUNDLE.aab> [--entry <DIR>] [--package <NAME>] [--credentials <KEY.json>] [--draft]
```

| Flag | Default | Meaning |
|---|---|---|
| `--entry <DIR>` | `.` | The app's directory or entry, whose `nsh.json` `id` is the package name. |
| `--package <NAME>` | `nsh.json` `id` | The package name on Google Play. |
| `--credentials <KEY.json>` | `NSH_PLAY_CREDENTIALS` | The service account's JSON key. |
| `--draft` | off | Release as a draft, which Google Play requires until the app's first release is made in Play Console. |

What the command does, in one edit of the app on Google Play:

1. Signs in as the service account.
2. Opens an edit, uploads the bundle, and reads the version code Google Play found in it.
3. Sets the internal testing track's release to that version code (completed, or a draft with `--draft`).
4. Commits the edit. If any step fails, the edit is deleted, so nothing is half released.

It prints the version code that is now on the internal testing track. Internal testers (up to 100, from the tester list
in Play Console) get the update from Google Play within minutes.

### After the upload

In Play Console:

- **Promote the release**: Testing → Internal testing → the release → Promote release, to closed testing, open
  testing or production. A new personal developer account must run a closed test before production access is granted.
- **The store listing**: app name, short and full description, icon, feature graphic, phone and tablet screenshots.
- **App content**: privacy policy, data safety, ads, content rating and target audience, required before the first
  production release.
- **Review and rollout**: releases to closed, open and production tracks are reviewed; a production release can be
  rolled out to a percentage of users and raised later.

### Common errors

| Message | What to do |
|---|---|
| `nsh.json has no \`id\`: give the app's package name with --package` | Run the command in the app's directory, pass `--entry`, or give `--package`. |
| `the service account key …` | The JSON key file is missing or unreadable: check `NSH_PLAY_CREDENTIALS` or `--credentials`. |
| `Invalid JWT Signature.` | The service account key was deleted or rotated in Google Cloud: download a new JSON key. |
| `could not open an edit on Google Play` with HTTP 403 | The service account has no access to the app in Play Console, or the Google Play Android Developer API is not enabled in its project. |
| `could not open an edit on Google Play` with HTTP 404 | No app with this package name exists in Play Console: create it, or fix `id`/`--package`. |
| `Google Play refused the bundle` … `version code that has already been used` | Raise `build` in `nsh.json` and package again. |
| `Google Play refused the bundle` … signed with the wrong key | The bundle was signed with the debug key or another upload key: package with the upload keystore registered for the app. |
| `Google Play refused the internal testing release` … draft | The app has had no release made in Play Console yet: pass `--draft` and roll it out in Play Console. |

## Publish for macOS

A Mac app is distributed outside the Mac App Store as a signed and notarized `.app`, downloaded from your site or a
GitHub Release. `nsh package --target macos --sign …` builds the app and signs it with your Developer ID;
notarization, stapling and uploading are done with Apple's command line tools and your release host.
[App Updates](./app-updates.md) then keeps installed copies up to date.

`nsh publish` has no macOS store upload: NSH does not package for the Mac App Store yet.

### Before the first release

Once per developer account:

1. **Join the Apple Developer Program.**
2. **Create a Developer ID Application certificate** (Certificates, Identifiers & Profiles → Certificates; only the
   account holder can create one) and install it in the login keychain of the Mac (or CI runner) that packages the
   app. `security find-identity -v -p codesigning` lists it as
   `Developer ID Application: Example Inc (ABCDE12345)`.
3. **Set up notarization credentials** for `notarytool`, stored once in the keychain under a profile name. With an App
   Store Connect team API key (the same kind [iOS uploads](#before-the-first-release) use):

   ```sh
   xcrun notarytool store-credentials notes-notary \
     --key /secure/AuthKey_ABC123DEFG.p8 --key-id ABC123DEFG --issuer 69a6de70-0000-0000-0000-000000000000
   ```

   or with an Apple ID and an app-specific password: `xcrun notarytool store-credentials notes-notary --apple-id
   you@example.com --team-id ABCDE12345`.

### Configure the app

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

### Build and sign

```sh
nsh package --target macos --arch universal \
  --sign "Developer ID Application: Example Inc (ABCDE12345)"
```

This writes `.nsh/out/<Name>.app`. With a certificate, NSH signs nested frameworks and extensions first, then the app,
with Hardened Runtime (`--options runtime`) and a secure timestamp (`--timestamp`) as notarization requires, and
verifies the result with `codesign --verify --deep --strict`. `--arch universal` runs on both Apple silicon and Intel
Macs (it needs `rustup target add x86_64-apple-darwin` for a Rust app); leave it out for this Mac's processor only.
Without `--sign` the app is signed ad hoc, which runs on the Mac that built it but is stopped by Gatekeeper elsewhere.

### Notarize and staple

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

### Distribute it

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

### Common errors

| Problem | What to do |
|---|---|
| `codesign` cannot find the identity | The certificate is not in the keychain the build uses, or its private key is missing: import the `.p12` exported with the key. |
| `errSecInternalComponent` while signing on CI | The keychain is locked: unlock it (`security unlock-keychain`) and allow `codesign` to use the key (`security set-key-partition-list`). |
| Notarization `Invalid`: not signed with a valid Developer ID certificate | The app was signed ad hoc or with an Apple Development certificate: package with `--sign "Developer ID Application: …"`. |
| Notarization `Invalid`: the signature does not include a secure timestamp | Sign through `nsh package --sign`, which adds it; do not re-sign by hand without `--timestamp`. |
| "Notes is damaged and can't be opened" after download | The archive lost the signature (zipped with a tool that drops attributes, or modified after signing): archive with `ditto` or `tar` after stapling. |
| Gatekeeper warns although notarization succeeded | The ticket was not stapled and the Mac is offline, or the downloaded app is not the one notarized: staple, then archive again. |
| The app cannot use the camera, microphone or other protected resources | Add the usage description to `macos.infoPlist` and the Hardened Runtime exception to `macos.entitlements`. |

## Publish for Windows

`nsh package --target windows` writes everything a Windows release needs for direct download: the app's directory, a
`.zip` of it, and a per-user installer built with Inno Setup. You sign the executables with your code signing
certificate and upload them to your site or a GitHub Release; [App Updates](./app-updates.md) keeps installed copies
up to date.

`nsh publish` has no Windows store upload: NSH does not package MSIX for the Microsoft Store yet.

### Before the first release

On the Windows machine (or CI runner) that packages the app:

1. **Install the tools.** A TSX app needs none; a Rust app's runner is built with Visual Studio Build Tools. For the
   installer, install Inno Setup 6: `winget install JRSoftware.InnoSetup`.
2. **Get a code signing certificate** from a certificate authority, or use a cloud signing service such as Azure
   Trusted Signing. Unsigned executables still run, but SmartScreen warns about an unknown publisher, and some
   antivirus products treat them with suspicion.
3. **Install the Windows SDK** for `signtool.exe` (it comes with the Visual Studio installer's Windows SDK component).

### Configure the app

| Key | In the build | Notes |
|---|---|---|
| `id` | The installer's AppId, the Start menu shortcut's AppUserModelID | Groups the app's windows in the taskbar and identifies it for notifications; a newer installer with the same id upgrades the installed app. Keep it stable. |
| `name` | `<Name>.exe`, the install directory and the Start menu entry | |
| `version` | The installer's version and the Installed apps entry | Must be semver and grow for [self-updates](./app-updates.md). |
| `icon` (or `icon.windows`) | Embedded in `<Name>.exe`, written as `nsh\icon.ico` | Package on Windows: only Windows can embed the icon ([Icons](./packaging.md#icons)). |

### Build

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

### Sign

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

### Distribute it

Upload `Notes-setup.exe` (and the `.zip`, for people who prefer a portable copy) to your site or a GitHub Release.

With [`"updater"`](./app-updates.md#set-the-source) in `nsh.json`, `nsh package` also writes
`<Name>-<version>-windows-<arch>.zip` for self-updates. Put the signed app in it, then write the manifest with the other
platforms' archives ([App Updates](./app-updates.md#publish-a-release)):

```powershell
nsh update-manifest *.zip *.tar.gz --notes CHANGES.md --key keys\update.secret
```

### SmartScreen

SmartScreen decides by reputation, which grows as people download and run files signed with the same certificate. A
new certificate, or a new kind of certificate, can show the "Windows protected your PC" warning for a while even when
the file is signed. Keep signing every release with the same certificate so the reputation carries over.

### Common errors

| Problem | What to do |
|---|---|
| No `<Name>-setup.exe` in `.nsh\out` | Inno Setup 6 is not installed, or not where `nsh package` looks: install it with `winget install JRSoftware.InnoSetup`, or compile `.nsh\out\<Name>.iss` with `ISCC.exe`. |
| The executable shows the default icon | It was packaged on macOS or Linux: package on Windows to embed the icon. |
| `signtool` is not recognized | Run it from a Developer PowerShell, or add the Windows SDK's `bin\<version>\x64` directory to `PATH`. |
| `SignTool Error: No certificates were found that met all the given criteria` | The certificate is not in the store `signtool` searches, or `/n` does not match its subject: list them with `certutil -user -store My`. |
| "Windows protected your PC" on a signed installer | SmartScreen reputation is still building for the certificate (above). |
| The installer asks to close the app | It upgrades in place and closes the running copy first; this is expected. |

## Publish for Linux

`nsh package --target linux` writes the app as a directory and a `.tar.gz` of it, with a desktop entry and the icon.
You upload the archive to your site or a GitHub Release, and users unpack it and install the desktop entry;
[App Updates](./app-updates.md) keeps installed copies up to date.

`nsh publish` has no Linux store upload, and `nsh package` does not write Flatpak, AppImage, `.deb` or `.rpm`
packages yet.

### Before the first release

Package on Linux: the runner is built for the machine it is built on, so package on the processor you publish for
(`x86_64` or `aarch64`), on a distribution old enough for the users you support, as a newer glibc than theirs stops the
app from starting. Ubuntu 24.04 is a good base for current desktops. The build needs the development packages of
GTK 4 (4.14 or later), Pango and WebKitGTK 6.0 ([Linux: Requirements](./linux.md#requirements)).

### Configure the app

| Key | In the package | Notes |
|---|---|---|
| `id` | `<id>.desktop`, `StartupWMClass`, the GTK application id, `<id>.png` | Desktops match the app's windows to its launcher by it; reverse-DNS, as `com.example.notes`. Keep it stable. |
| `name` | The executable `<Name>`, the directory, and the desktop entry's `Name` | |
| `version` | Self-update archives | Must be semver and grow for [self-updates](./app-updates.md). |
| `icon` (or `icon.linux`) | `<id>.png`, named by the desktop entry's `Icon` | A square PNG that fills its canvas ([Icons](./packaging.md#icons)). |

### Build

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

### Distribute it

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

### Common errors

| Problem | What to do |
|---|---|
| `error while loading shared libraries: libgtk-4.so.1` (or `libwebkitgtk-6.0.so.4`) | Install the runtime libraries above. |
| `version 'GLIBC_2.39' not found` | The app was packaged on a newer distribution than the user's: package on an older one. |
| The app does not show in the launcher | The desktop entry is not in `~/.local/share/applications`, or its `Exec` does not find the executable: link it onto the `PATH`. |
| The launcher shows a generic icon, or a second icon while the app runs | The icon is not in an icon theme directory, or the desktop entry was renamed: keep the file name `<id>.desktop`, which `StartupWMClass` matches. |
| `exec format error` | The archive was built for another processor: package on `x86_64` and `aarch64` separately. |

## Publish on the web

On the web an app is a static site. `nsh package --target web` writes it; you copy the directory to any web server,
object store or static host. There is no store, no signing and no review, and a deploy updates every user on their next
load. `nsh publish` has nothing to do here.

### Configure the app

| Key | In the site | Notes |
|---|---|---|
| `name` | The page title and the web app manifest's name | |
| `id` | The web app manifest's id | |
| `icon` (or `icon.web`) | The favicon, `apple-touch-icon` and manifest icons | Resized to 32, 180, 192 and 512 pixels ([Icons](./packaging.md#icons)). |
| `appearance` | The page's colors while it loads | |
| `web.base` | The path the site is served under | Needed when the app is opened at its own paths ([Links into the app](./web.md#links-into-the-app)). |
| `web.host` | The page's host script | [The host's functions](./web.md#the-hosts-functions). |

### Build

```sh
nsh package --target web            # a Rust app, from its directory
nsh package App.tsx --target web    # a TSX app
nsh package --target web --run      # and serve it on http://127.0.0.1:4173/ to try it
```

This writes `.nsh/out/<Name>/` (`--out` for another directory): `index.html`, `manifest.webmanifest`, and every other
file named after its content (`nsh.<digest>.wasm`, `app.<digest>.js`, `assets/…`), with Brotli and gzip copies
beside them. A release changes only `index.html` and the files that really changed. The full list of files and the
runtime's size: [Web: Publish an app](./web.md#publish-an-app).

By default the site uses relative URLs, so it works from any directory of any host. To serve the files from a CDN while
the page stays at the app's own address, pass `--asset-url https://cdn.example.com/notes/`.

### Deploy

Any static host works. Two rules make the app load fast and update at once:

| Files | `Cache-Control` |
|---|---|
| `index.html`, `manifest.webmanifest` | `no-cache` |
| everything else | `public, max-age=31536000, immutable` |

Serve `.wasm` as `application/wasm`, and send the precompressed `.br` and `.gz` copies to browsers that accept them
([Deploy it](./web.md#deploy-it) has nginx and Caddy settings). Upload the hashed files before `index.html`, so a
visitor never gets a page whose files are not there yet.

#### GitHub Pages

```yaml
on:
  push:
    tags: ["v*"]

permissions:
  pages: write
  id-token: write

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: github-pages
    steps:
      - uses: actions/checkout@v4
      # Install nsh (pin the version), then:
      - run: nsh package --target web --out site
      - uses: actions/upload-pages-artifact@v3
        with:
          path: site
      - uses: actions/deploy-pages@v4
```

A project site lives under `/<repo>/`; relative URLs handle that. An app opened at its own paths sets
`"web": { "base": "/<repo>/" }` and adds a copy of `index.html` as `404.html` (`cp site/index.html site/404.html`
before the upload), which GitHub Pages answers unknown paths with. GitHub Pages cannot set `Cache-Control` per file, so returning visitors revalidate
the hashed files too; a host that lets you set headers loads faster.

#### An object store and a CDN

```sh
nsh package --target web --out site
aws s3 sync site s3://example-notes --exclude index.html --exclude manifest.webmanifest \
  --cache-control "public, max-age=31536000, immutable"
aws s3 cp site/index.html s3://example-notes/ --cache-control no-cache
aws s3 cp site/manifest.webmanifest s3://example-notes/ --cache-control no-cache
```

Configure the CDN to serve `.wasm` as `application/wasm` and to compress with Brotli, or to serve the `.br` copies.

### Supported browsers

Safari 16.4 (iOS and iPadOS 16.4), Chrome and Edge 97, and Firefox 108 or later. Earlier browsers show a blank page
([Supported browsers](./web.md#supported-browsers)).

### Common errors

| Problem | What to do |
|---|---|
| A blank page and `Failed to load module script` in the console | The host serves `.js` or `.wasm` with the wrong content type, or the files were not uploaded: check the Network panel. |
| `WebAssembly.instantiateStreaming` fails with an incorrect response MIME type | Serve `.wasm` as `application/wasm`. |
| Opening a path of the app, or reloading on one, gives 404 | Answer every path under the app with `index.html` and set `web.base` ([Links into the app](./web.md#links-into-the-app)). |
| Users see the old version after a deploy | `index.html` is cached: serve it with `Cache-Control: no-cache`. |
| Files from the CDN are blocked by CORS | The CDN must answer with `Access-Control-Allow-Origin` for the page's origin when `--asset-url` names it. |
| A blank page on an older iPhone or Android WebView | The browser is older than the supported ones. |

## See also

- [Packaging](./packaging.md): `nsh package` for every platform
- [CLI](./cli.md): every command and flag
- [App Updates](./app-updates.md): apps that update themselves
- [OTA Updates](./ota-updates.md): new TSX without a store release
