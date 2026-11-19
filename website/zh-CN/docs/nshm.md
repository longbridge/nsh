---
title: nshm
description: "nshm（NSH 模块）：包含组件、模块和类的 Rust crate，任何 Native Shell 应用都可以用 nsh add 安装，并在 Rust 和 TSX 中同样使用；以及如何编写和发布、版本与锁文件。"
order: 5.7
group: start
---

# nshm

**nshm**（NSH 模块）是为 Native Shell 应用提供组件、模块和类的 Rust crate，发布在 crates.io 或 Git 上。应用用 `nsh add` 安装后，可以在 Rust 和 TSX 中使用它：同一份实现，在每个平台上都是原生的。

```sh
nsh add acme-gauge@1.0
```

```rust tabs
// Rust：crate 是应用 crate 的依赖。
acme_gauge::gauge().value(move || load.get())
```

```tsx tabs
// TSX：按 nshm 的标识导入。
import { Gauge } from "acme/gauge";

<Gauge value={load()} />
```

nshm 让功能不必成为 NSH 的一部分，也能交到需要它的应用手中：它按自己的节奏发布，只有安装它的应用才承担它的体积、构建时间和依赖。

## 使用 nshm

`nsh add` 可以从 crates.io、GitHub、任意 Git 地址或本地目录安装 nshm：

```sh
nsh add acme-gauge@1.0                                 # crates.io：匹配 1.0 的最新版本
nsh add github:acme/gauge@v1.0.0                       # GitHub 的标签
nsh add https://git.example.com/gauge.git --tag v1.0.0  # 任意 Git 地址
nsh add --path ../gauge                                # 本地目录，按当前内容使用
```

它把 nshm 记录到 `nsh.json` 的 `"modules"`，把解析到的版本或提交锁定在 `nsh.lock`（两者都随应用提交），应用有自己的 crate 时还会把它加入该 crate 的 `[dependencies]`。所有命令都用锁定的版本构建，再次运行 `nsh add` 即更新。纯 TSX 应用会像带 crate 的应用一样用它的 nshm 构建宿主，因此开发机和 CI 需要 Rust 工具链（[用 Rust 开发](./rust.md)）。

```json
{
  "modules": ["acme-gauge@1.0", "github:acme/chart@v0.3.0", { "path": "../gauge" }]
}
```

[CLI：nsh add](./cli.md#nsh-add) 介绍了每种来源写法、版本要求以及锁文件的维护方式。

## 编写 nshm

nshm 是普通的 Cargo 库，根部有 `#[native_module("<标识>")]` 入口，与应用自己的 crate 相同：标识就是 TSX 导入它的名字，函数注册它导出的内容（[用 TSX 扩展 Rust 应用](./rust.md#用-tsx-扩展-rust-应用)）。它不需要任何 NSH 专用的元数据。

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

nshm 兼容的 NSH 版本就是它自己对 `nsh-core` 的版本要求（`nsh-core = "0.6"`），`nsh add` 在编译前会与当前 `nsh` 比对。它的 TypeScript 声明由 Rust 类型生成，应用检查 TSX 时会像检查自己的 crate 一样检查对它的使用。

在应用旁边开发时用 `nsh add --path ../acme-gauge`，按目录当前内容使用。发布方式与普通 crate 相同（`cargo publish`），加上关键词 `nshm`，便于在 [crates.io](https://crates.io/keywords/nshm) 上找到；也可以推送到 Git，分享它的地址。

## 安全

nshm 的构建脚本和过程宏会在应用构建时执行，其代码以应用的权限运行。请像审查应用自己的代码一样审查它的源码和 `nsh.lock` 的变更。
