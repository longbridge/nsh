---
title: nsh/native
description: nativeComponent：把宿主应用注册的原生视图变成带类型的组件。
order: 9
---

# nsh/native

`nsh/native` 把你的应用与宿主应用提供的原生视图连接起来。嵌入了 NSH 的 iOS 或 Android 应用可以按名称注册自己的视图（地图、图表、相机预览等）；`nativeComponent` 为其中一个视图生成带类型的组件。由于 NSH 使用真正的原生视图渲染，这样的组件和其他视图一样位于视图层级中：由 NSH 布局，随父视图裁剪和滚动，层叠关系也正确。

```ts
import { nativeComponent } from "nsh/native";
```

原生侧的注册方法见[原生组件](../docs/native-components.md)，以及 [iOS 嵌入](../docs/embedding-ios.md) / [Android 嵌入](../docs/embedding-android.md)。

## nativeComponent

```ts
function nativeComponent<P extends object = {}, E extends object = {}>(
  name: string,
  spec: NativeComponentSpec<P, E>,
): (props: NativeComponentProps<P, E>) => YNode;

interface NativeComponentSpec<P, E> {
  /** 组件自己的属性：发送给组件（任意 JSON 值），每个属性在变化时发送。 */
  props: readonly (keyof P & string)[];
  /** 组件会发出的事件。 */
  events?: readonly (keyof E & string)[];
}
```

返回一个组件，对应宿主应用以 `name` 注册的原生组件（Swift `NshAppView.register(component:)`，Java `NshView.registerComponent`）。

- `P` 是组件自身属性的类型。`spec.props` 中列出的每个属性变化时都会以 JSON 发送到原生侧；设为 `undefined` 会移除该属性。
- `E` 把事件名映射到事件数据。事件 `regionChange` 对应处理属性 `onRegionChange`。
- 组件还接受除 `children` 之外的所有[视图属性](../elements/view.md)：`style`、`class`、布局、无障碍、触摸处理和 `ref`。

```ts
type NativeComponentProps<P, E> = Omit<ViewProps, "children"> & P & NativeEventHandlers<E>;
type NativeEventHandlers<E> = { [K in keyof E & string as `on${Capitalize<K>}`]?: (data: E[K]) => void };
```

### 尺寸

组件像普通视图一样布局，尺寸由样式决定。样式没有确定的维度会使用原生组件报告的尺寸（就像图片使用其原始尺寸一样）。

### 命令

它的 `ref` 拿到一个 `YNode`；`ref.command(name, args)` 调用原生组件的 `name` 命令，参数 `args` 为任意 JSON 值：

```ts
map.command("zoomTo", { level: 3 });
```

### 没有注册时

如果 `name` 下没有注册任何组件（换了宿主应用、开发宿主、浏览器），组件会渲染为一个空视图，并输出一次警告。请让应用在没有它时仍然可用，或在使用前确认宿主环境。

## 示例

这里的徽标是宿主应用的原生视图，注册名为 `embed.badge`。NSH 负责布局，向它发送 `text` 和 `color`，接收它的 `tap` 事件，并调用它的 `pulse` 命令：

```tsx
import { mount, signal, type YNode } from "nsh";
import { nativeComponent } from "nsh/native";
import { SafeAreaView } from "nsh/ui/mobile";

const Badge = nativeComponent<{ text: string; color?: string }, { tap: { count: number } }>("embed.badge", {
  props: ["text", "color"],
  events: ["tap"],
});

function App() {
  const taps = signal(0);
  let badge: YNode | undefined;
  return (
    <SafeAreaView style={{ flex: 1, padding: 20, gap: 16 }}>
      <Badge
        ref={(node) => (badge = node)}
        text={taps() === 0 ? "Tap the native badge" : `Tapped ${taps()}×`}
        color="#8250df"
        onTap={(e) => {
          taps.set(e.count);
          badge?.command("pulse");
        }}
        style={{ alignSelf: "flex-start", borderRadius: 8, overflow: "hidden" }}
      />
    </SafeAreaView>
  );
}

mount(App);
```

> [!NOTE]
> 属性、事件和命令参数都以 JSON 形式传到原生侧。注册的组件是宿主应用自己的代码：NSH 的[沙箱限制与权限](../docs/sandbox-and-capabilities.md)不约束它们的行为。
