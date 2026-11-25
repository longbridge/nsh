# What reviews missed

Bugs that passed review and CI, grouped by pattern, each with the case that
showed it. Read this before reviewing or changing a public API, an input path
or a platform backend. When a bug of a new kind turns up after merge, add a
pattern here with its PR. Area-specific facts belong in
[Pitfalls](pitfalls.md); this page is the checklist that applies across areas.

## One of a thing an app wants several of

An API that holds one of something an app could want several of becomes a
rewrite later, in every backend at once.

- `SetTray` keeps one tray icon per app: "the last window's wins"
  (`crates/nsh-core/src/desktop.rs`). An app with two windows that each want
  a status item cannot have both.
- Global shortcuts got this right: each registration has its own id and each
  press says which.

Ask of every new API: can an app want two of these at once (windows, tray
items, shortcuts, timers, sessions, players)? If so it takes a list or is
made per instance, and each callback says which one fired.

## Every press gets its release

An app that starts something on a press stops it on the release. A lost
release leaves it running. Check every path that drops, redirects or filters
events while a key or button is down: another window, a grab, focus moving,
a view being removed or disabled.

- macOS: a modifier released while another window had the keyboard was
  never recorded, so the surface reported it as still held (#3052).
- Linux: a view kept GTK keyboard focus after props disabled it or a style
  hid it (#2481).

## Changes applied one by one

Applying a list of changes in order can collide with itself.

- A swap `[a, b]` → `[b, a]` registers the new `a` while the old one still
  holds it. Release every old binding first, then take the new ones.
- A shorter list: a diff that walks the old length indexes past the new one.
- An item left as it was but refused now, because an earlier item took its
  resource: a diff by "changed or not" alone skips it.

## Work deferred to later

Work deferred to idle, a later frame or an async answer runs after the events
already queued. Until it runs, the old state still answers, and when it runs,
what it was for may be gone. Check what answers in between, and that the
deferred work still applies when it runs.

- macOS: a deferred view painted after the surrounding ShaderEffect had
  captured its input, so the effect showed stale glyphs (#2483).
- Testing: a host call answer that landed after the engine reported idle was
  missed, and the step ended before the app ran it (#2848).
- Windows: a focused field outlived its destroyed window and was dropped at
  thread exit, after the timers it touched were gone (#1801).
- Linux global shortcuts: each change closes the portal session at once and
  binds the whole set in a new one. Shortcuts the app kept do nothing until
  the new session is bound, and if the user declines the portal's dialog for
  the new set, the kept ones stay dead while the app was told they are
  registered (`crates/nsh-platform-linux/src/global_shortcut.rs`, `bind`).

## Tested on the platform

A unit test of a state machine proves the machine, not the OS. Grabs, focus,
portals, IME and timing show only on the real system. A change to a
platform's input or window code runs on that platform before it is called
done, and the PR says what was not run.
