---
title: 编码指南
description: "Native Shell 应用与组件库的架构和编码约定：分层、状态归属、受控组件、标识、样式、主线程、列表、命名、兼容性与测试，均附 Rust 与 TSX 示例。"
order: 29.5
group: ui
---

# 编码指南

本指南介绍让 Native Shell 应用保持正确、快速、易于修改的架构与代码模式。读者是 NSH 应用的开发者（TSX 模块、应用自己
crate 中的 Rust 组件、嵌入 NSH 的原生应用）、在 `nsh/base` 之上构建组件库的作者，以及与他们协作的编码智能体。请先阅读
设计指南：代码结构用来实现产品意图，而不是取代它。

这是一份规范性指南。**必须**表示正确性、生命周期或兼容性方面的规则；**应当**是默认架构，偏离它需要具体的理由。确切的签名
以参考页面和 `.nsh/types` 中生成的声明为准。

## 让常见路径低成本

**设计原则：普通界面默认就应当是低成本的。** NSH 给每个应用同一条快速路径：组件只运行一次，之后只有读取了变化的 signal 的
绑定会再次运行；响应式图、渲染器、布局、列表以及应用导入的每个模块都是原生的；应用代码的一轮执行变成一批原生操作。只要应用
代码在使用处读取 signal、把状态放在最窄的归属者中、不在事件处理函数里做重活，就能保持这条路径。先测量再加缓存
（[性能](./performance.md)）；后面的章节说明其中的机制。

## 架构一览

