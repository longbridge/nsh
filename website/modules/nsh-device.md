---
title: nsh/device
description: The window size, color scheme, font scale, platform, pixel ratio and right-to-left layout.
order: 6
---

# nsh/device

`nsh/device` tells the app about the device and its environment. `windowSize`, `colorScheme`, `fontScale` and `locale` are reactive accessors: read them in JSX, `computed` or `effect`, and what reads them updates when the platform reports a change (rotation, a window resize, switching to dark mode, changing the text size or the language).

```ts
import { colorScheme, fontScale, I18nManager, locale, PixelRatio, Platform, windowSize } from "nsh/device";
```

See [Device and environment](../docs/device-and-environment.md) for the guide.

## windowSize

```ts
const windowSize: Accessor<{ width: number; height: number }>;
```

The size of the surface in points: the window, or the embedded `NshAppView` / `NshView`. Reactive.

```tsx
const wide = () => windowSize().width >= 600;

<View style={{ flexDirection: wide() ? "row" : "column" }}>{/* ... */}</View>
```

## colorScheme

```ts
const colorScheme: Accessor<"light" | "dark">;
```

The system appearance. Reactive. Tailwind's `dark:` variant follows it too (see [Tailwind](../docs/tailwind.md)).

```tsx
const dark = () => colorScheme() === "dark";

<View style={{ flex: 1, backgroundColor: dark() ? "#000" : "#fff" }} />
```

## fontScale

```ts
const fontScale: Accessor<number>;
```

The user's preferred text size relative to the default (`1` is the default size; Dynamic Type on iOS, font size on Android), or the text size the embedding app sets in its place. Reactive. Text already scales with it unless `allowFontScaling={false}`; read it when other sizes should follow, such as icons next to text.

## locale

```ts
const locale: Accessor<string>;
```

The user's locale as a BCP 47 tag (`"en-US"`, `"zh-Hans-CN"`), or the locale the embedding app sets in its place. Reactive. `Intl` formats for it by default; read it to choose the strings of your own translations ([Device and environment](../docs/device-and-environment.md#language-and-region)).

## Platform

```ts
const Platform: {
  readonly OS: "ios" | "android" | "macos" | "windows" | "linux" | "web";
  readonly isDesktop: boolean;
  readonly Version: string | number;
  readonly isPad: boolean;
  readonly isTV: boolean;
  readonly isMacCatalyst: boolean;
  readonly constants: {
    readonly osVersion: string | number;
    readonly interfaceIdiom: "phone" | "pad" | "mac" | "tv" | "desktop";
  };
  select<T>(spec: PlatformSelectSpec<T>): T | undefined;
};
```

The platform the app runs on, constant while it runs.

| Member | Description |
|---|---|
| `OS` | The operating system. iOS, the iOS simulator and Mac Catalyst report `"ios"`, browsers `"web"`. |
| `isDesktop` | A desktop OS (macOS, Windows, Linux), or a browser with a mouse and a wide window. |
| `Version` | iOS and desktops: the version string (`"17.5"`); Android: the API level (a number). |
| `isPad` | A tablet: an iPad, or an Android tablet. |
| `isTV` | A TV (Apple TV idiom, Android TV). |
| `isMacCatalyst` | Mac Catalyst in the Mac idiom. |
| `constants.osVersion` | The same as `Version`. |
| `constants.interfaceIdiom` | `"phone"`, `"pad"`, `"mac"`, `"tv"` or `"desktop"`. |
| `select(spec)` | The value for this platform (see below). |

### Platform.select

```ts
interface PlatformSelectSpec<T> {
  ios?: T;
  android?: T;
  macos?: T;
  windows?: T;
  linux?: T;
  mobile?: T;   // iOS and Android
  desktop?: T;  // macOS, Windows and Linux
  native?: T;   // every platform
  default?: T;
}
```

Returns the value for the current OS (`ios`, `android`, `web`, ...), else `mobile` / `desktop`, else `native` (every platform but the web), else `default`, else `undefined`. In a browser, `mobile` or `desktop` follows the idiom.

```ts
const shadow = Platform.select({
  ios: { shadowColor: "#000", shadowOpacity: 0.15, shadowRadius: 8 },
  android: { elevation: 4 },
});
```

> [!TIP]
> For props that only one platform has, prefer the `ios={{…}}` / `android={{…}}` props of elements and components to branching on `Platform` (see [Platform-specific code](../docs/platform-specific.md)).

## PixelRatio

```ts
const PixelRatio: {
  get(): number;
  getFontScale(): number;
  getPixelSizeForLayoutSize(size: number): number;
  roundToNearestPixel(size: number): number;
};
```

Device pixels per point.

| Method | Description |
|---|---|
| `get()` | Device pixels per point (e.g. `3` on most iPhones). |
| `getFontScale()` | The same as `fontScale()`, without tracking. |
| `getPixelSizeForLayoutSize(size)` | `size` points in whole device pixels. |
| `roundToNearestPixel(size)` | `size` rounded to the nearest device pixel, in points. |

```ts
const hairline = 1 / PixelRatio.get();
```

`PixelRatio` is not reactive. A window can move to another screen (on Mac Catalyst, or an external display); read it in components that also read `windowSize()`, so they run again when the window changes.

## I18nManager

```ts
const I18nManager: {
  isRTL: boolean;                  // reactive
  doLeftAndRightSwapInRTL: true;
  allowRTL(allow: boolean): void;
  forceRTL(force: boolean): void;
  getConstants(): { isRTL: boolean; doLeftAndRightSwapInRTL: boolean };
};
```

Right-to-left layout. In right-to-left layout, rows run from the right, and `left` / `right` style properties (`marginLeft`, `paddingRight`, ...) mean start and end. Changes apply at once; no restart is needed.

| Member | Description |
|---|---|
| `isRTL` | Whether the app lays out right to left. Reading it is tracked. |
| `doLeftAndRightSwapInRTL` | Always `true`: `left` and `right` swap in right-to-left layout. |
| `allowRTL(allow)` | Follow right-to-left languages (default `true`). |
| `forceRTL(force)` | Lay out right to left whatever the language. |
| `getConstants()` | `{ isRTL, doLeftAndRightSwapInRTL }`. |

```tsx
<Text onPress={() => I18nManager.forceRTL(!I18nManager.isRTL)}>
  {I18nManager.isRTL ? "Right to left" : "Left to right"}
</Text>
```

## Related

- Safe area insets (`safeArea()`, `safeAreaInsets()`) and the software keyboard (`keyboardFrame()`) are part of [`nsh/base`](./nsh-base.md#safearea-and-safeareainsets).
- Screen reader and reduced motion settings are in [`nsh/accessibility`](./nsh-accessibility.md).
