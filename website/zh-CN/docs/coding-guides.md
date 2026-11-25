---
title: 编码指南
description: "构建可维护的 Native Shell 应用与组件库的架构、代码风格与命名规范，均附 TSX 与 Rust 示例。"
order: 29.5
group: ui
---

# 编码指南

本指南介绍如何组织 Native Shell（NSH）应用的结构和代码，让它长期易于修改。读者是工程师和编码智能体：开发 NSH 应用的人
（TSX 模块、Rust 组件、嵌入 NSH 的原生应用），以及在 `nsh/base` 之上写组件库的人。请先读设计指南。代码结构要落实产品设计，
而不是取代它。

本文用两个词表示要求的强弱。**必须**表示生命周期、正确性或生态上的硬约束。**应该**表示默认架构，偏离它要有具体理由。
精确的签名以当前源码、参考页面和 `.nsh/types` 中的声明为准。

## 先建立应用的界面基础，再编写页面

新应用先建立设计标准、Theme Tokens 和应用自己的风格组件，再写功能页面。已有应用先找到这些基础并复用。标准和共享界面代码
都放在应用仓库里，具体内容见[设计指南](./design-guides.md#先建立应用自己的设计系统)。

按以下顺序开发：

1. 明确应用支持的 Desktop、Mobile、Web、TUI 目标，以及各自的输入、导航和密度要求。
2. 在主题文件（`nsh.json` 的 `"theme"`）中定义语义颜色角色与浅色、深色值，并确定组件使用的字体、间距、圆角和尺寸尺度。
3. 创建应用的 `ui` 模块（`src/ui/`），在 `nsh/base` 之上封装、组合出核心风格控件（[应用的 `ui` 层](./design-guides.md#应用的-ui-层)）。每个控件只接受 `variant`、`size`、状态和 slot，不接受颜色或圆角。每个控件都要做齐可见部件、变体和交互状态。先在支持的目标上验证一组有代表性的组合，再铺到各个页面。
4. 用这一层拼出页面。缺少 token 或共享变体时，先在它所属的层补上，再在页面里使用。修改共享控件时，所有用到它的地方应一起变化。

NSH 提供原语（`nsh-core`，TSX 中为 `nsh`）和无样式的 `nsh-base` crate（TSX 中为 `nsh/base`）。其上的风格层归应用或社区组件库
所有。普通界面导入应用自己的风格控件，基础组件的组装和样式集中在共享层。各平台的外观可以调整，但基础组件的行为、焦点和无障碍
能力要保留。

主题颜色必须通过语义 token 读取：Rust 用 `theme().primary` 或 `theme().color("warning")`，TSX 用 `bg-primary`、
`text-warning` 或 `token("warning")`。语义 token 按用途（primary、warning）而不是具体色值来命名颜色。十六进制、RGB/RGBA、
HSL/HSLA 原始值只写在主题源里，页面和组件的样式中不写，状态样式、渐变和阴影也一样。`bg-blue-500` 这类色板类和任意颜色类
同样绕开了应用的语义角色，这时应补上缺少的角色。`bg-primary/90` 或 `theme().primary.opacity(0.9)` 这样调透明度，颜色仍来自
token。用户自选的颜色、图片像素、取色器选中的值都属于数据，周围的控件仍用主题 token。

完成前要检查三件事：新增的外观代码里有没有原始颜色值和色板类；是否用了共享组件；各状态在浅色、深色和各目标平台上是否正常。
没法验证的平台要写明，一张截图不能代表所有目标。

## 让常见用法自然低成本

**设计原则：框架的类型与所有权设计，要让组件的常见用法默认就很便宜。** 开发者应该能直接构建、复用界面，不必在每个调用处
手写缓存。这靠两点实现。组件只运行一次，构建出自己那部分树。之后只有读取了变化的 [signal](./reactivity.md) 的绑定才会
重新运行。signal 是能记录谁读取了它的值。响应式图（记录哪些绑定读了哪些 signal）、渲染器、布局和列表都是原生代码。应用代码
执行一轮，就变成一批原生操作。小开销仍会随元素数和更新次数累积，TSX 代码跑在解释器里，尤其明显。先测出热点，再加缓存
（[性能](./performance.md)）。这两点都不等于零开销。具体机制见下文的[状态所有权](#状态所有权)和渲染相关章节。

## 架构总览

<img class="diagram diagram-light" src="/diagrams/application-layers-light.svg" alt="NSH 应用架构层次：App Shell、页面和应用组件位于组件库、nsh/base 与 nsh 之上；依赖只向下。">
<img class="diagram diagram-dark" src="/diagrams/application-layers-dark.svg" alt="NSH 应用架构层次：App Shell、页面和应用组件位于组件库、nsh/base 与 nsh 之上；依赖只向下。">

依赖只向下。上层负责领域逻辑和流程编排，下层负责可复用的外观或行为。通用组件不能依赖某个应用页面，组件库也不能依赖某个
应用的主题（[三层结构](./architecture.md#三层结构)）。

边界定义如下：

- **app shell：** 入口（`App.tsx`，或 `nsh.json` 的 `"root"`）组合根视图与导航栈，不承载页面逻辑；
- **页面：** 一个 `NavPage`，包括它的内容、对话框和它发起的工作；
- **应用组件：** 跨页面复用且带有领域语义的模式；
- **组件库：** 带样式的通用界面，应用自己的或社区的，构建在 `nsh/base` 之上；
- **`nsh/base`：** 不带产品表现的可复用行为：导航、对话框、弹出框、菜单、面板、按压目标；
- **`nsh`：** 原语、样式、布局、响应式与环境。

### 应用文件的布局

NSH 按约定查找应用文件，所以无需配置。`App.tsx` 是入口，`nsh.json` 放在它旁边。

```text
App.tsx            # 入口，按约定：mount(App)
nsh.json
Cargo.toml         # 应用的 crate，包含 Rust 组件（"rust": "."）
src/
  lib.rs
pages/
  Inbox.tsx
  Compose.tsx
  Settings.tsx
components/
  Card.tsx
store.ts
```

- `pages/` 中每个 `NavPage` 一个文件。页面只在需要时才拆成更多文件。
- `components/` 存放应用的可复用组件，即多个页面都用到的组件。
- `store.ts` 存放应用的状态：它的 signal 以及修改它们的函数。页面读取这些 signal 并调用这些函数。
- 测试是放在被测代码旁边的 `*.test.tsx` 文件（`App.test.tsx`、`pages/Inbox.test.tsx`）。
- `Cargo.toml` 和 `src/lib.rs` 是应用自己的 crate，里面放 Rust 组件（`nsh.json` 的 `"rust": "."`）。`src/lib.rs` 中的
  `#[native_module]` 决定 TSX 导入时用的名字。纯 Rust 应用也是同样的 Cargo 项目，根组件写在 `src/lib.rs` 里
  （[项目结构](./project-structure.md)）。

App Shell 只负责组合页面，基本不放页面逻辑。页面读取 store 的 signal，调用 store 的函数，从不写入其他页面的状态。store 的
signal 保持私有，只导出读取和修改它们的函数。这样可变状态只能通过这些函数改动：

```tsx tabs
// store.ts
import { signal } from "nsh";

const name = signal("Untitled");

/** The workspace's name (tracked where it is read). */
export const workspaceName = () => name();

/** Renames the workspace. */
export function renameWorkspace(next: string) {
  name.set(next);
}
```

```rust tabs
/// The app's workspace: its state stays behind these functions.
#[derive(Clone, Copy)]
pub struct Workspace {
    name: Signal<SharedString>,
}

impl Workspace {
    /// A workspace named "Untitled".
    pub fn new() -> Self {
        Self { name: signal(SharedString::from("Untitled")) }
    }

    /// Its name (tracked where it is read).
    pub fn name(&self) -> SharedString {
        self.name.get()
    }

    /// Renames it.
    pub fn rename(&self, name: impl Into<SharedString>) {
        self.name.set(name.into());
    }
}
```

页面可以持有工作区，并把工作区 id 传给搜索。搜索不需要拿到工作区的整个集合，也不需要它的私有 signal。一个简单的无环依赖
关系如下：

```text
App.tsx ──▶ pages/ ──▶ store.ts
              └──────▶ components/ ──▶ nsh/base, nsh
```

页面可以依赖 store 和组件，但不能反过来依赖 App Shell，也不能访问其他页面的私有部分。一个页面要改变另一个页面显示的内容
时，通过 store 完成。有了这些边界，几位工程师或几个 AI 智能体可以并行改不同页面，文件冲突更少。改 store 时仍需要协调。

## 初始化与根视图所有权

应用只挂载一次。根组件声明应用的顶层窗口。每个窗口放一个 `flex: 1` 的视图，背景色取自主题。TSX 入口调用 `mount(App)`，
Rust 应用在 `nsh.json` 中指定根组件（`"root": "App"`）：

```tsx tabs
import { mount, View } from "nsh";
import { Window } from "nsh/desktop";

mount(() => (
  <Window id="main">
    <View class="flex-1 bg-background">
      <App />
    </View>
  </Window>
));
```

```rust tabs
use nsh_core::{element::desktop::window, prelude::*};

#[component]
pub fn App() -> impl IntoElement {
    provide_context(Workspace::new());
    window().id("main").content(
        view()
            .flex_1()
            .bg(move || theme().background)
            .child(inbox()),
    )
}
```

应用根负责整个应用共用的东西：共享状态、设置和窗口注册表。每个窗口的内容负责这个窗口自己的东西：导航栈，盖在窗口内容之上
的对话框、弹出框、菜单、面板和工具提示，以及安全区域。不要为每个页面再挂一个根。一个应用对应一个引擎。原生应用嵌入的每个
`NshAppView`、`NshView` 或 `AppView` 都有自己的 VM、模块状态和响应式图。所以同一个宿主应用里的多个 NSH 视图不会意外共享状态。

## 理解什么只运行一次，什么会再次运行

NSH 采用 Solid 的模型（[响应式](./reactivity.md)）：界面用声明式写法描述，状态在更新之间一直保留。signal 在多次更新之间
一直存在，组件函数体只在创建时运行一次，描述它那部分树。写代码时要分清这两者：

| 你写的 | 运行 | 再次运行的时机 |
| --- | --- | --- |
| 组件函数体 | 创建时运行一次 | 从不 |
| 绑定：放在值的位置上的闭包（Rust），或调用函数、读取属性的表达式（TSX） | 一次 | 它读取的 signal 变化时；只更新那一个 prop 或子元素 |
| `computed(fn)` | 创建时 | 它读取的 signal 变化时 |
| `effect(fn)` | 本轮的元素创建之后 | 它读取的 signal 变化时 |
| `each` / `<Each>` 的一行 | 它的 key 第一次出现时 | 从不；变化的是它的 item 和 index signal |
| `virtual_list` / `<List>` 的一个单元格 | 每个单元格一次 | 从不；单元格会被分配给其他行 |

- 正在运行的组件是**归属者**。它运行期间创建的 effect、computed 值、任务、定时器、按键绑定和 cleanup 都归它所有，随它一起
  释放；
- **context**（`provide_context` / `use_context`，`context()` 与 `<Ctx.Provider>`）把一个值提供给子树，并不表示读取它的
  元素拥有它；
- Rust 组件的 **`Scope`**（`cx`）发出它的事件，读取它得到的 inset；
- **ref**（`element_ref()`、`ref`）为命令式调用命名一个宿主元素（`focus()`、`scroll_to()`、`measure()`）；
- 布局和测量属于后续的原生阶段，只有需要解析后的几何尺寸时才使用 `onLayout` 和 `measure()`。

不要在事件处理函数或定时器里读取 context，因为那里没有组件可供查找。应该在组件函数体中读取，把拿到的值保存下来。可以长期
持有类型化的 handle（signal、resource、ref、领域 id）。它们在 Rust 中是 `Copy` 的，归属者消失后就不再起作用。

## 选择正确的组成单元

### 值类型界面使用普通函数

**轻量、可复用的界面片段，默认写成普通函数。** 调用者传入当前的 props 和处理函数，函数返回元素，之后只留下它建立的绑定。
函数的参数就是 props，显示的标签、值和回调都来自调用者的状态。它和所有组件一样只运行一次，之后变化的只是绑定。

```tsx tabs
function EmptyState(props: { title: string }) {
  return (
    <View class="items-center gap-2">
      <Text class="text-muted-foreground">{props.title}</Text>
    </View>
  );
}
```

```rust tabs
fn empty_state(title: impl Into<SharedString>) -> impl IntoElement {
    view()
        .gap_2()
        .items_center()
        .child(text(title.into()).text_color(move || theme().muted_foreground))
}
```

### 持久行为使用组件

当一块界面拥有持久状态或独立生命周期，例如 signal、effect、异步工作、定时器、按键绑定时，使用 `#[component]`（或创建
signal 的 TSX 组件）。这些状态在组件函数体中创建一次，不能在绑定里创建。

```tsx tabs
function Search(props: { placeholder: string }) {
  const query = signal("");
  return <Input value={query()} onChange={({ value }) => query.set(value)} placeholder={props.placeholder} />;
}
```

```rust tabs
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SearchProps {
    /// Shown while the field is empty.
    pub placeholder: Prop<SharedString>,
}

#[component]
pub fn Search(props: SearchProps) -> impl IntoElement {
    let query = signal(String::new());
    input()
        .value(move || query.get())
        .on_change(move |text: &str| query.set(text.to_owned()))
        .placeholder(move || props.placeholder.get().to_string())
}
```

`#[component]` 有类型化的 props（`#[derive(Props)]`）和事件（`#[derive(Event)]`）。TSX 导入它时，使用由它们生成的声明。
TSX 用 `new` 创建的持久对象（账本、解析器、会话）写成 `#[class]`，普通函数写成 `#[module]`
（[用 Rust 构建](./rust.md#用-tsx-扩展-rust-应用)）。应用最常用的部分、组件库和繁重的逻辑用 Rust 写，它们以原生代码运行。
经常变化或在运行时下发的部分用 TSX。

不要把每个视觉片段都做成组件。每个组件边界都会带来生命周期和协调成本。只有当一块界面需要在更新之间保持身份时，才做成组件。

### 元素、视图与行为系统各有职责

生态中的公开单元并非一种模板：

- `Button`、`Switch`、`MenuItem` 等语义元素；
- `Dialog`、`Popover`、`Menu`、`ContextMenu`、`Sheet`、`NavStack` 等复合行为根；
- `Input`、`List`、`TextView` 以及导航栈状态等有状态系统；
- 浮层及其定位、虚拟化、滚动、安全区域、键盘与动画等基础设施。

元素内部可以很复杂，但对调用者来说仍像一个普通值。有状态系统可以提供渲染函数，让应用自己决定外观，而不必重写行为。公开接口
按行为来划分，不看输出里有几个视图。

## 状态所有权

### 用 `SharedString` 保存持久化界面文本

**在 Rust 中，跨更新保存或放进回调闭包的界面文本必须优先用 `SharedString`。** 组件的标签、标题和占位文本都算。props 接收的
就是它（`Prop<SharedString>`），把它 clone 进绑定不会复制文本。编辑缓冲区或正在格式化拼接的文本用 `String`，拼好后再转换。
只在一次调用内临时读取时用 `&str`。在 TSX 中，字符串就是普通的值。由状态算出的文本放进 `computed` 或闭包，不要在多处重复拼。

界面要长期保留服务器返回的文本时，原样保存收到的值。格式化或派生出的展示文本另存一份，不要改写原始响应。

状态应放在能够保持其正确性的最窄归属者中：

- 领域状态属于 store（`store.ts`）或应用的 crate；
- 临时视图状态属于渲染它的组件，是一个 signal；
- 可复用的行为状态属于为该行为设计的组件（不传 `open` 的 `Menu` 或 `Popover` 自己保存它）；
- 子树共享的状态是通过 [context](./reactivity.md#context) 提供的 signal；
- 跨越一次运行的数据放在[存储](../modules/nsh.md#storage)或服务器上。

普通的选择和开关优先用受控值。受控组件的值由归属者提供：传入当前值，收到用户请求的新值，由归属者更新，绑定随之刷新。回调只
表达用户想做什么，不能再藏一份状态副本。

```tsx tabs
<Switch checked={showHidden()} onChange={({ checked }) => showHidden.set(checked)}>
  <Text>Show hidden files</Text>
</Switch>
```

```rust tabs
switch()
    .checked(show_hidden)
    .on_change(move |next: &bool| show_hidden.set(*next))
    .child(text("Show hidden files"))
```

不需要任何通知：写入 signal，就会精确更新读取它的绑定。Rust 组件用 `cx.emit(..)` 向归属者发出事件。effect 在归属者存活
期间一直跟随 signal。几个字段必须保持一致时，放在一个 [`batch`](./reactivity.md#batch) 里写入。这样 effect 和 computed 值
会同时看到全部修改。

应用自己的受控组件遵循同样的约定。下面是一个数量步进器，用 Rust 写一次，在 TSX 和 Rust 中都能使用：

```tsx tabs
import { signal } from "nsh";
import { QuantityStepper } from "my-app";   // Rust 组件：src/lib.rs 中有 #[native_module("my-app")]

function CartRow() {
  const quantity = signal(1);
  return (
    <QuantityStepper
      value={quantity()}
      min={1}
      max={10}
      decrementLabel="Fewer"
      incrementLabel="More"
      onChange={({ value }) => quantity.set(value)}
    />
  );
}
```

```rust tabs
use nsh_base::{button, theme};
use nsh_core::prelude::*;

/// The props of [`QuantityStepper`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct QuantityStepperProps {
    /// The quantity: the app owns the value.
    #[prop(default)]
    pub value: Prop<u32>,
    /// The smallest quantity (0 when left out).
    #[prop(default)]
    pub min: Prop<u32>,
    /// The largest quantity (99 when left out).
    #[prop(default = 99)]
    pub max: Prop<u32>,
    /// What screen readers say for the button that takes one away.
    pub decrement_label: Prop<SharedString>,
    /// What screen readers say for the button that adds one.
    pub increment_label: Prop<SharedString>,
}

/// What a [`QuantityStepper`] reports.
#[derive(Debug, Clone, PartialEq, Event)]
pub enum QuantityStepperEvent {
    /// The user asked for another quantity; the owner sets `value`.
    Change(u32),
}

/// A quantity with a button to take one away and one to add one.
#[component]
pub fn QuantityStepper(props: QuantityStepperProps, cx: Scope<QuantityStepperEvent>) -> impl IntoElement {
    // Read untracked: the user's act, measured against the owner's current value.
    let step = move |delta: i64| {
        let value = props.value.get_untracked();
        let (min, max) = (props.min.get_untracked(), props.max.get_untracked());
        let next = (i64::from(value) + delta).clamp(i64::from(min), i64::from(max)) as u32;
        if next != value {
            cx.emit(QuantityStepperEvent::Change(next));
        }
    };
    view()
        .flex_row()
        .items_center()
        .gap_3()
        .child(step_button(
            "−",
            move || props.decrement_label.get(),
            move || props.value.get() <= props.min.get(),
            move || step(-1),
        ))
        .child(text(move || props.value.get().to_string()).text_color(move || theme().foreground))
        .child(step_button(
            "+",
            move || props.increment_label.get(),
            move || props.value.get() >= props.max.get(),
            move || step(1),
        ))
}

fn step_button(
    symbol: &'static str,
    label: impl Fn() -> SharedString + 'static,
    disabled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    button()
        .disabled(disabled)
        .accessibility_label(move || Some(label()))
        .on_press(on_press)
        .w(px(32.0))
        .h(px(32.0))
        .items_center()
        .justify_center()
        .rounded(move || theme().radius)
        .bg(move || theme().muted)
        .active(|s| s.opacity(0.6))
        .disabled_style(|s| s.opacity(0.4))
        .child(text(symbol).text_color(move || theme().foreground))
}

// 归属者：
let quantity = signal(1_u32);
quantity_stepper()
    .value(quantity)
    .min(1)
    .max(10)
    .decrement_label("Fewer")
    .increment_label("More")
    .on_change(move |next: &u32| quantity.set(*next))
```

同一份契约的 TSX 写法，作为 `components/` 中的一个组件：

```tsx
import { Text, View } from "nsh";
import { Button } from "nsh/base";

export interface QuantityStepperProps {
  /** The quantity: the app owns the value. */
  value: number;
  /** The smallest quantity (0 when left out). */
  min?: number;
  /** The largest quantity (99 when left out). */
  max?: number;
  /** What screen readers say for the button that takes one away. */
  decrementLabel: string;
  /** What screen readers say for the button that adds one. */
  incrementLabel: string;
  /** The user asked for another quantity; the owner sets `value`. */
  onChange?: (e: { value: number }) => void;
}

export function QuantityStepper(props: QuantityStepperProps) {
  const min = () => props.min ?? 0;
  const max = () => props.max ?? 99;
  const step = (delta: number) => {
    const next = Math.min(max(), Math.max(min(), props.value + delta));
    if (next !== props.value) props.onChange?.({ value: next });
  };
  const buttonClass = "size-8 items-center justify-center rounded-(--radius) bg-muted active:opacity-60 disabled:opacity-40";
  return (
    <View class="flex-row items-center gap-3">
      <Button disabled={props.value <= min()} accessibilityLabel={props.decrementLabel} onPress={() => step(-1)} class={buttonClass}>
        <Text class="text-foreground">−</Text>
      </Button>
      <Text class="text-foreground">{props.value}</Text>
      <Button disabled={props.value >= max()} accessibilityLabel={props.incrementLabel} onPress={() => step(1)} class={buttonClass}>
        <Text class="text-foreground">+</Text>
      </Button>
    </View>
  );
}
```

### 限定所有权关系与订阅生命周期

组件拥有它创建的东西。组件消失时，它创建的一切也随之消失：`<If>` 分支隐藏、行被移除、页面被弹出都是如此。在 Rust 中，
任务、定时器、间隔（`spawn`、`set_timeout`、`set_interval`）和按键绑定（`on_key`）会随创建时的归属者一起结束。在 TSX 中，
用 [`cleanup`](./reactivity.md#cleanup) 注册停止操作，或从 effect 中返回它。监听用 `on(event, listener)`
（`host.on(name, listener)`、`BackHandler.on("press", listener)`）。它返回一个停止监听的函数（`() => void`，Rust 中是
`Stop`）。组件渲染期间注册的监听随组件一起移除，`keymap.bind` 的绑定也一样。

某个东西停不下来时，先查这几处：

- 在组件外创建的 effect 和 computed 值。在事件处理函数或定时器回调中创建的没有归属者，会一直存在；
- 注册后没保留停止函数的监听；
- 从 store 启动、没有组件归属的工作。

这不是完整清单。被丢弃的 `resource` 加载会中止（TSX 中的 `info.signal`），不会让组件继续存活。

不要因为读取或派生了一个值就去写 signal。不要在读取某个 signal 的绑定或 effect 里无条件写它。否则它会反复触发自己，直到
NSH 报错终止。由其他状态算出的值用 `computed`，不要用 effect 写入第二个 signal。

### 防止状态反馈环

文本输入、选择、筛选和受控弹出层通常有两条路径：归属者设置值，用户请求新值。归属者的值同步进来时，不要再通过用户回调发回去。
处理用户操作时，以不跟踪的方式读取归属者的当前值，再和新值比较。不跟踪是指这次读取不订阅 signal。Rust 中写作
`props.value.get_untracked()`（见上例），TSX 的事件处理函数本身就不跟踪。这样每次变化只报告一次。如果回调可能在同一次调用
中关闭、替换或更新触发它的组件，回调必须能安全重入。

## 稳定标识

key 是行为的一部分。它让行、页面或重复的控件有稳定的身份。数据变化时，由 key 决定哪一项保留自己的状态、焦点和测量结果。

- 行、标签页、页面和重复控件使用稳定的领域 id：`each(projects, |p| p.id, ..)`、
  `<Each of={projects()} key={(p) => p.id}>`、列表的 `keyExtractor`；
- 同一控件重复出现时，用所属对象为测试 id 加命名空间（`delete-${project.id}`）；
- 可插入或重排的数据不能使用翻译后的文案或可变索引作为标识；
- 渲染时不能生成新的随机 key 或 id。

```tsx tabs
<Each of={projects()} key={(p) => p.id}>
  {(project) => (
    <Button onPress={() => remove(project().id)}>
      <Text testID={`delete-${project().id}`}>Delete</Text>
    </Button>
  )}
</Each>
```

```rust tabs
each(projects, |p| p.id, move |project, _index| {
    let id = project.get_untracked().id;
    button()
        .on_press(move || delete(id))
        .child(text("Delete").test_id(&format!("delete-{id}")))
})
```

key 改变就是界面身份改变，由此带来的状态重置必须是有意为之。导航路由、列表 key、存储键和测试 id 都遵循这条规则。留在
`setRoutes` 里的路由会保留页面和状态。两个独立的行为共用一个 key，会互相覆盖状态。每次更新都换 key，则永远留不住状态。

## 渲染与组合

组件函数体保持声明式：读取状态、算出展示值、组合元素。领域操作、解析和复杂的修改放到 store 或应用 crate 的具名函数里。

```tsx tabs
function ProjectPage() {
  return (
    <View class="flex-1">
      <Toolbar />
      <Content />
    </View>
  );
}
```

```rust tabs
#[component]
pub fn ProjectPage() -> impl IntoElement {
    view()
        .flex_1()
        .child(toolbar())
        .child(content())
}
```

如果一个辅助函数能给一块区域起个清楚的名字，让读者少记一些状态，就把它提取出来。只有这块区域有自己的可复用约定或生命周期时，
才提取成新组件。构建器链或 JSX 块长，本身不是理由。

Rust 中统一使用类型化的样式方法和组件的构建器，TSX 中使用类名。小的条件修饰用 `.when(..)` / `.when_some(..)`（Rust）和
`classList`（TSX）。分支是差别很大的两套界面时，用 `when` / `<If>`。闭包里的三元表达式只要读取的内容一变，就会重建分支。

先用标准组件，再考虑自定义。不要为了贴合一张截图，用普通视图重做菜单、弹出框、面板或对话框。`nsh/base` 的 `Menu`、
`ContextMenu`、`Popover`、`Sheet`、`Tooltip` 和 `Dialog` 已经处理好定位、焦点转移、键盘导航、选择、禁用状态、按 Escape
和返回键关闭，以及无障碍。如果标准组件表达不了一个常用的合理模式，就改进它的 API，不要在每个调用处给内部视图加样式。

应用提供的渲染函数不能有副作用。列表的行、`TextView` 的 `renderCodeBlock`、菜单的条目，都可能在归属者需要单元格、部件或
打开菜单时随时运行。它们不能执行业务操作、追加数据，也不能开启无限期的订阅。

## 行为与表现的边界

Base 的长期规则是：

> Base 拥有可复用行为，以及实现行为所必需的结构；表现层拥有产品视觉语言。

“无样式”不等于“只有一个空视图”。浮层定位、键盘导航、编辑、虚拟化、返回导航、关闭和安全区域都需要内部结构和状态。把这些
工作推给每个调用者并不会更灵活，只会把脆弱的行为复制到每个应用里。

另一方面，Base 不能决定品牌色、字体、密度、最终图标、变体、文案或应用的组合方式。外观通过这些方式开放：主部件上的
`class` / `style`，其他每个部件的 `<part>Class` / `<part>Style`，类型化的状态样式（`active`、`hover`、`focus`、`selected`、
`checked`、`disabled`），命名插槽（`trigger`、`header`、`footer`），以及渲染函数。不要遍历子元素去猜哪个是标题、描述或关闭
按钮。每个部件都要显式提供，每个标签都通过 prop 或子元素传入。

## 主题与样式

从主题读取语义值，在 Rust 中用类型化的样式方法、在 TSX 中用 Tailwind 类名布局（[Tailwind](./tailwind.md)、
[主题 token](./tailwind.md#主题-token)）：

```tsx tabs
<View class="rounded-(--radius) border border-border bg-background text-foreground" />
```

```rust tabs
view()
    .bg(move || theme().background)
    .text_color(move || theme().foreground)
    .border_1()
    .border_color(move || theme().border)
    .rounded(move || theme().radius)
```

规则：

- 不写死产品颜色、圆角、间距或控件几何尺寸；
- 应用代码不得引入原始的十六进制、`rgb` / `rgba` 或 `hsla` 值；颜色从 `theme()` 或 token 类名（`bg-primary`）读取，缺少
  语义角色时补入应用的主题文件；
- 应用布局使用间距和字号刻度（`p-2`、`gap-3`、`w-64`、`text-sm`；`.p_2()`、`.gap_3()`、`.w_64()`、`.text_sm()`），而不是
  任意值（`p-[13px]`、`px(13.0)`）；
- 类名的含义与 Tailwind CSS v4 完全相同，样式值的含义见[样式参考](../modules/style-reference.md)（即 CSS 的含义，采用原生的写法：数字的单位是点）；`unknown class` 警告是需要修复的问题；
- token 按语义而不是调色板位置使用；
- 与状态无关的几何放在普通的构建器链或类名字符串中；
- 运行时交互状态使用 `active:`、`hover:` 和 `focus-visible:`（`.active(..)`、`.hover(..)`、`.focus_visible(..)`），它们原生
  生效，不需要往返应用代码。`focus-visible:` 是键盘的焦点环：键盘移来焦点时显示，点击或轻点之后不显示；任何焦点都要显示的
  地方（输入框的边框、列表里高亮的行）用 `focus:`；
- checked、selected、pressed、disabled 外观使用组件的语义状态样式（`selected:`、`checked:`、`disabled:`）；
- 弹出层的所有权使用显式状态（`open` 与 `onOpenChange`），使触发器在关闭前持续呈现打开或按下的外观；
- 变体保持少而有意义，不为每个调用处增加一个变体；
- 主要样式绑定决策区域真正的默认提交和回车键操作，不能从操作数量、频率或工具栏位置推导；
- 徽标和提示的变体保持语义化且稀缺。普通元数据使用中性样式；不能仅因为变体存在，就把每个枚举值或分区映射成不同颜色。

有效的优先级为：元素自身的样式，然后是状态样式，后面的状态优先，`disabled` 高于一切。后一层只替换自己设置的字段。

新写的外观代码优先用主题的语义 token。主题文件里的每个名称都是一个 token，类似 Tailwind `@theme` 里的变量。例如
`"up": "#16a34a"` 会生成 `text-up`、`bg-up/30`、TSX 中的 `token("up")` 和 Rust 中的 `theme().color("up")`。主题有浅色和深色
两套值，所以 token 不用 `dark:` 类名也会跟随外观模式。token 按用途命名，不按组件命名，否则每个控件都会长出自己的 token。

在应用的主题文件（`nsh.json` 的 `"theme"`）里修改主题，也可以让嵌入它的应用提供主题。主题变化时，引用 token 的类名和读取
`theme()` 的 Rust 绑定都会原生地更新样式，不运行任何 JavaScript。把 token 的值复制到自己 signal 里的代码，就不再跟随主题。

画在控件外侧的焦点环需要留出空间，带 `overflow: hidden`（`overflow-hidden`）的祖先会把它裁掉。布局尽量留出空间。设计上确实
要裁剪时，把焦点状态画在控件内部，不要让键盘焦点悄悄消失。

### 用户的字号就是应用的缩放

文本跟随用户的字号设置（动态字体、Android 的字体缩放、桌面系统的设置）。TSX 用 `fontScale()` 读取，Rust 用 `font_scale()`，
两者都是响应式的。字号刻度给出基准（`text-sm`，以及正文字号 `text-base`，16 点），再按用户设置缩放。这沿用了 Tailwind 的做法：
字号、间距和尺寸都用具名档位，而不是一堆互不相关的点数。

文本容器要能随文字变高：给最小高度和内边距，不给固定高度。自定义组件如果把缩放的文字和固定尺寸混在一起，必须在文档里说明
固定部分为什么不该变大。

评审时，应用界面里每个任意的点数值和原始颜色都要指出来。只有这几种情况可以例外：文档写明的物理或平台限制、运行时测得的几何
尺寸、图片或数据本身的颜色、主题定义本身。图方便或贴合截图都不算理由。

根据布局结果缓存的东西，在字体缩放变化时必须失效。可以把缩放值放进缓存键，也可以用一个随它变化的版本号。这包括：告诉列表的
行高、文本测量结果、弹出层的几何尺寸、按文字推算的图标尺寸，以及自定义画布的度量。

## 事件、命令与焦点

激活用按压事件：`onPress`、`onLongPress`、`onPressDown` 和 `onPressUp` 对手指、笔、鼠标和键盘的含义相同。触摸处理函数和
手势只用于需要跟着手指走的操作（平移、拖动）。可以由多种输入触发的命令，用键盘快捷键（`keymap.bind`、`on_key`）和菜单项。
处理函数放在拥有该命令的页面附近。

每个命令只写一次。`Button`、`Menu` 条目、`ContextMenu` 条目、`nsh/desktop` 的应用菜单项和键盘快捷键，都调用 store 里的
同一个函数，不要把修改逻辑复制五份。条件允许时，标签、图标、快捷键和禁用状态从同一处得出，各入口就不会互相矛盾。
`key_label("mod+s")` 会按平台习惯写出快捷键。菜单负责导航和关闭，命令是否可用、具体做什么由页面决定。

```tsx tabs
keymap.bind("mod+s", save);

<View>
  <Button onPress={save}><Text>Save</Text></Button>
  <Menu trigger={<View class="px-3 py-2"><Text>File</Text></View>}>
    <MenuItem onSelect={save}><Text>Save  {keymap.label("mod+s")}</Text></MenuItem>
  </Menu>
</View>
```

```rust tabs
on_key("mod+s", move |_| save());
view()
    .child(button().on_press(save).child(text("Save")))
    .child(
        menu()
            .trigger(view().px_3().py_2().child(text("File")))
            .child(menu_item().on_select(save).child(text(format!("Save  {}", key_label("mod+s"))))),
    )
```

按控件的语义选择元素。命令即使要显得低调，也用 `Button` 并给它低调的样式，不要换成链接。NSH 应用约定，`link` 角色只用于在
应用外打开的目标，例如 URL 或电子邮件地址。应用内的去处用导航，命令用 `Button`。

只有当嵌套的交互必须阻止父级处理同一次触摸时，才在触摸事件上调用 `stopPropagation()`。到处阻止传播会弄坏滚动、按压和手势，
而且很难排查。

**主线程从不等待应用代码。** 有时系统需要马上得到答案：这个页面能不能弹出，这个面板或弹出框能不能关闭，这段文本能不能修改。
这时 NSH 根据事先设好的 props 在原生层直接回答，之后再用事件通知应用。所以决定要写成 prop，一知道就设置，而不是写成返回
true 或 false 的回调：
`popOnBack={!dirty()}` 配合 `onBackPress` 进行确认（[popOnBack](./navigation.md#把用户留在当前页面poponback)），
`closeOnEscape` 和 `closeOnBackdropPress` 配合 `onCancel` 获知用户的尝试，输入框上的 `maxLength`，按钮上的 `disabled`。

菜单项的 `shortcut`（`nsh/desktop`）在菜单设置期间绑定按键，菜单按平台风格显示它。同一按键的 `keymap` 绑定优先。每个按键
只给一个归属者。菜单项或快捷键变化时，重新设置应用菜单。

焦点归属必须明确：

- `Button`、`Switch` 和菜单项在桌面和 Web 上是键盘焦点的停靠点；在它们之上构建控件，而不是在裸视图上；
- 在事件中，或在跟随状态变化的 effect 中，用 ref 的 `focus()` 移动焦点，用 `set_accessibility_focus()`（Rust）或
  `nsh` 的 `setAccessibilityFocus`（TSX）移动屏幕阅读器的焦点；
- 绘制清楚的 `focus-visible:` 状态；
- 禁止在渲染时无条件设置焦点。

复合控件要实现完整的键盘导航，而不是零散的几个快捷键：方向键，合适时的 Home / End 或翻页，确认、取消和 Tab。

浮层从最上层开始关闭：Escape 和 Android 返回键交给最上层正在监听的浮层（`onCancel`）。快速连续关闭、打开时，不要对正在关闭
的浮层执行操作。

## 异步任务与副作用

异步工作从事件、effect 或 [`resource`](./reactivity.md#resource) 启动，不要在组件函数体里无条件启动。effect 只跟踪同步读取
的内容，所以先读 signal，再启动工作。工作完成后通过 signal 更新状态。要考虑到组件可能已经不在了。相关的修改一起写入。

重复执行的工作挂在归属者下面，归属者消失时工作就会取消。Rust 的任务和定时器随归属者结束，TSX 的定时器在 `cleanup` 中停止。
必须独立完成的一次性任务可以从 store 启动，但这样就没人能取消它。错误和失败的请求都要明确处理。

异步操作要有明确的状态：resource 的 `state()` 取值为 `unresolved`、`pending`、`ready`、`refreshing` 或 `errored`。刷新时
继续显示旧数据：resource 重新加载期间保留原来的值。防止重复提交破坏性操作。可恢复的错误要显示在界面上，日志不能代替给用户的
反馈。

```tsx tabs
const results = resource(
  () => query() || undefined,
  async (q, { signal }) => {
    const response = await fetch(`https://api.example.com/search?q=${encodeURIComponent(q)}`, { signal });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return (await response.json()) as SearchHit[];
  },
);
```

```rust tabs
let results = resource(
    move || {
        let q = query.get();
        (!q.is_empty()).then_some(q)
    },
    |q, _info| async move {
        let response = fetch(format!("https://api.example.com/search?q={q}")).await?;
        if !response.ok() {
            return Err(format!("HTTP {}", response.status()));
        }
        Ok(response.text())
    },
);
```

把耗时的解析或计算移出 TSX 处理函数：放进 `computed`、移出交互路径，或改用 Rust。结果可能在查询、页面或选中项变化之后才到。
resource 会丢弃过时的加载。自己写的异步工作要带上版本号或 id，丢弃过期结果，不要把它套用到新状态上。

## 布局、测量与滚动

视图默认把子元素排成一列，子元素在横向上拉伸。`class="flex"`（`.flex_row()`）排成一行，与 CSS 相同。一排控件通常要居中，
用 `items-center` 写明。一排等高的列保留默认的拉伸，这样每列都填满行高，标题留在顶部：

```tsx tabs
<View class="flex flex-1">
  <Sidebar />
  <Content />
</View>
```

```rust tabs
view()
    .flex_row()
    .flex_1()
    .child(sidebar())
    .child(content())
```

大多数界面靠布局完成，不要自己测量（[布局](./layout.md)）。测量（`onLayout`、`measure()`）用于浮层、虚拟化、编辑器、图表
这类必须拿到最终几何尺寸才能正确工作的组件。

- 测量与几何放在拥有行为的层；
- 只有普通布局无法表达关系时才观察布局；
- 不能在每次布局事件中写入无关的应用状态；
- 测得的数据只在某个版本内有效；字号、宽度、主题或内容改变后可能过期；
- 弹出层交给 `nsh/base` 的浮层定位：它们的定位、翻转和安全区域边距遵循同一套规则。

对齐靠结构保证，而不是事后修正。相邻区域使用同一个间距 token 或共享缩进，不要各写一个相同的数字。重要的重复边缘、列和间距，
要加几何检查，或在真实平台上截图（`nsh test --target`）。不要只测默认窗口。大字号和显示缩放会把小数坐标变成一像素的偏差，
即使默认截图看起来是齐的。

评审精度时要测量实际结果，但不要用任意数值去补测出的偏差。要追查原因，比如内边距重复、缩进嵌套、边框归谁、字体度量或取整，
然后在源头修复。

每个可滚动区域只有一个归属者，避免同一方向上无意的嵌套滚动。滚动视图应该是容纳整个页面、面板或列表的那个元素，这样滚动条
贴着区域边缘。内容的内边距放在滚动视图内部（`contentClass` / `contentStyle`），不要在外面再包一层带内边距的容器。滚动条
悬在内容和页面边缘之间，通常说明滚动归属者选错了，或内边距放错了层。在内边距和外边距中用 `env(safe-area-inset-*)` 和
`env(keyboard-inset-height)` 避开安全区域和键盘，数值用对应的访问器读取。它们会在原生层跟随旋转和键盘变化。

## 列表、表格与大数据集

会增长的集合用 [`List`](./lists.md)（Rust 中的 `virtual_list`），例如聊天记录、信息流和历史记录。普通子元素的滚动视图只用于
少量、有上限的内容。行的身份和它在屏幕上的位置分开管理，不要每次更新都复制全部数据。复用、滚动和可见范围由列表负责，行的
外观由行函数负责。

分离以下状态：

- 源数据与领域 id；
- 筛选与排序；
- 选择；
- 滚动；
- 行渲染。

这样更新保持局部，也不会把视图树变成数据模型。

虚拟化是指列表只为可见的行创建视图。它改变的是行为，不只是速度。行函数每个单元格只运行一次，单元格会复用给其他行：

- **必须把每个条目的状态放在以 id 为键的数据中**，而不是行内创建的 signal 中，后者会随单元格移动到下一个条目；
- **必须在使用处、在绑定中读取条目**：在行函数开头复制出来的值属于这个单元格显示的第一行；
- 在 TSX 中以不可变的方式更新（新的数组，复用未变的条目，变化的条目是新对象），并给一个接近典型行高的
  `estimatedItemSize`；
- 即使多数行当前不存在，滚动到条目（`scrollToIndex`）仍在数据坐标上工作。

```tsx tabs
const liked = signal<Set<number>>(new Set());

<List each={posts()} estimatedItemSize={64} class="flex-1">
  {(post) => (
    <View class="flex-row p-4">
      <Text class="flex-1">{post().title}</Text>
      <Text onPress={() => toggle(post().id)}>{liked().has(post().id) ? "♥" : "♡"}</Text>
    </View>
  )}
</List>
```

```rust tabs
let liked = signal(HashSet::<u32>::new());

virtual_list(move || posts.with(Vec::len), move |at| {
    let post = computed(move || posts.with(|p| p[at.index()].clone()));
    view()
        .flex_row()
        .p_4()
        .child(text(move || post.with(|p| p.title.clone())).flex_1())
        .child(
            text(move || if liked.with(|s| s.contains(&post.with(|p| p.id))) { "♥" } else { "♡" })
                .on_press(move || toggle(post.with(|p| p.id))),
        )
})
.estimated_item_size(px(64.0))
```

## 公共 API 设计

可复用组件应遵守：

- props 建立合法的默认值：Rust 中用 `#[prop(default)]` 或 `#[prop(default = …)]`，TSX 中在读取 prop 的地方用 `??`；
- 每个 prop、事件和导出都有文档注释；在 Rust 中，它会成为 TSX 看到的 TypeScript 声明；
- 回调描述请求的变化；只有指针细节有意义时才带上它们；
- 显式的部件、插槽和渲染函数优于检查任意后代；
- 可复用行为不能强制产品层面的视觉选择。

应用以签名包的形式下发到宿主应用，之后可能不再重新构建，NSH 以后的每个版本都会原样运行它们。应用或组件库对外暴露的内容
也要同样谨慎，包括组件的 props 和事件、crate 的导出，以及宿主应用的原生组件和模块收到的函数：

- 输入只增加可选成员；类型只放宽；默认值和行为保持不变（新行为需要主动开启）；
- 参数可能增加的函数把选项放在一个对象里（`scrollTo({ x, y, animated })`），而不是再加一个位置参数；永远不会增加的参数（例如范围的起点和终点）保持为位置参数；
- 模式是可以增加取值的字符串联合类型，而不是以后需要第三种状态的布尔值；
- 一个值只有一种类型：`version: string`，而不是 `string | number`；
- 函数返回一个可以增加字段的对象，而不是元组或单独的值；
- 返回 Promise 的函数通过 reject 报告所有失败，调用方能处理的失败带上 `code`，NSH 自己的接口也是如此：调用方按 code 区分，
  消息是给人看的，可能会变；
- 重命名时保留旧名称，作为同一实现的 `@deprecated` 别名。

在 Rust 中，行为状态默认放在私有字段里，以后改动不会影响调用方。纯配置记录和序列化数据可以用公开字段。凡是带公开字段的
`pub struct`，都必须标注 `#[non_exhaustive]` 并实现 `Default`。这样调用方不必写出全部字段，以后加字段也不会影响他们。

重组内部代码时，公开的导入路径保持不变。其他代码导入的是 crate 的 `#[native_module]` 名称及其导出，以及 TSX 模块的导出。
命名优先用平台控件的术语和 NSH 已有的词，不用 Web 框架的说法。

## 平台与能力边界

不要假设每个平台都有同样的能力。标题栏和窗口控件、无障碍、剪贴板、触感反馈、滚动物理、字体和时序都可能不同。同一个包在
所有平台上运行（[平台特定代码](./platform-specific.md)）。平台特定的代码放在 NSH 为它留的位置，并定义其他平台上的行为：

- 元素在各平台上都要表达的意思是顶层 prop；平台系统自带的功能放进 `ios={{…}}`、`android={{…}}`、`macos={{…}}`、`windows={{…}}`、
  `desktop={{…}}` 或 `web={{…}}`（Rust 中是构建器方法），有这个功能的每个平台各放一份，只有当前平台的对象会生效；
- 移动平台的 API 来自 `nsh/mobile`，桌面的 API 来自 `nsh/desktop`；
- 按平台设置的样式使用 `ios:`、`android:` 和 `web:` 类名。

平台分支的外观可以不同，含义必须一致。例如对话框在 iOS 上是面板，在桌面上是盖在窗口上的一层，但应用里的打开状态仍要一致。
共享行为在无界面环境中测试，平台相关部分用 `nsh test --target` 测试。按 NSH 报告的值（`platform.os`、状态、原因）分支时，
每个分支都要有 `default`。以后的版本会增加平台和取值，今天下发的应用也要在上面运行（[兼容性](./compatibility.md)）。

应用在 `nsh.json` 的 `"capabilities"` 中声明它要访问 VM 之外的哪些东西。每一项称为一个能力（capability），例如可以请求的
主机、Web 视图显示的页面、剪贴板、状态栏（[沙箱与权限](./sandbox-and-capabilities.md)）。要下发到其他应用中的应用，应准确
列出用到的能力。宿主应用授予的可能比声明的少，所以被拒绝的请求要和失败的请求一样处理。被拒绝的调用会以 `code` 为
`"ERR_ACCESS_DENIED"` 的错误失败，其中 `permission` 是能力名，`resource` 是请求的内容。

## 文件、代码与命名风格

- 页面和组件以产品概念命名：`ProjectList`、`ProjectEditor`、`SettingsPage`；
- 处理函数以意图命名：`confirmDelete`、`openProject`、`onQueryChange`；
- 每个模块一个主要职责；必须读无关行为才能理解状态或生命周期时就应拆分；
- 组件的模块、props、事件与针对性的测试在共同变化时放在一起；
- 注释记录不变量和意外的生命周期约束，不复述显而易见的构建器调用；
- Rust 使用 `rustfmt` 并满足 Clippy，TSX 通过 `nsh check`，不能用宽泛的 `allow` 隐藏无关警告。

### 词汇是 API 的一部分

同一个概念在所有组件里用同一个词。给 prop 或事件命名前，先在 NSH 自己的 API 里找现成的词，再看各平台的控件。原语显示的内容
和全局 API 沿用 HTML、DOM 和 CSS 的名字（`src`、`html`、`placeholder`）。Rust 名称用 `snake_case`，到 TSX 中变成
`camelCase`（`on_open_change` 即 `onOpenChange`）。一个词保持一个词（`readonly`、`multiline`）。

| 概念 | 命名模式 | 示例 |
| --- | --- | --- |
| 组件 | 名词，`PascalCase` | `Button`、`Dialog`、`QuantityStepper` |
| 它的 props 和事件（Rust） | `<Component>Props`、`<Component>Event` | `ButtonProps`、`ButtonEvent::Press` → `onPress` |
| 行为的状态对象 | `<Control>State`，由 `create<Control>` 创建 | `NavStackState`、`createNavStack` |
| 值变化的原因 | `<Control>ChangeReason`，字符串联合类型 | `DialogChangeReason`：`"cancel"`、`"backdropPress"` |
| 状态 | 形容词，正向，不带 `is` 前缀 | `open`、`hidden`、`disabled`、`selected`、`checked`、`readonly` |
| 用户可以做什么 | 对应的形容词 | `scrollable`、`zoomable`、`selectable`、`resizable` |
| 没有形容词的功能 | `<feature>Enabled` | `pagingEnabled` |
| 部件是否显示 | 以部件命名的布尔值，默认 `true` | `scrollbar`、`caret`（`scrollbar={false}`） |
| 插槽是否显示 | `<slot>Visible`，默认 `true` | `headerVisible`（`header` 是页面的插槽） |
| 部件的样式 | `<part>Class`、`<part>Style` | `backdropClass`、`contentStyle` |
| 状态的样式 | `<state>Style`、`<state>:` 类名 | `activeStyle`、`disabled:` |
| 插槽 | 它是什么 | `header`、`footer`、`trigger`、`content`、`empty` |
| 应用提供的内容 | `render<Part>` | `renderCodeBlock` |
| 关闭开关 | `closeOn<Trigger>`，默认 `true` | `closeOnEscape`、`closeOnBackdropPress` |
| 值的变化 | `on<Value>Change({ <value>, ... })`，一个对象 | `onChange({ value })`、`onOpenChange({ open, reason })` |
| 其他事件 | `on<Event>`，现在时，一个对象 | `onPress()`、`onSubmit({ text })` |
| 阶段 | `<act>Start`、`<act>End` | `onPanStart`、`onLoadStart` |
| 鼠标位于视图上方 | `Enter` / `Move` / `Leave`，与 DOM 相同 | `onMouseEnter`、`onMouseMove`、`onMouseLeave` |
| 鼠标按键松开 | `Up`，与 DOM 相同 | `onMouseUp` |
| 开始监听 | `on(event, listener)`，返回 `() => void` | `host.on(name, l)`、`BackHandler.on("press", l)` |
| 环境中的值 | 访问器，旁边是 `set<Value>` | `colorScheme()`、`setColorScheme` |
| 只读的布尔值 | `is<Adjective>`、`has<Noun>` | `isAvailable` |

props 从不使用 `is<Adjective>`：这种形式只用于只读的布尔值（常量、事件数据的字段）。显示的部件以部件本身命名，从不用
`<part>Shown`、`<part>Hidden` 或 `shows<Part>`。

Rust 构建器接收并返回 `Self`，以它设置的 prop 命名，不加 `set_`（`.disabled(..)`、`.checked(..)`、`.on_change(..)`）。
原地修改用 `set_<field>`（`setRoutes`、`setColorScheme`）。布尔读取只有两种形式：值里装着某样东西时用 `has<Noun>`，描述
状态或许可时用 `is<Adjective>`。动作有形容词形式时就用形容词：用 `isClosable` 而不是 `canClose`，用 `isZoomable` 而不是
`canZoom`。动作是没有形容词形式的动词短语时，改用它需要的东西命名：用 `hasDefinition`，不用 `canGoToDefinition`。不再新增
`can` 开头的读取。

### 让外层名称承载上下文

名称总是放在某个上下文里读：prop 在组件里，字段在类型里。名称不重复外层已经说过的内容（用 `contentStyle`，不用
`contentContainerStyle`；`Dialog` 上用 `backdropClass`，不用 `dialogBackdropClass`）。同一类型的字段缩写程度保持一致。

### 使用精确的领域词汇

- **selected** 是成员关系或当前条目；**checked** 和 **pressed** 是复选框或开关的值；**focused** 是键盘的目标；
  **hovered** 是指针的停留。永远不要混用。
- **open / close** 是浮层的状态；**show / hide** 是临时的请求；**expand / collapse** 是结构。
- **disabled** 阻止交互；**readonly** 允许选择和导航但不允许编辑；**loading** 在操作进行时防止重复工作。
- **press** 是任何输入方式的激活，NSH 的事件都用 press（`onPress`）；**click** 是鼠标的。
- **index** 是位置；**id** 是标识。不要按索引持久化可重排的数据，也不要用索引作它的键。
- **value** 是受控的领域数据；**state** 是保留下来的行为。
- **src** 是元素显示的资源（图片、视频、页面）；其他地址都是 **url**。
- **role** 说明一个选项的含义（`"cancel"`、`"destructive"`）；外观属于应用。

当存在更窄的领域术语时，避免 `data`、`item2`、`handleAction`、`updateUi`、`process`、`manager` 或 `config` 这类含糊的
名称。

### 类型与模块命名

- Rust 类型使用 `UpperCamelCase`；模块、函数、方法、字段和局部变量使用 `snake_case`；常量使用 `SCREAMING_SNAKE_CASE`。上下文
  绑定命名为 `cx`；
- TSX 组件及其标签使用 `PascalCase`；props、函数和局部变量使用 `camelCase`。原语从 `nsh` 导入，基础组件从 `nsh/base` 导入；
- 以组件命名的模块拥有它的公开接口；可以把它的状态、部件和测试拆到多个文件中，而不改变别人导入的内容；
- 单一组件概念使用单数模块名，组件族使用既有名称（`menu` 包含 `Menu`、`MenuItem` 和 `Submenu`）；
- 标识符后缀 `Id`，集合使用有意义的复数；同一子系统不混用 `idx`、`index`、`ix`；
- 谓词尽量正向命名；正向的状态比多重否定更容易组合，但与控件语义一致的既有术语（如 `disabled`）应保持不变。

### 回调与事件的措辞

- 每个事件都传一个对象，即使只有一个字段、即使是值的变化，字段按它们报告的属性命名：`onChange({ value })`、
  `onOpenChange({ open, reason })`、`onLinkPress({ url })`。这样以后增加字段只是一次添加。API 的数据类型是一个字段，
  而不是载荷本身（`onImagePress({ image })`）；没有内容可报告的事件不传参数。
- 以动作命名，不用过去时，也不用请求：`onPress`、`onClose`、`onCancel`、`onBackPress`。
- 当动作有名字时，以意图而不是几何位置命名：`onLoadMore`，而不是“到达了末尾”。
- 生命周期成对使用 `onOpen` / `onClose`；阶段用 `Start` / `End`，从不用 `Begin`；鼠标位于视图上方是 `onMouseEnter` /
  `onMouseMove` / `onMouseLeave`（与 DOM 的名称相同），文件拖过视图是 `Enter` / `Leave`。
- 在文档中说明回调是在组件自身状态变化之前还是之后运行，以及它是否可能重入组件。

### 文档与界面文案

公开文档先说明组件做什么、谁拥有它的状态。示例必须使用当前的 API 和稳定的 id。记录默认值、平台差异、焦点行为、回调顺序，以及
主线程原生读取哪些 props。

标签、命令、确认对话框、大小写和省略号，遵循设计指南中的界面用语规范。每个领域对象、命令和状态只用一个固定术语。翻译键
描述稳定的意图（`dialog.deleteProject.title`），不照抄源语言的句子，也不写屏幕位置。不要用翻译片段拼句子。两个含义碰巧英文
相同，也不能共用一个键。

本地化翻的是意思，不是句式。每种语言自己决定语序、复数、标点和需要多少上下文。测试应能发现缺失的键和不一致的固定术语。
某处重复是否合理，仍由人来判断。每条字符串都要放回真实组件里，用接近真实的内容、更长的译文和大字号来验证。

## 测试策略

使用能够证明行为的最低层（[测试](./testing-and-trace.md)）：

1. 纯测试：状态转换、格式化、解析、排序（对 crate 中函数的 `cargo test`，或针对 store 函数的测试文件）；
2. 无界面的应用测试：组件、事件与生命周期，使用真实的引擎、渲染器、布局和列表，以 60 Hz 的显示帧运行，在 `nsh test`
   （`nsh/test`）或 `cargo test`（`nsh_testing::Headless`）中；
3. 交互测试：焦点、键盘、指针、布局与渲染状态，执行用户的操作（`press`、`type`、`scroll`、`drag`、`keyboard.press`），
   在无界面环境和真实平台上（`nsh test --target macos`、`ios`、`android`、`web` 等）；
4. 示例或应用的冒烟测试：完整流程；`nsh test --trace` 用于了解一次交互的开销。

```tsx tabs
import { expect, open, test } from "nsh/test";

test("a tap counts", async () => {
  const app = await open("./App.tsx");
  await app.press("role:button[Tap]");
  expect(app.texts()).toContain("Count: 1");
});
```

```rust tabs
#[test]
fn a_tap_counts() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<my_app::App>())).unwrap();
    app.wait_for("Count: 0").unwrap();
    app.press("Tap").unwrap();
    app.wait_for("Count: 1").unwrap();
}
```

交互组件测试它承诺的行为，而不是实现细节：按压和键盘激活、受控值的变化、禁用、焦点移动、事件的次数和顺序、稳定的身份、
关键的空状态和失败状态。模拟网络（`network` 路由），控制时间（手动时钟），不要等真实的网络和时间。缺陷能稳定复现时，先写
回归测试再修。

像用户一样查找视图：按角色和名称、标签或文本（`role:button[Save]`、`label:Close`）。没有别的名字可用时，用稳定的 `testID`。
选择器每一步都会重新解析，所以操作不会落在过期的快照上。选择器表达不了的视觉事实用截图验证（`app.screenshot`，在真实平台
上）。坐标只作为最后手段。自动化结果和人工结果分开报告。

## 性能规则

- 不能在读取某个状态的绑定或 effect 中无条件写入它；
- 不要在绑定或行函数中创建 signal、effect、按键绑定和昂贵的数据结构；
- 在绑定中读取 signal，让一次一致的修改只更新树中最窄的部分；
- 长集合使用 `List`，只渲染可见范围；
- 不为满足闭包而 clone 大的字符串或集合，捕获 `Copy` 的 handle 或共享数据（`SharedString`、`Rc`）；
- 保持 TSX 的处理函数和 computed 值轻量；把重活移出交互路径或改用 Rust；
- 加缓存前先用 `nsh test --trace` 测量，缓存必须有明确的失效归属者；
- 动画工作有界：用在 UI 线程上运行的动画值而不是每帧写入的 signal，用 `scrollValue` 而不是滚动监听
  （[动画](./animation.md)），并遵守减少动态效果的设置（`reduceMotion()`）。

## 常见失败模式

避免以下模式：

- 一个 signal 保存应用互不相关的状态；
- 很长的组件函数体中混入业务逻辑和网络请求；
- 可重排的内容使用随机或基于索引的 key；
- 每行的状态放在 `List` 行内创建的 signal 中；
- 在组件函数体中读取 prop 或 signal，却期望它会更新；
- 字面量颜色和圆角破坏自定义主题和深色模式；
- 已有语义组件时仍用可按压的自定义视图重做焦点、键盘、禁用和无障碍；
- 与受控值漂移的重复本地状态；
- 把一个 signal 复制到另一个 signal 的 effect，以及通过绑定循环的写入；
- 系统需要立即回答的决定交给回调，而不是事先设置的 prop；
- 没有归属者的嵌套滚动；
- 为一次性的页面新增组件变体；
- 可逆、低风险的操作也弹确认对话框；
- 测试只调用内部函数，从不按压、输入或滚动。

## 编码代理规则

修改前，必须先读最相关的实现、它的测试、模块的公开导出和对应的参考页面。签名要在当前源码和 `.nsh/types` 中查找，不能照着
React、React Native 或 CSS 的例子类推。NSH 的组件只运行一次，绑定跟随它读取的 signal。

每项改动都应能回答：

1. 行为归属者与表现归属者是谁；
2. 持久标识与状态生命周期是什么；
3. 按压、键盘、焦点、无障碍契约是什么；
4. 布局与滚动的归属者是谁；
5. 使用哪些主题 token，例外为何存在；
6. 哪个测试会在行为退化时失败。

生成的代码必须由人评审和测试。“能编译”不是界面质量的标准。只为让生成代码看起来整齐的大范围重构，也替代不了与应用架构
保持一致。

## 实现检查表

提交评审前，确认：

- 状态与副作用的归属是否明确；
- 普通函数与组件是否有意选择，props 和 signal 是否在使用处读取；
- 重复元素是否使用稳定的、基于领域的 key 和测试 id；
- 主题 token 与 Tailwind 刻度是否替代孤立的视觉字面量；
- 按压、键盘快捷键、焦点、禁用状态与浮层是否共同工作；
- 系统需要立即回答的决定是否是事先设置的 prop；
- 加载、空、错误、取消路径是否存在；
- 长数据是否使用 `List`，每个条目的状态是否放在数据中；
- 公开 API 的新增是否只做放宽，并保持依赖方向与封装；
- 测试是否在适当的层证明行为，既在无界面环境中，也在改动涉及的平台上；
- 格式化、Clippy、`nsh check`、针对性的测试与相关示例是否通过。

应用初始化请阅读[快速开始](./getting-started.md)，具体 API 以[元素](../elements/index.md)和[API](../modules/index.md)
参考页面为准。
