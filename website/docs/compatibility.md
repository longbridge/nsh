---
title: Backward Compatibility
description: "How Native Shell keeps apps running across releases: what a compatible release promises, the record in nsh-compat that holds every change to it, the release flow, and how to add, rename or change an API."
order: 22
group: delivery
---

# Backward Compatibility

Native Shell promises every app: **an app built for a compatible release runs on every later release, unchanged.**
An app that stops working after an NSH update is a bug in NSH.

The promise exists because a shipped app may never be rebuilt. Host apps download NSH apps as
[signed packages](./signed-packages.md) and run them as plugins. The app's team may have moved on, its source may sit
in a business process, and the host app updates NSH on its own schedule.

This page describes the promise, how NSH enforces it, and how to change NSH without breaking it.

## What an app can rely on

The promise covers everything a built app touches:

| Part | Why it matters |
| --- | --- |
| The exports of every module (`nsh`, `nsh/*`, `nsh/base`, the standard modules) and their members | A bundle imports them by name. If a name is missing, the app does not start. |
| `nsh:rt`, the functions compiled JSX calls | Each bundle carries the output of the compiler that built it. |
| The globals (timers, `fetch`, `console`, ...) and their members | Apps call them by name. |
| Element props, events and their payloads, style keys and their values | Apps pass them by name. A renamed prop is ignored, so the app runs wrong. |
| Tailwind utilities and variants | Classes are resolved at run time from the strings in the bundle. |
| Behavior: defaults, units, payloads, the order of callbacks | An app relies on what it saw when it was written. |
| The TypeScript declarations (`.nsh/types`) | An old app's source still type-checks when its team fixes a bug. |
| The package format and its signatures | A package signed for an earlier release still verifies. |

NSH keeps the promise with these rules:

1. **What a release shipped stays.** No module, export, member, prop, event, payload key, style key or value,
   Tailwind utility, global or `nsh:rt` function goes away.
2. **A rename keeps the old name** forever, as an alias of the same implementation. The old name is marked
   `@deprecated` and points to the new one. An export moved to another module stays in the old module too.
3. **Types only widen.** A union can gain alternatives. A function can gain overloads or trailing optional
   parameters. A parameter can become optional. What apps pass in (props, styles, options) gains optional members
   only. Values an app reads grow the same way: `platform.os` can gain a platform, and a state or a reason can gain
   a case. Existing cases keep their values. Your app should handle unknown values in a `switch`'s `default`:

   ```tsx
   switch (platform.os) {
     case "ios":
     case "android":
       return <MobileLayout />;
     default:
       // The desktops, the web, and platforms NSH adds later.
       return <WideLayout />;
   }
   ```
4. **Behavior is kept.** New behavior is opt-in, through a new prop, option or value.
5. **`nsh:rt` only grows**, each function keeping its signature and meaning.
6. **Formats are read forever.** Later releases read the packages, certificates, revocation lists and `app.json`
   of earlier releases. A package names the lowest runtime it needs (`nsh publish --min-runtime`).
7. **Security is the one exception.** If a capability cannot be kept safe, NSH closes it. The closure is listed with
   its reason and named in the release notes.

The promise starts at the first release recorded as compatible and lasts forever, whatever the version number says.
An API that exists only on `main`, not in a release, may still change.

## How it is enforced

