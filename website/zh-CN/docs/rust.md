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
| 创建 | `nsh init my-app --template ts` | `nsh init my-app --template rust` | `nsh init my-app --template mixed` |

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
nsh init my-app --template rust
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

监听器是接收事件的闭包（`move |text: &str| ..`），没有数据的事件也可以写成 `move || ..`；`.child(..)`
接受一个元素，或元素的 `Option`、`Vec`、数组、迭代器（`.child(rows.iter().map(|r| row(r)))`）。

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

`nsh` 只构建你的 crate 需要的东西：一个由你的 crate 和 NSH 核心组成的小程序，它向 CLI 描述 crate 的组件和模块，其余工作由
CLI 自己完成；再加上你运行或打包的每个目标的运行器。新 Rust 应用的第一次构建只编译你的 crate 和 NSH 的核心，大约 60 个
crate。`nsh test` 在同样构建的、包含你的 crate 的运行器中以无界面方式运行应用的测试。

`nsh run` 期间，修改 crate 的 Rust 代码会重新构建运行器并重启应用；在浏览器中，已打开的页面会自动加载新的运行时。
`cargo run` 在桌面后端上直接启动同一个根组件（`src/main.rs`）；嵌入 NSH 的原生应用在 macOS、Windows、Linux、GPUI 和
iOS 的 `AppView` 上用 `AppSource::Rust` 挂载 Rust 根组件。

