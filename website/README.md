# Native Shell website

The documentation site for nsh, published at <https://native-shell.com>
(`src/lib/site.ts`). Astro (static output), Tailwind 4, Pagefind search,
Shiki highlighting; [Bun](https://bun.com) runs everything.

```sh
bun run dev        # http://localhost:4321, live reload (search needs a build)
bun run build      # dist/: astro build, then the Pagefind index
bun run preview    # serve dist/
bun run test       # content rules + a link check of dist/ (build first)
```

`dev` first kills a dev server of this checkout that is still running (it
would hold the port). `dev` and `build` install the dependencies if needed and build the
WebAssembly player with the demos (`cargo xtask web`, incremental: a second
start takes seconds). Without Rust (and its `wasm32-unknown-unknown` target)
they print a warning and the pages show demo sources without the live preview.
While `dev` runs, demos are bundled from `demos/*.tsx` on each request and
`nsh.js` is served from `crates/nsh-platform-web/web`, so edits to either show
on reload; changes to the Rust runtime need a restart.

Set `PUBLIC_SITE_BASE=/sub/path/` to build for a sub-path.

## Writing documentation

Content is Markdown in four sections, each in English and Chinese:

| Directory | URL | Nav label |
| --- | --- | --- |
| `docs/*.md` | `/docs/<file>` | Guide · 指南 |
| `elements/*.md` | `/elements/<file>` | Elements · 元素 |
| `components/*.md` | `/components/<file>` | Components · 组件 |
| `modules/*.md` | `/modules/<file>` | API |
| `zh-CN/<section>/*.md` | `/zh-CN/<section>/<file>` | |

`index.md` is the section's own page (`/docs`). The file name is the URL;
the Chinese page has the same file name as the English one.

When changing a documentation page, update its English and Chinese
counterparts together. Keep their `order`, `group` and `demo` frontmatter in sync, and
link to pages in the same language.

Name the product **Native Shell** in formal places (page titles, the home page,
a page's first mention, the README's opening, meta descriptions) and **NSH**
everywhere else in prose, as GPUI is written: "NSH apps", "NSH's components",
"NSH 应用". Lowercase `nsh` is for code only, in backticks: the CLI (`nsh run`),
`nsh.json`, the modules (`nsh`, `nsh/list`), crates and packages. A short,
uppercase name is one the community can build on (`nsh-*` crates, NSH plugins).

Write in positive terms: say what nsh, a platform or a feature is and does,
in both languages.

- **Describe a thing by what it is.** "The macOS backend renders with AppKit
  views", not "this is not Mac Catalyst"; readers make no comparison the page
  does not put in their heads.
- **Leave out what is missing, planned or unfinished.** No "not yet", "in
  development", "demos only", "there is no …", "without …" or "no Node.js";
  a page lists what works. When a reader needs a boundary to use a feature
  correctly, state it as the rule that holds ("`AppView` loads bundles from
  the app's own files"), and keep the detailed reference (a "What differs on
  …" section, standards compliance notes) factual.
- **Name things after what nsh is.** nsh is native UI: the standard globals
  are "global APIs", not "web APIs"; the web platform renders HTML in a page
  (say "the web", "HTML", "the browser", never the technology underneath it).

Frontmatter (checked by `src/content.config.ts`):

```yaml
---
title: Getting started          # required; the page heading and sidebar label
description: "Create an app: …" # required; meta description, llms.txt
order: 2                        # optional; sidebar position, ascending
group: start                    # optional; the sidebar group (GROUPS in src/lib/sections.ts)
demo: guide-getting-started     # optional; a demo from demos/<name>.tsx
---
```

- **Quote values that contain `: `** (a colon and a space), or YAML fails to
  parse the file and the build stops. `bun run test` names such files.
- **Sidebar order**: `index` first, then by `order` (missing = 999), then
  alphabetically by title. Chinese pages sort by their English counterpart,
  so both locales list pages in the same order; the Chinese labels are the
  Chinese titles. A Chinese page keeps the same `order`, `group` and `demo`
  as its English page (`tests/content.test.ts`).
- **Groups**: a section's pages can be listed under group headings
  (`group:`, a key of `GROUPS` in `src/lib/sections.ts`; the Guide uses
  them). A group is a run of consecutive pages, so keep a group's `order`
  values together.
- **The heading comes from `title`.** A leading `# Title` in the body is
  removed, so pages may keep one for GitHub. The first paragraph is shown as
  the page's standfirst.
- **Link to files, relatively**: `[List](../elements/list.md)`,
  `[signals](./reactivity.md#signals)`. They become site URLs, and a Chinese
  page's links stay in Chinese. A link leaving `website/`
  (`../../docs/dev/architecture.md`) points at the file on GitHub.
- **Callouts** are GitHub alerts: `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`,
  `> [!WARNING]`, `> [!CAUTION]`.
- **Code blocks** highlight `tsx ts js json sh bash swift java kotlin rust
  toml diff xml html yaml css`.

Each page is also served as Markdown at its URL plus `.md`; `/llms.txt`
lists every page and `/llms-full.txt` holds all English pages.

## Deployment

`.github/workflows/website.yml` builds the site on every push to `main` that
touches the website or the crates, and publishes `dist/` to GitHub Pages at
<https://native-shell.com> (`public/CNAME`; `SITE_URL` in `src/lib/site.ts`).
CI sets `NSH_REQUIRE_PLAY=1`, so a failed player build fails the deployment
instead of publishing pages without their live demos. Pull requests build and
test without deploying.

## Playground

`/playground` (`src/components/Playground.astro`, `src/lib/playground-client.ts`)
edits and runs one-module apps in the browser: CodeMirror, and the player in
playground mode (`play/index.html?playground`, which loads
`nsh-playground.wasm`, the runtime with the TSX compiler). Demo panels link to
it with `#demo=<name>`; Share links carry the code itself (`#code=`, deflated,
base64url).

## Demos

`demos/<name>.tsx` are small runnable nsh apps (try one with
`nsh run website/demos/<name>.tsx`). A page with `demo: <name>` shows the demo
after its first paragraph: a frame at phone width (390×844 points, scaled down) with
the app running in an iframe of `play/index.html#<name>`, beside its source
(wide pages) or behind a Preview / Source switch (narrow ones), plus an
"Open in new tab" link. The home page runs `demos/counter.tsx` the same way
(`src/components/DemoFrame.astro`, `variant="phone"`).

The player in `public/play/` is the WebAssembly build of the nsh runtime
(`crates/nsh-platform-web`, rendering to the DOM) with every demo bundled by
`nsh build`; it is generated, not checked in:

```sh
bun run play       # = cargo xtask web: nsh.wasm, nsh.js, index.html, demos/*.js (dev and build run it)
```

Run it again after changing a demo or the runtime. Embedded demos follow
the page's theme (`?theme=`, then a message on each change) and take their
colours from `demos/palette.ts`; `demos/nsh.json` gives `nsh/ui/mobile`
components the matching light and dark theme (`demos/graphite-theme.json`).
The playground compiles in the browser, without that theme. The
player is built separately from the site. Whether it exists is decided
at build time: without `public/play/index.html`, pages show the demo's source
only and the home page's host app shows a placeholder. Adding or removing the
player, or editing a demo, invalidates Astro's content cache automatically
(`demoState()` in `src/lib/demo.js`).

## Layout of the code

| Path | Role |
| --- | --- |
| `src/content.config.ts` | The eight collections (four sections × two locales) and the schema |
| `src/lib/sections.ts` | Section labels, page lists in sidebar order, routes |
| `src/lib/remark-*.js`, `rehype-heading-anchors.js` | Title removal, callouts, `.md` links, demo insertion, heading anchors |
| `src/lib/demo.js` | Demo sources, the demo frame and the demo panel markup |
| `src/lib/markdown-endpoint.ts`, `llms.ts` | `<page>.md`, `llms.txt`, `llms-full.txt` |
| `src/pages/[section]/[...slug].astro` | Content pages (and `zh-CN/`); `[section].md.ts` / `[...slug].md.ts` the Markdown |
| `src/components/Home.astro` | The home page in both languages |
| `src/styles/global.css` | Tokens and everything shared; see `DESIGN.md` |
| `tests/` | `content.test.ts` (frontmatter, translations, demos), `links.test.ts` (built site) |
