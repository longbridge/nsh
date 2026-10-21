---
title: nsh/base
description: 应用界面的无样式基础组件：按钮、导航栈与对话框、安全区、系统警告对话框、软键盘，以及页面和对话框的状态栏。
order: 2.5
demo: ui-navigation
---

# nsh/base

`nsh/base` 是应用界面的基础组件层：不带样式、负责行为、状态、焦点和无障碍的组件，各平台通用。它位于基础元素（`nsh` 和各个 `nsh/<module>` 模块）与应用自己的组件库之间，就像 GPUI Kit 中 `gpui-base` 位于 `gpui` 与带样式的组件库之间（见[模块](./index.md#three-layers)）。它提供控件（`Button`）、导航（由 `createNavStack` 创建的状态驱动、由多个 `NavPage` 组成的 `NavStack`，以及在所有内容之上展示内容的 `Dialog`）、安全区（`SafeAreaView`、`safeArea()` 和 `safeAreaInsets()`）、系统警告对话框（`AlertDialog`）、软键盘（`Keyboard`、`keyboardFrame()` 和 `KeyboardAvoidingView`）。页面和对话框还决定状态栏（`statusBar`）。

外观由应用决定：页面、对话框或标题栏里的内容都是你自己的 JSX，用基础元素配合样式、[Tailwind](../docs/tailwind.md) 类和主题绘制；或者是[你自己的 crate](../docs/rust.md) 中的 Rust 组件；也可以是社区基于 `nsh/base` 构建的组件库。每个宿主都提供 `nsh/base`；Rust 应用通过 `nsh-base` crate 使用同一套 API（`nsh_base::{SafeAreaView, …}`）。

```tsx
import {
  AlertDialog, Button, createNavStack, Dialog, Keyboard, keyboardFrame, KeyboardAvoidingView, NavPage, NavStack,
  SafeAreaView, safeArea, safeAreaInsets,
} from "nsh/base";
import type {
  AlertDialogAction, AlertDialogOptions, AlertDialogPresentation, AlertDialogRole, AvoidBehavior, ButtonProps,
  DialogChangeReason, DialogProps, Edge, HeaderStyle, Insets, NavPageProps, NavStackProps, NavStackState, StatusBar,
  StatusBarForeground,
} from "nsh/base";
```

本页是 API 参考。完整的上手讲解见[导航指南](../docs/navigation.md)和[表单与键盘](../docs/forms-and-keyboard.md)。

<a id="controls"></a>

## 控件

<a id="button"></a>

### Button

一个只负责行为、没有外观的按压目标：处理按压、禁用和选中状态，并向屏幕阅读器报告它的角色。它没有自己的颜色、尺寸或文字。用 `class` 或 `style` 设置样式，用 `active:`、`hover:`、`focus:`、`selected:`、`disabled:` 变体（或 `activeStyle`、`hoverStyle`、`focusStyle`、`selectedStyle`、`disabledStyle`）设置各状态的样式，内容放在它里面。这样同一个 `Button` 可以做成你的主按钮、图标按钮、标签页或列表的一行。

```tsx
import { mount, signal, Text } from "nsh";
import { Button } from "nsh/base";

function App() {
  const count = signal(0);
  return (
    <Button
      onPress={() => count.update((n) => n + 1)}
      class="self-center px-5 py-3 rounded-xl bg-blue-600 active:bg-blue-700 disabled:opacity-40"
    >
      <Text class="text-white text-base font-semibold">{`Pressed ${count()}`}</Text>
    </Button>
  );
}

mount(App);
```

| 属性 | 类型 | |
|---|---|---|
| `disabled` | `boolean` | 不响应按压，也没有按压反馈；`disabled:` 样式生效，屏幕阅读器会报告它不可用。 |
| `selected` | `boolean` | 选中状态（切换中的触发器、当前标签页），屏幕阅读器会报告。值由应用持有，样式也由应用决定。 |
| `selectedStyle` | `StyleProp<ViewStyle>` | `selected`（且未禁用）时的样式，与 `selected:` 类相同。 |
| `accessibilityLabel` | `string` | 内容不足以说明时（比如只有图标），屏幕阅读器读出的文字。 |
| `role` | `AccessibilityRole` | 它对屏幕阅读器而言是什么（默认 `"button"`）：`"link"`、`"tab"`、`"menuitem"` 等。 |
| `onPress` | `() => void` | 被按下。 |
| `onPressDown`、`onPressUp` | `() => void` | 触摸在它上面按下，以及结束或移开：用来做你自己的按压反馈（弹簧缩放、高亮）。 |
| `class`、`style` | | 它的全部外观。 |
| `activeStyle`、`hoverStyle`、`focusStyle`、`disabledStyle` | `StyleProp<ViewStyle>` | 按下、悬停、获得焦点或禁用时的样式，与对应的 `<state>:` 类相同。后面的状态优先，`disabled` 优先于所有状态。 |

一排标签页就是每个标签一个 `role="tab"` 的 `Button`，当前标签设置 `selected`，外观写在 `selected:` 类里：

```tsx
<Button role="tab" selected={tab() === "inbox"} onPress={() => tab.set("inbox")}
  class="rounded-full px-4 py-2 selected:bg-blue-600 active:opacity-70">
  <Text class="text-base">Inbox</Text>
</Button>
```

<a id="navigation"></a>

## 导航

导航栈在 iOS 上是 `UINavigationController`，带系统标题栏和侧滑返回；在 Android 上是带工具栏和预测性返回的页面栈。在桌面平台（macOS、Windows、Linux、GPUI）和 Web 上，每个页面在顶部显示应用自己的 `header`，占满整个宽度，内容位于其下方；返回键（Escape、Alt+左方向键、鼠标的后退键）和浏览器的后退按钮会弹出页面。

页面的内容和桌面标题栏都是你自己的 JSX。这里用一个小小的 `Action` 组件绘制按钮：

```tsx
import { mount, Text, View } from "nsh";
import { Button, createNavStack, NavPage, NavStack } from "nsh/base";

type Route = { name: "home" } | { name: "detail"; id: number };

const nav = createNavStack<Route>([{ name: "home" }]);

function Action(props: { title: string; onPress: () => void }) {
  return (
    <Button onPress={props.onPress} class="rounded-lg bg-blue-600 px-4 py-3 active:bg-blue-700">
      <Text class="text-center text-base font-semibold text-white">{props.title}</Text>
    </Button>
  );
}

function Bar(props: { title: string }) {
  return (
    <View class="flex-row items-center gap-3 border-b border-neutral-200 px-4 py-3">
      {nav.routes().length > 1 && <Action title="Back" onPress={() => nav.pop()} />}
      <Text class="text-lg font-semibold">{props.title}</Text>
    </View>
  );
}

function Home() {
  return (
    <NavPage title="Inbox" header={<Bar title="Inbox" />}>
      <View style={{ flex: 1, padding: 16 }}>
        <Action title="Open message 1" onPress={() => nav.push({ name: "detail", id: 1 })} />
      </View>
    </NavPage>
  );
}

function Detail(props: { id: number }) {
  const title = `Message ${props.id}`;
  return (
    <NavPage title={title} header={<Bar title={title} />}>
      <View style={{ flex: 1, padding: 16 }}>
        <Text>The message.</Text>
      </View>
    </NavPage>
  );
}

function App() {
  return (
    <NavStack state={nav} style={{ flex: 1 }}>
      {(route) => (route.name === "home" ? <Home /> : <Detail id={route.id} />)}
    </NavStack>
  );
}

mount(App);
```

在 iOS 和 Android 上，页面显示带 `title` 和返回按钮的系统标题栏；在桌面平台和 Web 上，显示 `header`。只面向手机的应用可以不写 `header`；只面向桌面的应用可以不写系统标题栏的选项。

<a id="how-a-stack-works"></a>

### 栈的工作方式

- 栈的状态保存**路由**：由你自行决定的普通值，从栈底到栈顶排列。路由通常是一个对象，标明页面并携带参数。
- `NavStack` 为每次 push 调用一次子函数，并把结果显示为一个页面。对这个页面它不会再次调用该函数，因此页面在栈中期间（包括被其他页面覆盖时）会保留状态：signal、滚动位置、输入内容。
- 每次 push 都是一个独立的页面，同一个路由值 push 两次也是如此。
- 当用户以原生方式离开页面（返回按钮、侧滑返回、Android 返回键、桌面平台的返回键、浏览器的后退）时，栈会从状态中移除对应路由，因此 `nav.routes()` 始终与用户看到的一致。
- 如果子函数返回的不是 `<NavPage>`，栈会用一个默认选项的页面把它包起来。要设置标题栏和其他选项，请返回 `<NavPage>`。

### createNavStack

```ts
function createNavStack<R>(routes: R[]): NavStackState<R>
```

创建一个栈的状态，栈从 `routes` 开始（从栈底到栈顶）。只需创建一次（例如在模块顶层），然后传给 `<NavStack state>`。任何代码都可以导入它来进行导航。

| 成员 | 说明 |
|---|---|
| `routes: Accessor<readonly R[]>` | 路由，从栈底到栈顶。响应式。 |
| `push(route: R): void` | 压入一个页面。 |
| `pop(count?: number): void` | 弹出 `count` 个页面（默认 1）。至少保留一个页面。 |
| `popToRoot(): void` | 返回到第一个页面。 |
| `replace(route: R): void` | 替换栈顶页面。 |
| `setRoutes(routes: R[]): void` | 替换页面；保留下来的路由（同一个值）保留它的页面和状态。 |
| `remove(route: R): void` | 移除 `route`（用 `===` 比较）最上面的那个页面，无论它在栈中的什么位置。 |

```tsx
nav.push({ name: "detail", id: 7 });
nav.pop(2);
nav.setRoutes([{ name: "home" }, { name: "detail", id: 1 }]);
const depth = () => nav.routes().length;
```

### NavStack

```tsx
<NavStack state={nav} style={{ flex: 1 }}>
  {(route, index) => …}
</NavStack>
```

| 属性 | 类型 | 说明 |
|---|---|---|
| `state` | `NavStackState<R>` | 保存路由的状态（`createNavStack`）。必填。 |
| `children` | `(route: R, index: Accessor<number>) => JSX.Element` | 渲染一个路由。返回 `<NavPage>` 以设置其选项。必填。 |
| `style`、`class` | | 栈的样式，通常为 `flex: 1`。 |

子函数接收路由值本身，以及它在栈中位置的访问器。

### NavPage

`NavPage` 是栈中的一个页面。它包含页面的内容、它在桌面平台和 Web 上的 `header`，以及 iOS 和 Android 上系统标题栏的选项：

```tsx
<NavPage
  title="Profile"
  header={<ProfileBar />}
  headerStyle={{ backgroundColor: "#1a7f37", color: "#ffffff", tintColor: "#ffffff" }}
  headerRight={<Text onPress={edit} style={{ color: "#ffffff", fontSize: 17 }}>Edit</Text>}
>
  <Scroll style={{ flex: 1 }}>…</Scroll>
</NavPage>
```

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `header` | `JSX.Element` | 无 | 桌面平台与 Web：页面的标题栏，放在页面顶部，宽度与页面相同，高度由其布局决定，内容位于其下方。 |
| `title` | `string` | 无 | iOS 与 Android：系统标题栏的标题。 |
| `headerShown` | `boolean` | `true` | iOS 与 Android：显示系统标题栏。 |
| `headerTransparent` | `boolean` | `false` | iOS 与 Android：内容延伸到透明的系统标题栏下方。 |
| `headerLargeTitle` | `boolean` | `false` | iOS：随内容滚动收缩进栏内的大标题；内容在标题栏下方滚动。 |
| `headerBackTitle` | `string` | 无 | iOS 与 Android：返回到本页面的返回按钮文字（在 Android 上是它对屏幕阅读器的描述）。 |
| `headerBackVisible` | `boolean` | `true` | iOS 与 Android：显示返回按钮。 |
| `headerLeft` | `JSX.Element` | 无 | iOS 与 Android：系统标题栏起始端的内容，替代返回按钮。 |
| `headerRight` | `JSX.Element` | 无 | iOS 与 Android：系统标题栏末端的内容。 |
| `headerTitle` | `JSX.Element` | 无 | iOS 与 Android：替代标题的内容。 |
| `headerStyle` | `{ backgroundColor?, color?, tintColor? }` | 平台默认 | iOS 与 Android：系统标题栏的颜色：背景、标题（`color`）和按钮（`tintColor`）。 |
| `headerClass` | `string` | 无 | 以类设置同样的颜色：`bg-*`，标题用 `text-*`，按钮用 `tint-*`。`headerStyle` 优先于它们。 |
| `popOnBack` | `boolean` | `true` | 返回按钮和侧滑返回会弹出页面。见[离开页面](#leaving-a-page)。 |
| `popOnSwipe` | `boolean` | `true` | 交互式返回手势会弹出页面：iOS 侧滑返回、Android 跟随手指的预测性返回。返回按钮无论如何都会弹出页面。 |
| `animation` | `"default" \| "slide" \| "fade" \| "none"` | `"default"` | 页面进入和离开的方式。 |
| `statusBar` | `{ foreground?, hidden?, animated? }` | 无 | iOS 和 Android：页面在最上层时的状态栏。见[状态栏](#status-bar)。 |
| `style`、`class` | | 无 | 页面样式。 |
| `children` | `JSX.Element` | 无 | 内容。 |
| `onBackPress` | `() => void` | 无 | 用户在该页面按了返回（标题栏的返回按钮、Android 返回键、桌面平台上的 Escape、Alt+左方向键或鼠标后退键、浏览器的后退），无论 `popOnBack` 是否让它弹出。 |
| `onDismiss` | `() => void` | 无 | 用户弹出了该页面（返回按钮、侧滑），其路由已被移除。 |

标题栏项（`header`、`headerLeft`、`headerRight`、`headerTitle`）就是普通的 JSX：它们可以读取 signal、处理按压，并在页面显示期间更新。

页面内容从标题栏下方开始：在 iOS 和 Android 上是不透明的系统标题栏下方，在桌面平台和 Web 上是 `header` 下方。在 iOS 和 Android 上设置 `headerTransparent`、`headerLargeTitle` 或 `headerShown={false}` 时，内容从页面顶部开始；可以用 [SafeAreaView](#safeareaview) 或 `safeAreaInsets()` 避开标题栏和状态栏。

```tsx
<NavPage title="Large title" headerLargeTitle>
  <Scroll style={{ flex: 1 }}>…</Scroll>
</NavPage>
```

<a id="leaving-a-page"></a>

#### 离开页面

`popOnBack={false}` 让用户停留在当前页面，例如表单中有未保存的修改时。它为 false 时，侧滑返回被关闭，返回按钮和返回键只调用 `onBackPress` 而不弹出页面。询问用户后，再由你自己弹出：

```tsx
import { Input, signal } from "nsh";
import { AlertDialog, NavPage } from "nsh/base";

function EditNote() {
  const text = signal("");
  const confirm = async () => {
    const choice = await AlertDialog.show({
      title: "Discard your changes?",
      actions: [
        { label: "Keep editing", role: "cancel" },
        { label: "Discard", role: "destructive", id: "discard" },
      ],
    });
    if (choice === "discard") nav.pop();
  };
  return (
    <NavPage title="Edit" popOnBack={text().length === 0} onBackPress={() => text().length > 0 && confirm()}>
      <Input value={text()} onChange={(v) => text.set(v)} placeholder="A note" />
    </NavPage>
  );
}
```

> [!IMPORTANT]
> 系统会在用户操作的那一刻，在主线程上询问是否允许返回，而主线程从不等待 JS。因此 NSH 依据已经挂载的 `popOnBack` 值作答。一旦有可能丢失的内容就设置 `popOnBack={false}`，赶在用户返回之前。

每次按返回都会调用 `onBackPress`，页面弹出时也一样，因此它也能告诉你用户返回了。如果只想关闭侧滑手势而保留返回按钮，使用 `popOnSwipe={false}`。

### Dialog

`Dialog` 在所有内容之上（包括栈的标题栏）展示内容，位于它自己的原生窗口层中。它没有自己的外观：`class` 和 `style` 设置弹出层（容纳其子元素的视图）的样式，`backdropClass` 或 `backdropStyle` 在它后面加一个背景层，按压背景层会关闭对话框。`open` 为 true（默认）时显示。用户可以用 Escape、Android 返回键、按压背景层或在 iOS sheet 上向下轻扫来关闭它；随后会调用 `onOpenChange(false, reason)`，由你把 `open` 设为 false。

```tsx
import { signal, Text, View } from "nsh";
import { Button, Dialog } from "nsh/base";

function Settings() {
  const open = signal(false);
  return (
    <View>
      <Button onPress={() => open.set(true)} class="self-start active:opacity-60">
        <Text class="text-blue-600">About</Text>
      </Button>
      <Dialog
        open={open()}
        onOpenChange={(next) => open.set(next)}
        animation="fade"
        backdropClass="bg-black/40"
        class="m-auto w-80 gap-3 rounded-2xl bg-white p-5"
      >
        <Text class="text-xl font-bold">About</Text>
        <Button onPress={() => open.set(false)} class="self-end active:opacity-60">
          <Text class="font-semibold text-blue-600">Close</Text>
        </Button>
      </Dialog>
    </View>
  );
}
```

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `open` | `boolean` | `true` | 为 true 时显示。 |
| `animation` | `"slide" \| "fade" \| "none"` | `"slide"` | 出现和消失的方式。 |
| `presentation` | `"fullScreen" \| "pageSheet" \| "formSheet"` | `"fullScreen"` | iOS：`pageSheet` 和 `formSheet` 让后面的内容保持可见。 |
| `closeOnEscape` | `boolean` | `true` | Escape 和 Android 返回键会关闭它。 |
| `closeOnBackdropPress` | `boolean` | `true` | 按压背景层会关闭它。 |
| `closeOnSwipe` | `boolean` | `true` | 向下轻扫 sheet（iOS、手机浏览器）会关闭它。 |
| `statusBar` | `{ foreground?, hidden?, animated? }` | 无 | iOS 和 Android：对话框打开期间的状态栏。见[状态栏](#status-bar)。 |
| `class`、`style` | | 无 | 弹出层的样式。 |
| `backdropClass`、`backdropStyle` | | 无 | 设置任一项时，背景层铺满对话框，位于弹出层后面。 |
| `ios` | `{ supportedOrientations?: (…)[] }` | 无 | iOS：允许的屏幕方向（默认沿用呈现者的）。 |
| `android` | `{ statusBarTranslucent?: boolean }` | 无 | Android：内容延伸到状态栏下方（默认从状态栏下方开始）。 |
| `onOpenChange` | `(open: boolean, reason: "cancel" \| "backdropPress" \| "swipe") => void` | 无 | 用户关闭了它。把 `open` 设为 false。 |
| `onCancel` | `() => void` | 无 | 用户试图关闭它，无论 `closeOn*` 是否允许关闭。 |
| `onOpen` | `() => void` | 无 | 它已完成出现。 |
| `onClose` | `() => void` | 无 | 它消失了：用户关闭了它，或应用移除了它。 |

对话框会向其内容提供自己的 inset，因此其中的 [SafeAreaView](#safeareaview) 和 `safeAreaInsets()` 使用对话框的 inset。在桌面平台和 Web 上，对话框是覆盖整个窗口的透明层，显示的只有你设置了样式的弹出层和背景层。关于 sheet、全屏流程以及关闭前先询问的对话框，见 [Dialog](../elements/dialog.md)。

<a id="safe-areas"></a>

## 安全区

### SafeAreaView

`SafeAreaView` 通过添加内边距，让内容避开刘海、状态栏、Home 指示条、导航标题栏或 Mac Catalyst 的窗口标题栏。内边距取的是**视图所在位置**的 inset：在 [NavPage](#navpage) 内，是该页面标题栏下方的 inset；在 [Dialog](#dialog) 内，是对话框的 inset；其他情况下，是整个 surface 的安全区域。

```tsx
import { mount, Text } from "nsh";
import { SafeAreaView } from "nsh/base";

function App() {
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: "#fff8c5", padding: 16 }}>
      <Text style={{ fontSize: 24, fontWeight: "700" }}>Clear of the notch</Text>
    </SafeAreaView>
  );
}

mount(App);
```

inset 会**叠加**在视图自身的内边距上。在上面的例子里，内容从顶部 inset 再往下 16 pt 处开始，而背景色铺满整个区域，包括状态栏后面的部分。

默认四条边都会加内边距。传入 `edges` 可以只处理其中几条，比如底部 inset 已经由标签栏或你自己的底栏处理时：

```tsx
<SafeAreaView edges={["top", "left", "right"]} style={{ flex: 1 }}>
  …
</SafeAreaView>
```

标题栏可见且不透明的 [NavPage](#navpage) 已经把内容放在标题栏下方，所以那里的顶部 inset 为 0。SafeAreaView 用于：

- 没有导航栈的应用的根视图；
- 设置了 `headerShown={false}` 或 `headerTransparent` 的页面；
- 全屏 [Dialog](#dialog) 的内容。

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `edges` | `Edge[] \| null`（`"top" \| "right" \| "bottom" \| "left"`） | 全部四条边 | 要加内边距的边。 |
| `children` | `JSX.Element` | 无 | 内容。 |
| `class`、`classList`、`style`、`testID`、`onLayout`、`ref` | | | 作用于该视图。其内边距会与 inset 相加。 |

<a id="safearea-and-safeareainsets"></a>

### safeArea 与 safeAreaInsets

需要具体数值时（比如自己给底栏加内边距），使用下面两个 accessor 之一。它们都返回 `Insets` 对象（`{ top, right, bottom, left }`，单位 pt），并且是响应式的：读取它们的代码会在值变化时（例如旋转屏幕）重新运行。

| 导出 | 返回值 |
|---|---|
| `safeArea()` | surface 的安全区域：避开系统 UI（刘海、Home 指示条、标题栏）的部分。 |
| `safeAreaInsets()` | 在组件中调用。返回该组件所在位置的 inset 的 accessor：最近的页面（标题栏下方）的、最近的对话框的，否则为 surface 的安全区域。与 SafeAreaView 使用的 inset 相同。 |

```tsx
import { Text, View } from "nsh";
import { safeAreaInsets } from "nsh/base";

function BottomBar() {
  const insets = safeAreaInsets();
  return (
    <View style={{ flexDirection: "row", padding: 8, paddingBottom: 8 + insets().bottom }}>
      <Text>Bottom bar</Text>
    </View>
  );
}
```

<a id="navigation-and-safe-areas-on-each-platform"></a>

### 各平台上的导航与安全区

- **iOS**：使用 `UINavigationController`，带系统标题栏、返回按钮和侧滑返回；`headerLargeTitle` 就是 UIKit 的大标题。在转场进行中到达的栈变更会在转场结束后应用。inset 来自 UIKit 的安全区域（状态栏、刘海或灵动岛、Home 指示条）。
- **Android**：带工具栏的页面栈。系统返回键会弹出栈顶页面；在支持的 Android 版本上使用预测性返回。向上箭头对屏幕阅读器的描述是下层页面的 `headerBackTitle`，或系统的描述。如需自行处理返回键，见 [nsh/android](./nsh-android.md) 中的 `BackHandler`。NSH 报告系统栏和屏幕挖孔中与 NSH 视图重叠的部分；当 NSH [嵌入](../docs/embedding-android.md)在原生工具栏下方时，视图的顶部 inset 为 0。
- **Mac Catalyst**：与 iOS 相同。page sheet 和 form sheet 以与窗口同尺寸的 macOS sheet 形式呈现。顶部 inset 为窗口的标题栏。
- **桌面平台（macOS、Windows、Linux、GPUI）与 Web**：每个页面在顶部显示其 `header`，内容位于其下方。在桌面平台上用 Escape、Alt+左方向键和鼠标后退键返回，在 Web 上用浏览器的后退按钮。对话框是覆盖整个窗口的透明层。

<a id="alert-dialogs"></a>

## 警告对话框

`AlertDialog` 显示平台自带的警告框或操作表：在 iOS 上是 `UIAlertController`，在 Android 上是 `AlertDialog`（操作表则是列表对话框）。它返回一个 Promise，解析为用户所选的操作。

### AlertDialog

```ts
AlertDialog.show(options: AlertDialogOptions): Promise<string | null>
```

Promise 解析为所选操作的 `id`（没有 `id` 时为其 `label`）；如果对话框没有通过操作而被关闭（在 `closeOnBackdropPress` 和 `closeOnEscape` 允许时点击外部、按 Escape 或返回键），则解析为 `null`。`actions` 为空时 Promise 被拒绝。

```tsx
import { mount, signal, Text, View } from "nsh";
import { AlertDialog, Button } from "nsh/base";

function App() {
  const answer = signal("–");
  const confirm = async () => {
    const choice = await AlertDialog.show({
      title: "Delete the photo?",
      description: "This cannot be undone.",
      actions: [
        { label: "Cancel", role: "cancel" },
        { label: "Delete", role: "destructive", id: "delete" },
      ],
    });
    answer.set(choice === "delete" ? "deleted" : "kept");
  };
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48, gap: 12 }}>
      <Button onPress={confirm} class="rounded-lg bg-red-600 px-4 py-3 active:bg-red-700">
        <Text class="text-center font-semibold text-white">Delete…</Text>
      </Button>
      <Text>{answer()}</Text>
    </View>
  );
}

mount(App);
```

操作表使用同样的选项，加上 `presentation: "actionSheet"`：

```tsx
const choice = await AlertDialog.show({
  title: "Share the photo",
  presentation: "actionSheet",
  actions: [
    { label: "Copy link", id: "copy" },
    { label: "Save", id: "save" },
    { label: "Delete", role: "destructive", id: "delete" },
    { label: "Cancel", role: "cancel" },
  ],
});
```

所有文字都来自应用，因此警告框说的就是你的应用要说的话，用的是它的语言。

<a id="alertdialogoptions"></a>

#### AlertDialogOptions

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `title` | `string \| null` | 无 | 显示在描述上方。 |
| `description` | `string \| null` | 无 | 显示在标题下方。 |
| `actions` | `AlertDialogAction[]` | 必填 | 各操作，按顺序排列（至少一个；Android 警告框最多三个）。 |
| `presentation` | `"alert" \| "actionSheet"` | `"alert"` | 居中的警告框，或从底部出现的操作表（Android 上是列表对话框）。 |
| `closeOnEscape` | `boolean \| null` | 警告框为 `false`，操作表为 `true` | Escape 和 Android 返回键会不经操作关闭它。 |
| `closeOnBackdropPress` | `boolean \| null` | 警告框为 `false`，操作表为 `true` | 点击它外部会不经操作关闭它。 |

<a id="alertdialogaction"></a>

#### AlertDialogAction

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `label` | `string` | 必填 | 显示的文字。 |
| `role` | `"default" \| "cancel" \| "destructive"` | `"default"` | 它的含义：平台按角色放置并绘制它（破坏性操作在 iOS 上显示为红色）。 |
| `id` | `string \| null` | 其 `label` | 选中它时 `show` 解析的值。 |

> [!TIP]
> 主线程从不等待 JS，因此对话框以原生方式显示和应答，Promise 解析后你的代码继续执行。你可以在任何事件处理函数中 `await` 一个警告对话框，例如配合 [`popOnBack`](#leaving-a-page) 在离开页面前请用户确认。

<a id="alert-dialogs-on-each-platform"></a>

### 各平台上的警告对话框

- **iOS**：操作由 `UIAlertController` 布局。`role: "cancel"` 的操作会放在 iOS 放置取消操作的位置，操作表把它单独显示在底部。在 iPad 和 Mac Catalyst 上操作表以 popover 形式出现，带一个无标题的取消操作：点击 popover 外部即可关闭，Promise 解析为 `null`。
- **Android**：`AlertDialog` 最多显示三个操作，按位置映射：最后一个是 positive 按钮，倒数第二个是 negative 按钮，倒数第三个是 neutral 按钮。操作表是对话框中的操作列表，点击外部或按返回键代替其取消操作（此时 Promise 解析为 `null`）。列表对话框显示 `title`，没有标题时显示 `description`。
- **Windows、Linux、GPUI 与 Web**：NSH 以平台的对话框外观绘制警告框或操作表。

<a id="keyboard"></a>

## 键盘

`Keyboard` 用于收起软键盘，`keyboardFrame()` 访问器告诉你键盘遮挡了多少屏幕，`KeyboardAvoidingView` 让内容保持在键盘上方并随键盘移动。

### Keyboard.dismiss

```ts
Keyboard.dismiss(): void
```

让当前获得焦点的输入框失去焦点，从而收起键盘。没有输入框获得焦点时，一切保持原样。

```tsx
import { Input, mount, signal, Text, View } from "nsh";
import { Button, Keyboard } from "nsh/base";

function App() {
  const name = signal("");
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48, gap: 12 }}>
      <Input
        value={name()}
        onChange={(v) => name.set(v)}
        placeholder="Name"
        class="rounded-lg border border-neutral-300 px-3 py-2 text-base"
      />
      <Button onPress={() => Keyboard.dismiss()} class="rounded-lg bg-blue-600 px-4 py-3 active:bg-blue-700">
        <Text class="text-center font-semibold text-white">Done</Text>
      </Button>
    </View>
  );
}

mount(App);
```

> [!TIP]
> 如果想在用户拖动滚动视图时收起键盘，在[滚动视图](../elements/scroll.md)或 [List](../elements/list.md) 上设置 `keyboardDismissMode="on-drag"` 或 `"interactive"`。

<a id="keyboard-height"></a>

### 键盘高度

```ts
keyboardFrame: Accessor<{ height: number; duration: number }>
```

`keyboardFrame()` 返回软键盘覆盖在 NSH 界面上的高度（单位为 pt，键盘隐藏时为 0），以及键盘弹出或收起动画的时长（单位为秒）。它是响应式的：读取它的 effect 或 JSX 表达式会在键盘弹出、收起或高度变化时重新运行。

```tsx
import { Input, View } from "nsh";
import { keyboardFrame, safeAreaInsets } from "nsh/base";

function MessageBar() {
  const insets = safeAreaInsets();
  return (
    <View
      style={{
        flexDirection: "row",
        padding: 8,
        // The home indicator's inset only matters while the keyboard is hidden.
        paddingBottom: 8 + (keyboardFrame().height > 0 ? 0 : insets().bottom),
      }}
    >
      <Input placeholder="Message" style={{ flex: 1 }} />
    </View>
  );
}
```

这个高度是键盘与 NSH 界面重叠的部分。如果窗口本身会为键盘缩小（会调整窗口大小的 Android Activity），或者 NSH 嵌入的视图本来就在键盘上方结束，高度为 0。

> [!NOTE]
> 要让内容保持在键盘上方，使用 [KeyboardAvoidingView](#keyboardavoidingview) 通常比自己读取 `keyboardFrame()` 更简单：它会以原生方式随键盘同步动画。

### KeyboardAvoidingView

`KeyboardAvoidingView` 让其内容保持在软键盘上方。键盘弹出或收起时，视图会让出与键盘等高的空间，并与键盘同步执行动画。它适用于滚动视图之外的内容，例如聊天界面底部的消息输入栏。[滚动视图](../elements/scroll.md)或 [List](../elements/list.md) 中的输入框会以原生方式滚动到可见区域。

```tsx
import { Input, mount, Scroll, signal, Text, View } from "nsh";
import { KeyboardAvoidingView } from "nsh/base";

function App() {
  const draft = signal("");
  return (
    <KeyboardAvoidingView style={{ flex: 1, backgroundColor: "#ffffff" }}>
      <Scroll style={{ flex: 1 }} keyboardDismissMode="interactive">
        <Text style={{ padding: 16, paddingTop: 48 }}>Messages…</Text>
      </Scroll>
      <View style={{ flexDirection: "row", gap: 8, padding: 8, borderTopWidth: 1, borderTopColor: "#e5e5e5" }}>
        <Input
          value={draft()}
          onChange={(v) => draft.set(v)}
          placeholder="Message"
          style={{ flex: 1, fontSize: 16, padding: 8 }}
        />
      </View>
    </KeyboardAvoidingView>
  );
}

mount(App);
```

该视图假定自己延伸到屏幕底部（页面内容通常如此），因此会让出完整的键盘高度。如果它下方还有其他内容（例如标签栏），可以用 `offset` 传入负值减去这部分高度，或传入正值增加额外空间。

<a id="behaviors"></a>

#### 让位方式

| `behavior` | 如何让出空间 |
|---|---|
| `"padding"`（默认） | 把键盘高度加到视图的底部内边距上。上方内容随之缩小，背景仍延伸到屏幕底部。 |
| `"height"` | 把键盘高度作为底部外边距。整个视图（包括背景）在键盘顶部截止。 |

```tsx
<KeyboardAvoidingView behavior="height" offset={-49} style={{ flex: 1, padding: 16, gap: 12 }}>
  <Input placeholder="E-mail" keyboardType="email-address" class="rounded-lg border border-neutral-300 px-3 py-2" />
  <Button onPress={signIn} class="rounded-lg bg-blue-600 px-4 py-3 active:bg-blue-700">
    <Text class="text-center font-semibold text-white">Sign in</Text>
  </Button>
</KeyboardAvoidingView>
```

这里视图下方 49 pt 高的标签栏已经覆盖了一部分键盘高度。

这里的内边距或外边距会叠加在视图自身样式设置的值之上，你自己的 padding 保持不变。

<a id="keyboardavoidingview-props"></a>

#### KeyboardAvoidingView 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `enabled` | `boolean` | `true` | 是否避让键盘。为 false 时视图保持自身布局。 |
| `behavior` | `"padding" \| "height"` | `"padding"` | 视图如何让出空间（见[让位方式](#behaviors)）。 |
| `offset` | `number` | `0` | 键盘显示时加到键盘高度上的值（例如减去视图下方某个栏的高度）。总和至少为 0。 |
| `children` | `JSX.Element` | 无 | 内容。 |
| `class`、`classList`、`style`、`testID`、`onLayout`、`ref` | | | 作用于外层视图。 |

<a id="the-keyboard-on-each-platform"></a>

### 各平台上的键盘

- **iOS**：让位动画使用键盘自身的时长和 ease-in-ease-out 曲线。
- **Android**：如果窗口会为键盘缩小（Activity 会调整窗口大小），内容本来就在键盘上方结束，KeyboardAvoidingView 保持原有布局。键盘遮挡视图时，它的表现与 iOS 相同。
- **桌面平台与 Mac Catalyst** 使用硬件键盘输入：`keyboardFrame().height` 保持为 0，视图保持原有布局。

<a id="status-bar"></a>

## 状态栏

状态栏属于屏幕上正在显示的内容，和 UIKit 一样：在 UIKit 中，每个视图控制器声明自己的 `preferredStatusBarStyle`，由最上层的那个决定。在 NSH 中，页面和对话框就是这些视图控制器：`NavPage` 或 `Dialog` 用 `statusBar` 声明它需要的状态栏，应用则在 `nsh.json` 中声明默认值。

```tsx
import { NavPage } from "nsh/base";

function PhotoViewer() {
  return (
    <NavPage headerShown={false} statusBar={{ foreground: "light", hidden: true, animated: true }}>
      …
    </NavPage>
  );
}
```

`statusBar` 接受以下字段，每个都可省略：

| 字段 | 类型 | 说明 |
|---|---|---|
| `foreground` | `"auto" \| "light" \| "dark"` | 内容（时间、电量、信号）的颜色：`"light"` 用于深色背景，`"dark"` 用于浅色背景，`"auto"` 为平台针对当前外观的颜色。 |
| `hidden` | `boolean` | 隐藏状态栏。 |
| `animated` | `boolean` | 页面或对话框出现和离开时，以动画呈现变化。 |

这个值是响应式的：`statusBar={{ foreground: dark() ? "light" : "dark" }}` 会跟随信号变化。

<a id="which-one-applies"></a>

### 哪个生效

**最近显示**的页面或对话框决定状态栏，并且**逐个字段**生效：推入栈的页面位于它下面的页面之上，打开的对话框位于页面之上。它省略的字段取自下面那个，最终取自应用的默认值。页面被弹出或对话框关闭时，下面那个重新生效。

```tsx
<NavPage title="Inbox" statusBar={{ foreground: "dark" }}>…</NavPage>

// 推入在它之上：隐藏状态栏；再次显示时沿用收件箱的深色内容。
<NavPage title="Photo" statusBar={{ hidden: true }}>…</NavPage>
```

<a id="the-apps-default"></a>

### 应用的默认值

`nsh.json` 的 `"statusBar"` 是所有页面和对话框之下的设置，就像 UIKit 应用中 Info.plist 的 `UIStatusBarStyle` 和 `UIStatusBarHidden`：

```json
{
  "statusBar": { "foreground": "light", "hidden": false }
}
```

两个字段都可省略。不设置时，内容为 `"auto"`，状态栏显示。打包的 iOS 应用在启动屏上也会使用它。

<a id="the-status-bar-on-each-platform"></a>

### 各平台上的状态栏

- **iOS**：隐藏和显示以淡入淡出呈现。在[嵌入](../docs/embedding-ios.md)原生应用的场景下，状态栏的外观由宿主应用的视图控制器决定。宿主可以在其视图控制器中返回 `NshAppView.preferredStatusBarStyle`、`NshAppView.prefersStatusBarHidden` 和 `NshAppView.preferredStatusBarUpdateAnimation`，以应用 NSH 的设置。
- **Android**：应用以 edge-to-edge 方式绘制到状态栏之下（Android 15，即 API 35）：应用自己的背景就是状态栏的背景，`foreground` 让状态栏的内容在其上保持清晰。
- **桌面平台与 Web**：窗口标题栏和浏览器的栏属于系统和浏览器；`statusBar` 和 `nsh.json` 的 `"statusBar"` 用于手机。

> [!IMPORTANT]
> 如果你的 `nsh.json` 声明了 `"capabilities"`，只有其中包含 `"statusBar": true` 时状态栏才会改变，其他更改会被拒绝并记录日志。参见[沙箱与权限](../docs/sandbox-and-capabilities.md)。

```json
{
  "capabilities": {
    "statusBar": true
  }
}
```

<a id="related"></a>

## 相关内容

- [导航指南](../docs/navigation.md)。
- [表单与键盘](../docs/forms-and-keyboard.md)。
- [Dialog](../elements/dialog.md) 与 [Input](../elements/input.md)。
- [设备与环境](../docs/device-and-environment.md)。
- [nsh/android](./nsh-android.md)：`BackHandler`、用于简短消息的 `Toast.show` 及其他 Android 专用 API。