要在包括 Web 在内的所有平台上运行的代码，使用 NSH 提供的服务：HTTP 用 `fetch`（[网络](./networking.md#在-rust-组件中)），
等待用 `timer`、`set_timeout` 和 `set_interval`，时钟用 `nsh_core::time::Instant` 和 `nsh_core::time::unix_ms()`。
`background` 和 `blocking` 在原生目标的线程上运行任务。

## 桌面标题栏

Rust 应用通过 `nsh_core::ui::desktop` 绘制自定义标题栏。调用
`set_title_bar(TitleBarStyle::Hidden, Some(height))`，将背景设为
`.window_drag(true)`，并通过 `title_bar()` 为系统窗口按钮留出空间。
返回值的 `top` 是标题栏高度，`left` 和 `right` 是两侧系统按钮占用的宽度。
读取会订阅变化，包括进入和退出全屏。平台负责窗口拖动和用户设置的标题栏双击行为；
子元素上的 `.window_drag(false)` 可以排除父元素的拖动区域。

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

`set_title_bar(TitleBarStyle::Default, None)` 恢复系统标题栏。
`close_window()` 遵循宿主的正常窗口关闭行为。

## 用 TSX 扩展 Rust 应用

crate 的 `#[native_module("app/native")]` 函数声明 TSX 可以导入的内容：组件（`registry.register::<Chart>()`）、函数模块
（`registry.module::<Portfolio>()`，在单元结构体及其 `impl` 上标注 `#[module]`）、类（`registry.class::<Ledger>()`，在结构体及其 `impl` 上标注
`#[class]`）和资源；这个属性还会在 crate 根部生成 `pub fn registry()`，给测试和其他 Rust 代码使用同一个集合
（`my_app_native::registry()`，与 `nsh_ui_mobile::registry()` 并列）。在 `#[module]` 或 `#[class]` 的 `impl` 中，`pub fn`（包括 `pub(crate)`）会被导出；没有可见性修饰的函数是
辅助函数，只留在 Rust 中。它们的 TypeScript 声明（包括文档注释）由 Rust 类型生成到 `.nsh/types`，所以 `nsh check` 和编辑器会据此检查
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

/// 应用的持仓。
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

每个导出在 TypeScript 中的文档来自给它命名的那个 Rust 项的文档注释：组件来自 `#[component] fn`（`Chart`），模块和类来自它们的
结构体（`Portfolio`、`Ledger`），props、事件、函数和成员来自它们自己的文档注释（有 getter 的 setter 不需要）。`Props` 结构体和
事件枚举本身的文档注释，以及 `impl` 块上的文档注释，只留在 Rust 中；在 `#[module]` 或 `#[class]` 的 `impl` 块上写文档注释是编译
错误，因为 TypeScript 不会显示它。每个写入 `.nsh/types` 的命令都会对 TSX 看到的、没有文档注释的导出给出警告，并写出它的名字
（`Ledger.add`、`Chart.range`）。

TypeScript 只显示文档注释中第一个标题之前的部分。写给 Rust 调用者的说明，例如 `&self` 借用了什么、函数为什么接收 `cx`，放在标题
`# Rust`（或 rustdoc 的 `# Panics`、`# Examples`）下面，只留在 Rust 文档中：

```rust
/// 把 `text` 放到剪贴板；resolve 为复制的字符数。
///
/// # Rust
///
/// 接收 `cx` 让它成为一个 op，与 TS 在它之前做的 UI 改动按顺序执行。
pub fn copy(text: String, cx: &mut ModuleCx) -> u32 {
    let count = text.chars().count() as u32;
    cx.set_clipboard(text);
    count
}
```

props 是组件 `Props` 结构体的字段，事件是 `Event` 枚举的变体（`onSelect`），都用驼峰命名。Rust 组件自己的内容、事件和状态留在
Rust 中；TSX 传入 props、接收事件。类型不对的 prop（`u32` 类型的 prop 传 `level={1.5}`）和内置元素上一样给出警告：这个 prop 保持原值，组件的其余部分照常渲染。

函数和成员按 Rust 签名运行。不是 `async` 的在 TSX 调用中直接运行并返回值（`Portfolio.total()` 是数字，`ledger.balance` 是属性）：
它们计算并保存状态。`async fn` 返回 Promise，作为任务运行，可以使用 NSH 的服务（`fetch`、`timer`、对话框）；带
`cx: &mut ModuleCx` 参数的模块函数与 UI 按顺序作用于平台（toast、状态栏），组件 ref 的方法也是这样；它们的返回类型决定 TSX
得到什么：返回 `()` 的得到 `void`，没有回复（TSX 调用后继续往下执行）；返回 `Result<(), E>` 的得到 `Promise<void>`，遇到 `Err`
时以其消息 reject（调用与 UI 的修改一起运行，那时 TSX 的调用已经返回，所以由 Promise 带回错误；没人 await 或 catch 的会记录为
[未处理的 rejection](./dev-server-and-debugging.md#日志)）；返回值的得到 `Promise<T>`。参数和返回值的类型来自签名：数字、
字符串、`Option`、`Vec`、你的 `#[derive(JsObject)]` 结构体和 `#[derive(PropEnum)]` 枚举、你的类的实例，以及 TSX 函数
（`Callback<A, R>`：Rust 在调用中立即调用它，或之后用 `post` 和 `call_async` 调用；`Callback<f64>` 在 TS 中是
`(value: number) => void`，`#[args(balance)] listener: Callback<f64>` 给参数命名，得到 `(balance: number) => void`）。事件处理函数的参数
也叫 `value`，在事件的变体上写 `#[args(index)]` 可以给它命名。类型不对的参数在调用中抛出 `TypeError`，
返回 Promise 的调用则以它 reject；`Err` 以其消息抛出（或 reject）。类的对象拥有它的
Rust 值：对象被回收时释放，或调用 `ledger.dispose()`（也可以 `using ledger = new Ledger()`）立即释放。指定 TSX 入口来运行它：`nsh run App.tsx` 或 `nsh package App.tsx --target …`。TSX 的修改是热更新，
Rust 的修改会重新构建并重启应用。CLI 根据 crate 的描述知道它导出了什么，自己打包和编译 TSX；开发宿主和 `nsh test` 的
无界面运行器则与你的 crate 和 QuickJS 一起构建。

仓库中的 `examples/rust-exports` 是导出的参考示例：一个小型 TSX 应用，它的 crate 导出一个组件（必填、有默认值和可选的
props，一个事件，子元素）、一个模块（每种函数各一个）和一个类（构造函数、方法、getter 和 setter、静态成员、`async` 成员、以另一个实例为参数、
Rust 用 `post` 调用的回调、`dispose()` 和 `using`），每个都有简短的文档注释；它的测试（Rust 的，以及 TypeScript 的 `App.test.tsx`）在应用中逐一点按，并检查每个导出显示的内容。

仓库中的 `examples/gallery-rs` 是一个完整的 Rust 应用：每个 demo 都是 Rust，`nsh run examples/gallery-rs --target web`
（或 `ios`、`android` 等）运行它的根组件，`nsh test examples/gallery-rs` 以无界面方式运行它的测试，`cargo run -p gallery-native`
在桌面后端上启动它。

## 测试

对于带 Rust crate 的应用，`nsh test` 同样运行它的 TypeScript 测试（[测试](./testing-and-trace.md)）：在用你的 crate 构建的
无界面运行器中。在 Rust 应用中，`open(".")` 启动它的根组件：

```tsx
// App.test.tsx, next to nsh.json
import { expect, open, test } from "nsh/test";

test("a tap counts", async () => {
  const app = await open(".");
  await app.press("Tap");
  expect(app.texts()).toContain("Count: 1");
});
```

测试也可以在 `cargo test` 中用同一个驱动无界面地运行应用：运行在真实的引擎上，没有窗口，以 60 Hz 提供显示帧，所以定时器、
任务和动画都像在屏幕上一样运行。`nsh_engine::headless::Headless`（nsh-engine 的特性 `headless`）启动应用，维护一份它所显示
视图的模型（`app.screen()`：它们的文本、应用发送的命令），并执行 `nsh test` 的那些步骤（`press`、`type_text`、`submit`、
`scroll`、`wait_for`），视图用同样的选择器指定（文本、无障碍标签或 test id；`text:`、`label:`、`id:`）；步骤无法执行时
返回带有屏幕文字的错误。Rust 应用运行它的根组件；`EngineConfig::new` 是一个手机尺寸、其余都未设置的引擎：

```toml
[dev-dependencies]
nsh-engine = { version = "0.1", features = ["headless"] }
```

```rust
use nsh_engine::{EngineConfig, headless::Headless};

#[test]
fn a_tap_counts() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<my_app::App>())).unwrap();
    app.wait_for("Count: 0").unwrap();
    app.press("Tap").unwrap();
    app.wait_for("Count: 1").unwrap();
}
```

TSX 入口用 `nsh-testing` 按 `nsh run` 的方式构建，使用它的宿主提供的 UI 集合：NSH 的，以及每个 crate 由 `#[native_module]` 生成的
`registry()`：

```toml
[dev-dependencies]
nsh-testing = "0.1"
nsh-ui-mobile = "0.1"
```

```rust
#[test]
fn the_module_formats_money() {
    let sets = vec![nsh_ui_mobile::registry(), my_app_native::registry()];
    let mut app = nsh_testing::tsx("../App.tsx", sets).unwrap();
    app.wait_for("Formatted at once: $1,234.50").unwrap();
}
```

`nsh_testing::compile_tsx` 只构建一次入口供多个测试使用，`nsh_engine::headless::start` 把批次交给自己维护屏幕模型的测试。
`examples/rust-exports` 用这种方式测试它的 TSX 入口，`examples/gallery-rs` 测试它的 Rust 根组件。

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
- [CLI](./cli.md#nsh-init)：`nsh init --template rust` 和 `--template mixed`。
