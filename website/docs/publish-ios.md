---
title: Publish to the App Store
description: "Ship an NSH app to iOS users: Apple Developer setup, nsh.json keys, a distribution-signed IPA, nsh publish ios to App Store Connect and TestFlight, review and common errors."
order: 19.2
group: publishing
---

# Publish to the App Store

An iOS app reaches users through App Store Connect: you upload a build, testers get it in TestFlight, and once Apple
approves it, the App Store offers it. `nsh package --target ios` builds and signs the IPA; `nsh publish ios` uploads it
to App Store Connect. Everything after the upload (TestFlight groups, the store listing, review and release) is done in
App Store Connect.

Both commands run on macOS with Xcode installed, for Rust, Rust + TSX and TSX apps alike.

## Before the first release

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

## Configure the app

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

## Build the IPA

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

## Upload to TestFlight

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

## After the upload

In App Store Connect:

- **TestFlight**: once processed, the build is available to internal testers (members of your team) at once. External
  testers need a group and a Beta App Review of the first build of each version.
- **The App Store listing**: name, subtitle, description, keywords, screenshots for each device size, the privacy
  details (App Privacy) and the age rating. They are entered in App Store Connect.
- **Review**: add the build to the version, then Add for Review and Submit. After approval, the version is released
  manually, automatically, or on a date, as the version's release option says.

## Common errors

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

## See also

- [Publishing](./publishing.md): every platform, credentials and release CI
- [Packaging: iOS devices](./packaging.md#ios-devices)
- [iOS](./ios.md): running and embedding on iOS
- [OTA Updates](./ota-updates.md): new TSX for installed apps between store releases
