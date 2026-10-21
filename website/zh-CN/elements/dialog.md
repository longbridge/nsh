---
title: Dialog
description: Dialog 组件：在所有内容之上的模态界面，用于全屏流程、sheet 和由应用设定样式的对话框。
order: 11
---

# Dialog

Dialog 在应用其余内容之上、在它自己的原生窗口层中呈现内容：iOS 和 Mac Catalyst 上是一个被 present 的 view
controller，Android 上是一个 Dialog，桌面平台和 Web 上是覆盖整个窗口的一层。它用于全屏流程、sheet 和对话框。系统自带的手势（下滑关闭
sheet、Android 返回键、Escape）都能正常工作，并在原生侧决定。

Dialog 负责行为，外观交给应用：`class` 和 `style` 设定弹出层（包含子元素的视图）的样式，`backdropClass` /
`backdropStyle` 在它后面加一层背景。

```tsx
import { signal, Text, View } from "nsh";
import { Button, Dialog, SafeAreaView } from "nsh/base";

function Example() {
  const open = signal(false);
  return (
    <View style={{ flex: 1, justifyContent: "center", padding: 24 }}>
      <Button onPress={() => open.set(true)} class="items-center rounded-lg bg-blue-600 px-4 py-3 active:opacity-70">
        <Text class="font-semibold text-white">Open</Text>
      </Button>
      <Dialog open={open()} presentation="pageSheet" onOpenChange={(next) => open.set(next)} class="flex-1 bg-white">
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12 }}>
          <Text style={{ fontSize: 24, fontWeight: "700" }}>A sheet</Text>
          <Button onPress={() => open.set(false)} class="items-center rounded-lg bg-blue-600 px-4 py-3 active:opacity-70">
            <Text class="font-semibold text-white">Close</Text>
          </Button>
        </SafeAreaView>
      </Dialog>
    </View>
  );
}
```

`Dialog` 从 [`nsh/base`](../modules/nsh-base.md#dialog) 导入。`open` 不为 `false` 时显示，变为 `false` 时关闭；它还会给内容提供
dialog 的安全区域 inset，供其内部的 `SafeAreaView` 和 `safeAreaInsets()` 读取。`Button` 同样来自 `nsh/base`，是由应用设定样式的按压目标。

## 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `open` | `boolean` | `true` | 为 true 时显示。 |
| `animation` | `"slide" \| "fade" \| "none"` | `"slide"` | 出现和离开的方式。 |
| `presentation` | `"fullScreen" \| "pageSheet" \| "formSheet"` | `"fullScreen"` | 在 iOS 上的形式：`fullScreen` 以透明背景覆盖整个屏幕；`pageSheet` 和 `formSheet` 是系统 sheet，下面的内容保持可见（在大屏上 `formSheet` 是一个更小的居中 sheet）。 |
| `closeOnEscape` | `boolean` | `true` | Escape 和 Android 返回键在原生侧关闭它。 |
| `closeOnBackdropPress` | `boolean` | `true` | 按下背景层时关闭它。 |
| `closeOnSwipe` | `boolean` | `true` | 下滑 sheet（iOS、手机浏览器）时在原生侧关闭它。 |
| `statusBar` | `{ foreground?, hidden?, animated? }` | 无 | iOS 和 Android：对话框打开期间的状态栏，位于页面的设置之上（[状态栏](../modules/nsh-base.md#status-bar)）。 |
| `onOpen` | `() => void` | | 已完全出现。 |
| `onClose` | `() => void` | | 已离开：用户关闭了它，或应用移除了它。 |
| `onCancel` | `() => void` | | 用户尝试关闭它（Escape、返回、按下背景层、下滑），无论 `closeOn*` 是否允许关闭。 |
| `onOpenChange` | `(open: boolean, reason: "cancel" \| "backdropPress" \| "swipe") => void` | | 用户关闭了它：在这里同步 `open`。 |
| `class`、`style` | `string`、`StyleProp<ViewStyle>` | | 弹出层：包含子元素的视图。 |
| `backdropClass`、`backdropStyle` | `string`、`StyleProp<ViewStyle>` | | 铺满 dialog、位于弹出层后面的背景层，设置其中之一时出现。 |
| `children` | `JSX.Element` | | 内容。 |

| `ios` 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `supportedOrientations` | `("portrait" \| "portrait-upside-down" \| "landscape" \| "landscape-left" \| "landscape-right")[]` | 与呈现者相同 | dialog 允许的方向。 |

| `android` 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `statusBarTranslucent` | `boolean` | `false` | 内容延伸到状态栏下方；否则从状态栏下方开始。 |

<a id="opening-and-closing"></a>

## 打开与关闭

`open` 是应用的状态。用户关闭 dialog（Escape、Android 返回键、按下背景层、下滑 sheet）时，它在原生侧关闭，并通过
`onOpenChange(false, reason)` 通知应用，由应用把状态改成一致。`open` 一直为 `true` 时 dialog 保持关闭，`open` 从
`false` 变为 `true` 时再次打开。

由于主线程从不等待 JS，每个 `closeOn*` 标志都在用户操作时由原生侧读取。要在关闭前先确认，把这些标志设为
`false`：此时用户的尝试会调用 `onCancel`，由应用自己把 `open` 设为 `false` 来关闭：

```tsx
<Dialog
  open={open()}
  presentation="pageSheet"
  closeOnEscape={false}
  closeOnBackdropPress={false}
  closeOnSwipe={false}
  onCancel={() => askToDiscard.set(true)}
  class="flex-1 bg-white"
>
  …
</Dialog>
```

每次尝试都会调用 `onCancel`，无论这些标志是否允许关闭。

<a id="a-dialog-with-a-backdrop"></a>

## 带背景层的对话框

默认 `fullScreen` 形式的 dialog 显示应用绘制的内容：这里是一层变暗的背景和居中的卡片。按下背景层即关闭（`closeOnBackdropPress`）：

```tsx
<Dialog
  open={confirm()}
  animation="fade"
  onOpenChange={(next) => confirm.set(next)}
  backdropClass="bg-black/40"
  class="m-auto w-80 gap-3 rounded-2xl bg-white p-5"
>
  <Text style={{ fontSize: 20, fontWeight: "700" }}>Delete this item?</Text>
  <Button onPress={remove} class="items-center rounded-lg bg-red-600 px-4 py-3 active:opacity-70">
    <Text class="font-semibold text-white">Delete</Text>
  </Button>
</Dialog>
```

如果需要系统提示框或操作表，请使用 `nsh/base` 的 [`AlertDialog`](../modules/nsh-base.md#alertdialog)。

<a id="ref-methods"></a>

## Ref 方法

无。

<a id="platform-notes"></a>

## 平台说明

- iOS 和 Mac Catalyst：`presentation` 决定形式。`fullScreen` 以透明背景覆盖整个屏幕；`pageSheet` 和 `formSheet`
  是使用系统背景的系统 sheet，下滑时跟随手指（`closeOnSwipe`）。
- Android：dialog 铺满窗口，显示应用的弹出层和背景层。返回键首先交给最上层的 dialog，然后才是导航栈和 `BackHandler` 监听器。
- 桌面平台（macOS、Windows、Linux、GPUI）和 Web：dialog 是覆盖整个窗口的透明层，显示应用的弹出层和背景层。Escape 是一次关闭尝试。
- `supportedOrientations` 属于 iOS，`statusBarTranslucent` 属于 Android。未设置 `statusBarTranslucent` 时，Android
  dialog 的内容从状态栏下方开始。
