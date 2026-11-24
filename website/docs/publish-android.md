---
title: Publish to Google Play
description: "Ship an NSH app to Android users: Play Console and service account setup, the upload key, nsh.json keys, an Android App Bundle, nsh publish android to the internal testing track, promotion and common errors."
order: 19.3
group: publishing
---

# Publish to Google Play

Google Play takes Android App Bundles (`.aab`). `nsh package --target android --format aab` builds and signs one with
your upload key, without Gradle; `nsh publish android` uploads it with the Google Play Developer API and releases it
to the **internal testing** track. Promoting it to closed, open or production testing, the store listing and review
are done in Play Console.

Both commands run on macOS, Linux and Windows.

## Before the first release

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

## Configure the app

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

## Build the bundle

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

## Upload to the internal testing track

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

## After the upload

In Play Console:

- **Promote the release**: Testing → Internal testing → the release → Promote release, to closed testing, open
  testing or production. A new personal developer account must run a closed test before production access is granted.
- **The store listing**: app name, short and full description, icon, feature graphic, phone and tablet screenshots.
- **App content**: privacy policy, data safety, ads, content rating and target audience, required before the first
  production release.
- **Review and rollout**: releases to closed, open and production tracks are reviewed; a production release can be
  rolled out to a percentage of users and raised later.

## Common errors

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

- [Publishing](./publishing.md): every platform, credentials and release CI
- [Packaging: Android](./packaging.md#android)
- [Android](./android.md): running and embedding on Android
- [CLI: nsh publish android](./cli.md#nsh-publish-android)
