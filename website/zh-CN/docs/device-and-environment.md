---
title: 设备与环境
description: 读取窗口尺寸、配色方案、字体缩放、安全区域、平台、像素密度和布局方向，并在它们变化时作出响应。
order: 47
group: ui
demo: guide-device
---

# 设备与环境

应用运行时，所处的环境会不断变化：窗口改变尺寸（旋转、分屏、Mac 窗口），用户切换到深色模式或更大的字号，键盘出现又消失。NSH 把这些都以**响应式访问器**的形式提供：像 signal 一样调用它，读取了它的东西（样式、文本、计算值）就会在它变化时更新。不需要添加或移除监听器。

| 访问器 | 模块 | 值 |
|---|---|---|
| `windowSize()` | `nsh/device` | surface 的 `{ width, height }`，单位为点。 |
| `colorScheme()` | `nsh/device` | `"light"` 或 `"dark"`。 |
| `fontScale()` | `nsh/device` | 用户偏好的文字大小相对于默认值的比例（1 为默认）。 |
| `safeArea()` | `nsh/ui/mobile` | surface 中未被系统 UI 遮挡的 inset：`{ top, right, bottom, left }`。 |
| `useSafeAreaInsets()` | `nsh/ui/mobile` | 调用组件渲染位置（页面、模态视图）的 inset。 |
| `keyboard()` | `nsh/ui/mobile` | 软键盘的 `{ height, duration }`。 |
| `I18nManager.isRTL` | `nsh/device` | 应用是否从右到左布局。 |
| `reduceMotion()`、`screenReaderEnabled()` | `nsh/accessibility` | 见[无障碍](./accessibility.md)。 |

```tsx
import { mount, Text, View } from "nsh";
import { colorScheme, fontScale, windowSize } from "nsh/device";
import { safeArea } from "nsh/ui/mobile";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: safeArea().top + 16, paddingHorizontal: 16, backgroundColor: colorScheme() === "dark" ? "#0d1117" : "#ffffff" }}>
      <Text style={{ color: colorScheme() === "dark" ? "#e6edf3" : "#1f2328" }}>
        {Math.round(windowSize().width)} × {Math.round(windowSize().height)} pt, text at {fontScale().toFixed(2)}×
      </Text>
    </View>
  );
}

mount(App);
```

## 窗口尺寸与响应式布局

`windowSize()` 是 NSH 渲染目标 surface 的尺寸：手机上是屏幕，iPad 分屏或 Mac 上是窗口，嵌入时是宿主视图。用它来改变布局，而不只是改变尺寸：

```tsx
import { View } from "nsh";
import { windowSize } from "nsh/device";
import { Image } from "nsh/image";

function Gallery() {
  const columns = () => (windowSize().width >= 700 ? 4 : windowSize().width >= 500 ? 3 : 2);
  return (
    <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 8 }}>
      <Each of={photos}>
        {(p) => <Image src={p().url} style={{ width: (windowSize().width - 32 - (columns() - 1) * 8) / columns(), aspectRatio: 1 }} />}
      </Each>
    </View>
  );
}
```

很多时候 flexbox、百分比和 `aspectRatio` 就能完成任务，根本不需要读取尺寸；见[布局](./layout.md)。使用 Tailwind 时，断点变体（`sm:`、`md:`、`lg:`、`xl:`、`2xl:`、`max-*:`、`min-[…]:`）和 `portrait:` / `landscape:` 同样跟随窗口：`class="grid grid-cols-2 md:grid-cols-4"`。

## 深色模式

`colorScheme()` 跟随系统外观。支持深色模式最简单的方法是 Tailwind 的 `dark:` 变体，它在 `colorScheme()` 为 `"dark"` 时生效：

```tsx
<View class="flex-1 bg-white dark:bg-zinc-900 p-4">
  <Text class="text-zinc-900 dark:text-zinc-100">Follows the system appearance</Text>
</View>
```

使用内联样式时，在样式中读取 `colorScheme()`；颜色较多时，先统一派生出一套调色板：

```tsx
import { computed, Text, View } from "nsh";
import { colorScheme } from "nsh/device";

const palette = computed(() =>
  colorScheme() === "dark"
    ? { background: "#0d1117", surface: "#161b22", text: "#e6edf3", muted: "#8b949e" }
    : { background: "#f6f8fa", surface: "#ffffff", text: "#1f2328", muted: "#57606a" },
);

<View style={{ flex: 1, backgroundColor: palette().background }}>
  <Text style={{ color: palette().text }}>Hello</Text>
</View>
```