NSH 应用分层构建，与 GPUI Kit 相同（[三层结构](./architecture.md#三层结构)）。依赖向下：上层负责领域含义与编排，下层负责
可复用的行为与外观。

| 层 | 位置 | 负责 |
| --- | --- | --- |
| 应用外壳 | 入口（`mount(App)`，或 `nsh.json` 的 `"root"`） | 根视图、导航栈、它显示的各个页面 |
| 页面 | `pages/` 中的一个文件 | 一个 `NavPage`：它的内容、对话框以及它发起的工作 |
| 应用组件 | 应用的组件库 | 重复出现、了解领域、带有产品外观的模式 |
| 组件库 | 应用自己的，或社区的 | 构建在 `nsh/base` 之上、带样式的通用组件 |
| `nsh/base`（`nsh_base`） | Native Shell | 无样式的行为：导航、对话框、按压目标、安全区域、键盘、主题 token |
| `nsh`（`nsh_core::ui::prelude`） | Native Shell | 原语、样式、布局、响应式、环境 |

可复用组件从不依赖应用页面，组件库从不依赖某个页面。`nsh/base` 自身不绘制任何外观：外观总是属于应用或它的组件库。

### 应用文件的布局

应用的文件遵循 NSH 的约定，因此无需任何配置：`App.tsx` 是入口，`nsh.json` 在它旁边，两者都按约定找到。

```text
App.tsx            # 入口，按约定：mount(App)
nsh.json
pages/             # 每个 NavPage 一个文件
  Inbox.tsx
  Compose.tsx
  Settings.tsx
components/        # 应用的可复用组件
  Card.tsx
store.ts           # 应用的状态：signal 以及修改它们的函数
native/            # 应用的 Rust crate（nsh.json "rust": "native"，mixed 模板）
```

- `pages/` 中每个 `NavPage` 一个文件。页面只在需要时才拆成更多文件。
- `components/` 存放应用的可复用组件，即多个页面都用到的组件。
- `store.ts` 存放应用的状态：它的 signal 以及修改它们的函数。页面读取这些 signal 并调用这些函数。
- 测试是放在被测代码旁边的 `*.test.tsx` 文件（`App.test.tsx`、`pages/Inbox.test.tsx`）。
- `native/` 是应用的 Rust crate，与 `mixed` 模板生成的一样。只用 Rust 编写的应用（`rust` 模板）则是一个 Cargo 项目，根组件
  位于 `src/lib.rs`（[项目结构](./project-structure.md)）。

## 启动与根视图的归属

一个应用只有一个根视图。TSX 入口调用一次 `mount(App)`；Rust 应用在 `nsh.json` 中指定根组件（`"root": "App"`）。根视图
填满整个界面：给它 `flex: 1` 和来自主题的背景色。

```rust tabs
#[component]
pub fn App() -> impl IntoElement {
    view()
        .flex_1()
        .bg(move || theme().background())
        .child(home())
}
```

```tsx tabs
import { mount, View } from "nsh";

mount(() => (
  <View class="flex-1 bg-background">
    <App />
  </View>
));
```

一个界面对应一个引擎：原生应用嵌入的每个 `NshAppView`、`NshView` 或 `AppView` 都运行自己的 VM、自己的模块状态和自己的
响应式图。为 NSH 编写的代码把状态放在应用的模块和组件中，因此同一宿主应用中的多个界面不会意外共享状态。一个导航流程有一个
`NavStack` 及其状态；`Dialog` 呈现在所有内容之上，包括栈的标题栏，所以对话框是打开它的页面的一部分，无论页面如何布局。

## 什么只运行一次，什么会再次运行

NSH 的模型来自 Solid，运行在原生的响应式图上（[响应式](./reactivity.md)）。请明确区分：

| 你写的 | 运行 | 再次运行的时机 |
| --- | --- | --- |
| 组件函数体 | 创建时运行一次 | 从不 |
| 绑定：放在值的位置上的闭包（Rust），或调用函数、读取属性的表达式（TSX） | 一次 | 它读取的 signal 变化时；只更新那一个 prop 或子元素 |
| `computed(fn)` | 创建时 | 它读取的 signal 变化时 |
| `effect(fn)` | 本轮的元素创建之后 | 它读取的 signal 变化时 |
| `each` / `<Each>` 的一行 | 它的 key 第一次出现时 | 从不；变化的是它的 item 和 index signal |
| `virtual_list` / `<List>` 的一个单元格 | 每个单元格一次 | 从不；单元格会被分配给其他行 |

- **必须在使用处读取 signal 和 props。** 在函数体中读取的值只读一次；`const { title } = props` 只读一次 `title`，写成
  `({ title })` 的参数也一样。在 JSX 中读取 `props.title`，或用 [`split`](./jsx-and-components.md#展开-props) 把其余
  props 传下去。
- **必须在组件运行时创建 effect、computed 值、定时器和监听。** 它们属于该组件，随组件一起消失。在事件处理函数或定时器
  回调中创建的，没有归属者，会一直存在。
- **不要期望组件再次渲染。** 组件只构建一次它那部分的树；跟随变化的是绑定。

## 选择合适的单元

### 函数、组件，还是类

- **普通函数**返回元素（`fn row(..) -> impl IntoElement`，或只在本地使用的 TSX 函数组件），是命名一块界面区域的默认
  选择。它的参数就是它的 props。
- **`#[component]`** 是带类型化 props（`#[derive(Props)]`）和事件（`#[derive(Event)]`）的 Rust 组件。当界面有自己的
  props 或事件、属于组件库，或被 TSX 导入时，写成组件：它的 TypeScript 接口和声明由这些结构体生成。
- **`#[class]`** 在 Rust 中保存 TSX 用 `new` 创建的长期状态（账本、解析器、会话），`#[module]` 导出函数
  （[用 Rust 构建](./rust.md#用-tsx-扩展-rust-应用)）。

不要把每个视觉片段都变成组件：组件边界是一份契约，它应当对应一份契约。

### Rust 还是 TSX

Rust 组件从头到尾都是原生代码；TSX 在解释器 QuickJS 中运行自己的代码，它调用的一切都是原生的
（[性能](./performance.md#1-js-做得越少越好)）。应用最常用的部分、组件库和繁重的业务逻辑用 Rust 编写；经常变化或在运行时
下发的部分用 TSX。同一组件的两种接口共享一个定义：用 Rust 编写的组件在 Rust 中用 `switch()` 放置，在 TSX 中用
`<Switch>`，props 和事件相同。

## 状态归属

把每个状态放在能保证其正确的最窄归属者中：

- 临时界面状态（一个字段的草稿、某个展开区域是否打开）是渲染它的组件中的 signal；
- 子树共享的状态是通过 [context](./reactivity.md#context) 提供的 signal；
- 应用的领域状态放在 `store.ts`（TSX）或应用的 crate（Rust）中，通过具名函数修改；
- 跨越一次运行的数据放在[存储](../modules/nsh-storage.md)或服务器上；
- 可复用组件负责的行为留在组件内部；它的值不在其中（见下文）。

### 受控值

**组件是受控的：应用拥有值，组件报告请求的变化。** 把当前值传进去，接收变化，更新归属者，绑定随之更新。回调报告意图；它从不
制造第二个隐藏的数据来源。`nsh/base` 的每个组件都这样工作（`Dialog` 的 `open` 与 `onOpenChange`，`Button` 的
`selected`），建立在它之上的组件库也应如此。

应用组件库中的一个开关，用 Rust 编写一次，从 Rust 和 TSX 放置：

```rust tabs
use nsh_base::theme;
use nsh_core::{AccessibilityState, Checked, ui::prelude::*};

/// The props of [`Switch`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SwitchProps {
    /// On or off: the app owns the value.
    #[prop(default)]
    pub checked: Prop<bool>,
    /// No presses; its disabled style applies.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// What screen readers say.
    pub accessibility_label: Prop<Option<SharedString>>,
}

/// What a [`Switch`] reports.
#[derive(Debug, Clone, PartialEq, Event)]
pub enum SwitchEvent {
    /// The user asked for the other value; the owner sets `checked`.
    Change(bool),
}

/// An on / off switch drawn with the app's theme.
#[component]
pub fn Switch(props: SwitchProps, cx: Scope<SwitchEvent>) -> impl IntoElement {
    view()
        .w(px(48.0))
        .h(px(28.0))
        .p(px(2.0))
        .rounded_full()
        .bg(move || if props.checked.get() { theme().primary() } else { theme().muted() })
        .disabled(props.disabled)
        .disabled_style(|s| s.opacity(0.5))
        .role(AccessibilityRole::Switch)
        .label(move || props.accessibility_label.get().unwrap_or_default())
        .accessibility_state(move || AccessibilityState {
            checked: Some(if props.checked.get() { Checked::True } else { Checked::False }),
            ..AccessibilityState::default()
        })
        .on_press(move || cx.emit(SwitchEvent::Change(!props.checked.get_untracked())))
        .child(
            view()
                .w(px(24.0))
                .h(px(24.0))
                .rounded_full()
                .bg(move || theme().background())
                .ml(move || if props.checked.get() { px(20.0) } else { px(0.0) }),
        )
}

// 归属者：
let wifi = signal(true);
switch()
    .checked(wifi)
    .accessibility_label("Wi-Fi")
    .on_change(move |on: &bool| wifi.set(*on))
```

```tsx tabs
import { signal } from "nsh";
import { Switch } from "my-app/native";   // native/ 中的 Rust 组件：#[native_module("my-app/native")]

function WifiRow() {
  const wifi = signal(true);
  return <Switch checked={wifi()} accessibilityLabel="Wi-Fi" onChange={(on) => wifi.set(on)} />;
}
```

在 TSX 组件库中，同一份契约：

```tsx
import { View } from "nsh";

export interface SwitchProps {
  /** On or off: the app owns the value. */
  checked: boolean;
  /** No presses; `disabled:` styles apply. */
  disabled?: boolean;
  /** What screen readers say. */
  accessibilityLabel?: string;
  /** The user asked for the other value; the owner sets `checked`. */
  onChange?: (checked: boolean) => void;
}

export function Switch(props: SwitchProps) {
  return (
    <View
      role="switch"
      accessibilityLabel={props.accessibilityLabel}
      accessibilityState={{ checked: props.checked }}
      disabled={props.disabled}
      onPress={() => props.onChange?.(!props.checked)}
      class="h-7 w-12 rounded-full p-0.5 disabled:opacity-50"
      classList={{ "bg-primary": props.checked, "bg-muted": !props.checked }}
    >
      <View class="size-6 rounded-full bg-background" classList={{ "ml-5": props.checked }} />
    </View>
  );
}
```

相关的修改一起写入：构成同一个不变量的多个字段在一个 [`batch`](./reactivity.md#batch) 中设置，effect 和 computed 值会
同时看到它们。

### 派生，而不是同步

由其他状态计算出的值是 `computed`（只读一次的廉价值用普通闭包即可），而不是写入第二个 signal 的 effect。让 `b` 与 `a`
保持同步的 effect 会让 `b` 在写入与 effect 之间过期，并多花一轮；写入自己所读值的 effect 会不断地再次运行自己，直到 NSH
以错误终止它。effect 用于副作用：请求、定时器、对 ref 的命令式调用、日志。

### 避免反馈循环

输入框、选择、筛选和对话框都有两条路径：归属者设置值，用户请求新值。组件不得通过变化回调把归属者提供的值再报告回去，每个
逻辑变化只报告一次。处理用户的操作时以不跟踪的方式读取当前值（Rust 中如上所示的 `props.checked.get_untracked()`）；在 TSX
中事件处理函数本身就不跟踪。让回调可以安全地重入：变化处理函数可能同步地关闭、替换或移除调用它的组件。

### 让生命周期有界

- 组件启动的东西随组件结束。在 Rust 中，任务、定时器和间隔（`spawn`、`set_timeout`、`set_interval`）随创建它们时的
  归属者结束；在 TSX 中，用 [`cleanup`](./reactivity.md#cleanup) 注册停止操作，或从 effect 中返回它。
- 开始监听的函数返回一个停止监听的函数（`() => void`）；组件渲染期间注册的内容随组件一起移除，`keymap.bind` 就是如此。
- [`resource`](./reactivity.md#resource) 只保留最新的一次加载：新的加载会丢弃正在运行的那次，组件消失时也一样。在 TSX
  中，把 `info.signal` 传给 `fetch`，被丢弃的请求就会中止。

## 稳定的标识

key 就是行为。数据变化时，它决定哪一行保留自己的状态、焦点和测量结果。

- 用稳定的领域 id 作为行、页面和重复控件的 key：`each(projects, |p| p.id, ..)`、
  `<Each of={projects()} key={(p) => p.id}>`、列表的 `keyExtractor`。
- 导航路由是值：在 `setRoutes` 中保留下来的路由保留它的页面和状态。
- 不要用翻译后的文案作 key；条目可能插入或重排时，也不要用索引。
- 不要在渲染时生成新的随机 id 或 key：key 变了就是新的标识，它的状态会从头开始。
- `testID` 命名测试要找的东西；保持稳定，并由领域派生（`delete-${project.id}`）。

```rust tabs
each(projects, |p| p.id, |project, _index| {
    text(move || project.get().name)
})
```

```tsx tabs
<Each of={projects()} key={(p) => p.id}>
  {(project) => <Text testID={`project-${project().id}`}>{project().name}</Text>}
</Each>
```

## 渲染与组合

让组件保持声明式：读取状态，派生展示值，组合元素。把领域操作、解析和请求移到 `store.ts` 或应用 crate 的具名函数中。当一个辅助函数能命名一块有
意义的区域、减少读者需要同时记住的内容时，就提取它；当一块区域有自己可复用的契约时，才提取为组件，而不仅仅因为构建器链或
JSX 块很长。

- 超过几个元素的分支用 `when` / `<If>`：闭包中的三元表达式在它读取的任何内容变化时都会重新构建分支。
- 在构建自定义界面之前先使用语义组件：命令用 `nsh/base` 的 `Button`（按压、禁用与选中状态、屏幕阅读器的按钮角色），模态
  界面用 `Dialog`，导航用 `NavStack`。带 `onPress` 的 `View` 只承载按压，把角色和状态留给应用。
- 应用传入的渲染回调（列表的行、`TextView` 的 `renderCodeBlock`、组件的渲染函数）没有副作用。归属者需要单元格或部件时就会
  运行它们：它们从不执行业务操作、追加数据或开始订阅。
- 在子元素所在的位置读取一次 `props.children`：每次读取都会重新构建它们。

## 行为与外观的边界

`nsh/base` 以及建立在它之上的任何组件库都遵循的持久规则：

> 基础层负责可复用的行为以及该行为所需的结构。外观层负责产品的视觉语言。

“无样式”指基础层不拥有任何颜色、尺寸、圆角、字体、变体或文案，同时保留其行为所需的结构。导航、对话框、键盘、安全区域、
选择和文本文档在内部都需要结构和状态，把这些工作移给每个调用者只会复制脆弱的行为。

组件显式地暴露它的外观：

- 它自己的 `class` / `style` 设置主部件的样式，其他每个部件使用 `<part>Class` / `<part>Style`（`Dialog` 的
  `backdropClass`）；
- 状态使用 `<state>:` 类名和 `<state>Style` 对象（`active`、`hover`、`focus`、`disabled`、`selected`）；
- 插槽以它是什么来命名（`header`、`footer`、`trigger`、`content`、`empty`、`separator`）；
- 为组件拥有的值提供应用自己的内容时，使用渲染函数（`renderCodeBlock`；Rust 中是 `Renderer<T>` prop，
  [渲染函数](./rust.md#渲染函数与应用的类)）。

不要遍历任意后代去寻找标题、描述或关闭按钮：把部件显式化。文案总是属于应用：组件显示的每个标签都通过 prop 或子元素传入。

## 主题与样式

从主题读取颜色和数值，用 Tailwind 的刻度做布局（[Tailwind](./tailwind.md)、[主题 token](./tailwind.md#主题-token)）：

```rust tabs
view()
    .rounded(move || theme().radius_lg())
    .border_1()
    .border_color(move || theme().border())
    .bg(move || theme().background())
    .p_4()
    .child(text("Balance").text_color(move || theme().foreground()))
```

```tsx tabs
<View class="rounded-(--radius-lg) border border-border bg-background p-4">
  <Text class="text-foreground">Balance</Text>
</View>
```

- **不得在应用组件中硬编码产品颜色**：读取 token（`theme().primary()`、`bg-primary`），或把这个角色加进应用的主题文件。
  token 会原生地跟随外观模式和嵌入应用的主题。
- 使用间距和字号刻度（`p-4`、`gap-3`、`text-sm`；`.p_4()`、`.gap_3()`、`.text_sm()`），而不是任意的点数值；任意值留给
  测量得到的、物理的或平台的几何尺寸，并在旁边说明原因。
- 类名的含义与 Tailwind CSS v4 完全相同，CSS 值的含义与浏览器中相同。把 `unknown class` 警告当作需要修复的问题，而不是
  一个具有 NSH 特有含义的类名。
- 按压反馈属于应用：`active:` 类名、`activeStyle`，Rust 中的 `.active(|s| ..)`；`hover`、`focus`、`disabled` 和
  `selected` 同理。它们原生生效，不需要往返应用代码。
- 变体保持少而有语义（primary、secondary、danger），不要每个调用点一个；用语义 token 表达含义，而不是调色板中的位置。
- 文本跟随用户的字号设置：让文本容器可以增高，并用较大的字体缩放进行测试。
- 在 Rust 中，样式是类型化的方法；应用的 Tailwind 类名通过一个类名 prop 和 `.app_class(..)` 作用到组件的某个部件上。

## 事件、焦点与主线程

**主线程从不等待应用代码。** 当系统需要立即得到答案（这个页面能否弹出、这个面板能否关闭、这段文本能否修改）时，NSH 根据
事先挂载的 props 原生作答，事后再用事件告诉应用。所以决定是一个 prop，一旦知道就设置，而不是返回 true 或 false 的回调：

- `popOnBack={!dirty()}` 配合 `onBackPress` 进行确认，而不是否决返回操作（[popOnBack](./navigation.md#把用户留在当前页面poponback)）；
- `Dialog` 上的 `closeOnEscape`、`closeOnBackdropPress` 和 `closeOnSwipe`，配合 `onCancel` 获知用户的尝试；
- 输入框上的 `maxLength`、视图上的 `pointerEvents`、按钮上的 `disabled`。

```rust tabs
nav_page()
    .title("Edit")
    .pop_on_back(move || draft.with(String::is_empty))
    .on_back_press(move || {
        if !draft.with(String::is_empty) {
            confirm();
        }
    })
```

```tsx tabs
<NavPage title="Edit" popOnBack={!dirty()} onBackPress={() => dirty() && confirm()}>
  …
</NavPage>
```

一个命令只建模一次。按钮、`nsh/desktop` 的菜单项和键盘快捷键（`keymap.bind("mod+s", save)`）调用 `store.ts` 的同一个函数，这样
各个入口不会不一致；它们的标签和禁用状态从同一处派生。

激活使用按压：`onPress`、`onLongPress`、`onPressDown` 和 `onPressUp` 对手指、笔、鼠标和键盘含义相同。原始触摸处理函数和
手势留给跟随手指的操作（平移、拖动）。让焦点显式化：在事件中，或在跟随状态变化的 effect 中，用 ref 的 `focus()` 移动焦点，
不要在渲染时无条件地设置焦点，并给获得焦点的控件一个可见的 `focus:` 样式。

## 异步工作与副作用

从事件、effect 或 [`resource`](./reactivity.md#resource) 中启动异步工作，不要把它作为组件函数体无条件的副作用。effect
只跟踪同步读取的内容：先读取 signal，再启动工作。

- 用明确的状态表示加载：resource 的 `state()` 为 `unresolved`、`pending`、`ready`、`refreshing` 或 `errored`。刷新时
  显示之前的值。
- 结果可能在查询、页面或选择变化之后才到达。resource 会丢弃被取代的加载；你自己的工作要携带标识或版本号，过期时拒绝。
- 在界面中呈现可恢复的错误；日志不是给用户的反馈。
- 防止重复提交破坏性操作：请求进行期间禁用按钮。
- TSX 中的重活（解析大型数据、每次按键都对数千个条目排序）放进 `computed`、移出交互路径，或改用 Rust。

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

## 布局、测量与滚动

大多数界面使用布局，而不是自己测量（[布局](./layout.md)）。测量（`measure()`、`onLayout`）用于弹出层、浮层以及正确性依赖
于解析后几何尺寸的组件；它返回的结果只在宽度、字号或内容变化之前有效。

- 视图把子元素排成一列；`class="flex"` 是一行，与 CSS 相同。在方向重要的地方写明方向。
- 每个可滚动区域只有一个归属者。避免同一轴向上嵌套滚动视图；让滚动视图占据整个区域，把内容的内边距放在它里面，滚动指示器
  就会位于区域的边缘。
- 对齐靠构造而不是修正：相邻区域共用一个间距 token，而不是重复相同的数字；出现偏差时，追查重复的内边距、嵌套的缩进或边框
  归属，而不是用一个任意值去微调。
- 通过 `SafeAreaView`、`KeyboardAvoidingView` 及其访问器读取安全区域和键盘，它们原生地跟随旋转和键盘。

## 列表与大量数据

当数据可能超出一个小而有界的集合时，使用 [`List`](./lists.md)（Rust 中的 `virtual_list`）。它只为屏幕上的行创建视图并
复用它们，因此它的契约是：

- **必须把每个条目的状态放在以 id 为键的数据中**，而不是行内创建的 signal 中：单元格会移动到其他行。“已展开”“已选中”
  “已点赞”是一个 id 集合，或条目的一个字段。
- **必须在使用处、在绑定中读取条目**：在行函数开头复制出来的值属于这个单元格显示的第一行。
- 在 TSX 中以不可变的方式更新：新的数组，复用未变的条目，变化的条目是新对象。
- 给一个接近典型行高的 `estimatedItemSize`。
- 把源数据、筛选与排序、选择、滚动位置和行渲染分开，让更新保持局部，视图树永远不会变成数据模型。

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

## 平台与能力的边界

一个包在每个平台上运行（[平台特定代码](./platform-specific.md)）。大多数代码不需要平台判断；当某件事属于某个平台时，把它
放在 NSH 为它准备的位置：

- 两个平台都有的 prop 是顶层 prop；只有一个平台有的 prop 放进 `ios={{…}}`、`android={{…}}`、`desktop={{…}}` 或
  `web={{…}}`（Rust 中是构建器方法），只有当前平台的对象会生效；
- 某个平台独有的 API 来自 `nsh/ios`、`nsh/android` 和 `nsh/desktop`；
- 按平台设置的样式使用 `ios:`、`android:` 和 `web:` 类名。

即使外观不同，平台分支也保持语义契约：应用仍然知道对话框是否打开、请求是否发出。平台判断留给用户依赖的平台惯例（Android
的返回键、iOS 的面板、Android 的水波纹）。

应用在 `nsh.json` 的 `"capabilities"` 中声明它在 VM 之外能访问的内容：可以请求的主机、Web 视图显示的页面、剪贴板、状态栏
（[沙箱与权限](./sandbox-and-capabilities.md)）。将被下发到其他应用中的应用，准确列出它用到的内容，并像处理失败的请求那样
处理被拒绝的请求：宿主应用授予的权限可能少于应用声明的。文件和进程是标准模块（`fs/promises`、`process`），受同样的检查。

## 兼容性：作为插件下发的应用

NSH 应用以签名包的形式下发到宿主应用中，可能再也不会重新构建：NSH 之后的每个版本都原样运行它们。对应用或组件库暴露给他人
的内容，采用同样的纪律：组件的 props 和事件、Rust crate 的导出、宿主应用的原生组件和模块接收的函数。

- 输入只增加可选成员；类型只放宽；默认值和行为保持不变（新行为需要主动开启）。
- 选项是一个对象（`scrollTo({ x, y, animated })`），而不是第三个位置参数。
- 模式是可以增加取值的字符串联合类型，而不是以后需要第三种状态的布尔值。
- 一个值只有一种类型：`version: string`，而不是 `string | number`。
- 函数返回一个可以增加字段的对象，而不是元组或单独的值。
- 重命名时保留旧名称，作为同一实现的 `@deprecated` 别名。

在 Rust 中，库的公开结构体带 `#[non_exhaustive]` 并实现 `Default`，这样增加字段不会破坏调用者。

## 公开 API 设计

对于可复用组件：

- props 建立有效的默认值：Rust 中用 `#[prop(default)]` 或 `#[prop(default = …)]`，TSX 中在读取 prop 的地方用 `??`；
- 每个 prop、事件和导出都有文档注释；在 Rust 中，它会成为 TSX 看到的 TypeScript 声明；
- 回调用应用的语言描述请求的变化或已完成的操作；只有当指针细节对应用有意义时才传递它们；
- 显式的部件、插槽和渲染函数优于遍历子元素；
- 增加行为从不强迫应用做出视觉选择。

### 词汇是 API 的一部分

在各个组件中，同一概念使用同一个词。在给 prop 或事件命名之前，先在 NSH 自己的 API 中寻找这个术语，然后是 GPUI Kit 的
`gpui-base`、GPUI 和 `gpui-component`，再然后是各平台的控件。原语所显示的内容和全局 API 的名称取自 HTML、DOM 和 CSS
（`src`、`html`、`placeholder`）。Rust 名称是 `snake_case`，在 TSX 中转换为 `camelCase`（`on_open_change` 即
`onOpenChange`）；一个词保持一个词（`readonly`、`multiline`）。

| 概念 | 命名模式 | 示例 |
| --- | --- | --- |
| 组件 | 名词，`PascalCase` | `Button`、`Dialog`、`Switch` |
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
| 值的变化 | `on<Value>Change(next, details?)` | `onChange(text)`、`onOpenChange(open, reason)` |
| 其他事件 | `on<Event>`，现在时，一个对象 | `onPress()`、`onSubmit({ text })` |
| 阶段 | `<act>Start`、`<act>End` | `onPanStart`、`onLoadStart` |
| 指针位于视图上方 | `In` / `Out` | `onHoverIn`、`onHoverOut` |
| 开始监听 | 返回 `() => void` | `keymap.bind(..)` |
| 环境中的值 | 访问器，旁边是 `set<Value>` | `colorScheme()`、`setColorScheme` |
| 只读的布尔值 | `is<Adjective>`、`has<Noun>` | `isAvailable` |

props 从不使用 `is<Adjective>`：这种形式只用于只读的布尔值（常量、事件数据的字段）。显示的部件以部件本身命名，从不用
`<part>Shown`、`<part>Hidden` 或 `shows<Part>`。

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

### 回调与事件的措辞

- 值的变化先传值，其他内容放在后面：`onChange(next)`、`onOpenChange(open, reason)`。其他每个事件都传一个对象，即使只有一个
  字段（`onLinkPress({ url })`），这样以后增加字段只是一次添加；没有内容可报告的事件不传参数。
- 以动作命名，不用过去时，也不用请求：`onPress`、`onClose`、`onCancel`、`onBackPress`。
- 当动作有名字时，以意图而不是几何位置命名：`onLoadMore`，而不是“到达了末尾”。
- 生命周期成对使用 `onOpen` / `onClose`；阶段用 `Start` / `End`，从不用 `Begin`；指针位于视图上方是 `onHoverIn` /
  `onHoverOut`，文件拖过视图是 `Enter` / `Leave`。
- 在文档中说明回调是在组件自身状态变化之前还是之后运行，以及它是否可能重入组件。

### 让外层名称承载上下文

名称总是在某个上下文中被阅读：prop 在它的组件中，字段在它的类型中。两者都不重复外层已经说明的内容（`contentStyle`，而不是
`contentContainerStyle`；`Dialog` 的 `backdropClass`，而不是 `dialogBackdropClass`）。同一类型的字段保持相同的缩写程度。

### 类型与模块风格

- Rust 类型用 `UpperCamelCase`；模块、函数、字段和局部变量用 `snake_case`；常量用 `SCREAMING_SNAKE_CASE`。上下文绑定
  命名为 `cx`。
- TSX 组件及其标签用 `PascalCase`；props、函数和局部变量用 `camelCase`。原语从 `nsh` 导入，基础组件从 `nsh/base` 导入。
- 以产品概念命名组件（`ProjectList`、`ProjectEditor`），以意图命名处理函数（`confirmDelete`、`openProject`）。
- 一起变化的组件模块、props、事件和测试放在一起。

### 文档与文案

公开文档以组件做什么、谁拥有它的状态开头。示例使用当前的 API 和稳定的 id，并说明默认值、平台差异、焦点行为和回调顺序。标签、
命令和确认遵循设计指南，每个领域对象、命令和状态只使用一个术语。

翻译键描述稳定的意图（`dialog.deleteProject.title`），而不是原文句子或屏幕上的位置。不要用翻译后的片段拼接句子；让每种语言
掌握自己的语序、复数和标点，并在组件中用真实的内容和较大的字号检查文案。

## 测试策略

在能证明行为的最低层进行测试（[测试](./testing-and-trace.md)）：

1. 普通测试，用于状态转换、格式化、解析和排序（对 crate 中函数的 `cargo test`，或针对 TSX 模块中函数的测试文件）；
2. 无界面的应用测试：真实的引擎、渲染器、布局和列表，以 60 Hz 的显示帧运行，执行用户的操作（`press`、`type`、`scroll`、
   `drag`）并检查屏幕上的内容，使用 `nsh test`（`nsh/test`）或 `cargo test`（`nsh_testing::Headless`）；
3. 在真实平台上运行同样的测试（`nsh test --target macos`、`ios`、`android`、`web` 等），这时的操作是平台自己的输入，
   `app.screenshot` 记录平台绘制的结果；
4. `nsh test --trace` 用于了解一次交互的开销：每次提交的挂载项和耗时。

```rust tabs
#[test]
fn a_tap_counts() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<my_app::App>())).unwrap();
    app.wait_for("Count: 0").unwrap();
    app.press("Tap").unwrap();
    app.wait_for("Count: 1").unwrap();
}
```

```tsx tabs
import { expect, open, test } from "nsh/test";

test("a tap counts", async () => {
  const app = await open("./App.tsx");
  await app.press("role:button[Tap]");
  expect(app.texts()).toContain("Count: 1");
});
```

对于交互组件，覆盖它的契约而不是内部实现：激活、受控值的变化、禁用行为、焦点、事件的数量和顺序、稳定的标识，以及空、加载
和失败状态。像用户那样查找视图：按角色和名称、标签或文本（`role:button[Save]`、`label:Close`），在没有其他名称时使用稳定的
`testID`。模拟网络（`network` 路由）并控制时间（手动时钟），而不是等待真实的网络和时间。只要失败能够复现，就在修复缺陷之前
先加一个回归测试。

## 性能规则

- 在 TSX 中保持处理函数和 computed 值轻量；把重活放进 `computed`、移出交互路径，或改用 Rust。
- 在绑定中读取 signal，这样一次变化只更新一个 prop 或一个子元素。
- 不要在读取某个 signal 的绑定或 effect 中无条件地写入它。
- 长集合使用 `List`，每个条目的状态放在数据中。
- 用在 UI 线程上运行的动画值做动画，而不是每帧写入 signal；用 `scrollValue` 跟随滚动，而不是滚动监听
  （[动画](./animation.md)）。
- 避免为了满足闭包而克隆大的字符串或集合；共享它们（`Rc`、`SharedString`），或通过 signal 读取。
- 加缓存前先用 `nsh test --trace` 测量，并让每个缓存都有一个负责使其失效的归属者。
- 遵循减少动态效果的设置（`reduceMotion()`）。

## 常见的失败模式

避免这些模式：

- 在组件函数体中读取 prop 或 signal，却期望它会更新；
- 一个 signal 容纳应用互不相关的状态；
- 在很长的组件函数体中发请求、写业务逻辑；
- 用索引或随机值作可重排行的 key；
- 每行的状态放在 `List` 行内创建的 signal 中；
- 字面量颜色和圆角，破坏应用的主题和深色模式；
- 在 `Button` 已经提供角色、状态及其无障碍支持的地方使用带 `onPress` 的 `View`；
- 与受控值逐渐偏离的本地状态；
- 把一个 signal 复制到另一个 signal 的 effect；
- 系统需要立即回答的决定交给回调，而不是事先设置的 prop；
- 同一轴向上没有明确归属者的嵌套滚动视图；
- 为一次性的页面新增变体；
- 对可撤销、低风险的操作弹出确认对话框；
- 调用模块内部实现，却从不按压、输入或滚动的测试。

## 给编码智能体的规则

编辑之前，智能体要阅读最近的实现、它的测试、模块的公开边界以及相关的参考页面，并在代码和 `.nsh/types` 中核对签名，而不是
照着 React、React Native 或 Web 的示例类推。NSH 的 TSX 遵循 Solid 的模型：组件只运行一次，绑定跟随它们读取的 signal。

对于每次修改，智能体都能说出：

1. 行为的归属者和外观的归属者；
2. 状态的归属者和生命周期，以及赋予各行标识的 key；
3. 按压、键盘、焦点和无障碍的契约；
4. 布局和滚动的归属者；
5. 使用的主题 token，以及每个有意的例外；
6. 行为退化时会失败的那个测试。

生成的代码要由人审查和测试。“能构建”不是界面质量的标准；只让生成的代码看起来整齐的大范围重构，也不能代替与应用架构保持
一致。

## 实现检查清单

提交审查之前，确认：

- 状态、副作用及其生命周期都有明确的归属者；
- props 和 signal 在使用处读取；
- 组件是受控的：应用拥有每个值，组件报告变化；
- 重复的元素有稳定的、基于领域的 key 和测试 id；
- 用主题 token 和 Tailwind 刻度替代零散的视觉字面量；
- 系统需要立即回答的决定是事先设置的 prop；
- 按压、键盘、焦点、禁用状态和对话框能协同工作；
- 加载、空、错误和取消路径都有体现；
- 长集合使用 `List`，每个条目的状态放在数据中；
- 平台专属的代码位于平台 props 和平台模块中，权限列出了应用访问的内容；
- 新增的公开 API 只做放宽，并保持各层的依赖方向；
- 测试在合适的层证明行为，既在无界面环境中，也在修改所涉及的平台上。

搭建应用请参阅[快速开始](./getting-started.md)，每个组件和模块请参阅[元素](../elements/index.md)和
[API](../modules/index.md) 参考。
