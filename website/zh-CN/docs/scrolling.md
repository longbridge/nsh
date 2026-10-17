---
title: 滚动
description: scroll 元素、分页与吸附、缩放、下拉刷新、滚动事件、用 scrollValue 实现滚动联动效果、滚动视图中的键盘，以及从代码滚动。
order: 39
group: ui
demo: guide-scrolling
---

# 滚动

`<Scroll>` 就是平台自己的滚动视图（iOS 上的 UIScrollView，Android 上的 ScrollView）：惯性滑动、回弹、滚动条、下拉刷新以及吸附都是原生实现。JS 通过事件得知滚动情况，也可以把滚动偏移绑定到[原生值](./animation.md#原生值)上，做出逐帧跟随手指的效果。

`<Scroll>` 适合尺寸已知、规模适中的内容：设置页、表单、文章、一排标签。长列表或无限数据请用 [`List`](./lists.md)，它会复用行，并接受同样的滚动属性。

## 滚动视图

`<Scroll>` 本身是视口，它的子元素是内容。视口需要父元素给出尺寸（通常是 `flex: 1`）；内容按视口的宽度（纵向）或高度（横向）排版，长度按需延伸。

```tsx
import { mount, Scroll, Text, View } from "nsh";

function App() {
  return (
    <Scroll style={{ flex: 1, backgroundColor: "#f6f8fa" }}>
      <View style={{ padding: 16, gap: 12 }}>
        <Each of={Array.from({ length: 50 }, (_, i) => i + 1)}>
          {(n) => (
            <View style={{ padding: 16, borderRadius: 10, backgroundColor: "#ffffff" }}>
              <Text>Row {n()}</Text>
            </View>
          )}
        </Each>
      </View>
    </Scroll>
  );
}

mount(App);
```

上面的子视图也可以交给滚动视图自己：`contentContainerStyle`（或 `contentContainerClass`）把子元素包进一个带该样式、随内容一起滚动的视图。`flexGrow: 1` 让它至少和视口一样高，内容较短时可以居中，较长时照常滚动。横向滚动视图使用 `horizontal`，并以一个横排视图作为子元素：

```tsx
<Scroll horizontal showsIndicator={false}>
  <View style={{ flexDirection: "row", gap: 8, paddingHorizontal: 16 }}>
    <Each of={["Today", "This week", "This month", "This year"]}>
      {(label) => <Text style={{ paddingHorizontal: 14, paddingVertical: 8, borderRadius: 16, overflow: "hidden", backgroundColor: "#ffffff" }}>{label()}</Text>}
    </Each>
  </View>
</Scroll>
```

在从右到左布局中（见[设备与环境](./device-and-environment.md#从右到左)），横向滚动视图从最右端开始，并保持与右端的距离。

### 行为属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `horizontal` | `boolean` | `false` | 横向滚动。 |
| `scrollEnabled` | `boolean` | `true` | 用户可以滚动。从代码滚动不受影响。 |
| `bounces` | `boolean` | `true` | 允许越过边缘回弹。 |
| `showsIndicator` | `boolean` | `true` | 显示滚动条。 |
| `contentInset` | `{ top?, right?, bottom?, left? }` | | 内容四周可以滚动进入的留白。 |
| `contentOffset` | `{ x?, y? }` | | 初始位置。 |
| `decelerationRate` | `"normal" \| "fast" \| number` | 平台默认 | 惯性滑动减速的快慢：`"normal"`、`"fast"`，或每毫秒保留的速度比例（`0.998` 为 normal，`0.99` 为 fast；`0` 表示平台默认）。 |

## 分页与吸附

吸附在手指抬起时由原生决定，依据松手位置、预计滑行距离和速度；随后滚动视图滑到选中的停靠点。整个过程不涉及 JS。

| 属性 | 类型 | 说明 |
|---|---|---|
| `pagingEnabled` | `boolean` | 停在视口尺寸的整数倍处。 |
| `snapToInterval` | `number` | 停在该长度的整数倍处（0 为关闭）。 |
| `snapToOffsets` | `number[]` | 停在这些偏移处。 |
| `snapToAlignment` | `"start" \| "center" \| "end"` | 配合 `snapToInterval`：条目的哪个位置与视口对齐。 |
| `disableIntervalMomentum` | `boolean` | 一次快速滑动最多移动一个停靠点。 |

一个分页器：每页与滚动视图等宽，滑行结束时更新页码指示器。

```tsx
import { mount, Scroll, signal, Text, View } from "nsh";

const colors = ["#0969da", "#8250df", "#bf3989", "#1a7f37"];

function Pager() {
  const width = signal(0);
  const page = signal(0);
  const settle = (e: { x: number }) => page.set(Math.round(e.x / width()));
  return (
    <View style={{ gap: 8, padding: 16 }}>
      <Scroll
        horizontal
        pagingEnabled
        showsIndicator={false}
        style={{ height: 160, borderRadius: 12 }}
        onLayout={(e) => width.set(e.width)}
        onScrollEnd={settle}
      >
        <View style={{ flexDirection: "row" }}>
          <Each of={colors}>
            {(color, i) => (
              <View style={{ width: width(), height: 160, backgroundColor: color(), alignItems: "center", justifyContent: "center" }}>
                <Text style={{ color: "#ffffff", fontSize: 24, fontWeight: "700" }}>Page {i() + 1}</Text>
              </View>
            )}
          </Each>
        </View>
      </Scroll>
      <View style={{ flexDirection: "row", justifyContent: "center", gap: 6 }}>
        <Each of={colors}>
          {(_, i) => <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: page() === i() ? "#0969da" : "#d0d7de" }} />}
        </Each>
      </View>
    </View>
  );
}

mount(Pager);
```

吸附到中间、每次滑动只移动一张的卡片轮播：

```tsx
<Scroll
  horizontal
  snapToInterval={232}          // 卡片宽度 (220) + 间距 (12)
  snapToAlignment="center"
  decelerationRate="fast"
  disableIntervalMomentum
  showsIndicator={false}
>
  <View style={{ flexDirection: "row", gap: 12, paddingHorizontal: 16 }}>…</View>
</Scroll>
```

`List` 接受同样的吸附属性，长轮播用它更合适。

## 滚动事件

| 属性 | 参数 | 触发时机 |
|---|---|---|
| `onScroll` | `ScrollEvent` | 偏移改变。每帧最多一次；`scrollEventThrottle` 可以进一步拉开间隔。 |
| `scrollEventThrottle` | `number` | 两次 `onScroll` 之间的最少毫秒数。 |
| `onScrollBeginDrag` | `{ x, y }` | 用户开始拖动。 |
| `onScrollEndDrag` | `{ x, y }` | 用户抬起手指。 |
| `onMomentumScrollBegin` | `{ x, y }` | 快速滑动后的惯性滑行开始。 |
| `onMomentumScrollEnd` | `{ x, y }` | 惯性滑行结束（被新的拖动打断时也会触发）。 |
| `onScrollEnd` | `{ x, y }` | 用户的滚动停了下来：惯性滑行结束后，或没有惯性的拖动结束时。 |

`ScrollEvent` 包含 `x`、`y`（偏移）、`contentWidth`、`contentHeight`（内容尺寸，含缩放）、`width`、`height`（视口）以及 `zoomScale`。

```tsx
const atBottom = signal(false);

<Scroll
  style={{ flex: 1 }}
  scrollEventThrottle={100}
  onScroll={(e) => atBottom.set(e.y + e.height >= e.contentHeight - 20)}
>
  …
</Scroll>
```

> [!TIP]
> `onScroll` 会在滚动的每一帧运行 JS（受节流限制）。如果只是*看起来*要跟随滚动，比如渐隐的标题栏或视差图片，请改用 [`scrollValue`](#滚动联动效果scrollvalue)：它完全不消耗 JS。

## 滚动联动效果：scrollValue

`scrollValue` 接收一个[原生值](./animation.md#原生值)，并在内容移动的同一帧，由原生把（主轴方向的）滚动偏移写进去。把这个值，或由它经 `interpolate`、`diffClamp`、算术运算得到的节点，绑定到样式上。

```tsx
import { mount, Scroll, Text, View } from "nsh";
import { createNativeValue } from "nsh/animated";

function App() {
  const offset = createNativeValue(0);
  return (
    <View style={{ flex: 1 }}>
      <View
        style={{
          height: 56,
          justifyContent: "center",
          paddingHorizontal: 16,
          // 顶部透明，滚过 80 点后变为白色。
          backgroundColor: offset.interpolate({ inputRange: [0, 80], outputRange: ["rgba(255,255,255,0)", "#ffffff"] }),
        }}
      >
        <Text
          style={{
            fontSize: 22,
            fontWeight: "700",
            transformOrigin: "left center",
            transform: [{ scale: offset.interpolate({ inputRange: [0, 80], outputRange: [1, 0.8] }) }],
          }}
        >
          Inbox
        </Text>
      </View>
      <Scroll style={{ flex: 1 }} scrollValue={offset}>
        <View style={{ padding: 16, gap: 8 }}>…</View>
      </Scroll>
    </View>
  );
}

mount(App);
```

内容上滑时隐藏、一往下滑就重新出现的标题栏，就是对偏移做 `diffClamp`：

```tsx
const offset = createNativeValue(0);
const header = offset.diffClamp(0, 56);

<View style={{ flex: 1 }}>
  <Scroll style={{ flex: 1 }} scrollValue={offset}>
    <View style={{ paddingTop: 56 }}>…</View>
  </Scroll>
  <View style={{ position: "absolute", top: 0, left: 0, right: 0, height: 56, transform: [{ translateY: header.mul(-1) }] }}>…</View>
</View>
```

插值在输入范围之外默认会截断（clamp），所以下拉越过顶部（偏移为负）时效果不会超出终点；想做随下拉放大的弹性头图，传入 `extrapolate: "extend"`。

## 下拉刷新

`refreshing` 和 `onRefresh` 会加上平台的下拉刷新控件。`refreshing` 为 true 时控件显示；在 `onRefresh` 中把它设为 true，工作完成后再清除。

```tsx
const refreshing = signal(false);
const reload = async () => {
  refreshing.set(true);
  try {
    items.set(await (await fetch("https://api.example.com/items")).json());
  } finally {
    refreshing.set(false);
  }
};

<Scroll style={{ flex: 1 }} refreshing={refreshing()} onRefresh={reload}>…</Scroll>
```

iOS 上的控件是 UIRefreshControl；Mac Catalyst 的 Mac 界面风格没有 UIRefreshControl，NSH 会显示自己的加载指示器，行为相同。`List` 也接受这两个属性。

## 缩放

当 `maximumZoomScale` 大于 `minimumZoomScale` 时，`<Scroll>`（`List` 不行）就可以缩放：捏合会缩放内容，放大后可以双向滚动。

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `minimumZoomScale` | `number` | `1` | 最小缩放。 |
| `maximumZoomScale` | `number` | `1` | 最大缩放；大于最小值时捏合即可缩放。 |
| `zoomScale` | `number` | | 该属性改变时缩放到这个比例；否则保留用户的缩放。 |
| `bouncesZoom` | `boolean` | `true` | 捏合可以越过限制并弹回。 |
| `pinchGestureEnabled` | `boolean` | `true` | 用户可以捏合缩放。 |
| `centerContent` | `boolean` | `false` | 比视口小的内容（缩小时）居中显示。 |

`onScroll` 会报告 `zoomScale` 和缩放后的内容尺寸。在代码中，`ref.zoomTo({ scale, x, y, animated })` 会缩放，使内容中的点（`x`、`y`，未缩放的点坐标）落在视口中央；双击放大就是在按压处理函数里调用它：

```tsx
import { mount, Scroll, signal, type YNode } from "nsh";
import { Image } from "nsh/image";

function Photo() {
  let scroll!: YNode;
  const scale = signal(1);
  return (
    <Scroll
      ref={scroll}
      style={{ flex: 1, backgroundColor: "#000000" }}
      minimumZoomScale={1}
      maximumZoomScale={4}
      centerContent
      onScroll={(e) => scale.set(e.zoomScale)}
    >
      <Image
        src="https://picsum.photos/id/1015/1200/800"
        style={{ width: 390, height: 260 }}
        onPress={() => scroll.zoomTo({ scale: scale() > 1.5 ? 1 : 3 })}
      />
    </Scroll>
  );
}

mount(Photo);
```

在 iOS 上，缩放结束后，缩放视图里的文字和画布会按新的分辨率重新绘制，保持清晰。Android 没有自带可缩放的滚动视图：NSH 会把滚动视图切换为自己的实现，围绕捏合焦点缩放，支持任意手指数平移、惯性滑动，并在边缘和缩放极限处产生橡皮筋效果。与系统的手势检测一致，Android 要等两指间距达到约 27 mm 才开始捏合。

## 从代码滚动

给滚动视图设置 [`ref`](./jsx-and-components.md) 并调用它的方法。命令会在同一次更新的其他改动之后执行，所以滚动到刚刚添加的内容也没问题。

| 方法 | 说明 |
|---|---|
| `scrollTo({ x?, y?, animated? })` | 滚动到某个偏移。默认带动画。 |
| `zoomTo({ scale, x?, y?, animated? })` | 可缩放的滚动视图：缩放到 `scale`（限制在范围内），以内容点（`x`、`y`）或中心为基准。 |

```tsx
let scroll!: YNode;

<Scroll ref={scroll} style={{ flex: 1 }}>…</Scroll>
<View onPress={() => scroll.scrollTo({ y: 0 })}>…</View>
```

列表另外还有 `scrollToIndex`、`scrollToOffset` 和 `scrollToEnd`；见[列表](./lists.md#从代码滚动)。

## 滚动视图与键盘

NSH 会自动让获得焦点的输入框保持可见：键盘出现时，输入框所在的滚动视图会得到与键盘等高的底部 inset，并把输入框滚动到可见位置。这是原生完成的，应用中不需要任何代码。只有滚动视图之外的内容（例如固定在底部的消息栏）才需要 [`KeyboardAvoidingView`](./forms-and-keyboard.md#滚动视图之外的内容keyboardavoidingview)。

有两个属性决定滚动和点击如何与键盘交互：

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `keyboardDismissMode` | `"none" \| "on-drag" \| "interactive"` | `"none"` | 拖动内容时是否收起键盘。iOS 上 `"interactive"` 让键盘随手指下移；Android 上它与 `"on-drag"` 相同。 |
| `keyboardShouldPersistTaps` | `"never" \| "always" \| "handled"` | `"never"` | 键盘弹出时点击的行为。 |

`keyboardShouldPersistTaps`：

- `"never"`：点击输入框之外只会收起键盘，按钮收不到这次点击。用户要先点一次收起键盘，再点一次才能按下。
- `"handled"`：点在会处理它的东西（按钮）上时，点击照常传递，键盘保持；点在空白处则收起键盘。表单通常应选它。
- `"always"`：点击从不收起键盘。

两者都在点击或拖动发生时由原生决定，不等待 JS。要从代码收起键盘，调用 `nsh/ui/mobile` 中的 `Keyboard.dismiss()`。

## 嵌套滚动视图

纵向滚动视图里放横向滚动视图会如预期工作：各自接管自己方向上的拖动。同方向的两个滚动视图也可以嵌套（内层先滚动）。需要独占手指的 JS 响应者，比如滚动视图里的绘图画布，在 `onResponderGrant` 中返回 `true`，以阻止外层滚动视图接管触摸；见[触摸与手势](./touch-and-gestures.md#手势响应者系统)。

## 相关内容

- [`Scroll`](../elements/scroll.md)：元素参考，包含所有属性和事件。
- [列表](./lists.md)：使用相同滚动属性的虚拟化列表。
- [动画](./animation.md)：用于滚动联动效果的原生值、`interpolate` 和 `diffClamp`。
- [表单与键盘](./forms-and-keyboard.md)：输入框、焦点和键盘。