`nsh/ui/mobile` 的组件从组件主题中取色，主题为每种颜色提供浅色和深色两个值（`nsh.json` 的 `"theme"`），所以它们也会随系统切换。要让状态栏与背景相配，见 [`StatusBar`](../components/status-bar.md)。

> [!TIP]
> 开发时两种外观都要测试：`xcrun simctl ui booted appearance dark` 可以切换 iOS 模拟器，Android 的快捷设置里有深色主题开关。变化会立即传到应用中，无需重新加载。

## 文字大小

`fontScale()` 是用户的文字大小设置（iOS 的动态字体，Android 的字体大小）相对于默认值的比例。文本和输入框会自动跟随：当用户要求更大的文字时，`fontSize: 16` 会渲染得更大。可以针对单个元素关闭或设置上限：

| 属性（`<Text>` 和 `<Input>`） | 默认值 | 说明 |
|---|---|---|
| `allowFontScaling` | `true` | 跟随系统文字大小。 |
| `maxFontSizeMultiplier` | `0`（无上限） | 应用于该文本的最大缩放。 |

```tsx
<Text style={{ fontSize: 34, fontWeight: "700" }} maxFontSizeMultiplier={1.4}>Today</Text>
```

对于不会自动跟随文字的布局，读取 `fontScale()` 来调整，例如文字变大时把横排改为竖排：`flexDirection: fontScale() > 1.3 ? "column" : "row"`。

## 安全区域

刘海、Home 指示条、状态栏、Android 导航栏以及 Mac Catalyst 的标题栏会遮挡 surface 的一部分。NSH 应用在所有平台上都是全屏绘制，被遮挡的部分以 inset 的形式提供。

