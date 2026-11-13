# Static generation (`nsh render`)

`nsh render` writes a URL of an app as what a browser needs before the app runs, at build time: static HTML of its
first screen that the browser lays out at any width, the page's head, and the data the app read while it rendered.
It is static site generation: a build runs it once per page and publishes the files. NSH does not render when a
page is requested: there is no server, no cache and no per-visitor page, and a page is the same for every visitor.

It runs the app the way the native hosts do, in QuickJS on the core with no browser, in milliseconds a page, inside
NSH's sandbox (the app's `nsh.json` capabilities).

Builds on the web's document markup (headings from `role="header"`, links from `Link`'s `href`,
`web={{ element }}`), its addresses (`UrlScheme.url()` / `setUrl`), `nsh/web`'s `Head`, the web runtime's
`host.params` and `Core::layout_css`.

## Goals

- First screens for crawlers and visitors: document HTML, the head (`Head`), the text the app shows, laid out by the
  browser at any width and matching what the app then draws to within a pixel.
- The data the page was rendered with, embedded as the app's `host.params` (`{ fetches }`), so the app's first frame
  in the browser shows the same thing and takes over without a flash.
- The links the page shows (a build renders the pages they lead to).
- Bounded and deterministic: the engine's default limits (`EngineConfig::limits`), a manual clock and a time limit;
  the same app, URL, data and options give the same bytes.

## Non-goals

- Serving, caching, revalidating or rendering on request; deciding what to render when.
- Pages that differ per visitor (a session, a cookie), and an HTTP status: a rendered page is public, and a host
  that serves static files answers for a path it has no file for.
- Hydration: the app replaces the static copy (`#prerendered`) once it drew its first frame; nothing is attached to
  the copy's elements.
- Rendering for any web platform other than NSH's own web runtime (the copy is what `nsh.js` draws, and the packaged
  page replaces it).

## How it works

```
build ──nsh render <app> --path /notes/apple --base https://notes.example.com/──▶ nsh render
                                                                      │ a fresh engine: QuickJS + core,
                                                                      │ manual clock, the app's bundle
                                                                      │ and capabilities; fetch: the real
                                                                      │ network, responses recorded;
                                                                      │ run until settled (below)
                                                                      ▼
                                                  nsh-html Document (the mounted tree, from MountBatches)
                                                    + Core::layout_css per view
                                                                      ▼
build ◀──{"path":…,"html":…,"css":…,"head":…,"title":…,"lang":…,"links":[…],"fetches":{…},"timedOut":false,"ms":12}──
```

`nsh render [<app>] [--path <path>] [--base <url>] [--appearance light|dark]` renders one page and prints JSON; the
build writes the files (`examples/seo-site/build.mjs`). The app is its entry module or its directory, which the
command compiles as `nsh build --release` does.

| Option | Meaning |
|---|---|
| `--path` | The URL path of the page, with its query (`/` by default): the app starts at it (`UrlScheme.url()`) |
| `--base` | The site's URL, `http(s)` (`http://localhost/` by default): relative `fetch` URLs, the app's addresses and the canonical links resolve against it |
| `--appearance` | The appearance the app renders in (light by default) |

The JSON carries `path`, `html` (the root's markup), `css` (the page's own rules under its root `nsh-flow` and its
views' classes), `head` (the head's elements as markup), `title`, `lang`, `links` (the site's paths the page links
to), `fetches` (key → JSON of the responses the app read), `timedOut` and `ms`; when the render failed, `path` and
`error` alone (the build then publishes the app without a static copy of that page, or fails, as it chooses).

URLs are parsed, never compared as text (`render.rs`, the `url` crate): a link or a request is the site's when its
origin is `--base`'s, and a canonical URL resolves as `web/browser/page.js` resolves it (a path from the site's
directory; a relative path, `//host/x` or an absolute URL as a browser resolves a link).

### Running the app

A headless engine as `nsh test` starts one (`nsh_testing::headless`, feature `js`): QuickJS, the core and NSH's
modules, no platform. Its clock is manual (`Clock::manual`): timers and animations move only as the renderer moves
the clock, so a render's output does not depend on the machine's speed.

