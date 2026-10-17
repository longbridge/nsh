---
title: Elements
description: The host elements every NSH app is built from, and the props, events and ref methods they all share.
order: 1
---

# Elements

Host elements are the primitives every screen is made of: `<View>`, `<Text>`, `<Image>` and the rest, imported from NSH's
modules. Each one is a native view (a UIView subclass on iOS and Mac Catalyst, an Android `View`), created and laid out
by the NSH runtime. Everything an app shows is
made of them: the components of [`nsh/ui/mobile`](../components/index.md) are drawn with the same elements.

```tsx
import { mount, signal, Text, View } from "nsh";

function App() {
  const count = signal(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <Text style={{ fontSize: 24 }}>Tapped {count()} times</Text>
      <View onPress={() => count.update((n) => n + 1)} activeOpacity={0.6} class="rounded-lg bg-blue-600 px-4 py-2">
        <Text class="text-white">Tap</Text>
      </View>
    </View>
  );
}

mount(App);
```

Every tag is PascalCase and names a binding in scope: import each element from its module, as the table below says.
Only `<If>` and `<Each>` are globals that need no import. A lowercase tag such as `<view>` is a build error, because
TypeScript would check it against one global type per name, and generic components such as `List` and `Each` could not
infer their item types. The compiler recognizes the elements through their imports and makes the native views in place,
so importing them costs nothing at run time. See [JSX and components](../docs/jsx-and-components.md).

## All elements

| Tag | Import from | Native view (iOS / Android) | Page |
|---|---|---|---|
| `<View>` | `nsh` | UIView / ViewGroup | [View](./view.md) |
| `<Text>` | `nsh` | CoreText view / StaticLayout view | [Text](./text.md) |
| `<Image>` | `nsh/image` | a view with an image layer / an image view | [Image](./image.md) |
| `<Scroll>` | `nsh` | UIScrollView / ScrollView | [Scroll](./scroll.md) |
| `<Input>` | `nsh` | UITextField or UITextView / EditText | [Input](./input.md) |
| `<List>` | `nsh/list` | UIScrollView / ScrollView with recycled cells | [List](./list.md) |
| `<Canvas>` | `nsh/canvas` | a view replaying a 2D drawing | [Canvas](./canvas.md) |
| `<WebView>` | `nsh/webview` | WKWebView / android.webkit.WebView | [WebView](./web-view.md) |
| `<Video>` | `nsh/video` | AVPlayerLayer / MediaPlayer on a TextureView | [Video](./video.md) |
| `<Modal>` | `nsh/ui/mobile` | presented view controller / Dialog | [Modal](./modal.md) |
| `<InputAccessoryView>` | `nsh/ios` | a bar over the keyboard (iOS only) | [nsh/ios](../modules/nsh-ios.md) |
| `<If>`, `<Each>` | nothing: globals | none: control flow | [Control flow](./control-flow.md) |

> [!NOTE]
> A string is only drawn inside a `<Text>`. A string child of a `<View>` creates no native view and takes no part in
> layout.

## Common props

Every host element accepts the props below (the `CommonProps` type). Element pages list only their own props.

### Styling

| Prop | Type | Description |
|---|---|---|
| `style` | `StyleProp<ViewStyle>` (`TextStyle` for `<Text>` and `<Input>`, `ImageStyle` for `<Image>`) | Inline style, or an array of styles merged left to right (falsy entries skipped; arrays may nest, so `style={[base, props.style]}` takes a `props.style` that is itself an array). Wins over `class`. See the [style reference](../modules/style-reference.md). |
| `class` | `string` | Tailwind classes, resolved natively (`"flex-row gap-2 p-4 dark:bg-zinc-900"`). Later classes win over earlier ones. See [Tailwind](../docs/tailwind.md). |
| `classList` | `Record<string, boolean \| null \| undefined>` | Classes applied while their value is truthy (`{ "opacity-50": busy() }`). |
| `disabled` | `boolean` | Ignores presses (no press feedback either), applies `disabled:` classes and disables controls such as inputs. |

### Presses and feedback

Press handling runs natively: the platform recognizes the press and shows the feedback, and JS only hears about it
afterwards. Only the innermost pressable view responds; inside scroll views a press-in waits about 130 ms so a scroll can
start first.