- `safeArea()` 给出整个 surface 的 inset。
- `useSafeAreaInsets()` 给出**组件渲染位置**的 inset：在导航[页面](./navigation.md#标题栏下的安全区域)中从标题栏下方算起，在[模态视图](./navigation.md#模态视图)中是模态视图自己的。可能出现在多个地方的组件应该优先用它。
- `nsh/ui/mobile` 中的 `SafeAreaView` 是一个按这些 inset 添加内边距的视图（默认四边都加，或用 `edges={["top", "bottom"]}` 指定），叠加在它自己的内边距之上。

```tsx
import { View } from "nsh";
import { SafeAreaView, useSafeAreaInsets } from "nsh/ui/mobile";

function Screen() {
  return (
    <SafeAreaView edges={["top", "left", "right"]} style={{ flex: 1, backgroundColor: "#ffffff" }}>
      <Content />
      <BottomBar />
    </SafeAreaView>
  );
}

function BottomBar() {
  const insets = useSafeAreaInsets();
  return <View style={{ paddingBottom: insets().bottom + 8, paddingTop: 8, flexDirection: "row" }}>…</View>;
}
```

在 Android 上，嵌入在工具栏下方的视图只会得到与它重叠的那部分系统栏，与 UIKit 的安全区域一致：如果它没有延伸到状态栏，顶部 inset 就为 0。

## 平台

`nsh/device` 中的 `Platform` 描述应用运行的平台。它不是响应式的：这些信息在运行期间不会改变。

| 成员 | 说明 |
|---|---|
| `Platform.OS` | `"ios"`、`"android"`、`"macos"`、`"windows"` 或 `"linux"`。Mac Catalyst 报告 `"ios"`。 |
| `Platform.Version` | iOS 和桌面平台：版本字符串（`"17.5"`）；Android：API 级别（数字）。 |
| `Platform.isPad` | iPad（或 iPad 界面风格）。 |
| `Platform.isTV` | 电视。 |
| `Platform.isMacCatalyst` | Mac 界面风格的 Mac Catalyst。 |
| `Platform.isDesktop` | macOS、Windows 或 Linux。 |
| `Platform.constants` | `{ osVersion, interfaceIdiom }`，界面风格为 `"phone"`、`"pad"`、`"mac"`、`"tv"` 或 `"desktop"`。 |
| `Platform.select(spec)` | 当前平台对应的值；见[平台特定代码](./platform-specific.md#platformselect)。 |

```tsx
import { Platform } from "nsh/device";

const hapticsSupported = Platform.OS === "ios" || (Platform.OS === "android" && (Platform.Version as number) >= 30);
```

`nsh trace` 以 iOS 身份无界面运行。

在 [Linux](./linux.md) 上，`Platform.OS` 为 `"linux"`，界面风格为 `"desktop"`，`Platform.Version` 为内核的主、次版本号（`"6.12"`）：
各发行版给版本编号的方式无法相互比较。在那里，安全区域和键盘高度为 0，`colorScheme()` 实时跟随桌面的设置（XDG 设置门户），
`fontScale()` 跟随桌面的文字缩放，`PixelRatio.get()` 跟随显示器的缩放比例（包括小数缩放）。

在 [macOS](./macos.md)（AppKit 后端）上，`Platform.OS` 为 `"macos"`，界面风格为 `"desktop"`，
`Platform.Version` 为 macOS 的版本号。安全区域和键盘高度为 0，`colorScheme()` 实时跟随系统外观，`fontScale()` 为 1，
`PixelRatio.get()` 为显示器的缩放比例（backing scale）。

在 [Windows](./windows.md) 上，`Platform.OS` 为 `"windows"`，界面风格为 `"desktop"`，`Platform.Version` 为 `"10.0"`，Windows 11
上也是如此（Windows 沿用了这个版本号）。安全区域和键盘高度为 0，`colorScheme()` 实时跟随“设置 > 个性化 > 颜色”，`fontScale()`
跟随“文本大小”设置（1 到 2.25），`reduceMotion()` 跟随“动画效果”设置，`PixelRatio.get()` 为显示器的缩放比例，`Intl`
按用户的区域格式进行格式化。

## 像素密度

`nsh/device` 中的 `PixelRatio` 在点与设备像素之间换算。

| 方法 | 说明 |
|---|---|
| `PixelRatio.get()` | 每个点对应的设备像素数（大多数手机为 2 或 3）。 |
| `PixelRatio.getFontScale()` | 与 `fontScale()` 相同，但不是响应式的。 |
| `PixelRatio.getPixelSizeForLayoutSize(points)` | 把点换算为整数设备像素。 |
| `PixelRatio.roundToNearestPixel(points)` | 把点数取整到最近的设备像素。 |

一条一个设备像素高的细线：

```tsx
import { View } from "nsh";
import { PixelRatio } from "nsh/device";

const hairline = 1 / PixelRatio.get();
<View style={{ height: hairline, backgroundColor: "#d0d7de" }} />
```

`PixelRatio` 不是响应式的。窗口可能被移到另一块密度不同的屏幕上（接了外接显示器的 Mac），所以在同时读取 `windowSize()` 的组件中读取它，这种情况下 `windowSize()` 会变化。

## 从右到左

当设备语言是从右到左书写的（阿拉伯语、希伯来语、波斯语等），NSH 会从右到左布局应用：横排从右侧开始，`left` 和 `right` 样式属性互换，文字右对齐，原生控件镜像显示。`nsh/device` 中的 `I18nManager` 控制这一行为：

| 成员 | 说明 |
|---|---|
| `I18nManager.isRTL` | 应用是否从右到左布局（响应式）。 |
| `I18nManager.allowRTL(allow)` | 跟随从右到左的语言（默认 `true`）。 |
| `I18nManager.forceRTL(force)` | 无论语言如何都从右到左布局。 |
| `I18nManager.getConstants()` | `{ isRTL, doLeftAndRightSwapInRTL }`。 |

修改会立即生效，无需重启应用；本页的演示就在切换 `forceRTL`。

用逻辑属性编写两个方向都适用的布局：`marginStart` / `marginEnd`、`paddingStart` / `paddingEnd`、`start` / `end`、`borderTopStartRadius` 等，它们在从左到右布局中表示左和右，从右到左时则相反。`left` 和 `right` 本身也会互换。`direction` 样式（`"inherit"`、`"ltr"`、`"rtl"`）可以为子树覆盖方向，例如让电话号码或播放器控件保持从左到右。Tailwind 的 `rtl:` 和 `ltr:` 变体按方向生效。

在从右到左布局中，横向滚动视图和列表从最右端开始。在 Android 上，宿主应用必须在清单中声明 `android:supportsRtl="true"`，原生控件才会镜像。

## 相关内容

- [`nsh/device`](../modules/nsh-device.md)：参考。
- [平台特定代码](./platform-specific.md)：`Platform.select`、平台属性与模块。
- [无障碍](./accessibility.md)：屏幕阅读器与减少动态效果。
- [`SafeAreaView`](../components/safe-area-view.md) 与 [`StatusBar`](../components/status-bar.md)。
- [Tailwind](./tailwind.md)：`dark:`、断点与方向变体。
