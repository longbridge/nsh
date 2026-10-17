---
title: 平台特定代码
description: 为手机和桌面编写同一个应用：ios={{}} / android={{}} / desktop={{}} 中的平台专属属性，nsh/ios 与 nsh/android 模块，Platform.OS 与 Platform.select，以及平台相关的 Tailwind 变体。
order: 17
group: platforms
---

# 平台特定代码

同一个 NSH 包可以在 iOS、Android、macOS、Windows 和 Linux 上运行（也可以在 Mac Catalyst 中运行）。应用的大部分代码根本不需要判断平台：元素、样式、布局、组件和模块在各处的行为都相同，`nsh/ui/mobile` 的组件由 NSH 自己绘制而不是借用各平台的控件，所以在各处看起来都一样。当某样东西确实只属于一个平台时，NSH 给它安排了明确的位置，让平台代码一目了然、范围可控：

| 位置 | 用途 |
|---|---|
| [`ios={{…}}`、`android={{…}}`、`desktop={{…}}`](#平台专属属性) | 只有一个平台才有的属性。 |
| [`nsh/ios`、`nsh/android`](#平台模块) | 只有一个平台才有的 API。 |
| [`Platform.OS`、`Platform.select`](#platformos-与-platformselect) | 按平台选择值或代码。 |
| [`ios:` / `android:` 类](#tailwind-中的平台变体) | 按平台设置样式。 |

## 平台专属属性

只存在于某一个平台的属性，归入以该平台命名的对象中。每个元素都接受 `ios`、`android` 和 `desktop`（macOS、Windows、Linux）；在其他平台上这个对象会被忽略，在它自己的平台上只应用其中的键。

```tsx
<View
  onPress={open}
  activeOpacity={0.6}
  android={{ ripple: { color: "rgba(0,0,0,0.12)" }, elevation: 4 }}
  style={{ padding: 16, borderRadius: 12, backgroundColor: "#ffffff" }}
>
  <Text>Open</Text>
</View>

<Input
  placeholder="Code"
  ios={{ textContentType: "oneTimeCode" }}
/>

<Modal visible={open()} ios={{ presentationStyle: "pageSheet" }} android={{ statusBarTranslucent: true }}>
  …
</Modal>
```

这些对象和其他属性一样是响应式的；从对象中移除的键会被清除。

目前已有的：

| 属性 | 适用于 | 说明 |
|---|---|---|
| `android={{ ripple }}` | 所有元素 | 按下时的 Material 水波纹：`{ color?, borderless?, radius?, foreground? }`。 |
| `android={{ elevation }}` | 所有元素 | 根据视图形状生成的 Material 阴影，单位为点。 |
| `android={{ accessibilityLiveRegion }}` | 所有元素 | `"none"`、`"polite"` 或 `"assertive"`：TalkBack 播报视图的变化。 |
| `android={{ statusBarTranslucent }}` | `Modal` | 模态视图的内容延伸到状态栏下方。 |
| `android={{ backgroundColor }}` | `StatusBar` | 状态栏颜色（Android 15 全面屏之前）。 |
| `ios={{ inputAccessoryViewID }}` | `<Input>` | 显示在键盘上方的 `InputAccessoryView`。 |
| `ios={{ textContentType }}` | `<Input>` | iOS 对自动填充类型的叫法。 |
| `ios={{ headerLargeTitle }}`、`ios={{ headerBackTitle }}` | `Screen` | 大标题；返回按钮上的文字。 |
| `ios={{ presentationStyle }}`、`ios={{ supportedOrientations }}` | `Modal` | `"pageSheet"` / `"formSheet"` 表单样式；允许的方向。 |
| `ios={{ allowsBackForwardNavigationGestures }}` | `WebView` | 滑动在页面历史中前进后退。 |
| `desktop={{ headerLargeTitle }}`、`desktop={{ headerBackTitle }}` | `Screen` | 大标题；返回按钮上的文字。 |
| `desktop={{ presentationStyle }}` | `Modal` | `"pageSheet"` / `"formSheet"`：窗口中居中的卡片。 |
| `desktop={{ allowsBackForwardNavigationGestures }}` | `WebView` | macOS：双指在触控板上轻扫，在页面历史中前进后退；Windows：在触摸屏上轻扫；Linux：在触控板上轻扫。 |
| `desktop={{ windowDrag }}` | 所有元素 | 按住它拖动窗口，像系统标题栏一样（见 [nsh/desktop](../modules/nsh-desktop.md#自定义标题栏)）。 |

`desktop={{…}}` 在所有地方都可以使用；上面列出的是目前仅限桌面的属性。类型（`IOSProps`、`AndroidProps`、`DesktopProps` 及其各元素的形式）会告诉编辑器每个元素接受什么。

在两个平台上都存在、只是效果略有不同的属性不做分组：`Alert.alert` 的 `cancelable` 选项两个平台都有（iOS 提示框本来就不会因点击外部而关闭），`pagingEnabled`、`refreshing`、`hitSlop` 等在所有平台上都可用。

## 平台模块

`nsh/ios` 和 `nsh/android` 存放只有一个平台才有的 API，`nsh/desktop` 存放只有桌面才有的 API。它们在所有平台上都可以导入，因此同一个包可以到处运行：在其他平台上，它们的函数什么都不做，组件什么都不渲染。每个模块都导出 `isAvailable`，只在自己的平台上为 `true`。

### nsh/android

| 导出 | 说明 |
|---|---|
| `Toast.show(message, duration?)` | 在屏幕底部显示 toast；`"short"`（默认）或 `"long"`。 |
| `BackHandler.addEventListener("hardwareBackPress", listener)` | 在模态视图之后、导航栈之前监听返回键；处理了就返回 `true`。返回 `{ remove() }`。 |
| `BackHandler.exitApp()` | 退出应用。 |
| `setStatusBarBackgroundColor(color, animated?)` | `StatusBar` 组件所设置内容之下的状态栏颜色（全面屏之前，API 35）。 |
| `isAvailable` | Android 上为 `true`。 |

```tsx
import { cleanup } from "nsh";
import { BackHandler, Toast } from "nsh/android";
import { Platform } from "nsh/device";
import { Alert } from "nsh/ui/mobile";

function saved() {
  if (Platform.OS === "android") Toast.show("Saved");
  else Alert.alert("Saved");
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

返回键先由最上层的模态视图处理，然后是 `BackHandler` 监听器（最近添加的最先），再然后是最上层的导航栈，都没有处理时应用退出。见[导航](./navigation.md#android-返回键)。

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

状态栏在两个平台上都用 `nsh/ui/mobile` 中的 [`StatusBar`](../components/status-bar.md) 设置：`barStyle`（`"default"`、`"light-content"`、`"dark-content"`）、`hidden` 和 `animated`，Android 的颜色放在 `android={{ backgroundColor }}` 中。最近挂载的 `StatusBar` 按属性逐项生效，所以页面可以为自己渲染一个，页面消失后之前的设置会恢复。`StatusBar.setBarStyle()` 和 `StatusBar.setHidden()` 以命令式方式设置基础值。

```tsx
import { View } from "nsh";
import { StatusBar } from "nsh/ui/mobile";

function PhotoViewer() {
  return (
    <View style={{ flex: 1, backgroundColor: "#000000" }}>
      <StatusBar barStyle="light-content" animated />
      …
    </View>
  );
}
```

在嵌入原生应用的场景下，iOS 上由宿主应用的视图控制器决定状态栏。在 `nsh.json` 中声明了 `"capabilities"` 的应用，需要 `"statusBar": true` 才能修改它。

## Platform.OS 与 Platform.select

`nsh/device` 中的 `Platform` 告诉你应用运行在哪里：`Platform.OS` 为 `"ios"`、`"android"`、`"macos"`、`"windows"` 或 `"linux"`（Mac Catalyst 为 `"ios"`，同时 `Platform.isMacCatalyst` 为真；[macOS 后端](./macos.md)为 `"macos"`，[Linux 后端](./linux.md)为 `"linux"`，[Windows 后端](./windows.md)为 `"windows"`，它们的元素会应用 `desktop` 属性）。`Version`、`isPad` 等见[设备与环境](./device-and-environment.md#平台)。

```tsx
import { Platform } from "nsh/device";

if (Platform.OS === "android") {
  // …
}
```

### Platform.select

`Platform.select(spec)` 返回当前平台对应的值。它依次查找该操作系统自己的键，然后是 `mobile`（iOS 和 Android）或 `desktop`（macOS、Windows、Linux），再是 `native`（所有平台），最后是 `default`：

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

`ios:` 和 `android:` 变体只在一个平台上应用对应的类：

```tsx
<View class="rounded-xl bg-white p-4 ios:shadow-md android:border android:border-zinc-200">…</View>
<Text class="text-base android:font-medium">Title</Text>
```

## 优先使用共享的行为

在添加平台判断之前，先想想这个差异对用户是否重要。元素和样式已经在应当不同的地方映射到各平台的原生行为（滚动物理效果、文字渲染、键盘、无障碍），在用户期望一致的地方（你的应用自己的外观）保持一致。平台判断最好留给用户依赖的平台惯例：Android 的返回键和 toast、iOS 的表单弹窗和大标题、自动填充的名称、水波纹与不透明度反馈。

## 相关内容

- [`nsh/ios`](../modules/nsh-ios.md) 与 [`nsh/android`](../modules/nsh-android.md)：参考。
- [`nsh/device`](../modules/nsh-device.md)：`Platform`。
- [原生组件](./native-components.md)：使用宿主应用在各平台上提供的视图。
- [Tailwind](./tailwind.md)：所有变体。
