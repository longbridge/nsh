---
title: 导航
description: 用 createNavigator、Stack 和 Screen 构建原生导航栈，标题栏选项，用 preventBack 把用户留在当前页面，模态视图，以及 Android 返回键。
order: 41
group: ui
---

# 导航

`nsh/ui/mobile` 提供原生的导航栈和原生的模态视图。`Stack` 在 iOS 上是 UINavigationController，带有它的标题栏、侧滑返回和大标题；在 Android 上是带工具栏、支持预测性返回的页面栈。路由列表保存在 navigator 中；栈把每个路由渲染为一个页面，只要路由还在栈里，页面就保持挂载，状态也随之保留。

```tsx
import { mount, Scroll, Text, View } from "nsh";
import { createNavigator, Screen, Stack } from "nsh/ui/mobile";

type Route = { name: "inbox" } | { name: "message"; id: number };

const nav = createNavigator<Route>({ name: "inbox" });

function Inbox() {
  return (
    <Screen title="Inbox">
      <Scroll style={{ flex: 1 }}>
        <Each of={[1, 2, 3, 4, 5]}>
          {(id) => (
            <View onPress={() => nav.push({ name: "message", id: id() })} underlayColor="#eaeef2" style={{ padding: 16 }}>
              <Text>Message {id()}</Text>
            </View>
          )}
        </Each>
      </Scroll>
    </Screen>
  );
}

function Message(props: { id: number }) {
  return (
    <Screen title={`Message ${props.id}`}>
      <View style={{ flex: 1, padding: 16 }}>
        <Text>The body of message {props.id}.</Text>
      </View>
    </Screen>
  );
}

function App() {
  return (
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route) => {
        switch (route.name) {
          case "inbox":
            return <Inbox />;
          case "message":
            return <Message id={route.id} />;
        }
      }}
    </Stack>
  );
}

mount(App);
```

## Navigator

`createNavigator(initial)` 创建路由栈。路由可以是任何值（字符串、对象，或者像上面那样的可辨识联合类型）；路由按标识比较，所以每个页面都应该 push 一个新对象。navigator 只创建一次，放在模块顶层或根组件中，在需要导航的页面里导入它。

| 成员 | 说明 |
|---|---|
| `routes()` | 从底到顶的路由（响应式）。 |
| `push(route)` | 压入一个页面。 |
| `pop(count?)` | 弹出 `count` 个页面（默认 1）；至少保留一个。 |
| `popToTop()` | 回到第一个页面。 |
| `replace(route)` | 替换栈顶页面。 |
| `reset(routes)` | 替换所有页面，例如登录之后。 |
| `remove(route)` | 无论路由在哪个位置都将其移除。 |

`createNavigator` 也接受数组，用来一开始就处在多层深处：`createNavigator<Route>([{ name: "inbox" }, { name: "message", id: 3 }])`。

当用户以原生方式离开页面时（标题栏的返回按钮、iOS 侧滑返回、Android 返回键或返回手势），栈会自动从 navigator 中移除对应路由，所以 `routes()` 总是与屏幕上的内容一致。

路由携带页面需要的数据。gallery 示例应用推送的路由直接保存页面组件，页面很多时路由类型依然很小：

```ts
import { type JSX } from "nsh";
import { createNavigator } from "nsh/ui/mobile";

export type Route = { name: "home" } | { name: "page"; page: () => JSX.Element };
export const nav = createNavigator<Route>({ name: "home" });

// 在别处：nav.push({ name: "page", page: Settings });
```

## Stack

| 属性 | 类型 | 说明 |
|---|---|---|
| `navigator` | `Navigator<R>` | 要显示的路由。 |
| `children` | `(route: R, index: Accessor<number>) => JSX.Element` | 渲染一个路由；返回 `<Screen>` 以设置标题栏选项。 |
| `style` | `StyleProp<ViewStyle>` | 栈自身的样式（通常是 `flex: 1`）。 |

