---
title: 用 Rust 开发
description: 完全用 Rust 编写 Native Shell 应用，在所有平台上都不带 QuickJS；或者在 Rust 之上用 TSX 编写页面：如何选择，以及两者如何共享组件和模块。
order: 5.5
group: start
---

# 用 Rust 开发

Native Shell 应用可以用 TSX 编写，也可以用 Rust，或者两者结合。三种方式运行在同一个引擎上：响应式图、渲染器、Taffy
布局和 `nsh/ui/mobile` 组件始终是 Rust，各平台都用自己的原生 UI 渲染。不同的是你自己的代码用什么语言，以及应用是否带着
JavaScript 虚拟机。

| | TSX | Rust | Rust + TSX |
|---|---|---|---|
| 你的 UI、状态和事件处理 | TSX，运行在 QuickJS 上 | Rust | Rust 组件和模块，TSX 页面使用它们 |
| 应用运行的内容 | QuickJS 和引擎 | 只有引擎和你的 crate，所有平台和浏览器都一样 | QuickJS 和引擎，加上你的 crate |
| 适合 | 运行时下发的应用和模块、快速迭代 | 业务复杂度高的中大型应用 | Rust 核心加上经常变化的部分 |
| 修改代码 | 热更新 | 重新构建并重启 | TSX 热更新，Rust 重新构建并重启 |
| 创建 | `nsh init my-app` | `nsh init my-app --rust` | `nsh init my-app --rust`，然后 `nsh run App.tsx` |

## 如何选择

**Rust** 适合业务逻辑庞大、要求高的应用：交易、编辑器、数据密集的工具。整个应用都有 Rust 的类型系统和编译期检查，可以使用
crates.io 生态，你自己的代码（不只是 NSH 的代码）也是原生性能，还能用 `cargo test` 对真实 UI 做无界面测试。应用从头到尾都是
Rust 和原生代码：在 iOS、Android、macOS、Windows、Linux、GPUI 和 Web 上，应用里既不带也不运行 JavaScript 虚拟机。
组件是只运行一次的函数，把信号绑定到显示的内容上，和 TSX 一样（[响应式](./reactivity.md)），样式用类型化的方法而不是 class 字符串。

**Rust + TSX** 保留这个核心，在值得的地方加上 TSX：页面、设置、引导、实验性功能，任何需要热更新快速迭代、或作为[签名包](./signed-packages.md)
在运行时下发的部分。TSX 像导入其他 NSH 模块一样导入你的 Rust 组件和模块，两部分通过类型化的接口衔接。TSX 运行在 QuickJS
上，这时应用会带上 QuickJS。

## 完全用 Rust 编写的应用

```sh
nsh init my-app --rust
cd my-app
nsh run                        # 本机的后端；--target ios、android、web、macos、linux、windows 或 gpui
nsh package --target web       # 或 ios、ios-simulator、android、macos、linux、windows、gpui
```

`src/lib.rs` 包含整个应用：组件、状态、事件处理和任务。

```rust
use nsh_core::ui::prelude::*;
use nsh_ui_mobile::{Button, ButtonEvent, ButtonProps};

/// TSX 通过 `app/native` 可以使用的组件。
#[native_module("app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

#[component]
pub fn App() -> impl IntoElement {
    let count = signal(0_u32);
    let quote = signal(SharedString::from("…"));
    // 异步工作在任务里运行；请求走 NSH 的网络栈。
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

`nsh.json` 指定根组件，`nsh` 负责挂载它：

```json
{
  "name": "My App",
  "rust": ".",
  "root": "App",
  "theme": "theme.json",
  "appearance": "system"
}
```

`"root"` 是组件在 crate 中的路径（`App`、`shell::Gallery`；有[多个 crate](#多个-crate) 时在第一个中）；`"theme"` 和 `"appearance"` 和 TSX 应用一样作用于它。
有了它，对应用目录执行 `nsh run` 和 `nsh package` 时，会为目标平台构建应用自己的运行器，直接挂载根组件，不链接 QuickJS：

| 目标 | 运行的内容 |
|---|---|
| macOS、Windows、Linux、GPUI | 带有你的 crate 的发布运行器，在窗口中运行；`nsh run` 使用 debug 构建，并在终端显示应用日志 |
| iOS、iOS 模拟器、Mac Catalyst | 带有你的 crate 的 iOS 运行器 |
| Android | 带有你的 crate 的应用 `libnsh.so`，由运行时的 Activity 启动 |
| Web | 包含引擎和你的 crate 的 `nsh.wasm`（整个 Rust gallery 为 3.3 MB，brotli 压缩后 0.75 MB）；页面直接挂载根组件 |

`nsh run` 期间，修改 crate 的 Rust 代码会重新构建运行器并重启应用；在浏览器中，已打开的页面会自动加载新的运行时。
`cargo run` 在桌面后端上直接启动同一个根组件（`src/main.rs`）；嵌入 NSH 的原生应用在 macOS、Windows、Linux、GPUI 和
iOS 的 `AppView` 上用 `AppSource::Rust` 挂载 Rust 根组件。

要在包括 Web 在内的所有平台上运行的代码，使用 NSH 提供的服务：HTTP 用 `fetch`（[网络](./networking.md#在-rust-组件中)），
等待用 `timer`、`set_timeout` 和 `set_interval`，时钟用 `nsh_core::time::Instant` 和 `nsh_core::time::unix_ms()`。
`background` 和 `blocking` 在原生目标的线程上运行任务。

## 用 TSX 扩展 Rust 应用

crate 的 `#[native_module("app/native")]` 函数声明 TSX 可以导入的内容：组件（`registry.register::<Chart>()`）、函数模块
（`registry.module::<Portfolio>()`，一个 `#[module]` `impl`）、类（`registry.class::<Ledger>()`，在结构体及其 `impl` 上标注
`#[class]`）和资源。它们的 TypeScript 声明（包括文档注释）由 Rust 类型生成到 `.nsh/types`，所以 `nsh check` 和编辑器会据此检查
TSX：

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