**Settled** (`render.rs` `settle`) is the headless driver's own settling (`Headless::settle`: the engine idle,
`EngineControl::flush`, its batches applied, animations that end by themselves finished), then no request in flight
and one more window of the clock (100 ms: the timers due within it run) that brings no batch. So data loaded after a
`setTimeout(…, 0)` or a short debounce is in the page, while an interval (a price polled every two seconds) never
holds the render back. A request in flight is waited for in real time with the clock still. Entrances end at their
last frame, which is also what a visitor sees once they played. The whole render has 5 seconds; reaching them
renders what is mounted and reports `timedOut`.

**Network.** `fetch` goes to the network (`nsh_testing`'s `UreqNetwork`, the one `nsh test` uses for unstubbed
requests), checked against the app's `network` capabilities as on every platform. Relative URLs resolve against the
`--base` URL: the bundle's base URL (`Bundle::with_base_url`), which the web runtime sets from `document.baseURI`
and no native host sets. Each response of the site (a request of `--base`'s origin) with status 200 whose body is
JSON is kept (`render.rs` `Capture`) and returned as `fetches`: a `GET` under its path and query
(`/api/notes?tag=q3`), another method under its method, path and body as sent (`POST /api/search {"q":"q3"}`), the
keys the app looks them up by.

### The head

On the hosts that draw views `nsh/web`'s `Head` renders nothing: no node, no op, no mount item (`nsh-js`
`web_page.rs`). Only in an app that is a page of a site, one whose VM has a base URL (`HostOptions::base_url`, from
`Bundle::with_base_url`, which `nsh render` sets), is a `Head` a zero-sized native component (`nsh/head`) whose
props (`title`, `description`, `canonical`, `lang`, `meta`, `jsonLd`) reach the mount batches as a component's
(`MountItem::ComponentProps`). `nsh_html::Document::head` folds the heads the page shows in tree order, later ones
winning per field and `meta` by `name` / `property`, one without a `content` left out, as `web/browser/page.js`
does; `render.rs` `head_html` writes the elements `page.js` writes. The web runtime keeps its own `Head`, which
writes the page's `<head>`; the two read the same props. No new op, tag or flag: the base URL already marks a page
for requests and media URLs.

### HTML from the mounted tree: `nsh-html`

A crate with no platform (`crates/nsh-html`, depends on `nsh-core` only) that `nsh render` and the web backend share:

- **CSS of styles** (`css`, moved from `nsh-platform-web`'s `encode.rs` and `text/css.rs`, which use it; hidden
  from the crate's documentation, not an API of its own): `view_css` (backgrounds, gradients, borders, radii,
  shadows, filters, transforms, opacity), `text_css` (fonts, colors, decorations, numeric variants, shadows),
  `css_color`, `css_font`, `object_fit`. One implementation keeps the static copy and the live app drawing the same
  styles.
