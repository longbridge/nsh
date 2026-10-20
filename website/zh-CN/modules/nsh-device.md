---
title: nsh/device
description: 窗口尺寸、配色方案、字体缩放、平台信息、像素密度和从右到左布局。
order: 6
---

# nsh/device

`nsh/device` 向应用提供设备及其环境的信息。`windowSize`、`colorScheme`、`fontScale` 和 `locale` 是响应式的 accessor：在 JSX、`computed` 或 `effect` 中读取它们，平台报告变化（旋转、窗口缩放、切换深色模式、修改文字大小或语言）时，读取它们的地方会随之更新。

```ts
import { colorScheme, fontScale, I18nManager, locale, PixelRatio, Platform, windowSize } from "nsh/device";
```

教程见[设备与环境](../docs/device-and-environment.md)。

## windowSize

```ts
const windowSize: Accessor<{ width: number; height: number }>;
```

界面的尺寸（点）：窗口，或嵌入时的 `NshAppView` / `NshView`。响应式。

```tsx
const wide = () => windowSize().width >= 600;

<View style={{ flexDirection: wide() ? "row" : "column" }}>{/* ... */}</View>
```

## colorScheme

```ts
const colorScheme: Accessor<"light" | "dark">;
```

系统外观。响应式。Tailwind 的 `dark:` 变体也跟随它（见 [Tailwind](../docs/tailwind.md)）。

```tsx
const dark = () => colorScheme() === "dark";

<View style={{ flex: 1, backgroundColor: dark() ? "#000" : "#fff" }} />
```

## fontScale

```ts
const fontScale: Accessor<number>;
```

用户偏好的文字大小相对于默认大小的比例（`1` 为默认；iOS 上是动态字体，Android 上是字体大小设置），或嵌入它的应用设置的文字大小。响应式。除非设置了 `allowFontScaling={false}`，文本本身已经会随之缩放；当其他尺寸（例如文字旁边的图标）也需要跟随时再读取它。

## locale

```ts
const locale: Accessor<string>;
```

用户的区域设置，BCP 47 标签（`"en-US"`、`"zh-Hans-CN"`），或嵌入它的应用设置的区域设置。响应式。`Intl` 默认按它格式化；读取它来选择你自己的翻译文案（[设备与环境](../docs/device-and-environment.md#语言和地区)）。

## Platform

```ts
const Platform: {
  readonly OS: "ios" | "android" | "macos" | "windows" | "linux" | "web";
  readonly isDesktop: boolean;
  readonly Version: string | number;
  readonly isPad: boolean;
  readonly isTV: boolean;
  readonly isMacCatalyst: boolean;
  readonly constants: {
    readonly osVersion: string | number;
    readonly interfaceIdiom: "phone" | "pad" | "mac" | "tv" | "desktop";
  };
  select<T>(spec: PlatformSelectSpec<T>): T | undefined;
};
```

应用所运行的平台，运行期间不变。

| 成员 | 说明 |
|---|---|
| `OS` | 操作系统。iOS、iOS 模拟器和 Mac Catalyst 都报告 `"ios"`，浏览器报告 `"web"`。 |
| `isDesktop` | 是否为桌面系统（macOS、Windows、Linux），或有鼠标且窗口较宽的浏览器。 |
| `Version` | iOS 和桌面系统：版本字符串（`"17.5"`）；Android：API 级别（数字）。 |
| `isPad` | 平板：iPad 或 Android 平板。 |
| `isTV` | 电视（Apple TV 形态、Android TV）。 |
| `isMacCatalyst` | 以 Mac 形态运行的 Mac Catalyst。 |
| `constants.osVersion` | 与 `Version` 相同。 |
| `constants.interfaceIdiom` | `"phone"`、`"pad"`、`"mac"`、`"tv"` 或 `"desktop"`。 |
| `select(spec)` | 当前平台对应的值（见下文）。 |

### Platform.select

```ts
interface PlatformSelectSpec<T> {
  ios?: T;
  android?: T;
  macos?: T;
  windows?: T;
  linux?: T;
  mobile?: T;   // iOS 和 Android
  desktop?: T;  // macOS、Windows 和 Linux
  native?: T;   // 所有平台
  default?: T;
}
```

依次返回当前操作系统（`ios`、`android`、`web` 等）对应的值，否则 `mobile` / `desktop`，否则 `native`（除 Web 外的所有平台），否则 `default`，都没有则返回 `undefined`。在浏览器中，`mobile` 还是 `desktop` 由界面风格决定。

```ts
const shadow = Platform.select({
  ios: { shadowColor: "#000", shadowOpacity: 0.15, shadowRadius: 8 },
  android: { elevation: 4 },
});
```

> [!TIP]
> 对于只有某个平台才有的属性，优先使用元素和组件的 `ios={{…}}` / `android={{…}}` 属性，而不是按 `Platform` 分支（见[平台专有代码](../docs/platform-specific.md)）。

## PixelRatio

```ts
const PixelRatio: {
  get(): number;
  getFontScale(): number;
  getPixelSizeForLayoutSize(size: number): number;
  roundToNearestPixel(size: number): number;
};
```

每个点对应的设备像素数。

| 方法 | 说明 |
|---|---|
| `get()` | 每点的设备像素数（大多数 iPhone 上为 `3`）。 |
| `getFontScale()` | 与 `fontScale()` 相同，但不追踪。 |
| `getPixelSizeForLayoutSize(size)` | `size` 个点对应的整数设备像素数。 |
| `roundToNearestPixel(size)` | 把 `size` 取整到最近的设备像素，单位仍为点。 |

```ts
const hairline = 1 / PixelRatio.get();
```

`PixelRatio` 不是响应式的。窗口可能被移动到另一块屏幕上（Mac Catalyst 或外接显示器）；请在同时读取了 `windowSize()` 的组件中读取它，这样窗口变化时组件会重新运行。

## I18nManager

```ts
const I18nManager: {
  isRTL: boolean;                  // 响应式
  doLeftAndRightSwapInRTL: true;
  allowRTL(allow: boolean): void;
  forceRTL(force: boolean): void;
  getConstants(): { isRTL: boolean; doLeftAndRightSwapInRTL: boolean };
};
```

从右到左布局。在从右到左布局中，行从右侧开始排列，`left` / `right` 样式属性（`marginLeft`、`paddingRight` 等）表示起始端和结束端。修改即时生效，无需重启。

| 成员 | 说明 |
|---|---|
| `isRTL` | 应用是否从右到左布局。读取它会被追踪。 |
| `doLeftAndRightSwapInRTL` | 始终为 `true`：从右到左布局时 `left` 和 `right` 互换。 |
| `allowRTL(allow)` | 跟随从右到左书写的语言（默认 `true`）。 |
| `forceRTL(force)` | 无论语言如何都从右到左布局。 |
| `getConstants()` | `{ isRTL, doLeftAndRightSwapInRTL }`。 |

```tsx
<Text onPress={() => I18nManager.forceRTL(!I18nManager.isRTL)}>
  {I18nManager.isRTL ? "Right to left" : "Left to right"}
</Text>
```

## 相关内容

- 安全区域和键盘（`safeArea()`、`keyboard()`、`useSafeAreaInsets()`）属于 [`nsh/ui/mobile`](../components/index.md)。
- 屏幕阅读器和减弱动态效果的设置在 [`nsh/accessibility`](./nsh-accessibility.md) 中。
