---
title: nsh/native
description: nativeComponent, which turns a native view registered by the embedding app into a typed component.
order: 9
---

# nsh/native

`nsh/native` connects your app to native views that the host app provides. An iOS or Android app that embeds NSH can register its own views (a map, a chart, a camera preview) by name; `nativeComponent` gives you a typed component for one of them. Because NSH renders with real native views, such a component sits in the view hierarchy like any other view: it is laid out by NSH, clips and scrolls with its parents, and overlaps correctly.

```ts
import { nativeComponent } from "nsh/native";
```

See [Native components](../docs/native-components.md) for registering the native side, and [Embedding on iOS](../docs/embedding-ios.md) / [Embedding on Android](../docs/embedding-android.md).

## nativeComponent

```ts
function nativeComponent<P extends object = {}, E extends object = {}>(
  name: string,
  spec: NativeComponentSpec<P, E>,
): (props: NativeComponentProps<P, E>) => YNode;

interface NativeComponentSpec<P, E> {
  /** Its own props: sent to the component (any JSON value), each when it changes. */
  props: readonly (keyof P & string)[];
  /** The events it emits. */
  events?: readonly (keyof E & string)[];
}
```

Returns a component for the native component the embedding app registered as `name` (Swift `NshAppView.register(component:)`, Java `NshView.registerComponent`).

- `P` is the type of its own props. Each prop listed in `spec.props` is sent to the native side as JSON when it changes; setting it to `undefined` removes it.
- `E` maps event names to their payloads. An event `regionChange` becomes the handler prop `onRegionChange`.
- The component also takes every [view prop](../elements/view.md) except `children`: `style`, `class`, layout, accessibility, touch handlers and `ref`.

```ts
type NativeComponentProps<P, E> = Omit<ViewProps, "children"> & P & NativeEventHandlers<E>;
type NativeEventHandlers<E> = { [K in keyof E & string as `on${Capitalize<K>}`]?: (data: E[K]) => void };
```

### Size

The component is laid out like a view, at the size its style gives it. Where the style leaves a dimension open, it uses the size the native component reports (as an image uses its natural size).

### Commands

Its `ref` receives a `YNode`; `ref.command(name, args)` calls command `name` of the native component with `args` (any JSON value):

```ts
map.command("zoomTo", { level: 3 });
```

### When nothing is registered

Where nothing is registered under `name` (a different host app, the dev host, the browser), the component renders an empty view and a warning is logged once. Keep the app usable without it, or check for your host before using it.

## Example

The badge here is a native view of the host app, registered as `embed.badge`. NSH lays it out, sends it `text` and `color`, receives its `tap` events, and calls its `pulse` command:

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
> Props, events and command arguments cross to the native side as JSON. Registered components are the host app's own code: NSH's [sandbox limits and capabilities](../docs/sandbox-and-capabilities.md) do not apply to what they do.