| Prop | Type | Default | Description |
|---|---|---|---|
| `onPress` | `() => void` | | A tap ended inside the view. |
| `onLongPress` | `() => void` | | The finger stayed down for `delayLongPress`. |
| `onPressIn` | `() => void` | | A finger went down on the view. |
| `onPressOut` | `() => void` | | The finger lifted or left. |
| `delayLongPress` | `number` | `500` | Milliseconds before a press becomes a long press. |
| `activeOpacity` | `number` | | The view's opacity while pressed. |
| `underlayColor` | `ColorValue` | | The background while pressed; the children fade to `underlayOpacity`. |
| `underlayOpacity` | `number` | `0.85` | The children's opacity while `underlayColor` shows. |
| `hitSlop` | `number \| { top?, right?, bottom?, left? }` | | Extends the touch area beyond the bounds, in points. |
| `pointerEvents` | `"auto" \| "none" \| "box-none" \| "box-only"` | `"auto"` | Which touches the view and its children receive: `"none"` neither, `"box-none"` only the children, `"box-only"` only the view itself. |

`active:` Tailwind classes also apply while a view is pressed, with no JS round trip.

### Hover

On macOS, Linux and the GPUI backend, JS can hear the mouse come over a view and leave it (other platforms do not raise these events
yet; `hover:` classes style hovered views everywhere the platform tracks hover).

| Prop | Type | Description |
|---|---|---|
| `onHoverIn` | `() => void` | The mouse came over the view or a view inside it. |
| `onHoverOut` | `() => void` | The mouse left the view and the views inside it, or the window. |

Like CSS `:hover`, a view counts as hovered while the mouse is over it or over anything inside it, so the views around
the one under the mouse hear `onHoverIn` too (from the outside in; `onHoverOut` from the inside out). The events come
after the fact and never on touches. A view removed from under the mouse hears `onHoverOut`, on Linux at the next mouse
move.

```tsx
const hovered = signal<string | null>(null);

<View onHoverIn={() => hovered.set(file.path)} onHoverOut={() => hovered.set(null)} class="hover:bg-zinc-100">
  <Text>{file.name}</Text>
</View>
```

### Layout and identity

