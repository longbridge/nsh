---
title: Device and environment
description: Reading the window size, color scheme, font scale, safe areas, the platform, pixel density and layout direction, and reacting when they change.
order: 47
group: ui
demo: guide-device
---

# Device and environment

The environment an app runs in changes while it runs: the window resizes (rotation, split view, a Mac window), the user switches to dark mode or a larger text size, the keyboard comes and goes. NSH exposes each of these as a **reactive accessor**: call it like a signal, and whatever read it (a style, a text, a computed value) updates when it changes. There are no listeners to add or remove.

| Accessor | Module | Value |
|---|---|---|
| `windowSize()` | `nsh/device` | `{ width, height }` of the surface, in points. |
| `colorScheme()` | `nsh/device` | `"light"` or `"dark"`: the system's, or the app's own (`setColorScheme`). |
| `fontScale()` | `nsh/device` | The user's preferred text size relative to the default (1 is the default). |
| `safeArea()` | `nsh/ui/mobile` | Insets of the surface not covered by system UI: `{ top, right, bottom, left }`. |
| `useSafeAreaInsets()` | `nsh/ui/mobile` | The insets where the calling component renders (a screen, a modal). |
| `keyboard()` | `nsh/ui/mobile` | `{ height, duration }` of the software keyboard. |
| `I18nManager.isRTL` | `nsh/device` | Whether the app lays out right to left. |
| `reduceMotion()`, `screenReaderEnabled()` | `nsh/accessibility` | See [Accessibility](./accessibility.md). |

```tsx
import { mount, Text, View } from "nsh";
import { colorScheme, fontScale, windowSize } from "nsh/device";
import { safeArea } from "nsh/ui/mobile";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: safeArea().top + 16, paddingHorizontal: 16, backgroundColor: colorScheme() === "dark" ? "#0d1117" : "#ffffff" }}>
      <Text style={{ color: colorScheme() === "dark" ? "#e6edf3" : "#1f2328" }}>
        {Math.round(windowSize().width)} × {Math.round(windowSize().height)} pt, text at {fontScale().toFixed(2)}×
      </Text>
    </View>
  );
}

mount(App);
```

## Window size and responsive layout

`windowSize()` is the size of the surface NSH renders into: the screen on a phone, the window on an iPad in split view or on a Mac, the host view when NSH is embedded. Use it to change the layout, not only sizes:

```tsx
import { View } from "nsh";
import { windowSize } from "nsh/device";
import { Image } from "nsh/image";

function Gallery() {
  const columns = () => (windowSize().width >= 700 ? 4 : windowSize().width >= 500 ? 3 : 2);
  return (
    <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 8 }}>
      <Each of={photos}>
        {(p) => <Image src={p().url} style={{ width: (windowSize().width - 32 - (columns() - 1) * 8) / columns(), aspectRatio: 1 }} />}
      </Each>
    </View>
  );
}
```

Often flexbox, percentages and `aspectRatio` do the job without reading the size at all; see [Layout](./layout.md). With Tailwind, the breakpoint variants (`sm:`, `md:`, `lg:`, `xl:`, `2xl:`, `max-*:`, `min-[…]:`) and `portrait:` / `landscape:` follow the window too: `class="grid grid-cols-2 md:grid-cols-4"`.

## Dark mode

`colorScheme()` follows the system appearance unless the app sets its own. The simplest way to support dark mode is the `dark:` Tailwind variant, which applies when `colorScheme()` is `"dark"`:

```tsx
<View class="flex-1 bg-white dark:bg-zinc-900 p-4">
  <Text class="text-zinc-900 dark:text-zinc-100">Follows the system appearance</Text>
</View>
```

With inline styles, read `colorScheme()` in the style; for more than a few colors, derive a palette once:

```tsx
import { computed, Text, View } from "nsh";
import { colorScheme } from "nsh/device";

const palette = computed(() =>
  colorScheme() === "dark"
    ? { background: "#0d1117", surface: "#161b22", text: "#e6edf3", muted: "#8b949e" }
    : { background: "#f6f8fa", surface: "#ffffff", text: "#1f2328", muted: "#57606a" },
);

<View style={{ flex: 1, backgroundColor: palette().background }}>
  <Text style={{ color: palette().text }}>Hello</Text>
</View>
```

