# Developer documentation

For people working on native-shell itself (`AGENTS.md` at the root is the short version agents read first).

| Page | What |
|---|---|
| [architecture.md](architecture.md) | Threads, the data flow from JS to native views, crates |
| [native-pipeline.md](native-pipeline.md) | How apps are compiled and run: the JSX compiler, the native modules, the renderer, types, hot reload |
| [js-modules.md](js-modules.md) | The module layers (`nsh/*`, `nsh/ui/*`), where each module is implemented and declared, platform props, where new things go |
| [components.md](components.md) | Rust components: the host node, renders and diffing, children and slots, events, the JS face and declarations |
| [elements.md](elements.md) | Host elements and runtime components, touch, accessibility, the environment, why the main thread never waits for JS |
| [text.md](text.md) | Text blocks, spans, inline attachments |
| [styles.md](styles.md) | Tailwind, effects, view styles, Android differences |
| [scrolling-and-lists.md](scrolling-and-lists.md) | Scroll views, zooming, `List` |
| [animation.md](animation.md) | Native values, composites, layout animation |
| [canvas.md](canvas.md) | `Canvas` |
| [images.md](images.md) | Loading, decoding, caching, animated images |
| [sandbox.md](sandbox.md) | VM limits, hardening, capabilities |
| [signed-packages.md](signed-packages.md) | Keys, packages, verification, revocation |
| [embedding.md](embedding.md) | `nsh package`, the iOS / Android SDKs, native components |
| [tooling.md](tooling.md) | `.nsh/types`, the dev server, hot reload, symbolication |
| [macos.md](macos.md) | The macOS backend (AppKit): building, how each area maps to AppKit, `--script` checks, what differs |
| [linux.md](linux.md) | The Linux backend (GTK 4): building, how each area maps to GTK, `--script` checks, what differs |
| [windows.md](windows.md) | The Windows backend (Win32, Direct2D): building, how each area maps to Windows, `--script` checks, what differs |
| [gpui.md](gpui.md) | The GPUI backend (macOS, Windows, Linux): building, the surface element and `DrawCx`, embedding as a GPUI view, what differs |
| [web.md](web.md) | The WebAssembly runtime of the website's demos: `LocalEngine`, the clock, the DOM protocol |
| [performance.md](performance.md) | Baselines |
| [pitfalls.md](pitfalls.md) | Verified constraints and pitfalls, by area |

How a module works inside is documented in the module itself (`//!` docs, Javadoc); these pages describe behavior
and decisions and point there.
