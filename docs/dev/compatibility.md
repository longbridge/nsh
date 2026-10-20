# Backward compatibility

NSH runs apps it did not build: host apps download signed packages and run them as plugins, and an app that shipped
may never be built again (its team moved on, its source is frozen in a business process, the host app updated NSH
without asking). So every release of NSH runs every app built for an earlier one, unchanged. An app that stops
starting after an NSH update is a bug in NSH, never in the app.

This page is the contract and how it is enforced. The rules hold from v0.1.2 on, the first release whose apps `api/`
keeps, and for good, whatever the version number says; earlier versions made no promises.

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

The standard catalog also imports `buffer`, `crypto`, `path`, `url` and `zlib` from quickjs-stdlib. That project
owns their API, declarations and exhaustive compatibility tests, including its `Buffer` and cryptography globals.
NSH keeps the catalog, native integration, capability checks and resource limits under local tests; `api/` records
NSH-owned declarations and behavior rather than copying the upstream API into its fixture coverage requirements.

## The rules

1. **Nothing that reached `main` is removed or renamed**, released or not: no module, export, member, prop, event,
   payload key, style key or value, Tailwind utility, global or `nsh:rt` function. Removing something wrong-headed is
   not a reason either: it is deprecated (rule 2) and stays.
2. **A rename adds the new name and keeps the old one** as an alias of the same implementation (the same function or
   class object at run time), declared `@deprecated` with the new name, and kept for good. The same goes for moving an
   export between modules: the old module keeps exporting it.
3. **Types only widen.** A union gains alternatives, a function gains overloads or trailing optional parameters, a
   parameter becomes optional, a prop accepts more values, a function or method that returned `void` returns a value
   (a callback an app passes in keeps returning `void`). Nothing else narrows or changes a return type, and what apps
   pass in (props, styles, options, configs) gains optional members only. Parameter names are not part of a type:
   TypeScript compares function types by structure, so `(data: number) => void` and `(value: number) => void` are one
   type, and a function, method or callback may rename its parameters; their count, optionality and types may not
   change.
4. **Behavior is kept.** Defaults, units, coordinate spaces, the payload of an event and when it fires stay as they
   were. New behavior is opt-in, through a new prop, option or value. A bug fix that changes what a working app sees
   is a behavior change: fix it behind an opt-in, or make sure no app can rely on the old result.
5. **`nsh:rt` is append-only.** The JSX compiler may start emitting new functions; the ones it emitted before keep
   their signatures and semantics, because bundles built by older compilers call them as those compilers did.
6. **Formats are read forever.** Readers of packages, certificates, revocation lists and `app.json` accept every
   format an earlier release wrote. New fields are optional. The other direction holds for packages too: a runtime
   ignores the header fields, files and capabilities a later release added (never granting what it does not know),
   unless the header's `critical` names them, so a package published with a lower minimum runtime
   (`nsh publish --min-runtime`) runs on the older runtimes it names ("Format evolution" in `nsh-package`).
