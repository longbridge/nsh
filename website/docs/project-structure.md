---
title: Project structure
description: The files of an NSH app, every key of nsh.json, the TypeScript setup in .nsh/types, assets, and how modules and packages are imported.
order: 5
group: start
---

# Project structure

An NSH app is a directory with an entry module and an optional `nsh.json`. Everything else (more modules, images, fonts, JSON files) is found by following the entry's imports, so there is no file list to maintain.

```text
my-app/
├── App.tsx            the entry module: calls mount()
├── App.test.tsx       its tests (nsh test)
├── nsh.json           name, id, version, icon, Tailwind theme, capabilities
├── tsconfig.json      TypeScript settings for editors
├── .gitignore         .nsh/ and out/
├── components/        your own modules (any layout you like)
│   └── Header.tsx
├── assets/
│   ├── logo@2x.png    imported images
│   └── Inter.ttf      imported fonts
├── .nsh/              generated: type declarations, development host builds, test entries
└── out/               generated: nsh build / nsh package output
```

Every command takes the entry as its argument (`nsh run my-app/App.tsx`). The directory that contains the entry is the app's root: `nsh.json` is read from there, asset paths are relative to it, and `.nsh/` is written there.

## The entry module

The entry is a normal module that ends by rendering the root component:

```tsx
import { mount } from "nsh";
import { Home } from "./components/Home";

function App() {
  return <Home />;
}

mount(App);
```

`mount(component, props?)` renders the component into the app's surface and returns a function that removes it again. An app calls it once. Code at the top level of the entry runs when the bundle loads, before the first frame.

## nsh.json

`nsh.json` sits next to the entry. Every key is optional; `nsh init` writes the first four.

```json
{
  "name": "Gallery",
  "id": "com.example.gallery",
  "version": "1.0.0",
  "build": 1,
  "icon": "icon.png",
  "tailwind": {
    "colors": { "brand": { "500": "#7c3aed", "600": "#6d28d9" } },
    "radius": { "card": 14 }
  },
  "capabilities": {
    "network": { "hosts": ["api.example.com", "*.cdn.example.com"] }
  }
}
```