const ledger = new Ledger();   // 状态保存在 Rust 中

function Overview() {
  const balance = signal(ledger.add(Portfolio.total()));   // Rust，立即调用
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

props 是组件 `Props` 结构体的字段，事件是 `Event` 枚举的变体（`onSelect`），都用驼峰命名。Rust 组件自己的内容、事件和状态留在
Rust 中；TSX 传入 props、接收事件。

函数和成员按 Rust 签名运行。不是 `async` 的在 TSX 调用中直接运行并返回值（`Portfolio.total()` 是数字，`ledger.balance` 是属性）：
它们计算并保存状态。`async fn` 返回 Promise，作为任务运行，可以使用 NSH 的服务（`fetch`、`timer`、对话框）；带
`cx: &mut ModuleCx` 参数的模块函数与 UI 按顺序作用于平台（toast、状态栏），返回其值的 Promise。参数和返回值的类型来自签名：数字、
字符串、`Option`、`Vec`、你的 `#[derive(JsObject)]` 结构体和 `#[derive(PropEnum)]` 枚举、你的类的实例，以及 TSX 函数
（`Callback<A, R>`：Rust 在调用中立即调用它，或之后用 `post` 和 `call_async` 调用）。`Err` 以其消息抛出（或 reject）。类的对象拥有它的
Rust 值：对象被回收时释放，或调用 `ledger.dispose()`（也可以 `using ledger = new Ledger()`）立即释放。指定 TSX 入口来运行它：`nsh run App.tsx` 或 `nsh package App.tsx --target …`。TSX 的修改是热更新，
Rust 的修改会重新构建并重启应用。

仓库中的 `examples/gallery-rs` 是一个完整的 Rust 应用，两种入口都有：每个 demo 都是 Rust，`nsh run examples/gallery-rs --target web`
（或 `ios`、`android` 等）运行它的根组件，`cargo run -p gallery-native` 在桌面后端上启动它，`nsh run examples/gallery-rs/App.tsx`
通过 TSX 入口运行它。

## 多个 crate

应用的 Rust 代码可以拆分成多个 crate，每个 crate 用自己的名称导入：`"rust"` 可以是 crate 路径的列表，列表中的每个 crate
都有自己的 `#[native_module]` 函数。

```json
{
  "rust": ["native/charts", "native/storage"],
  "root": "App"
}
```

```tsx
import { Chart } from "app/charts";        // native/charts：#[native_module("app/charts")]
import { Store } from "app/storage";       // native/storage：#[native_module("app/storage")]
```

每个运行器和宿主都链接全部这些 crate，`.nsh/types` 为每个名称生成声明。crate 之间通过 Cargo 路径依赖共享代码
（`shared = { path = "../shared" }`）：一个公司组件 crate 可以被多个 crate 使用、被多个应用列出，只有 `"rust"` 中列出的
crate 会导出给 TSX。列出的每个 crate 都有自己的包名和导入名称；重复时 `nsh` 会报告共用它的两个路径。Rust 应用的
`"root"` 是列出的第一个 crate 中的组件。`nsh run` 和 `nsh dev` 期间，修改其中任何一个 crate，或它们通过路径依赖的
crate，都会重新构建并重启应用。

## 另请参阅

- [项目结构](./project-structure.md)：`nsh.json` 的每个键。
- [Web](./web.md#带-rust-组件的应用)：浏览器中的 Rust 应用。
- [网络](./networking.md#在-rust-组件中)：在 Rust 中使用 `fetch`。
- [CLI](./cli.md#nsh-init)：`nsh init --rust`。
