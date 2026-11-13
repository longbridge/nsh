# Cargo features of the runtimes

The large optional parts of a native runtime are Cargo features of the platform crates (`nsh-platform-linux`,
`-macos`, `-windows`, `-ios`, `-android`, `-gpui`) and the app runners (`nsh-*-app`). `websocket` and `updater` are also features of `nsh-core`, `nsh-engine` and `nsh-platform-tui`. All are on by default, and `dev` turns all on.

| Feature | What it links | Off |
| --- | --- | --- |
| `shader` | naga, wgpu | Shader views draw nothing, shader transitions end at once ([shaders.md](shaders.md)) |
| `editor` | `Editor`, `DiffView`, ropey, imara-diff | Importing them fails |
| `rich-editor` | `RichEditor` and its classes | Importing them fails |
| `plot` | `nsh/base/plot` | Importing it fails |
| `term` | `Terminal`, vte | Importing it fails |
| `highlight` | syntect and its syntaxes | Code shows plain |
| `math` | RaTeX, the KaTeX fonts | A formula shows as its TeX, or as the app's `renderMath` |
| `zlib` | the JS module `zlib` (implies `js`) | Importing it fails |
| `crypto` | the JS module `crypto`, Web Crypto's globals (implies `js`) | Importing it fails |
| `websocket` | the native WebSocket client: tokio-tungstenite, rustls, the platform verifier (webpki-roots on Android) | `connect` of `websocket` rejects with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` |
| `updater` | `nsh/updater`'s checks, downloads and installs (nsh-updater) | `checkForUpdate` and `checkForPackage` find none; downloads and installs reject |
| `svg` | resvg, usvg, tiny-skia (SVG images) | An SVG `<Image>` shows nothing; its load fails naming the feature |
| `net` | ureq and rustls: `fetch` and remote images (Linux, Windows, TUI, GPUI; macOS, iOS and Android use the system's HTTP) | `fetch` rejects; `<Image>` shows files, assets and inline data only |
| `sound` | GStreamer (Linux only: `nsh-platform-linux`, `nsh-linux-app`; the other platforms play sounds with the system's audio) | `Sound.load` rejects with `not-supported`, naming the feature; `AudioPlayer` and `Video` play as before, through GTK |

An app turns them off with `default-features = false` on the platform crate and names what it keeps. A direct dependency
on `nsh-base` must also say `default-features = false`, or Cargo turns its defaults back on.

Only a project building its own executable with Cargo (an embedder, or its own `main` on `nsh_*_app`) can leave them
out. What `nsh` builds has everything: the prebuilt runners, and the workspace it generates for an app with Rust crates.
The web already links per app by what it imports (`docs/dev/browser-js-size.md`).

A part that is off leaves `nsh_base` altogether, so Rust using it does not compile. JS fails at load: `Could not find
export 'Editor' in module 'nsh/base'`. Nothing checks at build time that an app's imports match its runtime's features,
because `nsh` bundles against every part.

`nsh-cli`'s generated crates and `xtask dist` use `default-features = false`: a new feature goes in `RUST_APP_PLATFORM`
(`crates/nsh-cli/src/rust_app.rs`; one only some platforms have, as Linux's `sound` or Linux's and Windows' `net`, in
`rust_app_platform`: Cargo resolves the generated workspace for every target, so a feature named on a crate without it
fails every build) and in `dist.rs`, or those builds lose it.