| Key | Type | Used by | Description |
|---|---|---|---|
| `name` | string | `package` | The display name under the app icon. Default: the entry directory's name. |
| `id` | string | `package`, `publish`, `nsh/storage` | Bundle identifier / application id in reverse-DNS form, e.g. `com.yourcompany.app`; required by `package` and `publish`. `nsh run` uses `com.example.<name>` without one. |
| `version` | string | `package`, `publish` | The user-visible version, e.g. `1.2.0` (`CFBundleShortVersionString`, Android `versionName`). |
| `build` | number | `package`, `publish` | The build number (`CFBundleVersion`, Android `versionCode`). Signed packages require it to grow with every release. |
| `minRuntime` | string | `publish` | The lowest NSH runtime that runs the signed package, e.g. `0.2.0`. Default: the CLI's version ([Signed packages](./signed-packages.md#which-runtimes-run-a-package)). |
| `icon` | string | `package` | A square PNG, ideally 1024×1024, relative to `nsh.json`. |
| `tailwind` | object | every build | Extends the Tailwind theme your `class` strings resolve against ([Tailwind](./tailwind.md#theme)). |
| `appearance` | string | every build, `package` | `"light"`, `"dark"` or `"system"` (the default): an app that is always light or dark says so here. `colorScheme()`, `dark:` classes, the components' theme and the platform's controls follow it from the start ([Device and environment](./device-and-environment.md#dark-mode)). |
| `theme` | string or object | every build | The theme the app's Rust components draw with: a theme file's path relative to `nsh.json`, or the theme inline ([Building with Rust](./rust.md#theme)). |
| `statusBar` | object | every build, `package` | The status bar's default under every page and dialog (iOS and Android): `{ "foreground": "auto" \| "light" \| "dark", "hidden": boolean }`, both optional. Packaged iOS apps show it from the launch screen on ([Status bar](../modules/nsh-base.md#status-bar)). |
| `capabilities` | object | every build | What the app may reach outside its VM: network hosts, web view hosts, the status bar ([Sandbox and capabilities](./sandbox-and-capabilities.md)). |
| `resolve` | object | every build | How bare imports resolve: the export conditions of npm packages and aliases ([npm packages](#npm-packages)). |
| `rust` | string or string[] | every command | The app's Rust crate, or a list of them, relative to `nsh.json`: their components and modules, which TSX imports by each crate's `#[native_module]` specifier ([Building with Rust](./rust.md#several-crates)). |
| `root` | string | `run`, `package` | A Rust app's root component, its path in the `rust` crate, the first one of a list (`App`, `shell::Gallery`): `nsh run` and `nsh package` of the app's directory mount it with no QuickJS, on every platform; `nsh run App.tsx` still runs the TSX entry ([Building with Rust](./rust.md)). |

The flags of `nsh package` and `nsh publish` (`--name`, `--id`, `--app-version`, `--build`, `--icon`, `--min-runtime`) override the file's values ([CLI](./cli.md)).

`tailwind`, `theme`, `appearance`, `statusBar`, `capabilities` and `resolve` are checked when the app is built. A typo is a build error rather than a setting that silently does nothing:

```text
error building App.tsx failed
nsh.json: tailwind: unknown theme key `colours`
```

> [!WARNING]
> `nsh package` and `nsh publish` read `nsh.json` strictly and currently reject keys they do not know, including `theme`. If you set a theme, package from a copy of the app without it until this is fixed.

### capabilities

Without a `capabilities` key, an app may use the network, web views and the status bar freely: a standalone app is its developer's own. With the key, everything not listed is denied, both in development and in packaged apps:

```json
"capabilities": {
  "network": {
    "hosts": ["api.example.com", "*.cdn.example.com"],
    "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }]
  },
  "webview": { "hosts": ["*.example.com"] },
  "statusBar": true
}
```

`network` covers `fetch`, remote images and video sources; local assets are always allowed. An app that embeds NSH may grant less than an app declares. The details are in [Sandbox and capabilities](./sandbox-and-capabilities.md).

## tsconfig.json and .nsh/types

NSH does not need `node_modules`. Instead, every command that builds the app (`init`, `build`, `test`, `dev`, `run`, `package`, `publish`, `check`) writes the TypeScript declarations of everything NSH provides into `.nsh/types`, and NSH's TypeScript settings into `.nsh/tsconfig.json`. The app's `tsconfig.json` extends them:

```json
{
  "extends": "./.nsh/tsconfig.json",
  "include": ["**/*.ts", "**/*.tsx"]
}
```

`.nsh/tsconfig.json` points the `nsh` imports and your app's crates to `.nsh/types`, includes the globals and asset declarations, and sets the compiler options NSH apps use (`lib`, `jsx`, `strict`, ...). NSH keeps it current: when a new version adds a library or a path, your app has it with the next build, and your `tsconfig.json` stays as you wrote it. Options you add to `tsconfig.json` apply over NSH's.

| File in `.nsh/types/nsh/` | Declares |
|---|---|
| `index.d.ts` | `nsh`: `signal`, `computed`, `effect`, `mount`, `View`, `Text`, ... and re-exports the types below |
| `types.d.ts` | Styles (`ViewStyle`, `TextStyle`, ...), every element's props and events, `YNode` (what a `ref` receives) |
| `jsx-runtime.d.ts` | The JSX runtime TypeScript resolves through `jsxImportSource` (the `JSX` namespace) |
| `globals.d.ts` | `If` and `Each`; `console`, timers, `requestAnimationFrame`, `fetch` and the other [global APIs](../modules/globals.md) |
| `assets.d.ts` | What importing an image or font gives |
| `list.d.ts`, `animated.d.ts`, `device.d.ts`, ... | One file per module (`nsh/list`, `nsh/animated`, `nsh/device`, ...) |
| `base.d.ts` | `nsh/base` |

`lib` is `ES2023` without `DOM`: there is no `document` or `window`. The files in `.nsh` are regenerated (only when their contents change), so do not edit them and do not commit them; `.gitignore` already excludes `.nsh/`. If your editor shows `nsh` as missing after cloning a project, run any build command once, e.g. `nsh check`.

`.nsh/tsconfig.json` adds `"ESNext.Disposable"`, which declares `using` declarations and `Symbol.dispose`, which the runtime has: `using account = new Account()` frees a [Rust class](./rust.md)'s value at the end of the block. Its `jsxImportSource: "nsh"` and `jsx: "preserve"` only matter to the editor: NSH compiles JSX with its own compiler, whatever the `tsconfig.json` says. TypeScript is checked by your editor or by `nsh check`; the other commands strip types without checking them.

An app made by an earlier `nsh init` gets the new `tsconfig.json` with its next build when its own is NSH's template, unchanged; NSH prints a line saying so. A `tsconfig.json` you changed is yours: `nsh check` points out when it does not extend `./.nsh/tsconfig.json`, and you add the `extends` line and remove the options it sets.

## Modules

Split the app into as many `.ts` and `.tsx` modules as you like and import them with relative paths; the extension may be left out:

```tsx
import { Header } from "./components/Header";
import { formatPrice } from "./lib/format";
import strings from "./strings.json";
```

All of them are bundled into one script. JSON files import as their parsed value.

### Loading code on demand

`import()` of one of the app's modules returns a promise of its exports, as in a browser:

```tsx
const open = async () => {
  const { Settings } = await import("./screens/Settings");
  screen.set(() => Settings);
};
```

Apps on devices ship one bundle, so the module is already there and the promise resolves at once. A web app published split (`nsh build --split`) gets the module, and what only it uses, as a separate file that is downloaded the first time `import()` asks for it ([Web](./web.md#split-bundles)). The specifier must be a literal relative path: the bundler has to see which module is meant.

### NSH's modules

Imports of `nsh` and `nsh/...` are not bundled: the runtime provides them natively.

| Module | What it has |
|---|---|
| `nsh` | Reactivity (`signal`, `computed`, `effect`, ...), `mount`, `split`, `context`; `View`, `Text`, `Scroll`, `Input` ([reference](../modules/nsh.md)) |
| `nsh/base` | The unstyled foundation of an app's UI: navigation stacks and their pages (`NavStack`, `NavPage`, `createNavStack`), dialogs (`Dialog`), a press target (`Button`), safe areas (`SafeAreaView`, `safeArea`, `safeAreaInsets`), the system's alert dialog (`AlertDialog`), the software keyboard (`Keyboard`, `keyboardFrame`, `KeyboardAvoidingView`); pages and dialogs set the status bar (`statusBar`) ([reference](../modules/nsh-base.md)) |
| `nsh/list` | `List`: virtualized, recycling lists |
| `nsh/animated` | Native animations |
| `nsh/canvas` | 2D drawing |
| `nsh/gesture` | `PanResponder` |
| `nsh/device` | `Platform`, `windowSize`, `colorScheme`, `fontScale`, `PixelRatio`, `I18nManager` |
| `nsh/accessibility` | Screen reader and reduce motion state, announcements |
| `nsh/image` | `Image` (the element), `Image.getSize`, `Image.prefetch` |
| `nsh/webview`, `nsh/video` | `WebView`, `Video` |
| `nsh/native` | Components and functions the embedding app registers |
| `nsh/ios`, `nsh/android` | Platform-only APIs |

The [modules reference](../modules/index.md) documents each one. Importing a module NSH does not have (`nsh/foo`) is a build error. So is a name your code uses that the module does not export (`import { Buton } from "nsh/base"`): the error points at the import and names the closest export (``did you mean `Button`?``). Names used only as types are checked by `nsh check`.

### npm packages

NSH needs nothing from npm, but the bundler resolves bare imports from a `node_modules` directory if you install packages with a package manager. Packages resolve their browser builds, because the runtime provides web APIs (`fetch`, streams, `URL`, `TextEncoder`, ...) and not Node's (`fs`, `Buffer`, `process`). Packages that touch the DOM do not work: there is none.

A package whose browser build uses the DOM often has another build for workers or React Native. `resolve.conditions` picks the export conditions packages resolve with, in place of the default `["browser"]`: `import`, `require`, `module` and `default` always apply, and without `browser` packages resolve through their `module` and `main` fields. `resolve.alias` replaces an import: by a path relative to `nsh.json` (`./` or `../`), another package, or `false` for an empty module. TypeScript (`nsh check`, editors) types the import as written: a name that is not an installed package gets its types from a `.d.ts` file of the app.

```json
"resolve": {
  "conditions": ["worker"],
  "alias": { "dom-only-helper": "./shims/helper.ts", "node-polyfill": false }
}
```

```ts
// shims/aliases.d.ts
declare module "node-polyfill";
```

`solid-js` cannot be imported: the bundler refuses it and points to `nsh`'s own `signal`, `computed` and `effect`, which replace it.

## Assets

Images and fonts are imported like modules. The bundler records each imported file (and an image's pixel-density variants) and ships it with the app: `nsh build` copies them to `out/assets/`, `nsh package` puts them in the app, and during development the dev server serves them.

```tsx
import { Text } from "nsh";
import { Image } from "nsh/image";
import logo from "./assets/logo@2x.png";   // { uri: "asset:/assets/logo@2x.png", width: 64, height: 64 }
import Inter from "./assets/Inter.ttf";     // "Inter"

<Image src={logo} />
<Text style={{ fontFamily: Inter }}>Hello</Text>
```

| Extension | Import gives |
|---|---|
| `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.heic` | An `ImageAsset`: `{ uri, width, height, scales? }`, sized in points. An `@2x` / `@3x` suffix divides the pixel size by 2 / 3. A file without a suffix also ships its `@1.5x`–`@4x` siblings, listed in `scales`, and the screen's variant is loaded ([pixel densities](./images-and-assets.md#pixel-densities)). |
| `.ttf`, `.otf` | The font's family name, a string. The font is registered before the first frame. |
| `.json` | The parsed JSON |

See [Images and assets](./images-and-assets.md) and [Fonts](./fonts.md).

## Generated directories

- **`.nsh/`**: the type declarations (`.nsh/types`) and what `nsh run` installs on simulators and devices (the Catalyst host app, the Android development APK). Safe to delete; it is recreated.
- **`out/`**: the default output of `nsh build` (`bundle.js`, `bundle.js.map`, `bundle.nshc`, `capabilities.json`, `assets/`) and `nsh package` (`.app`, `.ipa`, `.apk`). Change it with `-o`.
