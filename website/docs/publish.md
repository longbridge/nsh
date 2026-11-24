---
title: Publishing
description: "Publishing an NSH app to app stores: App Store Connect and TestFlight for iOS, Google Play for Android, store credentials and release CI."
order: 18.5
group: publishing
---

# Publishing

Publishing is uploading a packaged app to an app store: App Store Connect for iOS and Google Play for Android, with
`nsh publish ios` and `nsh publish android`. [Packaging](./packaging.md) builds the file first, and also covers the
platforms without a store upload: signing, notarizing and distributing macOS, Windows and Linux apps, and hosting a
web app.

- [iOS](#publish-to-the-app-store): App Store Connect and TestFlight
- [Android](#publish-to-google-play): Google Play

## What each store supports

| Platform | What `nsh package` writes | Store | What `nsh publish` does |
|---|---|---|---|
| iOS | A signed `.ipa` | App Store Connect: TestFlight, then the App Store | Uploads the IPA for TestFlight (`nsh publish ios`) |
| Android | A signed `.aab` (or `.apk`) | Google Play | Uploads the bundle and releases it to the internal testing track (`nsh publish android`) |

Store review submissions, phased rollouts and production releases are made in each store's console; no `nsh` command
makes them. macOS, Windows, Linux and the web have no store upload in NSH yet; they ship as direct downloads or a
static site, described in Packaging: [macOS](./packaging.md#sign-notarize-and-distribute),
[Windows](./packaging.md#sign-and-distribute), [Linux](./packaging.md#distribute) and [Web](./packaging.md#host-the-site).

`nsh publish <ENTRY> --key … --cert …`, without a platform, makes a signed package for
[OTA Updates](./ota-updates.md) and [Signed packages](./signed-packages.md), not a store upload.

## One release, step by step

1. **Set the version in `nsh.json`.** `version` is what users see (`1.4.0`); `build` is a whole number that must grow
   with every upload to a store, on every platform (`CFBundleVersion` on Apple platforms, `versionCode` on Android).
   Both can also be given on the command line (`--app-version`, `--build`), which is handy when CI numbers builds.
2. **Package.** iOS needs a Mac with Xcode; Android packages anywhere. See [Packaging](./packaging.md).
3. **Sign.** Both sign as they package, given an Apple Distribution identity and profile, or an upload keystore.
4. **Publish.** Upload to the store with `nsh publish ios` or `nsh publish android`.
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

## See also

- [Packaging](./packaging.md): `nsh package` for every platform, and distributing apps without a store
- [CLI](./cli.md): every command and flag
- [App Updates](./app-updates.md): apps that update themselves
- [OTA Updates](./ota-updates.md): new TSX without a store release