渲染函数在每个路由被压入时运行一次；`route` 是原始值，`index()` 是它当前的位置。返回值成为页面内容。返回 `<Screen>` 可以设置标题栏；返回其他内容时会自动包在一个使用默认选项的页面里。

只要路由还在栈中，页面就保持挂载：返回时，上一个页面与离开时完全相同，包括滚动位置、输入框和 signal。路由离开栈时，页面的 [`cleanup`](./reactivity.md) 处理函数会运行。

## Screen 与标题栏

`Screen` 包裹页面内容并携带标题栏选项。所有选项都是响应式的：改变 signal，标题栏随之更新。

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `title` | `string` | | 标题栏的标题。 |
| `headerShown` | `boolean` | `true` | 显示标题栏。 |
| `headerTransparent` | `boolean` | `false` | 内容延伸到标题栏下方。 |
| `headerTintColor` | `ColorValue` | | 返回按钮和栏按钮的颜色。 |
| `headerBackgroundColor` | `ColorValue` | | 标题栏背景色。 |
| `headerTitleColor` | `ColorValue` | | 标题颜色。 |
| `headerBackVisible` | `boolean` | `true` | 显示返回按钮。 |
| `headerLeft` | `JSX.Element` | | 标题栏开头的视图（替换返回按钮）。 |
| `headerRight` | `JSX.Element` | | 标题栏末尾的视图。 |
| `headerTitle` | `JSX.Element` | | 替换标题的视图。 |
| `animation` | `"default" \| "fade" \| "none"` | `"default"` | 页面进入和离开的方式。 |
| `gestureEnabled` | `boolean` | `true` | 交互式返回手势（iOS 侧滑返回、Android 预测性返回）。返回按钮无论如何都会返回。 |
| `preventBack` | `boolean` | `false` | 用户不能离开；返回操作改为调用 `onBackRequest`。 |
| `onBackRequest` | `() => void` | | 用户试图离开设置了 `preventBack` 的页面。 |
| `onDismiss` | `() => void` | | 用户以原生方式离开了页面；路由已被移除。 |
| `ios={{ headerLargeTitle }}` | `boolean` | | iOS：大标题，内容滚动时缩进标题栏。 |
| `ios={{ headerBackTitle }}` | `string` | | iOS：返回到本页面的返回按钮上的文字。 |
| `style` | `StyleProp<ViewStyle>` | | 页面的内容视图。 |

标题栏中的项目就是普通视图，所以可以按压，也是响应式的：

```tsx
function Compose() {
  const text = signal("");
  return (
    <Screen
      title="New message"
      headerLeft={<Text onPress={() => nav.pop()} style={{ fontSize: 17, color: "#0969da" }}>Cancel</Text>}
      headerRight={
        <Text
          onPress={() => send(text())}
          style={{ fontSize: 17, fontWeight: "600", color: text() ? "#0969da" : "#8c959f" }}
        >
          Send
        </Text>
      }
    >
      <Input multiline value={text()} onChangeText={(t) => text.set(t)} style={{ flex: 1, padding: 16, fontSize: 17 }} />
    </Screen>
  );
}
```

彩色标题栏：

```tsx
<Screen title="Profile" headerBackgroundColor="#1a7f37" headerTintColor="#ffffff" headerTitleColor="#ffffff">
  …
</Screen>
```

没有标题栏的页面四周仍有系统栏；用 [`SafeAreaView`](../components/safe-area-view.md) 给内容留出边距：

```tsx
<Screen headerShown={false}>
  <SafeAreaView style={{ flex: 1, padding: 16 }}>…</SafeAreaView>
</Screen>
```

### 标题栏下的安全区域