The components of `nsh/ui/mobile` take their colors from the component theme, which has light and dark values (`nsh.json` `"theme"`), so they switch with the system as well. To match the status bar to your background, see [`StatusBar`](../components/status-bar.md).

### An app with its own appearance

An app that is always dark (or always light) says so in `nsh.json`, and `colorScheme()`, `dark:` classes, the component theme, the status bar's default style and alerts follow it from the first frame (packaged apps also launch in it):

```json
{ "appearance": "dark" }
```

`setColorScheme` from `nsh/device` changes it at run time, for an in-app appearance setting; `"system"` follows the system again:

```tsx
import { colorScheme, setColorScheme } from "nsh/device";

<Button title={colorScheme() === "dark" ? "Light" : "Dark"} onPress={() => setColorScheme(colorScheme() === "dark" ? "light" : "dark")} />
```

> [!TIP]
> Test both appearances while developing: `xcrun simctl ui booted appearance dark` switches an iOS simulator, and Android has a dark theme toggle in Quick Settings. The change reaches your app at once, without a reload.

## Text size

`fontScale()` is the user's text size setting (Dynamic Type on iOS, font size on Android) relative to the default. Text and inputs follow it by themselves: `fontSize: 16` renders larger when the user asked for larger text. Per element you can opt out or cap it:

| Prop (on `<Text>` and `<Input>`) | Default | Description |
|---|---|---|
| `allowFontScaling` | `true` | Follow the system text size. |
| `maxFontSizeMultiplier` | `0` (none) | The largest scale applied to this text. |

```tsx
<Text style={{ fontSize: 34, fontWeight: "700" }} maxFontSizeMultiplier={1.4}>Today</Text>
```

Read `fontScale()` to adapt layout that does not follow text automatically, such as switching a row to a column when the text gets large: `flexDirection: fontScale() > 1.3 ? "column" : "row"`.

## Safe areas

Notches, the home indicator, the status bar, Android's navigation bar and the Mac Catalyst title bar cover parts of the surface. NSH apps draw edge to edge on every platform, and get the covered parts as insets.

