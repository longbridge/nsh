---
title: Building with Rust
description: "Write a Native Shell app entirely in Rust, with no QuickJS on any platform, or in Rust with TSX screens on top: when to choose which, and how the two share components and modules."
order: 5.5
group: start
---

# Building with Rust

Native Shell apps are written in TSX, in Rust, or in both. All three run on the same engine: the reactive graph, the
renderer, Taffy layout and the `nsh/ui/mobile` components are Rust in every case, and every platform renders with its
own UI. What differs is the language of your own code, and with it whether the app carries a JavaScript VM.

| | TSX | Rust | Rust with TSX |
|---|---|---|---|
| Your UI, state and handlers | TSX on QuickJS | Rust | Rust components and modules, TSX screens that use them |
| The app runs | QuickJS and the engine | the engine and your crate only, on every platform and in the browser | QuickJS and the engine, with your crate |
| Suits | Apps and modules delivered at run time, quick iteration | Medium and large apps with demanding business logic | A Rust core with parts that change often |
| Edits | Hot updates | Rebuild and restart | Hot updates for TSX, rebuild and restart for Rust |
| Start | `nsh init my-app --template ts` | `nsh init my-app --template rust` | `nsh init my-app --template mixed` |

## Choosing

**Rust** suits apps whose business logic is large and demanding: trading, editors, data-heavy tools. You get Rust's
type system and compile-time checks across the whole app, the crates.io ecosystem, native performance for your own
code (not only for NSH's), and headless tests of the real UI with `cargo test`. The app is Rust and native code from
start to end: no JavaScript VM ships with it or runs in it, on iOS, Android, macOS, Windows, Linux, GPUI and the web.
Components are functions that run once and bind signals to what they show, as in TSX ([Reactivity](./reactivity.md)),
written with typed styles instead of class strings.

**Rust with TSX** keeps that core and adds TSX where it pays off: screens, settings, onboarding, experiments,
anything you iterate on with hot updates or deliver at run time as a [signed package](./signed-packages.md). TSX
imports your Rust components and modules like any NSH module, so the two halves meet at typed interfaces. The TSX
runs on QuickJS, which the app then carries.

## An app written in Rust

```sh
nsh init my-app --template rust
cd my-app
nsh run                        # this machine's backend; --target ios, android, web, macos, linux, windows or gpui
nsh package --target web       # or ios, ios-simulator, android, macos, linux, windows, gpui
```

`src/lib.rs` holds the app: components, state, handlers, tasks.

```rust
use nsh_core::ui::prelude::*;
use nsh_ui_mobile::{Button, ButtonEvent, ButtonProps};

/// The components available to TSX through `app/native`.
#[native_module("app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

#[component]
pub fn App() -> impl IntoElement {
    let count = signal(0_u32);
    let quote = signal(SharedString::from("…"));
    // Async work runs in tasks; requests go through NSH's network stack.
    spawn(async move {
        if let Ok(response) = fetch("https://api.example.com/quote").await {
            quote.set(response.text().into());
        }
    });
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_4()
        .child(text(move || format!("Count: {}", count.get())))
        .child(text(quote))
        .child(
            Button::element(ButtonProps { title: "Tap".into(), ..ButtonProps::default() })
                .on_event(move |_: &ButtonEvent| count.update(|c| *c += 1)),
        )
}
```

Listeners are closures of their event (`move |text: &str| ..`), and `move || ..` for an event without data;
`.child(..)` takes an element, or an `Option`, a `Vec`, an array or an iterator of elements
(`.child(rows.iter().map(|r| row(r)))`).

`nsh.json` names the root, and `nsh` mounts it:

```json
{
  "name": "My App",
  "rust": ".",
  "root": "App",
  "theme": "theme.json",
  "appearance": "system"
}
```

`"root"` is the component's path in the crate (`App`, `shell::Gallery`; in the first one with
[several crates](#several-crates)); `"theme"` and `"appearance"` apply to it as they do to a TSX app. With it,
`nsh run` and `nsh package` of the app's directory build the app's own runner for the target, with the root mounted
and no QuickJS linked in:

| Target | What runs |
|---|---|
| macOS, Windows, Linux, GPUI | The release runner with your crate, in a window; `nsh run` runs a debug build and shows the app's logs in the terminal |
| iOS, the iOS simulator, Mac Catalyst | The iOS runner with your crate, in an app |
| Android | The app's `libnsh.so` with your crate, started by the runtime's activity |
| Web | `nsh.wasm` with the engine and your crate (3.3 MB for the whole Rust gallery, 0.75 MB with brotli); the page mounts the root |

`nsh` builds what your crate needs and nothing more: a small program of your crate and NSH's core that describes its
components and modules to the CLI, which then does the rest itself, and the runner of each target you run or
package. The first build of a new Rust app compiles your crate and NSH's core, about 60 crates. `nsh trace` mounts
the root headless in a runner of your crate built the same way.

During `nsh run`, a change to the crate's Rust code builds the runner again and restarts the app; in the browser,
the open page loads the new runtime by itself. `cargo run` starts the same root directly on the desktop backend
(`src/main.rs`), and native apps that embed NSH mount a Rust root with `AppSource::Rust` on the `AppView` of macOS,
Windows, Linux, GPUI and iOS.

Code for every platform, the web included, uses NSH's services: `fetch` for HTTP
([Networking](./networking.md#from-rust-components)), `timer`, `set_timeout` and `set_interval` for waiting,
`nsh_core::time::Instant` and `nsh_core::time::unix_ms()` for the clock. `background` and `blocking` run work on the
native targets' threads.

## Desktop title bars

Rust apps draw their own title bars with `nsh_core::ui::desktop`. Call
`set_title_bar(TitleBarStyle::Hidden, Some(height))`, mark the bar's background
with `.window_drag(true)`, and reserve the native window controls' space with
`title_bar()`. Its `top` is the bar's height, and `left` and `right` are the widths
occupied by the system controls. Reading it subscribes to changes, including full
screen. The platform handles window dragging and the user's title-bar double-click
behavior; `.window_drag(false)` keeps a child out of its parent's drag region.

```rust
use nsh_core::{
    desktop::TitleBarStyle,
    ui::{desktop::{close_window, set_title_bar, title_bar}, prelude::*},
};

#[component]
fn TitleBar() -> impl IntoElement {
    set_title_bar(TitleBarStyle::Hidden, Some(42.0));
    view()
        .h(px(42.0))
        .flex_row()
        .items_center()
        .window_drag(true)
        .pl(move || px(title_bar().left + 12.0))
        .pr(move || px(title_bar().right + 12.0))
        .child(text("My app").flex_1())
        .child(text("Close").window_drag(false).on_press(close_window))
}
```

`set_title_bar(TitleBarStyle::Default, None)` restores the system title bar.
`close_window()` follows the embedding host's normal window-close behavior.

## Extending a Rust app with TSX

The crate's `#[native_module("app/native")]` function names what TSX may import: components
(`registry.register::<Chart>()`), modules of functions (`registry.module::<Portfolio>()`, `#[module]` on a unit struct and its `impl`),
classes (`registry.class::<Ledger>()`, `#[class]` on a struct and its `impl`) and assets. In a `#[module]` or
`#[class]` `impl`, the `pub fn`s are exported (`pub(crate)` too); functions without a visibility are helpers that
stay in Rust. Their TypeScript
declarations, doc comments included, are generated from the Rust types into `.nsh/types`, so TSX is checked against
them by `nsh check` and your editor:

```rust
/// A ledger kept in Rust.
#[class]
pub struct Ledger {
    entries: Vec<f64>,
}

#[class]
impl Ledger {
    /// An empty ledger.
    pub fn new() -> Self {
        Ledger { entries: Vec::new() }
    }

    /// Adds `amount`; the new balance.
    pub fn add(&mut self, amount: f64) -> f64 {
        self.entries.push(amount);
        self.balance()
    }

    /// The balance.
    #[getter]
    pub fn balance(&self) -> f64 {
        self.entries.iter().sum()
    }

    /// Uploads the entries; resolves when the server has them.
    pub async fn upload(this: Instance<Ledger>, url: String) -> Result<(), String> {
        let body = this.with(|l| format!("{:?}", l.entries));
        fetch(HttpRequest::post(url, body)).await.map(|_| ())
    }
}

/// The app's holdings.
#[module]
pub struct Portfolio;

#[module]
impl Portfolio {
    /// The value of the holdings.
    pub fn total() -> f64 {
        1250.0
    }
}
```

```tsx
import { mount, signal, View, Text } from "nsh";
import { Button } from "nsh/ui/mobile";
import { Chart, Ledger, Portfolio } from "app/native";   // Rust

const ledger = new Ledger();   // its state lives in Rust

function Overview() {
  const balance = signal(ledger.add(Portfolio.total()));   // Rust, called at once
  return (
    <View className="p-4 gap-2">
      <Text>Balance: {balance()}</Text>
      <Chart symbol="NSH" range="1M" onSelect={(e) => balance.set(ledger.add(e.price))} />
      <Button title="Upload" onPress={() => ledger.upload("https://example.com/ledger")} />
    </View>
  );
}

mount(() => <Overview />);
```

Each export is documented in TypeScript by the doc comment of the Rust item that names it: a component by its
`#[component] fn` (`Chart`), a module and a class by their struct (`Portfolio`, `Ledger`), and props, events,
functions and members by their own (a setter needs none beside its getter). The `Props` struct's and the event
enum's own doc comments, and those of `impl` blocks, stay in Rust; a doc comment on a `#[module]` or `#[class]`
`impl` block is a compile error, since TypeScript would not show it. Each command that writes `.nsh/types` warns
about an export TSX sees without a doc comment, naming it (`Ledger.add`, `Chart.range`).

TypeScript shows a doc comment up to its first heading. Notes for Rust callers, such as what `&self` borrows or
why a function takes `cx`, go under a heading, `# Rust` (or rustdoc's `# Panics`, `# Examples`), and stay in the
Rust docs:

```rust
/// Puts `text` on the clipboard; resolves with the number of characters copied.
///
/// # Rust
///
/// Taking `cx` makes it an op, run in order with the UI changes TS made before it.
pub fn copy(text: String, cx: &mut ModuleCx) -> u32 {
    let count = text.chars().count() as u32;
    cx.set_clipboard(text);
    count
}
```

Props are the fields of the component's `Props` struct and events the variants of its `Event` enum (`onSelect`),
both in camel case. A Rust component's own content, events and state stay in Rust; TSX passes props in and hears
events. A prop of the wrong type (`level={1.5}` for a `u32` prop) is a warning, as on a built-in element: the
prop keeps its value and the rest of the component renders.

A function or a member runs the way its Rust signature says. One that is not `async` runs inside the TSX call and
returns its value (`Portfolio.total()` is a number, `ledger.balance` a property): it computes and keeps state. An
`async fn` returns a Promise and runs as a task with NSH's services (`fetch`, `timer`, dialogs); a module function
taking `cx: &mut ModuleCx` reaches the platform in order with the UI (a toast, the status bar) and returns a Promise
of its value (`Promise<void>` for `()`, settled once it ran, so `await` waits for it). Arguments and results are typed from the signature: numbers, strings, `Option`, `Vec`, your
`#[derive(JsObject)]` structs and `#[derive(PropEnum)]` enums, instances of your classes, and TSX functions
(`Callback<A, R>`, which Rust calls at once inside a call, or later with `post` and `call_async`; TS sees
`(value: number) => void` for a `Callback<f64>`, and `#[args(balance)] listener: Callback<f64>` names the argument,
`(balance: number) => void`). An event's handler argument is `value` too, and `#[args(index)]` on the event's
variant names it. An argument of
the wrong type throws a `TypeError` in the call, or rejects the Promise of one that returns a Promise; an `Err`
throws (or rejects) with its message. A class's object owns its Rust value: it is freed when the object is collected, or at
once with `ledger.dispose()` (also `using ledger = new Ledger()`). Name the TSX entry to run it, `nsh run App.tsx` or `nsh package App.tsx --target …`: TSX edits are hot
updates, Rust edits rebuild and restart the app. The CLI bundles and compiles the TSX itself, knowing your crate's
exports from its description; the development hosts and `nsh trace`'s headless runner are built with your crate and
QuickJS.

`examples/rust-exports` in the repository is the reference for exporting: a small TSX app whose crate exports one
component (props required, defaulted and optional, an event, children), one module (a function of each kind) and
one class (a constructor, methods, a getter and a setter, a static, an `async` member, another instance as an
argument, a callback Rust posts to, `dispose()` and `using`), each with a short doc comment, and headless tests that
press through the app and check what each one shows.

`examples/gallery-rs` in the repository is a complete Rust app: every demo is Rust, `nsh run examples/gallery-rs
--target web` (or `ios`, `android`, …) runs its root, `nsh trace examples/gallery-rs` runs it headless, and
`cargo run -p gallery-native` starts it on the desktop backend.

## Several crates

An app's Rust code can be split into crates, each imported by its own specifier: `"rust"` takes a list of crate
paths, and each crate in it has its own `#[native_module]` function.

```json
{
  "rust": ["native/charts", "native/storage"],
  "root": "App"
}
```

```tsx
import { Chart } from "app/charts";        // native/charts: #[native_module("app/charts")]
import { Store } from "app/storage";       // native/storage: #[native_module("app/storage")]
```

Every runner and host links all of them, and `.nsh/types` declares each specifier. Crates share code through Cargo
path dependencies (`shared = { path = "../shared" }`): a crate of company components can be used by several crates
and listed by several apps, and only the crates listed in `"rust"` export to TSX. Each listed crate has a package
name and a specifier of its own; `nsh` reports a repeated one with the two paths that share it. A Rust app's
`"root"` is a component of the first crate listed. During `nsh run` and `nsh dev`, a change to any of them, or to a
crate they depend on by path, rebuilds the app and restarts it.

## See also

- [Project structure](./project-structure.md): every key of `nsh.json`.
- [Web](./web.md#apps-with-rust-components): Rust apps in the browser.
- [Networking](./networking.md#from-rust-components): `fetch` from Rust.
- [CLI](./cli.md#nsh-init): `nsh init --template rust` and `--template mixed`.