在页面内部，`useSafeAreaInsets()` 和 `SafeAreaView` 报告的是标题栏下方的 inset，而不是整个 surface 的。使用 `headerTransparent` 时，内容从页面顶部开始，顶部 inset 等于标题栏高度，因此 `SafeAreaView` 能让文字避开标题栏，同时背景图铺满整个页面。使用 `ios={{ headerLargeTitle: true }}` 时，把滚动视图直接放在页面里，标题才能随滚动收起。

## 把用户留在当前页面：preventBack

要在丢弃未保存的修改前先询问用户，就在有内容可能丢失时设置 `preventBack`，并处理 `onBackRequest`：

```tsx
import { Input, signal } from "nsh";
import { Alert, Screen } from "nsh/ui/mobile";

function EditNote() {
  const text = signal("");
  const confirm = async () => {
    const choice = await Alert.alert("Discard your changes?", undefined, [
      { text: "Keep editing", style: "cancel" },
      { text: "Discard", style: "destructive" },
    ]);
    if (choice === 1) nav.pop();
  };
  return (
    <Screen title="Edit" preventBack={text().length > 0} onBackRequest={confirm}>
      <Input value={text()} onChangeText={(t) => text.set(t)} placeholder="A note" style={{ margin: 16, fontSize: 17 }} />
    </Screen>
  );
}
```

开启 `preventBack` 后，侧滑返回被禁用，返回按钮（标题栏中的、Android 返回键和返回手势）调用 `onBackRequest` 而不是返回。从代码调用 `nav.pop()` 仍然会返回。

> [!IMPORTANT]
> `preventBack` 是一个属性，而不是返回 true 或 false 的回调，因为主线程从不等待 JS。用户开始侧滑或按下返回时，系统会立即询问页面能否离开，NSH 根据已有的属性在原生端直接回答。所以要在有内容可能丢失时就设置 `preventBack`（像上面那样根据文本决定），而不是等用户试图离开时才设置。

`gestureEnabled={false}` 是较轻的选择：它只关闭交互式手势（iOS 侧滑返回、Android 预测性返回动画），返回按钮仍会返回。

## 模态视图

`Modal` 在其他所有内容之上呈现内容，处于独立的原生窗口层：iOS 上是被 present 的视图控制器，Android 上是一个对话框。把它渲染在其状态所在的地方，用 `visible` 控制：

```tsx
import { signal, Text, View } from "nsh";
import { Modal, SafeAreaView } from "nsh/ui/mobile";

function Settings() {
  const open = signal(false);
  return (
    <View style={{ flex: 1 }}>
      <View onPress={() => open.set(true)} style={{ padding: 16 }}>
        <Text>Edit profile</Text>
      </View>
      <Modal visible={open()} animationType="slide" ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => open.set(false)}>
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12 }}>
          <Text style={{ fontSize: 22, fontWeight: "700" }}>Profile</Text>
          <View onPress={() => open.set(false)}>
            <Text style={{ color: "#0969da" }}>Done</Text>
          </View>
        </SafeAreaView>
      </Modal>
    </View>
  );
}
```

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `visible` | `boolean` | `true` | 为 true 时显示。 |
| `animationType` | `"slide" \| "fade" \| "none"` | | 出现的方式。 |
| `transparent` | `boolean` | `false` | 背景透出后面的内容（对话框、自定义面板）。 |
| `dismissible` | `boolean` | `false` | 用户以原生方式关闭它（iOS 下滑关闭页面表单、Android 返回键），之后再调用 `onRequestClose` 和 `onDismiss`。 |
| `onRequestClose` | `() => void` | | 用户请求关闭：把 `visible` 设为 false。 |
| `onShow` | `() => void` | | 已经出现。 |
| `onDismiss` | `() => void` | | 已经消失。 |
| `ios={{ presentationStyle }}` | `"fullScreen" \| "pageSheet" \| "formSheet"` | `"fullScreen"` | iOS：表单样式会保留后面内容的可见部分。 |
| `ios={{ supportedOrientations }}` | `("portrait" \| "landscape" \| …)[]` | 与呈现者相同 | iOS：允许的方向。 |
| `android={{ statusBarTranslucent }}` | `boolean` | `false` | Android：内容延伸到状态栏下方。 |

