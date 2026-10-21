---
title: Video
description: Video, a native video player (AVPlayer, MediaPlayer), with its props, events and ref methods.
order: 2.4
---

# Video

`nsh` exports `Video`: a video player built on the platform's player (`AVPlayer` on iOS and Mac Catalyst, `MediaPlayer` on Android). Playback is controlled with props, so pausing is a signal away.

```ts
import { Video, type VideoProps } from "nsh";
```

See [Web view and video](../docs/web-view-and-video.md) for the guide and [`<Video>`](../elements/video.md) for its props.

## Video

```ts
const Video: (props: VideoProps) => YNode;

interface VideoProps extends Omit<ViewProps, "children">, VideoHostProps {
  source: string | { uri: string };
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
        source="https://example.com/clip.mp4"
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

## Props

`Video` takes the [view props](../elements/view.md) (style, classes, layout, accessibility, `ref`; no children), plus:

| Prop | Type | Default | Description |
|---|---|---|---|
| `source` | `string \| { uri: string }` | (required) | The video's URL. |
| `paused` | `boolean` | `false` | Playback is paused. |
| `muted` | `boolean` | `false` | No sound. |
| `volume` | `number` | `1` | 0..1. |
| `rate` | `number` | `1` | The playback speed. |
| `repeat` | `boolean` | `false` | Plays again from the start at the end. |
| `controls` | `boolean` | `false` | Shows the platform's playback controls. |
| `resizeMode` | `"cover" \| "contain" \| "stretch" \| "center"` | | How the video fits its box. |
| `progressUpdateInterval` | `number` | `250` | Milliseconds between `onProgress` events. |

## Events

| Event | Payload | When |
|---|---|---|
| `onLoadStart` | | The video starts loading. |
| `onLoad` | `{ duration: number; width: number; height: number }` | It is ready to play: its length in seconds and its size. |
| `onProgress` | `{ currentTime: number; duration: number }` | While playing, every `progressUpdateInterval` ms. Seconds. |
| `onBuffer` | `{ isBuffering: boolean }` | Playback waits for data, or resumes. |
| `onEnd` | | Playback reached the end. |
| `onError` | `{ message: string }` | The video could not be loaded or played. |

## Ref methods

| Method | Description |
|---|---|
| `seek(seconds)` | Jumps to `seconds`. |

```ts
const aspect = signal(16 / 9);

<Video source={url} onLoad={(e) => aspect.set(e.width / e.height)} style={{ width: "100%", aspectRatio: aspect() }} />;
```

## Capabilities

When your app declares `capabilities` in `nsh.json`, remote video sources must be allowed under `network`, like `fetch`; a refused source shows nothing. See [Sandbox and capabilities](../docs/sandbox-and-capabilities.md).
