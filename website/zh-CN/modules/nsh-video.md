---
title: Video
description: Video：原生视频播放器（AVPlayer、MediaPlayer），以及它的属性、事件和 ref 方法。
order: 2.4
---

# Video

`nsh` 导出 `Video`：基于平台播放器构建的视频播放器（iOS 和 Mac Catalyst 上是 `AVPlayer`，Android 上是 `MediaPlayer`）。播放由属性控制，所以暂停只需改一个 signal。

```ts
import { Video, type VideoProps } from "nsh";
```

教程见[网页视图与视频](../docs/web-view-and-video.md)，属性见 [`<Video>`](../elements/video.md)。

## Video

```ts
const Video: (props: VideoProps) => YNode;

interface VideoProps extends Omit<ViewProps, "children">, VideoHostProps {
  src: string;
}
```

```tsx
import { mount, signal, Text, Video, View, type YNode } from "nsh";

function Player() {
  const paused = signal(true);
  const time = signal("");
  let video!: YNode;
  return (
    <View style={{ flex: 1, padding: 16, gap: 12 }}>
      <Video
        ref={video}
        src="https://example.com/clip.mp4"
        paused={paused()}
        muted
        repeat
        resizeMode="cover"
        style={{ height: 220, borderRadius: 8, overflow: "hidden", backgroundColor: "#000" }}
        onProgress={(e) => time.set(`${e.currentTime.toFixed(1)} / ${e.duration.toFixed(1)} s`)}
      />
      <View style={{ flexDirection: "row", gap: 16 }}>
        <Text onPress={() => paused.update((p) => !p)}>{paused() ? "Play" : "Pause"}</Text>
        <Text onPress={() => video.seek(0)}>Restart</Text>
        <Text>{time()}</Text>
      </View>
    </View>
  );
}

mount(Player);
```

## 属性

`Video` 接受[视图属性](../elements/view.md)（样式、class、布局、无障碍、`ref`；不接受子元素），另外还有：

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `src` | `string` | （必填） | 视频的 URL。 |
| `paused` | `boolean` | `false` | 暂停播放。 |
| `muted` | `boolean` | `false` | 静音。 |
| `volume` | `number` | `1` | 0..1。 |
| `rate` | `number` | `1` | 播放速度。 |
| `repeat` | `boolean` | `false` | 播放到结尾后从头重新播放。 |
| `controls` | `boolean` | `false` | 显示平台自带的播放控件。 |
| `resizeMode` | `"cover" \| "contain" \| "stretch" \| "center"` | | 视频如何适应其区域。 |
| `progressUpdateInterval` | `number` | `250` | `onProgress` 事件之间的毫秒数。 |

## 事件

| 事件 | 数据 | 时机 |
|---|---|---|
| `onLoadStart` | | 视频开始加载。 |
| `onLoad` | `{ duration: number; size: Size }` | 可以播放：时长（秒）和尺寸。 |
| `onProgress` | `{ currentTime: number; duration: number }` | 播放期间，每 `progressUpdateInterval` 毫秒一次。单位为秒。 |
| `onBuffer` | `{ isBuffering: boolean }` | 播放在等待数据，或恢复播放。 |
| `onEnd` | | 播放到达结尾。 |
| `onError` | `{ message: string }` | 视频无法加载或播放。 |

## ref 方法

| 方法 | 说明 |
|---|---|
| `seek(seconds)` | 跳转到 `seconds`。 |

```ts
const aspect = signal(16 / 9);

<Video src={url} onLoad={(e) => aspect.set(e.size.width / e.size.height)} style={{ width: "100%", aspectRatio: aspect() }} />;
```

## 权限

如果应用在 `nsh.json` 中声明了 `capabilities`，远程视频源需要像 `fetch` 一样在 `network` 中允许；被拒绝的视频源不显示任何内容。见[沙箱与权限](../docs/sandbox-and-capabilities.md)。
