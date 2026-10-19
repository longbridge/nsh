# Backward compatibility

NSH runs apps it did not build: host apps download signed packages and run them as plugins, and an app that shipped
may never be built again (its team moved on, its source is frozen in a business process, the host app updated NSH
without asking). So every release of NSH runs every app built for an earlier one, unchanged. An app that stops
starting after an NSH update is a bug in NSH, never in the app.

This page is the contract and how it is enforced. The rules hold from v0.1.2 on, the first release `api/` keeps;
earlier versions made no promises.

## What apps depend on

| Surface | Where it is | Why it matters |
|---|---|---|
| The exports of every module (`nsh`, `nsh/*`, `nsh/ui/mobile`) and their members | `crates/nsh-js` (`modules.rs` and the tables it lists), `crates/nsh-ui-mobile` | a bundle imports them by name: one missing name and the module fails to link, so the app never starts |
| `nsh:rt`, what compiled JSX calls (`element`, `prop`, `insert`, ...) | `crates/nsh-js/src/dom.rs` (`RT_EXPORTS`) | bundles carry the output of the compiler that built them, so every function any compiler ever emitted keeps its name, its arguments and what it does |
| The globals (web APIs, timers, `console`) and their members | `crates/nsh-js` (`host.rs`, `web/`) | as above |
| Host element props, events, style keys and their values | `crates/nsh-core/src/js/` (`Prop`, `host::EVENTS`, `style_keys!`) | bundles pass them by name; a prop renamed is silently ignored, so the app runs wrong |
| Tailwind utilities and variants | `crates/nsh-core/src/tailwind/` | resolved at run time from the class strings in the bundle |
| Behavior: defaults, units, event payloads, the order of callbacks | everywhere | an app relies on what it saw when it was written |
| The TypeScript declarations (`.nsh/types`) | generated (`crates/nsh-js/src/declarations.rs`) | an old app's source must still type-check and build when its team fixes a bug |
| The package format (`.nshpkg`) and its signatures | `crates/nsh-package` | a package signed for an earlier release still verifies; it carries its JS source, which runs when its bytecode is not this runtime's (`BYTECODE_VERSION` may change freely) |

## The rules

1. **Nothing released is removed or renamed**: no module, export, member, prop, event, payload key, style key or value,
   Tailwind utility, global or `nsh:rt` function. Removing something wrong-headed is not a reason either: it is
   deprecated (rule 2) and stays.
2. **A rename adds the new name and keeps the old one** as an alias of the same implementation (the same function or
   class object at run time), declared `@deprecated` with the new name, and kept for good. The same goes for moving an
   export between modules: the old module keeps exporting it.
3. **Types only widen.** A union gains alternatives, a function gains overloads or trailing optional parameters, a
   parameter becomes optional, a prop accepts more values. Nothing narrows, a return type never changes, and what apps
   pass in (props, styles, options, configs) gains optional members only.
4. **Behavior is kept.** Defaults, units, coordinate spaces, the payload of an event and when it fires stay as they
   were. New behavior is opt-in, through a new prop, option or value. A bug fix that changes what a working app sees
   is a behavior change: fix it behind an opt-in, or make sure no app can rely on the old result.
5. **`nsh:rt` is append-only.** The JSX compiler may start emitting new functions; the ones it emitted before keep
   their signatures and semantics, because bundles built by older compilers call them as those compilers did.
6. **Formats are read forever.** Readers of packages, certificates, revocation lists and `app.json` accept every
   format an earlier release wrote. New fields are optional.
7. **The only exception is security**: a capability that cannot be kept safe may be closed. That change edits the
   frozen baseline in the same pull request, says why in its description and the release notes, and is the only
   reason a file under `api/v*/` is ever edited by hand.

## How it is enforced

`api/` at the root of the repository holds what apps depend on, as text the tests compare:

```
api/
  current/              this checkout's surface (must match the code exactly)
    runtime.txt         every module's exports and the globals as the runtime makes them, with their members and kinds
    types/              the declarations of `.nsh/types`
  v0.1.2/               the baseline of a release, frozen when it is made
    runtime.txt
    types/
    apps/<app>/         what that release's `nsh build --release` wrote for an example app (bundle.js, assets/)
      script.txt        what a user does and sees in it
```

The engine's tests (`crates/nsh-engine/tests/engine/compat/`) check, on every pull request (the Core workflow):

- `current_surface_is_recorded`: `api/current/` is the code's surface. Any change to the API, additions included,
  shows up as a diff of `api/current/` in review. `cargo xtask api` records it.
- `releases_are_still_offered`: everything every baseline offered is still offered, by rules 1 to 3: no name of
  `runtime.txt` is gone or changed kind, and every declaration of `types/` is kept, the same or widened (`compare.rs`
  says exactly what counts as widening).
- `apps_of_releases_still_run`: each kept app, as its release built it, starts on this runtime without an error and
  does what its script says. This is what catches what names cannot: `nsh:rt`'s calling conventions, props and
  styles that still parse, behavior.
- `the_version_has_a_baseline`: once the workspace version is a release's, `api/v<version>/` exists. The release
  workflow checks it too, so a release cannot be tagged without its baseline.

### Changing the API

1. Make the change additive (the rules above).
2. Run `cargo xtask api` and commit `api/current/` with the change. Its diff is the review of the API change, next to
   the `## Public API` section of the pull request.
3. `cargo test -p nsh-engine --test engine compat::` must pass without touching `api/v*/`.

### Making a release

Bump the workspace version, then run `cargo xtask api --freeze`: it records `api/current/` and writes
`api/v<version>/` with the surface and the example apps (`APPS` in `crates/xtask/src/api.rs`) built by this checkout,
each with the script of the previous baseline. Check that the scripts still pass (`compat::apps_of_releases_still_run`),
extend them to what the release added, and commit the baseline with the version bump. From then on it is never edited
(rule 7 aside).

### Scripts

`script.txt` is one step per line (`compat/apps.rs` has the full list): `expect <text>` waits for a text on screen,
`absent <text>` checks one is gone, `tap id:<testID>` / `label:<label>` / `text:<text>` presses a view (or its nearest
pressable ancestor), `wait <ms>` runs timers, and `viewport <w> <h>` (first) sizes the screen. A script should open
every screen of its app and use what the release added. `NSH_API_DUMP=1` prints the texts each step adds and removes:

```sh
NSH_API_DUMP=1 cargo test -p nsh-engine --test engine compat::apps_of_releases_still_run -- --nocapture
```

## What the tests do not see

The tests hold the names, the declarations and what the kept apps do. They cannot prove that a behavior no script
exercises is unchanged, or that a platform backend draws the same: rules 4 and 6 are also the reviewer's job. A pull
request that changes behavior apps can see says so, and a new feature worth keeping working gets a step in the next
release's scripts.

The host side (the iOS and Android SDKs, `nsh.h`, `AppView`) is the host app's API, rebuilt with each NSH update; it
is versioned with the crates, not by these baselines.