7. **The only exception is security**: a capability that cannot be kept safe may be closed. The pull request that
   closes it adds the entries it removes to `api/closed.txt`, each with its reason (`<entry> -- <reason>`, the entry
   as the `compat` tests report the break: `[<module>] <path>`, `<file>: <Name>` or `<file>: <Name>.<member>`; the
   file's header has the details), and says why in its description and the release notes. `api/closed.txt` is the
   only list of removals there will ever be.

## How it is enforced

Compatibility rests on a test suite of old apps that never changes, as the web platform's rests on web-platform-tests:
while every fixture passes unchanged, a change keeps the apps built before it working. `api/` at the root of the
repository holds that suite and the record of what apps depend on:

```
api/
  runtime.txt           every module's exports (`nsh:rt` included) and the globals, with their members and kinds
  types/                the declarations of `.nsh/types`
  tailwind.txt          every Tailwind class the resolver offers and the style it resolves to, in each context
  apps/<app>/           fixture apps: bundle.js and assets/ as some checkout's `nsh build --release` wrote them
    script.txt          what a user does and sees in it, and the API it checks (`covers`)
  cases/<case>.tsx      the sources of small fixtures: the tests of single APIs
  uncovered.txt         the API no fixture's script checks yet; it only shrinks
  closed.txt            the entries closed for security (rule 7), each with its reason
```

The record and the suite only grow, as the web platform does: a name that shipped is kept, whatever release shipped
it. There are no copies per version. Git's history is the baseline: every change reaches `main` through a pull
request, each pull request is held to `api/` as of its merge base, so by induction everything that ever reached
`main`, every release included, is still offered and still passes its tests. Releases need nothing from `api/`.

The engine's tests (`crates/nsh-engine/tests/engine/compat/`) check, on every pull request (the Core workflow):

- `surface_is_recorded`: `api/runtime.txt`, `api/types/` and `api/tailwind.txt` are the code's surface. Any change to
  the API, additions included, shows up as their diff in review. `cargo xtask api` records it.
- `nothing_merged_is_removed`: everything `api/` held at git's merge base with `main` (on `main`, the parent commit;
  `NSH_API_BASE=<rev>` names another) is still offered, by rules 1 to 3: no name of `runtime.txt` is gone or changed
  kind; every declaration of `types/` is kept, the same or widened (`compare.rs` says exactly what counts as widening;
  it compares declarations with their parameters' names erased); every class of `tailwind.txt` still resolves to the
  same line (a class may be added, its output never changes). Entries in `closed.txt` are the only exceptions. The
  files of the fixture apps are unchanged, and their scripts only gain steps. In CI the test needs git's history and
  fails without it; locally, without it, it skips with a note.
- `apps_still_run`: each fixture app, as it was built, starts on this runtime without an error and does what its
  script says. This is what catches what names cannot: `nsh:rt`'s calling conventions, props and styles that still
  parse, behavior.
- `coverage::every_api_is_tested`: every export, global NSH provides (the language's own builtins aside), host prop,
  event and style key is checked by a fixture's script, or listed in `uncovered.txt`; the list holds nothing a script
  checks. A script claims what it checks with `covers <item>` steps (`[<module>] <name>`, `[globals] <name>`,
  `[props] <name>`, `[events] <name>`, `[styles] <name>`; platform props as `ios.<name>`, `android.<name>`, ...).
  Each claim is verified: the bundle uses the item, and for props, events and style keys, renaming the item in the
  bundle makes a check step of the script fail. For exports and globals only the use can be verified: the claim is
  the author's word that a step checks the result.
- `coverage::uncovered_only_shrinks`: `uncovered.txt` gains no entry over the merge base, so a new API lands with its
  test.
- `declarations_are_understood`: every declaration is one `compare.rs` can read, so none escapes the comparison.

`tailwind.txt` has one line per class, `<class> <style>`: the resolved style's fields that differ from the defaults,
then ` | <context>: <style>` for each context (dark, a platform, a window width, a state, a group's state, a sibling
position, rtl, reduce motion) where the class resolves differently. The classes are enumerated from the code: the
resolver's utilities and prefixes, each prefix with every theme scale name, the component theme's tokens, a number
scale, fractions and representative arbitrary values, and each variant on one utility. The style is printed from the
resolver's own types, so renaming one of their fields changes the record without changing what apps see: such a pull
request records it with `cargo xtask api` and says so.

### Changing the API

1. Make the change additive (the rules above).
2. Test it: write `api/cases/<case>.tsx`, a small app that uses the new API and shows its result, keep its build with
   `cargo xtask api --keep api/cases/<case>.tsx`, and write steps in `api/apps/<case>-<version>/script.txt` that check
   the result, with a `covers <item>` step for each part. `--keep` records the surface before adding the fixture;
   coverage is checked after its script is complete, by the following plain `cargo xtask api` run.
3. Run `cargo xtask api` and commit `api/` with the change: the record's diff is the review of the API change, next
   to the `## Public API` section of the pull request, and `uncovered.txt` loses what the scripts now check. Covering
   an older entry of `uncovered.txt` is welcome the same way.
4. `cargo test -p nsh-engine --test engine compat::` must pass with the fixtures' bundles, assets and existing steps
   unchanged and nothing added to `api/closed.txt` (rule 7 aside).

### Keeping apps

A fixture's `bundle.js` and `assets/` are never rebuilt or changed: they are what a compiler of their time wrote, and
apps built then run as they are. When the JSX compiler's output changes (it starts emitting a new `nsh:rt` function,
for one), keep what the new compiler writes too:

```sh
cargo xtask api --keep <app>...
```

`<app>` is an example's name, or the path of an app directory or of a single entry module (`api/cases/<case>.tsx`).
It is built with this checkout into `api/apps/<name>-<version>/`, with the newest script of that app or a stub. Check
that the scripts pass (`compat::apps_still_run`), extend them to what the change added, and commit the fixtures with
the change. Existing steps stay.

### Scripts

`script.txt` is one step per line (`compat/apps.rs` has the full list): `expect <text>` waits for a text on screen,
`absent <text>` checks one is gone, `tap id:<testID>` / `label:<label>` / `text:<text>` presses a view (or its nearest
pressable ancestor), `wait <ms>` runs timers, and `viewport <w> <h>` and `platform <os>` (first) size the screen and
pick the platform whose `ios={{…}}` / `android={{…}}` / `desktop={{…}}` / `web={{…}}` props apply. What a view looks
like is checked as the platform receives it: `frame <selector> <x> <y> <width> <height>` (its layout, in points
relative to its parent), `prop` / `noprop` (a host prop), `style` / `nostyle` (a paint or text style key), `listens`
(an event the platform reports); `log <text>` checks a `console` line, and `covers <item>` is a claim, not a step.
Other steps stand in for the platform's input: `event <selector> <onEvent> [<key>=<value>...]` reports an event with
its payload keys as the handler reads them (`event id:video onProgress currentTime=3.25 duration=12.5`),
`touch start <selector> <x> <y>` / `touch move|end|cancel [<x> <y>]` move one finger through the touch handlers and
the responder system, `type <selector> <text>` types into an input (`onKeyPress`, `onChangeText` and
`onSelectionChange` per character), `key <selector> <key>` presses a key, and `scroll <selector> <x> <y>` scrolls a
scroll view (`onScroll`).
Props and style keys are named and written as apps write them (`style id:card backgroundColor #3366ffff`,
`prop id:field ios.keyboardAppearance dark`), in the stable text forms `compat/values.rs` defines, never from the
core's types, so the scripts stay valid while those change. A script should open every screen of its app and check
what its app offers. `NSH_API_DUMP=1` prints the texts each step adds and removes:

```sh
NSH_API_DUMP=1 cargo test -p nsh-engine --test engine compat::apps_still_run -- --nocapture
```

## What the tests do not see

The tests hold the names, the declarations, the Tailwind classes and what the fixtures check. They cannot prove that
a behavior no script checks is unchanged, or that a platform backend draws the same: rules 4 and 6 are also the
reviewer's job. A pull request that changes behavior apps can see says so, and every behavior worth keeping gets a
step in a fixture's script.

The host side (the iOS and Android SDKs, `nsh.h`, `AppView`) is the host app's API, rebuilt with each NSH update; it
follows the crates' versions and is outside `api/`.
