---
title: Scroll
description: 原生滚动视图：纵向和横向滚动、分页与吸附、下拉刷新、缩放以及滚动事件。
order: 5
demo: el-scroll
---

# Scroll

`<Scroll>` 是原生滚动视图（iOS 和 Mac Catalyst 上是 UIScrollView，Android 上是 ScrollView）。它把所有子元素作为内容进行布局，并在原生侧滚动：滚动、惯性滑动、回弹和吸附都不需要等待 JS。

```tsx
<Scroll style={{ flex: 1 }}>
  <View style={{ padding: 16, gap: 8 }}>
    <Each of={rows}>{(row) => <Text>{row().title}</Text>}</Each>
  </View>
</Scroll>
```

`<Scroll>` 会创建所有子元素，适合中等规模的页面。对于很长或数量不定的数据，请使用[列表](./list.md)：它只创建视口附近的行，并复用它们。

滚动视图在滚动方向上必须有确定的尺寸：在有尺寸的容器中给它 `flex: 1`（或 `height`）。其内容按滚动视图的宽度（`horizontal` 时为高度）布局，高度（宽度）按需增长。

## 属性

除[通用属性](./index.md)外（`style` 的类型为 `ViewStyle`）：

### 滚动

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `horizontal` | `boolean` | `false` | 横向滚动。 |
| `scrollEnabled` | `boolean` | `true` | 用户可以滚动。 |
| `showsIndicator` | `boolean` | `true` | 显示滚动条。 |
| `bounces` | `boolean` | `true` | 允许越过边缘回弹。 |
| `pagingEnabled` | `boolean` | `false` | 停在视口尺寸的整数倍处。 |
| `snapToInterval` | `number` | `0` | 停在该长度的整数倍处（0 表示关闭）。 |
| `snapToOffsets` | `number[]` | | 停在这些偏移量处。 |
| `snapToAlignment` | `"start" \| "center" \| "end"` | `"start"` | 使用 `snapToInterval` 时，条目的哪个位置与视口对齐。 |
| `disableIntervalMomentum` | `boolean` | `false` | 一次惯性滑动最多移动一个吸附点。 |
| `decelerationRate` | `"normal" \| "fast" \| number` | 平台默认 | 惯性滑动减速的快慢：每毫秒保留的速度比例（`normal` 为 0.998，`fast` 为 0.99）。 |
| `contentInset` | `{ top?, right?, bottom?, left? }` | | 内容四周的留白。 |
| `contentOffset` | `{ x?, y? }` | | 初始位置。 |
| `keyboardDismissMode` | `"none" \| "on-drag" \| "interactive"` | `"none"` | 拖动时是否收起键盘。 |
| `keyboardShouldPersistTaps` | `"never" \| "always" \| "handled" \| boolean` | `"never"` | 点击是否收起键盘：`never`（点击只收起键盘）、`always`（键盘保持）、`handled`（点击被某个元素处理时键盘保持）。 |
| `scrollValue` | `NativeValue` | | 接收滚动偏移量（主轴方向）的原生值，用于随滚动变化的动画。 |
| `scrollEventThrottle` | `number` | | 两次 `onScroll` 事件之间的最小毫秒数（无论如何每帧最多一次）。 |

### 下拉刷新

| 属性 | 类型 | 说明 |
|---|---|---|
| `refreshing` | `boolean` | 显示刷新指示器。 |
| `onRefresh` | `() => void` | 用户下拉刷新：开始加载，并在结束前保持 `refreshing` 为真。 |

```tsx
const refreshing = signal(false);
async function reload() {
  refreshing.set(true);
  await load();
  refreshing.set(false);
}

<Scroll style={{ flex: 1 }} refreshing={refreshing()} onRefresh={reload}>
  …
</Scroll>;
```

### 缩放

