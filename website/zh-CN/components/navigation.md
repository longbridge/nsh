---
title: 导航
description: 用 createNavigator、Stack 和 Screen 构建原生导航栈，涵盖标题栏选项、返回处理和 Modal。
order: 16
demo: ui-navigation
---

# 导航

`nsh/ui/mobile` 提供原生导航：由 `createNavigator` 创建的导航器驱动的、由多个 `Screen` 组成的 `Stack`，以及用于在所有内容之上展示内容的 `Modal`。在 iOS 上，栈就是 `UINavigationController`，带原生标题栏和侧滑返回；在 Android 上，它是带工具栏和预测性返回的页面栈。

```tsx
import { createNavigator, Modal, Screen, Stack } from "nsh/ui/mobile";
```

本页是 API 参考。完整的上手讲解见[导航指南](../docs/navigation.md)。

## 用法

```tsx
import { mount, View } from "nsh";
import { Button, createNavigator, Screen, Stack } from "nsh/ui/mobile";

type Route = { name: "home" } | { name: "detail"; id: number };

const nav = createNavigator<Route>({ name: "home" });

function Home() {
  return (
    <Screen title="Inbox">
      <View style={{ flex: 1, padding: 16 }}>
        <Button title="Open message 1" onPress={() => nav.push({ name: "detail", id: 1 })} />
      </View>
    </Screen>
  );
}

function Detail(props: { id: number }) {
  return (
    <Screen title={`Message ${props.id}`}>
      <View style={{ flex: 1, padding: 16 }}>
        <Button title="Back" variant="secondary" onPress={() => nav.pop()} />
      </View>
    </Screen>
  );
}

function App() {
  return (
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route) => (route.name === "home" ? <Home /> : <Detail id={route.id} />)}
    </Stack>
  );
}

mount(App);
```

## 栈的工作方式

- 导航器保存**路由**：由你自行决定的普通值，从栈底到栈顶排列。路由通常是一个对象，标明页面并携带参数。
- `Stack` 为每个路由调用一次子函数，并把结果显示为一个页面。对同一个路由它不会再次调用该函数，因此页面在栈中期间（包括被其他页面覆盖时）会保留状态：signal、滚动位置、输入内容。
- 当用户以原生方式离开页面（返回按钮、侧滑返回、Android 返回键）时，栈会从导航器中移除对应路由，因此 `nav.routes()` 始终与用户看到的一致。
- 路由按**引用**（`===`）比较。栈中的每个路由都必须是不同的值：每次 push 一个新对象（`{ name: "detail", id }`），不要重复使用同一个；对可能出现两次的页面，不要用字符串作为路由。
- 如果子函数返回的不是 `<Screen>`，栈会用一个默认选项的页面把它包起来。要设置标题和其他标题栏选项，请返回 `<Screen>`。

## createNavigator

```ts
function createNavigator<R>(initial: R | R[]): Navigator<R>
```

创建一个导航器，其栈以 `initial`（一个或多个路由）开始。只需创建一次（例如在模块顶层），然后传给 `<Stack navigator>`。任何代码都可以导入它来进行导航。

| 成员 | 说明 |
|---|---|
| `routes: Accessor<readonly R[]>` | 路由，从栈底到栈顶。响应式。 |
| `push(route: R): void` | 压入一个页面。 |
| `pop(count?: number): void` | 弹出 `count` 个页面（默认 1）。至少保留一个页面。 |
| `popToTop(): void` | 返回到第一个页面。 |
| `replace(route: R): void` | 替换栈顶页面。 |
| `reset(routes: R[]): void` | 替换所有页面。 |
| `remove(route: R): void` | 移除某个路由，无论它在栈中的什么位置。 |

```tsx
nav.push({ name: "detail", id: 7 });
nav.pop(2);
nav.reset([{ name: "home" }, { name: "detail", id: 1 }]);
const depth = () => nav.routes().length;
```

## Stack

```tsx
<Stack navigator={nav} style={{ flex: 1 }}>
  {(route, index) => …}
</Stack>
```

| 属性 | 类型 | 说明 |
|---|---|---|
| `navigator` | `Navigator<R>` | 保存路由的导航器。必填。 |
| `children` | `(route: R, index: Accessor<number>) => JSX.Element` | 渲染一个路由。返回 `<Screen>` 以设置标题栏选项。必填。 |
| `style` | `StyleProp<ViewStyle>` | 栈的样式，通常为 `{ flex: 1 }`。 |

子函数接收路由值本身，以及它在栈中位置的访问器。

## Screen

`Screen` 是栈中的一个页面，并配置其标题栏：