模态视图默认是**受控**的：在 iOS 表单上下滑或按 Android 返回键只是发出请求（`onRequestClose`），直到你把 `visible` 设为 false 它才关闭。这样你可以先确认。设置 `dismissible` 后，平台会立即关闭它，然后再通知你；适合没有内容会丢失的面板。与 `preventBack` 一样，这是一个属性，因为系统会当场询问能否关闭。

透明模态视图可以做成自定义对话框：

```tsx
<Modal visible={open()} transparent animationType="fade" onRequestClose={() => open.set(false)}>
  <View style={{ flex: 1, backgroundColor: "rgba(0,0,0,0.4)", alignItems: "center", justifyContent: "center", padding: 32 }}>
    <View style={{ alignSelf: "stretch", backgroundColor: "#ffffff", borderRadius: 14, padding: 20, gap: 12 }}>
      <Text style={{ fontSize: 20, fontWeight: "700" }}>Delete the file?</Text>
      <View onPress={() => open.set(false)}><Text style={{ color: "#cf222e" }}>Delete</Text></View>
    </View>
  </View>
</Modal>
```

在模态视图内部，`SafeAreaView` 和 `useSafeAreaInsets()` 给出的是模态视图自己的 inset。系统对话框（带按钮的提示框、操作表）请用 [`Alert`](../components/alert.md) 和 [`ActionSheet`](../components/action-sheet.md)；从底部滑出、覆盖在暗色背景上并跟随手指的面板，请用 [`Sheet`](../components/sheet.md)。

## Android 返回键

在 Android 上，返回键（或返回手势）按以下顺序处理：

1. 最上层的模态视图：调用 `onRequestClose`（设置了 `dismissible` 时直接关闭）。
2. 你在 `nsh/android` 中注册的 `BackHandler` 监听器，最近添加的最先执行。
3. 最上层的栈：弹出栈顶页面，设置了 `preventBack` 时调用 `onBackRequest`。
4. 都没有处理：应用退出。

监听器处理了按键时返回 `true`，栈就不会弹出。注册了监听器时，Android 的返回手势不会预览弹出效果（手势结束后应用才给出答复），所以只在监听器可能处理按键时才注册它：

```tsx
import { cleanup, signal } from "nsh";
import { BackHandler } from "nsh/android";

function Editor() {
  const selecting = signal(false);
  const sub = BackHandler.addEventListener("hardwareBackPress", () => {
    if (!selecting()) return false; // 交给栈去返回
    selecting.set(false);
    return true;
  });
  cleanup(() => sub.remove());
  // …
}
```

`BackHandler.exitApp()` 退出应用。在 iOS 上也可以导入 `nsh/android`，它的调用什么都不做，所以同一份代码可以在两个平台上运行。见[平台特定代码](./platform-specific.md)。

## 标签页与其他布局

`nsh/ui/mobile` 没有标签栏组件。标签栏就是一排可按压的视图加一个保存当前标签的 signal；用函数子元素或 [`<If>`](../elements/control-flow.md) 渲染选中标签的内容。要在切换时保留每个标签的状态，给每个标签一个独立的 `Stack` 和 navigator，并让它们都保持挂载，用 `display: "none"` 隐藏未选中的标签。

## 相关内容

- [导航组件](../components/navigation.md)：`createNavigator`、`Stack`、`Screen` 和 `Modal` 参考。
- [`SafeAreaView`](../components/safe-area-view.md) 与 [`StatusBar`](../components/status-bar.md)。
- [`Alert`](../components/alert.md)、[`ActionSheet`](../components/action-sheet.md) 与 [`Sheet`](../components/sheet.md)。
- [`nsh/android`](../modules/nsh-android.md)：`BackHandler`。