把 `maximumZoomScale` 设得大于 `minimumZoomScale`，内容就可以双指捏合缩放；放大后可以双向滚动。列表不支持缩放。

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `minimumZoomScale` | `number` | `1` | 最小缩放比例。 |
| `maximumZoomScale` | `number` | `1` | 最大缩放比例。 |
| `zoomScale` | `number` | | 值变化时缩放到该比例；否则保留用户的缩放。 |
| `bouncesZoom` | `boolean` | `true` | 捏合可以超出限制并弹回。 |
| `pinchGestureEnabled` | `boolean` | `true` | 用户可以捏合缩放。 |
| `centerContent` | `boolean` | `false` | 内容（缩小后）小于视口时居中显示。 |

```tsx
let photo!: YNode;
<Scroll ref={photo} maximumZoomScale={4} centerContent style={{ flex: 1, backgroundColor: "#000" }}>
  <Image src={picture} style={{ width: 390, height: 520 }} onPress={() => photo.zoomTo({ scale: 1, animated: true })} />
</Scroll>;
```

## 事件

| 事件 | 载荷 | 触发时机 |
|---|---|---|
| `onScroll` | `ScrollEvent` | 偏移量变化（每帧最多一次，且不比 `scrollEventThrottle` 更频繁）。 |
| `onScrollBeginDrag` / `onScrollEndDrag` | `{ x, y }` | 用户开始 / 停止拖动。 |
| `onMomentumScrollBegin` / `onMomentumScrollEnd` | `{ x, y }` | 拖动后的惯性滑动开始 / 停止。被新一次拖动打断的滑动同样以 `onMomentumScrollEnd` 结束。 |
| `onScrollEnd` | `{ x, y }` | 滚动停了下来。 |
| `onRefresh` | 无 | 触发了下拉刷新。 |

`ScrollEvent` 为 `{ x, y, contentWidth, contentHeight, width, height, zoomScale }`：偏移量、（缩放后的）内容尺寸、视口尺寸和缩放比例（不缩放时为 1）。

> [!TIP]
> 要让某个元素随滚动位置移动（收缩的标题栏、视差图片），请把 `scrollValue` 绑定到一个原生值，并在样式中对它插值。它在 UI 线程上以屏幕刷新率运行，而 `onScroll`（每帧一个 JS 事件）无法保证这一点。参见[动画](../docs/animation.md)。

## ref 方法

| 方法 | 说明 |
|---|---|
| `scrollTo({ x?, y?, animated? })` | 滚动到某个偏移量。 |
| `zoomTo({ scale, x?, y?, animated? })` | 缩放到 `scale`（限制在上下限内），使内容中的点（`x`、`y`，未缩放时的点坐标）位于视口中央，未给出时围绕中央缩放。例如双击放大被点击的位置。 |

```tsx
let scroller!: YNode;
<Scroll ref={scroller}>…</Scroll>;
scroller.scrollTo({ y: 0, animated: true });
```

## 键盘

滚动视图中的输入框会在原生侧自动避开键盘：滚动视图获得底部内边距，并滚动到获得焦点的输入框，全程不经过 JS。对于滚动视图之外的内容，请使用 [`nsh/ui/mobile`](../components/keyboard-avoiding-view.md) 的 `KeyboardAvoidingView`。参见[表单与键盘](../docs/forms-and-keyboard.md)。

## 从右到左

在从右到左的布局中，横向滚动视图从右端开始，并在内容变化时保持与右端的距离；其子元素会被镜像。

## 平台说明

- Android：下拉刷新由 NSH 自行绘制（越界回弹效果仍是原生的），`contentInset` 以内边距的方式实现，`decelerationRate` 只影响吸附时的滑动。
- Android 没有可缩放的滚动视图：NSH 会切换到自己的实现，围绕内容左上角缩放并支持双向滚动。双指间距超过 Android 的最小距离（约 27 mm）后才开始捏合。
- Mac Catalyst（Mac 界面风格）没有系统刷新控件；NSH 会自行检测下拉并显示加载指示器。
- iOS 在缩放结束后会按缩放后的分辨率重绘文本和画布。