```tsx
<Screen
  title="Profile"
  headerTintColor="#ffffff"
  headerBackgroundColor="#1a7f37"
  headerTitleColor="#ffffff"
  headerRight={<Text onPress={edit} style={{ color: "#ffffff", fontSize: 17 }}>Edit</Text>}
>
  <Scroll style={{ flex: 1 }}>…</Scroll>
</Screen>
```

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `title` | `string` | 无 | 标题栏的标题。 |
| `headerShown` | `boolean` | `true` | 是否显示标题栏。 |
| `headerTransparent` | `boolean` | `false` | 内容延伸到透明标题栏下方。 |
| `headerTintColor` | `ColorValue` | 平台默认 | 返回按钮和栏按钮的颜色。 |
| `headerBackgroundColor` | `ColorValue` | 平台默认 | 标题栏背景色。 |
| `headerTitleColor` | `ColorValue` | 平台默认 | 标题颜色。 |
| `headerBackVisible` | `boolean` | `true` | 是否显示返回按钮。 |
| `headerLeft` | `JSX.Element` | 无 | 标题栏左侧的内容，替代返回按钮。 |
| `headerRight` | `JSX.Element` | 无 | 标题栏右侧的内容。 |
| `headerTitle` | `JSX.Element` | 无 | 替代标题的内容。 |
| `gestureEnabled` | `boolean` | `true` | 交互式返回手势：iOS 侧滑返回、Android 跟随手指的预测性返回。无论如何，返回按钮都可用。 |
| `preventBack` | `boolean` | `false` | 用户不能离开该页面。见[离开页面](#leaving-a-screen)。 |
| `animation` | `"default" \| "fade" \| "none"` | `"default"` | 页面进入和离开的方式。 |
| `ios` | `{ headerLargeTitle?: boolean; headerBackTitle?: string }` | 无 | 仅 iOS：随内容滚动收缩进栏内的大标题；返回到本页面的返回按钮文字。 |
| `desktop` | `{ headerLargeTitle?: boolean; headerBackTitle?: string }` | 无 | 仅桌面（macOS、Windows、Linux）：桌面标题栏上的同样选项。 |
| `style` | `StyleProp<ViewStyle>` | 无 | 页面样式。 |
| `children` | `JSX.Element` | 无 | 内容。 |
| `onBackRequest` | `() => void` | 无 | 用户试图离开设置了 `preventBack` 的页面。 |
| `onDismiss` | `() => void` | 无 | 用户以原生方式离开了页面，其路由已被移除。 |

标题栏项（`headerLeft`、`headerRight`、`headerTitle`）就是普通的 JSX：它们可以读取 signal、处理点击，并在页面显示期间更新。

页面内容从不透明标题栏的下方开始。设置 `headerTransparent` 或 `headerShown={false}` 时，内容从屏幕顶部开始；可以用 [SafeAreaView](./safe-area-view.md) 或 `useSafeAreaInsets()` 避开标题栏和状态栏。

```tsx
<Screen title="Large title" ios={{ headerLargeTitle: true }}>
  <Scroll style={{ flex: 1 }}>…</Scroll>
</Screen>
```

<a id="leaving-a-screen"></a>

### 离开页面

`preventBack` 让用户停留在当前页面，例如表单中有未保存的修改时。它为 true 时，侧滑返回被关闭，标题栏的返回按钮和 Android 返回键会调用 `onBackRequest` 而不是弹出页面。询问用户后，再由你自己弹出：

```tsx
import { Input, signal } from "nsh";
import { Alert, Screen } from "nsh/ui/mobile";

function EditNote() {
  const text = signal("");
  const confirm = async () => {
    const i = await Alert.alert("Discard your changes?", undefined, [
      { text: "Keep editing", style: "cancel" },
      { text: "Discard", style: "destructive" },
    ]);
    if (i === 1) nav.pop();
  };
  return (
    <Screen title="Edit" preventBack={text().length > 0} onBackRequest={confirm}>
      <Input value={text()} onChangeText={(v) => text.set(v)} placeholder="A note" />
    </Screen>
  );
}
```

> [!IMPORTANT]
> 系统会在用户操作的那一刻，在主线程上询问是否允许返回，而主线程从不等待 JS。因此 NSH 依据已经挂载的 `preventBack` 值作答。一旦有可能丢失的内容就应设置 `preventBack`，而不是在用户返回时才运行的处理函数里设置。

如果只想关闭侧滑手势而保留返回按钮，使用 `gestureEnabled={false}`。

<a id="modal"></a>

## Modal

`Modal` 在所有内容之上（包括栈的标题栏）展示内容，位于它自己的原生窗口层中。`visible` 为 true（默认）时显示，并且它是受控的：用户请求关闭时会调用 `onRequestClose`，由你把 `visible` 设为 false。

```tsx
import { signal, Text, View } from "nsh";
import { Button, Modal, SafeAreaView } from "nsh/ui/mobile";

function Settings() {
  const open = signal(false);
  return (
    <View>
      <Button title="About" onPress={() => open.set(true)} />
      <Modal visible={open()} ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => open.set(false)}>
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12 }}>
          <Text style={{ fontSize: 24, fontWeight: "700" }}>About</Text>
          <Button title="Close" onPress={() => open.set(false)} />
        </SafeAreaView>
      </Modal>
    </View>
  );
}
```

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `visible` | `boolean` | `true` | 为 true 时显示。 |
| `animationType` | `"slide" \| "fade" \| "none"` | `"slide"` | Modal 出现的方式。 |
| `transparent` | `boolean` | `false` | 背景透出后面的内容，用于对话框和浮层。 |
| `dismissible` | `boolean` | `false` | 用户以原生方式关闭它（iOS 下滑关闭 sheet、Android 返回键），随后调用 `onRequestClose` 和 `onDismiss`。不设置时，这些手势只是请求关闭。 |
| `ios` | `{ presentationStyle?: "fullScreen" \| "pageSheet" \| "formSheet"; supportedOrientations?: (…)[] }` | 无 | 仅 iOS：`pageSheet` 和 `formSheet` 会让后面的内容保持可见；Modal 允许的屏幕方向（默认与呈现它的界面一致）。 |
| `desktop` | `{ presentationStyle?: "fullScreen" \| "pageSheet" \| "formSheet" }` | 无 | 仅桌面（macOS、Windows、Linux）：`pageSheet` 和 `formSheet` 是窗口中居中的卡片，背后的内容变暗；点击背景或按 Escape 请求关闭。 |
| `android` | `{ statusBarTranslucent?: boolean }` | 无 | 仅 Android：内容延伸到状态栏下方（默认从状态栏下方开始）。 |
| `style` | `StyleProp<ViewStyle>` | 无 | Modal 的样式。 |
| `children` | `JSX.Element` | 无 | 内容。 |
| `onRequestClose` | `() => void` | 无 | 用户请求关闭（Android 返回键、iOS 下滑 sheet）。请把 `visible` 设为 false。 |
| `onShow` | `() => void` | 无 | Modal 已显示。 |
| `onDismiss` | `() => void` | 无 | Modal 已关闭。 |

Modal 会把自己的安全区边距提供给其内容，因此其中的 [SafeAreaView](./safe-area-view.md) 和 `useSafeAreaInsets()` 使用的是 Modal 的边距。设置 `transparent` 和 `animationType="fade"` 的 Modal 可以当作自定义对话框：

```tsx
<Modal visible={open()} transparent animationType="fade" onRequestClose={close}>
  <View style={{ flex: 1, backgroundColor: "rgba(0,0,0,0.4)", alignItems: "center", justifyContent: "center", padding: 32 }}>
    <View style={{ alignSelf: "stretch", backgroundColor: "#ffffff", borderRadius: 14, padding: 20, gap: 12 }}>
      <Text style={{ fontSize: 20, fontWeight: "700" }}>A dialog</Text>
      <Button title="OK" onPress={close} />
    </View>
  </View>
</Modal>
```

关于该元素的更多内容见 [Modal](../elements/modal.md)。

## 平台说明

- **iOS**：使用 `UINavigationController`，带原生标题栏、返回按钮和侧滑返回。`ios={{ headerLargeTitle }}` 和 `headerBackTitle` 仅在 iOS 上有效。在转场进行中到达的栈变更会在转场结束后应用。
- **Android**：带工具栏的页面栈。系统返回键会弹出栈顶页面；在支持的 Android 版本上使用预测性返回。如需自行处理返回键，见 [nsh/android](../modules/nsh-android.md) 中的 `BackHandler`。
- **Mac Catalyst**：与 iOS 相同。page sheet 和 form sheet 以与窗口同尺寸的 macOS sheet 形式呈现。

## 相关内容

- [导航指南](../docs/navigation.md)。
- [SafeAreaView](./safe-area-view.md)：标题栏下方和 Modal 中的安全区边距。
- [Sheet](./sheet.md)：由 NSH 绘制的底部面板，两个平台外观一致。
- [Alert](./alert.md)：离开页面前请用户确认。
