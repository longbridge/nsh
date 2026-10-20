---
title: 平台特定代码
description: 为手机、桌面和浏览器编写同一个应用：ios={{}} / android={{}} / desktop={{}} / web={{}} 中的平台专属属性，nsh/ios 与 nsh/android 模块，Platform.OS 与 Platform.select，以及平台相关的 Tailwind 变体。
order: 17
group: platforms
---

# 平台特定代码

同一个 NSH 包可以在 iOS、Android、macOS、Windows 和 Linux 上运行（也可以在 Mac Catalyst 中运行），还可以通过 [Web 运行时](./web.md)在浏览器中运行。应用的大部分代码根本不需要判断平台：元素、样式、布局和模块在各处的行为都相同，你用它们绘制的组件在各处看起来也都一样。当某样东西确实只属于一个平台时，NSH 给它安排了明确的位置，让平台代码一目了然、范围可控：

| 位置 | 用途 |
|---|---|
| [`ios={{…}}`、`android={{…}}`、`desktop={{…}}`、`web={{…}}`](#平台专属属性) | 只有一个平台才有的属性。 |
| [`nsh/ios`、`nsh/android`](#平台模块) | 只有一个平台才有的 API。 |
| [`Platform.OS`、`Platform.select`](#platformos-与-platformselect) | 按平台选择值或代码。 |
| [`ios:` / `android:` / `web:` 类](#tailwind-中的平台变体) | 按平台设置样式。 |

## 平台专属属性

只存在于某一个平台的属性，归入以该平台命名的对象中。每个元素都接受 `ios`、`android`、`desktop`（macOS、Windows、Linux，以及界面风格为桌面的浏览器）和 `web`（浏览器）；在其他平台上这个对象会被忽略，在它自己的平台上只应用其中的键。桌面浏览器同时应用 `desktop` 和 `web`。

```tsx
<View
  onPress={open}
  activeStyle={{ opacity: 0.6 }}
  android={{ ripple: { color: "rgba(0,0,0,0.12)" }, elevation: 4 }}
  style={{ padding: 16, borderRadius: 12, backgroundColor: "#ffffff" }}
>
  <Text>Open</Text>
</View>

<Input
  placeholder="Code"
  ios={{ textContentType: "oneTimeCode" }}
/>

<Dialog open={open()} ios={{ supportedOrientations: ["portrait"] }} android={{ statusBarTranslucent: true }}>
  …
</Dialog>
```

这些对象和其他属性一样是响应式的；从对象中移除的键会被清除。

目前已有的：

| 属性 | 适用于 | 说明 |
|---|---|---|
| `android={{ ripple }}` | 所有元素 | 按下时的 Material 水波纹：`{ color?, borderless?, radius?, foreground? }`。 |
| `android={{ elevation }}` | 所有元素 | 根据视图形状生成的 Material 阴影，单位为点。 |
| `android={{ accessibilityLiveRegion }}` | 所有元素 | `"none"`、`"polite"` 或 `"assertive"`：TalkBack 播报视图的变化。 |
| `android={{ statusBarTranslucent }}` | `Dialog` | 对话框的内容延伸到状态栏下方。 |
| `ios={{ inputAccessoryViewID }}` | `<Input>` | 显示在键盘上方的 `InputAccessoryView`。 |
| `ios={{ textContentType }}` | `<Input>` | iOS 对自动填充类型的叫法。 |
| `ios={{ supportedOrientations }}` | `Dialog` | 允许的方向。 |
| `ios={{ allowsBackForwardNavigationGestures }}` | `WebView` | 滑动在页面历史中前进后退。 |
| `desktop={{ allowsBackForwardNavigationGestures }}` | `WebView` | macOS：双指在触控板上轻扫，在页面历史中前进后退；Windows：在触摸屏上轻扫；Linux：在触控板上轻扫。 |
| `desktop={{ windowDrag }}` | 所有元素 | 按住它拖动窗口，像系统标题栏一样（见 [nsh/desktop](../modules/nsh-desktop.md#自定义标题栏)）。 |

`desktop={{…}}` 和 `web={{…}}` 在所有地方都可以使用。类型（`IOSProps`、`AndroidProps`、`DesktopProps` 及其各元素的形式）会告诉编辑器每个元素接受什么。

两个及以上平台都有的功能是顶层属性，在有意义的平台上生效：`NavPage` 的 `title`、`headerStyle`、`headerLargeTitle` 和 `headerBackTitle` 设置 iOS 和 Android 的系统标题栏，它的 `header` 插槽是桌面和 Web 上的标题栏，`Dialog` 的 `presentation` 选择 iOS 的表单样式。`AlertDialog.show` 的 `closeOnBackdropPress`、`pagingEnabled`、`refreshing`、`hitSlop` 等在所有平台上都可用。

## 平台模块

`nsh/ios` 和 `nsh/android` 存放只有一个平台才有的 API，`nsh/desktop` 存放只有桌面才有的 API。它们在所有平台上都可以导入，因此同一个包可以到处运行：在其他平台上，它们的函数什么都不做，组件什么都不渲染。每个模块都导出 `isAvailable`，只在自己的平台上为 `true`。

### nsh/android

| 导出 | 说明 |
|---|---|
| `Toast.show(message, duration?)` | 在屏幕底部显示 toast；`"short"`（默认）或 `"long"`。 |
| `BackHandler.addEventListener("hardwareBackPress", listener)` | 在对话框之后、导航栈之前监听返回键；处理了就返回 `true`。返回 `{ remove() }`。 |
| `BackHandler.exitApp()` | 退出应用。 |
| `isAvailable` | Android 上为 `true`。 |

```tsx
import { cleanup } from "nsh";
import { BackHandler, Toast } from "nsh/android";
import { Platform } from "nsh/device";
import { AlertDialog } from "nsh/base";

function saved() {
  if (Platform.OS === "android") Toast.show("Saved");
  else AlertDialog.show({ title: "Saved", actions: [{ label: "OK" }] });
}

function Player() {
  // 返回键先退出全屏，再离开页面。
  const sub = BackHandler.addEventListener("hardwareBackPress", () => {
    if (!fullScreen()) return false;
    fullScreen.set(false);
    return true;
  });
  cleanup(() => sub.remove());
  // …
}
```

返回键先由最上层的对话框处理，然后是 `BackHandler` 监听器（最近添加的最先），再然后是最上层的导航栈，都没有处理时应用退出。见[导航](./navigation.md#android-返回键)。

### nsh/ios

| 导出 | 说明 |
|---|---|
| `InputAccessoryView` | 在 `ios={{ inputAccessoryViewID }}` 中引用其 `nativeID` 的输入框的键盘上方显示的工具栏。 |
| `isAvailable` | iOS 和 Mac Catalyst 上为 `true`。 |

见[表单与键盘](./forms-and-keyboard.md#键盘上方的工具栏inputaccessoryview)。

### nsh/desktop

| 导出 | 说明 |
|---|---|
| `Window` | 窗口：`setTitle`、`setTitleBar("hidden")`（自定义标题栏）、`minimize`、`toggleMaximize`、`toggleFullScreen`、`close`。 |
| `titleBar` | 隐藏的标题栏的高度，以及系统窗口按钮占用的空间（响应式）。 |
| `Menu` | `setApplicationMenu(items)`（菜单栏，带快捷键）和 `popup(items, at?)`（右键菜单）。 |
| `isAvailable` | 在 macOS、Windows 和 Linux 上为 `true`。 |

见 [nsh/desktop](../modules/nsh-desktop.md)。

### 状态栏

在两个平台上，状态栏都属于最上层的页面或对话框，和 UIKit 中由最上层视图控制器决定一样：[`NavPage` 和 `Dialog` 接受 `statusBar`](../modules/nsh-base.md#status-bar)，其中有 `foreground`（`"auto"`、深色背景用 `"light"`、浅色背景用 `"dark"`）、`hidden` 和 `animated`。最近显示的页面或对话框按字段逐项生效，它离开后下面那个重新生效。`nsh.json` 的 `"statusBar"` 是它们之下的应用默认值。

```tsx
import { View } from "nsh";
import { NavPage } from "nsh/base";

function PhotoViewer() {
  return (
    <NavPage headerShown={false} statusBar={{ foreground: "light", animated: true }}>
      <View style={{ flex: 1, backgroundColor: "#000000" }}>…</View>
    </NavPage>
  );
}
```

在嵌入原生应用的场景下，iOS 上由宿主应用的视图控制器决定状态栏。在 `nsh.json` 中声明了 `"capabilities"` 的应用，需要 `"statusBar": true` 才能修改它。

## Platform.OS 与 Platform.select

`nsh/device` 中的 `Platform` 告诉你应用运行在哪里：`Platform.OS` 为 `"ios"`、`"android"`、`"macos"`、`"windows"`、`"linux"` 或 `"web"`（Mac Catalyst 为 `"ios"`，同时 `Platform.isMacCatalyst` 为真；[macOS 后端](./macos.md)为 `"macos"`，[Linux 后端](./linux.md)为 `"linux"`，[Windows 后端](./windows.md)为 `"windows"`，它们的元素会应用 `desktop` 属性；[浏览器](./web.md)为 `"web"`，界面风格为 `"desktop"` 时也会应用 `desktop` 属性）。`Version`、`isPad` 等见[设备与环境](./device-and-environment.md#平台)。

```tsx
import { Platform } from "nsh/device";

if (Platform.OS === "android") {
  // …
}
```

### Platform.select

`Platform.select(spec)` 返回当前平台对应的值。它依次查找该操作系统自己的键，然后是 `mobile`（iOS、Android、手机和平板浏览器）或 `desktop`（macOS、Windows、Linux、桌面浏览器），再是 `native`（除 Web 外的所有平台，与 React Native 相同），最后是 `default`：

```tsx
const styles = {
  card: {
    borderRadius: 12,
    backgroundColor: "#ffffff",
    ...Platform.select({
      ios: { shadowColor: "#000000", shadowOpacity: 0.12, shadowRadius: 8, shadowOffset: { width: 0, height: 2 } },
      android: { borderWidth: 1, borderColor: "#e5e7eb" },
    }),
  },
};

const hint = Platform.select({ ios: "Swipe down to close", android: "Press back to close", default: "Close" });
```

没有匹配项且没有 `default` 时返回 `undefined`。

### 渲染不同的组件

组件也可以这样选择；选择永远不变时放在模块顶层：

```tsx
const Spinner = Platform.select({ ios: IosSpinner, default: MaterialSpinner })!;
```

或者用 [`<If>`](../elements/control-flow.md) 内联：

```tsx
<If when={Platform.OS === "ios"} else={<Text>Android has no input accessories.</Text>}>
  <InputAccessoryView nativeID="bar">…</InputAccessoryView>
</If>
```

## Tailwind 中的平台变体

`ios:`、`android:` 和 `web:` 变体只在一个平台上应用对应的类：

```tsx
<View class="rounded-xl bg-white p-4 ios:shadow-md android:border android:border-zinc-200">…</View>
<Text class="text-base android:font-medium">Title</Text>
```

## 优先使用共享的行为

在添加平台判断之前，先想想这个差异对用户是否重要。元素和样式已经在应当不同的地方映射到各平台的原生行为（滚动物理效果、文字渲染、键盘、无障碍），在用户期望一致的地方（你的应用自己的外观）保持一致。平台判断最好留给用户依赖的平台惯例：Android 的返回键和 toast、iOS 的表单弹窗和大标题、自动填充的名称、Android 的水波纹。

## 相关内容

- [`nsh/ios`](../modules/nsh-ios.md) 与 [`nsh/android`](../modules/nsh-android.md)：参考。
- [`nsh/device`](../modules/nsh-device.md)：`Platform`。
- [原生组件](./native-components.md)：使用宿主应用在各平台上提供的视图。
- [Tailwind](./tailwind.md)：所有变体。