- `safeArea()` gives the surface's insets.
- `useSafeAreaInsets()` gives the insets **where the component renders**: inside a navigation [screen](./navigation.md#safe-areas-under-the-header) they are measured below its header, inside a [modal](./navigation.md#modals) they are the modal's. Prefer it in components that may be shown in several places.
- `SafeAreaView` from `nsh/ui/mobile` is a view padded by those insets (all edges by default, or `edges={["top", "bottom"]}`), on top of its own padding.

```tsx
import { View } from "nsh";
import { SafeAreaView, useSafeAreaInsets } from "nsh/ui/mobile";

function Screen() {
  return (
    <SafeAreaView edges={["top", "left", "right"]} style={{ flex: 1, backgroundColor: "#ffffff" }}>
      <Content />
      <BottomBar />
    </SafeAreaView>
  );
}

function BottomBar() {
  const insets = useSafeAreaInsets();
  return <View style={{ paddingBottom: insets().bottom + 8, paddingTop: 8, flexDirection: "row" }}>…</View>;
}
```

On Android, a view embedded below a toolbar gets only the parts of the system bars that overlap it, like UIKit's safe area: no top inset if it does not reach the status bar.

## The platform

`Platform` from `nsh/device` describes where the app runs. It is not reactive: these do not change while the app runs.

| Member | Description |
|---|---|
| `Platform.OS` | `"ios"`, `"android"`, `"macos"`, `"windows"` or `"linux"`. Mac Catalyst reports `"ios"`. |
| `Platform.Version` | iOS and desktops: the version string (`"17.5"`); Android: the API level (a number). |
| `Platform.isPad` | An iPad (or the iPad idiom). |
| `Platform.isTV` | A TV. |
| `Platform.isMacCatalyst` | Mac Catalyst in the Mac idiom. |
| `Platform.isDesktop` | macOS, Windows or Linux. |
| `Platform.constants` | `{ osVersion, interfaceIdiom }`, the idiom being `"phone"`, `"pad"`, `"mac"`, `"tv"` or `"desktop"`. |
| `Platform.select(spec)` | The value for this platform; see [Platform-specific code](./platform-specific.md#platformselect). |

```tsx
import { Platform } from "nsh/device";

const hapticsSupported = Platform.OS === "ios" || (Platform.OS === "android" && (Platform.Version as number) >= 30);
```

`nsh trace` runs headless as iOS.

On [Linux](./linux.md), `Platform.OS` is `"linux"`, the idiom `"desktop"` and `Platform.Version` the kernel's major
and minor version (`"6.12"`): distributions number their releases in ways that cannot be compared. There, the safe
area and the keyboard height are 0, `colorScheme()` follows the desktop's setting (the XDG settings portal) live,
`fontScale()` the desktop's text scaling and `PixelRatio.get()` the display's scale, fractional ones included.

On [macOS](./macos.md) (the AppKit backend), `Platform.OS` is `"macos"`, the idiom `"desktop"` and
`Platform.Version` the macOS version. The safe area and the keyboard height are 0, `colorScheme()` follows the
system's appearance live, `fontScale()` is 1 and `PixelRatio.get()` the display's backing scale.

On [Windows](./windows.md), `Platform.OS` is `"windows"`, the idiom `"desktop"` and `Platform.Version` `"10.0"`, on
Windows 11 too (Windows kept the version number). The safe area and the keyboard height are 0, `colorScheme()` follows
Settings > Personalization > Colors live, `fontScale()` the Text size setting (1 to 2.25), `reduceMotion()` the
animation effects setting, `PixelRatio.get()` the monitor's scale, and `Intl` formats in the user's regional format.

## Pixel density

`PixelRatio` from `nsh/device` converts between points and device pixels.

| Method | Description |
|---|---|
| `PixelRatio.get()` | Device pixels per point (2 or 3 on most phones). |
| `PixelRatio.getFontScale()` | The same as `fontScale()`, not reactive. |
| `PixelRatio.getPixelSizeForLayoutSize(points)` | Points to whole device pixels. |
| `PixelRatio.roundToNearestPixel(points)` | Points rounded to the nearest device pixel. |

A hairline, one device pixel tall:

```tsx
import { View } from "nsh";
import { PixelRatio } from "nsh/device";

const hairline = 1 / PixelRatio.get();
<View style={{ height: hairline, backgroundColor: "#d0d7de" }} />
```

`PixelRatio` is not reactive. A window can move to a screen with another density (a Mac with an external display), so read it in components that also read `windowSize()`, which changes in that case.

## Right to left

When the device language is written right to left (Arabic, Hebrew, Persian, ...), NSH lays the app out right to left: rows run from the right, `left` and `right` style properties swap, text aligns to the right, and native widgets mirror. `I18nManager` from `nsh/device` controls it:

| Member | Description |
|---|---|
| `I18nManager.isRTL` | Whether the app lays out right to left (reactive). |
| `I18nManager.allowRTL(allow)` | Follow right-to-left languages (default `true`). |
| `I18nManager.forceRTL(force)` | Lay out right to left whatever the language. |
| `I18nManager.getConstants()` | `{ isRTL, doLeftAndRightSwapInRTL }`. |

Changes apply at once, without restarting the app; the demo on this page toggles `forceRTL`.

Write layouts that work both ways with the logical properties: `marginStart` / `marginEnd`, `paddingStart` / `paddingEnd`, `start` / `end`, `borderTopStartRadius` and so on, which mean left and right in left-to-right layouts and the reverse right to left. `left` and `right` themselves also swap. The `direction` style (`"inherit"`, `"ltr"`, `"rtl"`) overrides the direction for a subtree, for example to keep a phone number or a media player's controls left to right. The `rtl:` and `ltr:` Tailwind variants apply per direction.

Horizontal scroll views and lists start at the right end in right-to-left layouts. On Android, the host app must declare `android:supportsRtl="true"` in its manifest for native widgets to mirror.

## Related

- [`nsh/device`](../modules/nsh-device.md): the reference.
- [Platform-specific code](./platform-specific.md): `Platform.select`, platform props and modules.
- [Accessibility](./accessibility.md): screen readers and reduced motion.
- [`SafeAreaView`](../components/safe-area-view.md) and [`StatusBar`](../components/status-bar.md).
- [Tailwind](./tailwind.md): `dark:`, breakpoint and orientation variants.