- **`Document`**: applies `MountBatch`es (`Create`, `Insert`, `Remove`, `Delete`, `ViewStyle`, `Text`, `ViewProps`,
  `ScreenConfig`, `Image`, `ScrollConfig`) to a model of the tree, the way `nsh.js` applies them to the DOM, and
  serializes it: each view the element the web draws it as (`web={{ element }}`, a heading for `role="header"` by its
  level, `<a href>` for a `Link`, else `div`; `nsh.js` `elementTag`), ARIA from its accessibility
  props, the classes of its styles (each distinct CSS text a class `nsh-s<n>`, numbered in the order the page first
  uses them and defined once) and its layout from `Core::layout_css` as the element's inline style; a text out of
  the layout (`display: none`, which the core still mounts) stays hidden; borders as the runtime draws them (an overlay, with the border's
  room reserved in the layout); texts as spans per style run, wrapped by the browser, each line as tall as the style's
  line height (`ResolvedTextStyle::line_height`, else the font's `normal`), the element's own line box zero
  (a strut fix); images as `<img alt>`; scroll views as boxes that scroll; a page's header first in
  its page; only the top page of each stack. Kinds with no static form (canvases, shaders, videos, web views, native
  components) are boxes of their layout's size, with their accessibility label as text where they have one.

**Layout as CSS.** `Core::layout_css` (`nsh-core` `core/flow_css.rs`) writes a view's taffy style: display,
position and insets (a relative view's offsets too), the flex container and item properties, grids (templates with
their tracks' minimums and line names, implicit tracks, `grid-auto-flow`, placements by number, span or name), sizes
with shares and the sizing keywords (`min-content`, `max-content`, `fit-content`), margins, padding, the borders'
room, `box-sizing: content-box` and `direction: rtl`. Its limits: a `calc()` length and `stretch` have no form
there and are left out (the browser uses the property's initial value), and `grid-template-areas` is never written
because the core has no style for it.

What the HTML does not depend on: the renderer's text measurements. The browser lays the copy out itself, so a text's
width on the server changes nothing in the HTML. Measurements matter only where the app or the core reads them: a
virtualized list's visible rows (the copy holds those rows), `onLayout` handlers, and pixels. `nsh render` therefore renders with the core's `ApproxTextEngine`.

## Where the code goes

| Part | Location |
|---|---|
| CSS of styles, `Document` | `crates/nsh-html` (`nsh-platform-web` depends on it) |
| A view's layout as CSS | `nsh-core` `core/flow_css.rs` (`Core::layout_css`), asked of the engine with `EngineControl::layout_css` |
| `Head` off the web | `nsh-js` (`web_page.rs`: nothing without a page, the `nsh/head` component in one), `nsh_html::Document::head`, `render.rs` `head_html`; the web's own in `web/browser/page.js` |
| The page the app opens | `nsh_testing::Headless::start_at_page`, the function web apps' tests start a page with: the page's URL reaches the core as the web's address at start (`EventKind::UrlScheme` with `page`, `quiet` and `base`) before the app's code runs, so `UrlScheme.url()` reads it; relative requests and the app's addresses resolve against `Bundle::base_url`, the site's URL, as a page with a `<base href>` resolves them |
| `nsh render` | `crates/nsh-cli/src/render.rs`: renders on a headless engine (`nsh_testing::Headless`, manual clock), settles, asks the engine for the views' layout (`EngineControl::layout_css`), writes the page with `nsh_html::Document`; `Capture` keeps the JSON bodies of the site's responses |
| The example | `examples/seo-site/build.mjs` (renders each page, writes it into the packaged page) |

Open: an app's own Rust components (`nsh.json` `"rust"`) need a runner built with its sets, as `nsh test` builds;
`Document` writes canvases, shaders, videos and web views as empty boxes; lists render the rows in view.

## Verification

- **The writer** (`nsh-html` `document.rs`'s tests): elements by role and `web={{ element }}`, escaping, a text's
  runs, images, a hidden text, a stack's top page, the heads' folding.
- **The layout** (`nsh-core` `core/tests.rs` `layout_css_*`): flex, grids, insets, shares and the box.
- **The command** (`nsh-cli` `render.rs`'s tests): the head's elements, the URLs (`--base`, `--path`, canonical
  links, what is the site's), and an app rendered end to end, whose `<Head>` and views are the page's.
- **The hosts** (`nsh-engine` `tests/engine/native.rs`): a `Head` mounts nothing without a page and carries its
  props in one.
- **The example**: `examples/seo-site`'s pages render with `nsh render` and show their headings, paragraphs and links
  without JavaScript.

Not tested yet: that two renders of a page give the same bytes, and an app that never settles or runs into its
limits (it ends with `timedOut` or an `error`).

## Alternatives considered

- **Headless Chrome**: exact, since it is the browser runtime itself, but about a second and hundreds of megabytes a
  page, and the app's code runs in a page origin rather than NSH's sandbox.
- **The web runtime in Node with a DOM implementation** (jsdom, happy-dom): no canvas text measurement, layout or
  CSSOM fidelity, and still a JavaScript engine of the platform's rather than NSH's sandbox.
- **Frames as absolute positions**: exact at one width, wrong at every other; a page opened on a phone showed the
  desktop layout, then jumped.
