---
title: nshm
description: "nshm (NSH modules): Rust crates of components, modules and classes that any Native Shell app installs with nsh add and uses from Rust and TSX alike, how to write and publish one, versions and the lock file."
order: 5.7
group: start
---

# nshm

An **nshm** (NSH module) is a Rust crate that gives Native Shell apps components, modules and classes, published on crates.io or Git. An app installs one with `nsh add` and uses it from Rust and from TSX: the same implementation, native on every platform.

```sh
nsh add acme-gauge@1.0
```

```rust tabs
// Rust: the crate is a dependency of the app's crate.
acme_gauge::gauge().value(move || load.get())
```

```tsx tabs
// TSX: imported by the nshm's specifier.
import { Gauge } from "acme/gauge";

<Gauge value={load()} />
```

An nshm is how a feature reaches the apps that need it without being part of NSH: it ships on its own schedule, and only the apps that install it pay its size, build time and dependencies.

## Using an nshm

`nsh add` installs an nshm from crates.io, GitHub, any Git URL or a local directory:

```sh
nsh add acme-gauge@1.0                                 # crates.io: the newest version matching 1.0
nsh add github:acme/gauge@v1.0.0                       # GitHub at a tag
nsh add https://git.example.com/gauge.git --tag v1.0.0  # any Git URL
nsh add --path ../gauge                                # a local directory, as it is
```

It lists the nshm in `nsh.json` `"modules"`, pins the version or commit it resolved to in `nsh.lock` (commit both with the app), and adds the crate to the `[dependencies]` of the app's own crate when it has one. Every command builds with the locked version; running `nsh add` again updates it. A pure TSX app gets hosts built with its nshm, as an app with a crate of its own does, so its development machine and CI need the Rust toolchain ([Building with Rust](./rust.md)).

```json
{
  "modules": ["acme-gauge@1.0", "github:acme/chart@v0.3.0", { "path": "../gauge" }]
}
```

[CLI: nsh add](./cli.md#nsh-add) covers every source form, version requirements and how the lock file is kept.

## Writing an nshm

An nshm is an ordinary Cargo library with a `#[native_module("<specifier>")]` entry at its root, the same as an app's own crate: the specifier is what TSX imports, and the function registers what it exports ([Extending a Rust app with TSX](./rust.md#extending-a-rust-app-with-tsx)). It needs no NSH-specific metadata.

```toml
# Cargo.toml
[package]
name = "acme-gauge"
version = "1.0.0"
edition = "2024"
keywords = ["nshm"]

[dependencies]
nsh-core = "0.6"
```

```rust
// src/lib.rs
use nsh_core::prelude::*;

#[native_module("acme/gauge")]
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

Develop it beside an app with `nsh add --path ../acme-gauge`, which reads the directory as it is. Publish it as any crate (`cargo publish`), with the keyword `nshm` so that it can be found on [crates.io](https://crates.io/keywords/nshm), or push it to Git and share its URL.

## Security

An nshm's build scripts and procedural macros run when the app builds, and its code runs with the app's permissions. Review its source and the changes to `nsh.lock` as you review the app's own code.