The record of what each release froze lives in a separate repository,
[longbridge/nsh-compat](https://github.com/longbridge/nsh-compat). Changes to NSH are checked against it but cannot
edit it. So a pull request that renames an API cannot also rewrite the record of that API.

```
nsh                                       nsh-compat
├─ crates/          the code              ├─ releases/v<version>/   one per compatible release, never changed
└─ tests/compat/    apps that test the    │  ├─ runtime.txt         every export, global and nsh:rt function
   ├─ cases/        API being written     │  ├─ types/              the declarations apps build against
   └─ examples/                           │  ├─ tailwind.txt        every Tailwind class and what it sets
                                          │  └─ apps/               the test apps as that release built them
                                          ├─ closed.txt             security closures, each with its reason
                                          └─ src/                   the checks
```

Every pull request to NSH runs the `nsh-compat` checks against its checkout. CI runs them in the **Compat**
workflow. Locally, run `cargo xtask compat`.

| Check | What passes |
| --- | --- |
| `checkout_apps_run` | The apps of `tests/compat/`, built by this checkout, do what their scripts say, and every part of the API a script claims to check is really checked. |
| `nothing_released_is_removed` | Everything each recorded release offered (exports, members and their kinds, declarations, Tailwind classes and what they set) is still offered, the same or widened. |
| `released_apps_run` | The apps each recorded release built, as its compiler built them, start on this runtime and do what their scripts say. |

The first check tests the API being written. The other two protect every recorded release. Renaming a released
API fails like this:

```
v0.2.0: runtime [nsh] Platform: removed
Keep it: a rename keeps the old name as a @deprecated alias of the new one, a type only widens.
```

`nsh-compat` is reviewed by NSH's maintainers, and its own CI runs it against NSH's `main`.

## The release flow

1. **Develop.** An API lands on `main` with a case in `tests/compat/`. A case is a small test app whose script
   checks the API. Until a release records the API, you may rename or reshape it, and the case changes with it.
2. **Record.** At a release, `cargo xtask compat record <version>` writes `releases/v<version>/` into a checkout of
   `nsh-compat`. The record holds the API surface of the release and every app of `tests/compat/`, built by its
   compiler, with their scripts. It records an API only if a script checks every part of it.
3. **Freeze.** A pull request to `nsh-compat` merges the record. From then on, every NSH change must pass it and
   every earlier record.

## Developing within the promise

### Adding an API

Every new export, global, prop, event or style key comes with a case:

1. Write a module `tests/compat/cases/<domain>/<case>.tsx` whose default export is a component that uses the API and
   shows its result. The domains are `base`, `core`, `events`, `layout`, `paint`, `props`, `modules` and `web`, plus
   `android`, `macos` and `browser` for what one platform or one `nsh.json` setting decides.
2. Add it to the domain's `App.tsx`, which shows one case at a time.
3. Append the steps to the domain's `script.txt`: `tap id:case-<case>`, then the steps that check what the API does,
   and a `covers <item>` step for each part it adds.
4. Run `cargo xtask compat` (`NSH_COMPAT_APP=<domain>` runs one domain).

```txt
# props-media-video
tap id:case-props-media-video
prop id:clip src https://example.com/clip.mp4
prop id:clip paused true
tap id:play
prop id:clip paused false
covers [props] src
covers [props] paused
```

NSH verifies each `covers` step. The app must use the item. For a prop, event or style key, removing it from the
built app must make a step of the script fail. Items are named `[<module>] <export>`, `[globals] <name>`,
`[props] <name>`, `[events] <name>`, `[styles] <name>`, a component's own as `[props] <Component>.<name>`, and a
platform's props as `ios.<name>`, `android.<name>`, `desktop.<name>`, `web.<name>`.

### Renaming or moving an API

Add the new name and keep the old one as an alias of the same implementation, declared `@deprecated` with the new
name. Update the case and its script to the new name. If no release recorded the API yet, rename it directly.

### Changing a type

Widen it: more alternatives in a union, more overloads, trailing optional parameters, optional new members of what
apps pass in. A union apps read gains members the same way, each new member for a new case. Parameter names are free to change; their count, optionality and types stay.

### Changing behavior

Keep the old behavior and add the new one behind a new prop, option or value. A bug fix that changes what a working
app sees is also a behavior change. Put it behind an opt-in, or make sure no app can rely on the old result. Add a
step for the behavior to a case, so it stays fixed.

### Changing the compiler

No extra steps. Each release's apps stay as that release's compiler built them, and your compiler builds the cases
fresh. The functions the compiler emits into bundles (`nsh:rt`) only grow.

### Closing an API for security

First, a pull request to `nsh-compat` adds the entry to `closed.txt` with the reason, as `<entry> -- <reason>`. Write
the entry exactly as the check reports it. After that lands, the NSH change that removes the API passes. The release
notes explain why.

## Scripts

A `script.txt` has one step per line.

Steps that act: `tap <selector>`, `type <selector> <text>`,
`key <selector> <key>`, `scroll <selector> <x> <y>`, `touch start|move|end|cancel`, `event <selector> <onEvent>
[<key>=<value>...]`, `wait <ms>`, `keyboard <height> <duration>`, `pixelratio <ratio>`.

Steps that check:
`expect <text>`, `absent <text>`, `frame <selector> <x> <y> <width> <height>`, `origin`, `prop` / `noprop`,
`style` / `nostyle`, `listens`, `state`, `statusbar`, `log <text>`.

The first lines may set up the screen: `viewport <w> <h>`, `platform <os> [desktop]`, `fontscale <scale>`.

A selector is `id:<testID>`, `label:<label>`, `text:<text>` or `span:<text>`. Write props and styles as apps write
them (`style id:card backgroundColor #3366ffff`). When writing a script, set `NSH_COMPAT_DUMP=1` to print what each
step adds to and removes from the screen.

## Commands

```sh
cargo xtask compat                      # the checks against this checkout
NSH_COMPAT_APP=props cargo xtask compat -v
cargo xtask compat record 0.2.0         # at a release: record it in nsh-compat, then open a pull request there
```

`cargo xtask compat` keeps a checkout of `nsh-compat` in `target/nsh-compat/` and updates it on each run. If that
checkout has local changes, the command uses it as it is. This lets you try a change to the checks against NSH
before you open its pull request.
