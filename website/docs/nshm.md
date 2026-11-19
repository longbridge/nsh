---
title: nshm
description: "nshm (NSH modules): Rust crates of components, modules and classes that any Native Shell app installs with nsh add and uses from Rust and TSX alike, how to write and publish one, versions and the lock file."
order: 5.7
group: start
---

# nshm

An **nshm** (NSH module) is a Rust crate that gives Native Shell apps components, modules and classes, published on crates.io or Git. An app installs one with `nsh add` and uses it from Rust and from TSX: the same implementation, native on every platform.

[nsh-omarchy](https://github.com/huacnlee/nsh-omarchy), for example, gives an app [Omarchy](https://omarchy.org)'s look: buttons, fields, menus, dialogs, tables and charts that follow the system's Omarchy theme as it changes.

```sh
nsh add nsh-omarchy                  # from crates.io
nsh add github:huacnlee/nsh-omarchy  # or from GitHub
```

Then use it from the language the app is written in, Rust or TSX. **In a Rust app**, its components are functions of the crate:

```rust
// src/lib.rs: the app's root component
use nsh_core::prelude::*;
use nsh_omarchy::tokens;

#[component]
pub fn App() -> impl IntoElement {
    // Draw with the Omarchy theme the system uses, and follow its changes.
    nsh_omarchy::init();
    let saved = signal(0_u32);
    let notify = signal(true);
    view()
        .flex_1()
        .p(px(tokens::SPACE_6))
        .gap(px(tokens::SPACE_4))
        .bg(move || tokens::background())
        .child(
            nsh_omarchy::panel("Settings")
                .child(
                    nsh_omarchy::switch("Notifications")
                        .checked(move || notify.get())
                        .on_change(move |on: &bool| notify.set(*on)),
                )
                .child(
                    nsh_omarchy::button_primary("Save")
                        .on_press(move || saved.update(|n| *n += 1)),
                ),
        )
        .child(nsh_omarchy::body_text(move || {
            format!("Saved {} times", saved.get())
        }))
}
```

**In a TSX app**, import the crate's exports by its specifier, `nsh-omarchy`. The app needs no Rust code of its own: `nsh` builds the app's hosts with the crate.

```tsx
// App.tsx
import { mount, signal, Text, View } from "nsh";
import { BarChart, Icon, Markdown, Omarchy, type ThemeInfo } from "nsh-omarchy";

function App() {
  // Rust owns the theme; TSX applies one and hears every change.
  Omarchy.followSystem();
  const theme = signal<ThemeInfo | null>(Omarchy.current());
  Omarchy.onChange((t) => theme.set(t));
  return (
    <View class="flex-1 gap-4 p-6 bg-background">
      <View class="flex-row items-center gap-2">
        <Icon name="sun" label="Theme" />
        <Text class="text-foreground">{() => theme()?.name ?? "…"}</Text>
      </View>
      <BarChart data={[{ label: "Mon", value: 12 }, { label: "Tue", value: 18 }]} accessibilityLabel="Builds" />
      <Markdown source="Read the **manual**." />
    </View>
  );
}

mount(App);
```

An nshm is how a feature reaches the apps that need it without being part of NSH: it ships on its own schedule, and only the apps that install it pay its size, build time and dependencies.

## Using an nshm

`nsh add` installs an nshm from crates.io, GitHub, any Git URL or a local directory:

```sh
nsh add nsh-omarchy                                             # crates.io: the newest version
nsh add github:huacnlee/nsh-omarchy                             # GitHub: the default branch, or @<tag>
nsh add https://github.com/huacnlee/nsh-omarchy.git --rev main  # any Git URL
nsh add --path ../nsh-omarchy                                   # a local directory, as it is
```

It lists the nshm in `nsh.json` `"modules"`, pins the version or commit it resolved to in `nsh.lock` (commit both with the app), and adds the crate to the `[dependencies]` of the app's own crate when it has one. Every command builds with the locked version; running `nsh add` again updates it. A pure TSX app gets hosts built with its nshm, as an app with a crate of its own does, so its development machine and CI need the Rust toolchain ([Building with Rust](./rust.md)).

```json
{
  "modules": ["nsh-omarchy", { "path": "../my-gauge" }]
}
```

[CLI: nsh add](./cli.md#nsh-add) covers every source form, version requirements and how the lock file is kept.

## Writing an nshm

An nshm is an ordinary Cargo library with a `#[native_module("<specifier>")]` entry at its root, the same as an app's own crate: the specifier is what TSX imports, and the function registers what it exports ([Extending a Rust app with TSX](./rust.md#extending-a-rust-app-with-tsx)). It needs no NSH-specific metadata. A small one:

```toml
# Cargo.toml
[package]
name = "my-gauge"
version = "0.1.0"
edition = "2024"
keywords = ["nshm"]

[dependencies]
nsh-core = "0.6"
```

```rust
// src/lib.rs
use nsh_core::prelude::*;

#[native_module("my-gauge")]
fn native(registry: &mut Registry) {
    registry.register::<Gauge>();
}

#[derive(Clone, PartialEq, Props)]
pub struct GaugeProps {
    /// The value shown, from 0 to 1.
    pub value: Prop<f32>,
}

/// A horizontal gauge.
#[component]
pub fn Gauge(props: GaugeProps) -> impl IntoElement {
    view()
        .h(px(8.0))
        .rounded_full()
        .bg(rgb(0xe5e7eb))
        .child(
            view()
                .h_full()
                .rounded_full()
                .bg(rgb(0x2563eb))
                .w(move || relative(props.value.get())),
        )
}
```

The NSH versions an nshm builds with are its own `nsh-core` requirement (`nsh-core = "0.6"`), which `nsh add` checks against the running `nsh` before anything compiles. Its TypeScript declarations are generated from its Rust types, so apps check their TSX against it as they do against their own crate.

[nsh-omarchy's `src/lib.rs`](https://github.com/huacnlee/nsh-omarchy/blob/main/src/lib.rs) is a complete one: a library of Rust components for Rust apps, and a theme module, charts and `Markdown` for TSX.

Develop it beside an app with `nsh add --path ../my-gauge`, which reads the directory as it is. Publish it as any crate (`cargo publish`), with the keyword `nshm` so that it can be found on [crates.io](https://crates.io/keywords/nshm), or push it to Git and share its URL.

## Security

An nshm's build scripts and procedural macros run when the app builds, and its code runs with the app's permissions. Review its source and the changes to `nsh.lock` as you review the app's own code.
