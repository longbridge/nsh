---
title: nsh/device
description: The window size, color scheme, font scale, platform, pixel ratio and right-to-left layout.
order: 6
---

# nsh/device

The device API tells the app about the device and its environment. Rust code calls the functions of `nsh_core::ui::prelude` (`window()`, `dark()`, `font_scale()`, `locale()`, `os()`, `idiom()`, `pixel_ratio()`, `rtl()`, ...); TSX imports `nsh/device`. The window size, the color scheme, the font scale and the locale are reactive: read them in a binding (a closure passed to an element in Rust, JSX in TSX), `computed` or `effect`, and what reads them updates when the platform reports a change (rotation, a window resize, switching to dark mode, changing the text size or the language).

```rust tabs
use nsh_core::{
    platform::{Idiom, Os},
    ui::prelude::*, // window, dark, font_scale, locale, os, idiom, pixel_ratio, rtl, ...
};
```

```tsx tabs
import { colorScheme, fontScale, I18nManager, locale, PixelRatio, Platform, windowSize } from "nsh/device";
```

See [Device and environment](../docs/device-and-environment.md) for the guide.

## windowSize

```ts
const windowSize: Accessor<Size>;
```

The size of the surface in points: the window, or the embedded `NshAppView` / `NshView`. Reactive. Rust: `window() -> Size<Points>`.

```rust tabs
let wide = move || window().width >= px(600.0);

view().style_when(wide, |s| s.flex_row())
```

```tsx tabs
const wide = () => windowSize().width >= 600;

<View style={{ flexDirection: wide() ? "row" : "column" }}>{/* ... */}</View>
```

## colorScheme

```ts
const colorScheme: Accessor<"light" | "dark">;
```

The system appearance. Reactive. Rust: `dark() -> bool`; theme colors (`AdaptiveColor`) and the `.dark(…)` style variant follow it without reading it. Tailwind's `dark:` variant follows it too (see [Tailwind](../docs/tailwind.md)).

```rust tabs
view().flex_1().bg(move || if dark() { black() } else { white() })
```

```tsx tabs
const dark = () => colorScheme() === "dark";

<View style={{ flex: 1, backgroundColor: dark() ? "#000" : "#fff" }} />
```

## fontScale

```ts
const fontScale: Accessor<number>;
```

The user's preferred text size relative to the default (`1` is the default size; Dynamic Type on iOS, font size on Android), or the text size the embedding app sets in its place. Reactive. Rust: `font_scale() -> f32`. Text already scales with it unless `allowFontScaling={false}`; read it when other sizes should follow, such as icons next to text.

## locale

```ts
const locale: Accessor<string>;
```

The user's locale as a BCP 47 tag (`"en-US"`, `"zh-Hans-CN"`), or the locale the embedding app sets in its place. Reactive. Rust: `locale() -> Locale` (`locale().as_str()` is the tag). `Intl` formats for it by default; read it to choose the strings of your own translations ([Device and environment](../docs/device-and-environment.md#language-and-region)).

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

| Member | Rust | Description |
|---|---|---|
| `OS` | `os() -> Os` (`os().name()` is the TS string) | The operating system. iOS, the iOS simulator and Mac Catalyst report `"ios"`, browsers `"web"`. |
| `isDesktop` | `is_desktop()` | A desktop OS (macOS, Windows, Linux), or a browser with a mouse and a wide window. |
| `Version` | `os_version() -> (u16, u16)` | iOS and desktops: the version string (`"17.5"`); Android: the API level (a number). Rust gives the major and minor version, or the API level and 0 on Android. |
| `isPad` | `idiom() == Idiom::Pad` | A tablet: an iPad, or an Android tablet. |
| `isTV` | `idiom() == Idiom::Tv` | A TV (Apple TV idiom, Android TV). |
| `isMacCatalyst` | `idiom() == Idiom::Mac` | Mac Catalyst in the Mac idiom. |
| `constants.osVersion` | `os_version()` | The same as `Version`. |
| `constants.interfaceIdiom` | `idiom() -> Idiom` | `"phone"`, `"pad"`, `"mac"`, `"tv"` or `"desktop"`. |
| `select(spec)` | `match os() { … }` | The value for this platform (see below). |

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

Returns the value for the current OS (`ios`, `android`, `web`, ...), else `mobile` / `desktop`, else `native` (every platform but the web), else `default`, else `undefined`. In a browser, `mobile` or `desktop` follows the idiom. Rust code matches on `os()` (and `is_desktop()` for `mobile` / `desktop`).

```rust tabs
use nsh_core::platform::Os;

let card = view().p_4();
let card = match os() {
    Os::Ios => card.shadow_color(black()).shadow_opacity(0.15).shadow_radius(px(8.0)),
    Os::Android => card.elevation(px(4.0)),
    _ => card,
};
```

```tsx tabs
const shadow = Platform.select({
  ios: { shadowColor: "#000", shadowOpacity: 0.15, shadowRadius: 8 },
  android: { elevation: 4 },
});
```

> [!TIP]
> For props that only one platform has, prefer the `ios={{…}}` / `android={{…}}` props of TSX elements and components to branching on `Platform` (see [Platform-specific code](../docs/platform-specific.md)).

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

| Method | Rust | Description |
|---|---|---|
| `get()` | `pixel_ratio()` | Device pixels per point (e.g. `3` on most iPhones). |
| `getFontScale()` | `untracked(font_scale)` | The same as `fontScale()`, without tracking. |
| `getPixelSizeForLayoutSize(size)` | `(size * pixel_ratio()).round()` | `size` points in whole device pixels. |
| `roundToNearestPixel(size)` | `round_to_nearest_pixel(size)` | `size` rounded to the nearest device pixel, in points. |

```rust tabs
let hairline = px(1.0 / pixel_ratio());
```

```tsx tabs
const hairline = 1 / PixelRatio.get();
```

`PixelRatio` is not reactive (Rust's `pixel_ratio()` is: it subscribes like `window()`). A window can move to another screen (on Mac Catalyst, or an external display); read it in components that also read `windowSize()`, so they run again when the window changes.

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

| Member | Rust | Description |
|---|---|---|
| `isRTL` | `rtl()` | Whether the app lays out right to left. Reading it is tracked. |
| `doLeftAndRightSwapInRTL` | | Always `true`: `left` and `right` swap in right-to-left layout. |
| `allowRTL(allow)` | `allow_rtl(allow)` | Follow right-to-left languages (default `true`). |
| `forceRTL(force)` | `force_rtl(force)` | Lay out right to left whatever the language. |
| `getConstants()` | | `{ isRTL, doLeftAndRightSwapInRTL }`. |

```rust tabs
text(move || if rtl() { "Right to left" } else { "Left to right" }).on_press(|| force_rtl(!rtl()))
```

```tsx tabs
<Text onPress={() => I18nManager.forceRTL(!I18nManager.isRTL)}>
  {I18nManager.isRTL ? "Right to left" : "Left to right"}
</Text>
```

## Related

- Safe area insets (Rust `safe_area()`; TSX `safeArea()`, `safeAreaInsets()`) and the software keyboard (Rust `keyboard_height()`; TSX `keyboardFrame()`) are part of [`nsh/base`](./nsh-base.md#safearea-and-safeareainsets).
- Screen reader and reduced motion settings are in [`nsh/accessibility`](./nsh-accessibility.md).
