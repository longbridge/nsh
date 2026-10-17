---
title: StatusBar
description: Control the status bar's content color and visibility by rendering StatusBar or calling its functions.
order: 9
---

# StatusBar

`StatusBar` sets how the system status bar looks: the color of its content (time, battery, signal) and whether it is
hidden. You can render it as a component, so that the setting applies while that part of the UI is shown, or call its
static functions.

```tsx
import { StatusBar } from "nsh/ui/mobile";
```

## Rendering a StatusBar

A rendered `StatusBar` takes up no space. Its setting applies while it is mounted:

```tsx
import { mount, Text, View } from "nsh";
import { StatusBar } from "nsh/ui/mobile";

function App() {
  return (
    <View style={{ flex: 1, backgroundColor: "#0b0b0f" }}>
      <StatusBar barStyle="light-content" />
      <Text style={{ color: "#ffffff", padding: 16, paddingTop: 64 }}>Dark screen, light status bar</Text>
    </View>
  );
}

mount(App);
```

You can render several `StatusBar`s, for example one per screen. The **most recently mounted** one wins, **prop by
prop**: a `StatusBar` that only sets `hidden` keeps the `barStyle` of the one below it. When a `StatusBar` unmounts,
for example when its screen is popped, the previous setting applies again.

```tsx
function PhotoViewer() {
  return (
    <Screen headerShown={false}>
      <StatusBar hidden animated />
      …
    </Screen>
  );
}
```

## Setting it with functions

The static functions set a base setting. Every rendered `StatusBar` overrides it.

```tsx
StatusBar.setBarStyle("dark-content", true);
StatusBar.setHidden(true, "fade");
StatusBar.setHidden(false);
```

| Function | Description |
|---|---|
| `StatusBar.setBarStyle(barStyle: BarStyle, animated?: boolean \| null): void` | Sets the color of the content. `animated` defaults to `false`. |
| `StatusBar.setHidden(hidden: boolean, animation?: "none" \| "fade" \| "slide" \| null): void` | Hides or shows the bar. Any animation other than `"none"` animates the change. |

## Bar styles

| `barStyle` | Content |
|---|---|
| `"default"` | The platform's default for the current appearance (light or dark mode). |
| `"light-content"` | Light content, for dark backgrounds. |
| `"dark-content"` | Dark content, for light backgrounds. |

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `barStyle` | `"default" \| "light-content" \| "dark-content" \| null` | not set | The color of the content. |
| `hidden` | `boolean \| null` | not set | Hides the status bar. |
| `animated` | `boolean \| null` | not set | Animates changes of `barStyle` and `hidden`. |
| `android` | `{ backgroundColor?: string \| null }` | not set | Android only: the bar's background color. |

A prop that is not set leaves that part of the status bar to the `StatusBar` below, or to the functions. `StatusBar`
has no events.

## Platform notes

- **iOS**: hiding and showing animate with a fade; `"slide"` also fades. In an app
  [embedded](../docs/embedding-ios.md) in a native one, the host app's view controller decides how the status bar
  looks. The host applies NSH's setting by returning `NshAppView.preferredStatusBarStyle`,
  `NshAppView.prefersStatusBarHidden` and `NshAppView.preferredStatusBarUpdateAnimation` from its view controller.
- **Android**: `android={{ backgroundColor }}` colors the bar. It only applies before edge-to-edge (Android 15, API 35,
  draws apps behind the status bar).
- **Mac Catalyst** has no status bar.

> [!IMPORTANT]
> If your `nsh.json` declares `"capabilities"`, the status bar can only be changed when it includes
> `"statusBar": true`. Otherwise changes are refused and logged. See
> [Sandbox and capabilities](../docs/sandbox-and-capabilities.md).

```json
{
  "capabilities": {
    "statusBar": true
  }
}
```

## Related

- [nsh/android](../modules/nsh-android.md): `setStatusBarBackgroundColor` and other Android-only APIs.
- [SafeAreaView](./safe-area-view.md): keeping content out from under the status bar.