| Prop | Type | Description |
|---|---|---|
| `onLayout` | `(e: LayoutEvent) => void` | The view's frame relative to its parent, `{ x, y, width, height }`, after each layout that changes it. |
| `ref` | `YNode \| (node: YNode) => void` | The native node (see [ref methods](#ref-methods)). `ref={el}` assigns a variable, `ref={fn}` calls a function. |
| `nativeID` | `string` | Names the view for others, e.g. the `InputAccessoryView` an input refers to. |
| `testID` | `string` | An identifier for UI tests (on Android, the view's resource id). |

### Gestures driving native values

These props bind a gesture to [native animated values](../modules/nsh-animated.md) that move on the UI thread, with no
JS per frame. See [touch and gestures](../docs/touch-and-gestures.md).

| Prop | Type | Description |
|---|---|---|
| `pan` | `{ x?: NativeValue; y?: NativeValue }` | Drags drive these values (points). |
| `onPanStart` | `(e: PanEvent) => void` | A drag started. |
| `onPanEnd` | `(e: PanEvent) => void` | A drag ended, with the release velocity (e.g. to start a `decay`). |
| `pinch` | `{ scale?: NativeValue }` | Two-finger pinches drive `scale`. |
| `onPinchStart` / `onPinchEnd` | `(e: { scale: number; velocity: number }) => void` | A pinch started / ended. |
| `rotation` | `{ angle?: NativeValue }` | Two-finger rotations drive `angle` (degrees). |
| `onRotationStart` / `onRotationEnd` | `(e: { rotation: number; velocity: number }) => void` | A rotation started / ended. |

`PanEvent` is `{ translationX, translationY, velocityX, velocityY, valueX?, valueY? }`: the translation since the start,
the velocity in points per second, and the bound values at that moment (a drag can grab a value mid-animation).

### Raw touches and the responder system

For gestures the props above do not cover, every element takes the touch and responder props:
`onTouchStart`, `onTouchMove`, `onTouchEnd`, `onTouchCancel` (and their `…Capture` forms),
`onStartShouldSetResponder`, `onMoveShouldSetResponder` (and `…Capture`), `onResponderGrant`, `onResponderReject`,
`onResponderStart`, `onResponderMove`, `onResponderEnd`, `onResponderRelease`, `onResponderTerminate` and
`onResponderTerminationRequest`. Each handler gets a `GestureResponderEvent`
(`{ identifier, locationX, locationY, pageX, pageY, target, timestamp, touches, changedTouches, nativeEvent,
stopPropagation() }`); the `…ShouldSetResponder` handlers also get a `gestureState` (`dx`, `dy`, `vx`, `vy`, ...).
Returning `true` from `onResponderGrant` keeps native scroll views from taking the touches. `panActivation`
(`{ activeOffsetX, activeOffsetY, failOffsetX, failOffsetY }`) makes the view the responder only once the platform
activates its pan, so drags inside scroll views leave the other axis to them
([Touch and gestures](../docs/touch-and-gestures.md#pans-inside-scroll-views-panactivation)).
`PanResponder` in [`nsh/gesture`](../modules/nsh-gesture.md) builds on them.

### Accessibility

| Prop | Type | Description |
|---|---|---|
| `accessible` | `boolean` | The view is one element for assistive technologies; its children are not exposed separately. |
| `accessibilityLabel` | `string` | What screen readers read. Views without a label read their texts. |
| `accessibilityHint` | `string` | What activating the element does. |
| `accessibilityRole` / `role` | `AccessibilityRole` | What the element is (`"button"`, `"header"`, `"link"`, `"switch"`, …). |
| `accessibilityState` | `{ disabled?, selected?, checked?, busy?, expanded? }` | The state assistive technologies report (`checked` may be `"mixed"`). |
| `accessibilityValue` | `{ min?, max?, now?, text? }` | The value of a range or textual element. |
| `accessibilityElementsHidden` | `boolean` | Hides the view and its subtree from assistive technologies. |
| `importantForAccessibility` | `"auto" \| "yes" \| "no" \| "no-hide-descendants"` | Whether the view is exposed. |
| `accessibilityActions` | `{ name: string; label?: string }[]` | Custom actions offered to assistive technologies. |
| `onAccessibilityAction` | `(e: { actionName: string }) => void` | The user invoked one of `accessibilityActions`. |

See [accessibility](../docs/accessibility.md).

### Platform-only props

Props that exist on one platform go in an object named after it, so the shared props stay the same everywhere. Only the
current platform's object is sent; the others are ignored.

| Prop | Description |
|---|---|
| `ios={{…}}` | iOS and Mac Catalyst. `<Input>`, `Modal` and `WebView` add their own keys. |
| `android={{…}}` | Android. Every element takes `ripple`, `elevation` and `accessibilityLiveRegion` (below). |
| `desktop={{…}}` | Desktop platforms; ignored on mobile. Every element takes `windowDrag` (below). |

| `android` key | Type | Description |
|---|---|---|
| `ripple` | `{ color?, borderless?, radius?, foreground? }` | The material ripple while pressed. |
| `elevation` | `number` | The material shadow (points), from the view's shape. |
| `accessibilityLiveRegion` | `"none" \| "polite" \| "assertive"` | How changes of the view are announced to TalkBack. |

| `desktop` key | Type | Description |
|---|---|---|
| `windowDrag` | `boolean` | Pressing the view drags the window, as its title bar would; `false` keeps a part of a draggable view clickable. See [a title bar of your own](../modules/nsh-desktop.md#a-title-bar-of-your-own). |

```tsx
<View onPress={save} android={{ ripple: { color: "#00000022" }, elevation: 2 }} class="rounded-lg bg-white p-4">
  <Text>Save</Text>
</View>
```

See [platform-specific code](../docs/platform-specific.md).

## Ref methods

`ref` receives a `YNode`. Its methods send commands to the native view, in order with the changes made before them in the
same turn. Each element page says which apply to it.

| Method | Applies to | Description |
|---|---|---|
| `focus()` / `blur()` | `<Input>` | Focuses (shows the keyboard) / unfocuses. |
| `setSelection(start, end?)` | `<Input>` | Selects `start..end` in UTF-16 units; a caret without `end`. |
| `scrollTo({ x?, y?, animated? })` | `<Scroll>`, `List` | Scrolls to an offset. |
| `zoomTo({ scale, x?, y?, animated? })` | zoomable `<Scroll>` | Zooms to `scale` around a content point. |
| `scrollToOffset({ offset, animated? })` | `List` | Scrolls to an offset along the list's axis. |
| `scrollToIndex({ index, animated?, viewPosition?, viewOffset? })` | `List` | Scrolls an item into view. |
| `scrollToEnd({ animated? })` | `List` | Scrolls to the end of the content. |
| `reload()`, `goBack()`, `goForward()`, `stopLoading()`, `injectJavaScript(script)`, `postMessage(data)` | `WebView` | Controls the page. |
| `seek(seconds)` | `Video` | Jumps to a time. |
| `command(name, args?)` | native components | Calls a command of a registered native component ([`nsh/native`](../modules/nsh-native.md)). |

```tsx
import { Input, View, type YNode } from "nsh";

let input!: YNode;
<View onPress={() => input.focus()}>
  <Input ref={input} placeholder="Tap the row to type" />
</View>;
```

## Why handlers never block the UI

The main thread never waits for JS. When the system needs an answer on the spot (may this text change, may the back
button pop, may this sheet close), NSH answers it natively from props set beforehand (`maxLength`, `preventBack`,
`dismissible`, `pointerEvents`, …), and the event reaches JS afterwards. This is why such behavior is a prop, not a
return value of a handler.
