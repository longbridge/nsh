---
title: Text
description: Showing text with the text element - nested spans and links, inline views, fonts and decorations, truncation, system text size, selection and onTextLayout.
order: 35
group: ui
---

# Text

All text is shown by `<Text>`. A `<Text>` element is one paragraph, typeset by the platform's text engine (CoreText on Apple platforms, `StaticLayout` on Android) on the engine thread when it is laid out, and only drawn on the main thread. Nested `<Text>` elements are spans of the same paragraph, not separate views.

```tsx
<Text style={{ fontSize: 16, lineHeight: 24, color: "#1f2328" }}>
  By continuing you agree to the{" "}
  <Text style={{ color: "#0969da", textDecorationLine: "underline" }} onPress={openTerms}>
    Terms of service
  </Text>
  .
</Text>
```

> [!IMPORTANT]
> Strings are only displayed inside `<Text>`. A string placed directly in a `<View>` is not shown.

The element itself is documented on [text](../elements/text.md); this page is about using it.

## Strings and whitespace

JSX text follows the same rules as in React: each line is trimmed, lines that are only whitespace are dropped, and the remaining lines are joined with a single space. Whitespace within one line is kept, so `{first} {last}` keeps its space. Use `{" "}` for a space at the start or end of a line, and `{"\n"}` for a line break:

```tsx
<Text>
  Hello,
  {name()}!{"\n"}
  Welcome back.
</Text>
```

Expressions in text follow what they read: `<Text>{count()} items</Text>` updates its first span when `count` changes.

## Spans and inheritance

A `<Text>` inside a `<Text>` is a span. It inherits the text style of its parent and overrides what it sets:

```tsx
<Text style={{ fontSize: 17, color: "#1f2328" }}>
  <Text style={{ fontWeight: "300" }}>Light </Text>
  <Text style={{ fontWeight: "700" }}>Bold </Text>
  <Text style={{ fontStyle: "italic" }}>Italic </Text>
  <Text style={{ letterSpacing: 4 }}>Spaced</Text>
</Text>
```

Text style classes also pass down from views, as in CSS: `<View class="text-white text-sm">` styles the `<Text>` and `<Input>` elements inside it.

### Links

A span with `onPress` (or `onLongPress`) is a link: a press on its characters goes to the span, a press elsewhere to the paragraph. Links in nested spans work too.

```tsx
<Text onPress={() => log("the sentence")}>
  Read the <Text style={{ color: "#0969da" }} onPress={() => open(termsUrl)}>terms</Text> first.
</Text>
```

### Backgrounds of spans

A span's `backgroundColor` and `borderRadius` paint a box behind its characters on each line it reaches, as tall as
its font; the text lays out the same and copies whole, so inline code is a span
([Text element](../elements/text.md#span-backgrounds)):

```tsx
<Text>
  Run <Text style={{ fontFamily: "ui-monospace", backgroundColor: "#eff1f3", borderRadius: 4 }}> npm test </Text> first.
</Text>
```

### Views and images inside text

Any element other than `<Text>` inside a `<Text>` is laid out inline, on the baseline, and wraps with the words. It can take presses of its own:

```tsx
<Text style={{ fontSize: 16, lineHeight: 26 }}>
  Made with <Image src={logo} style={{ width: 18, height: 18 }} /> nsh. Status{" "}
  <View style={{ backgroundColor: "#1a7f37", borderRadius: 9, paddingHorizontal: 7, height: 18, justifyContent: "center" }}>
    <Text style={{ color: "#ffffff", fontSize: 11, fontWeight: "700" }}>LIVE</Text>
  </View>
</Text>
```

Inline elements cut off by `numberOfLines` or an ellipsis are hidden; copying the text leaves them out.

## Text styles

| Property | Values |
|---|---|
| `color` | A [color](./styling.md#colors). Default black |
| `fontSize` | Points. Default 14 |
| `fontWeight` | `"normal"` (400), `"bold"` (700), or `100`–`900` (number or string) |
| `fontStyle` | `"normal"`, `"italic"` |
| `fontFamily` | A system family, a generic family, or an [imported font](./fonts.md) |
| `lineHeight` | Points |
| `letterSpacing` | Points |
| `textAlign` | `"auto"`, `"left"`, `"center"`, `"right"`, `"justify"` |
| `textAlignVertical` | `"auto"`, `"top"`, `"center"`, `"bottom"`: where text shorter than its box sits |
| `textTransform` | `"none"`, `"uppercase"`, `"lowercase"`, `"capitalize"` |
| `textDecorationLine` | `"none"`, `"underline"`, `"line-through"`, `"underline line-through"` |
| `textDecorationColor`, `textDecorationStyle` | Color; `"solid"`, `"double"`, `"dotted"`, `"dashed"`, `"wavy"` |
| `textDecorationThickness`, `textUnderlineOffset` | Points, or `"auto"` (thickness also `"from-font"`) |
| `textShadowColor`, `textShadowOffset`, `textShadowRadius` | A shadow behind the glyphs |
| `fontVariant` | OpenType features: `["tabular-nums"]`, `"small-caps"`, `"oldstyle-nums"`, `"lining-nums"`, `"proportional-nums"` |
| `writingDirection` | `"auto"`, `"ltr"`, `"rtl"`: the base direction of the paragraph |

A `<Text>` is also a view: it takes backgrounds, borders, corners, padding and the other [view styles](./styling.md).

```tsx
<Text style={{ fontSize: 18, fontVariant: ["tabular-nums"] }}>{price().toFixed(2)}</Text>
<Text style={{ textDecorationLine: "underline", textDecorationStyle: "wavy", textDecorationColor: "#cf222e" }}>Typo</Text>
<Text style={{ fontSize: 26, fontWeight: "800", color: "#ffffff", textShadowColor: "rgba(0,0,0,0.6)", textShadowOffset: { width: 2, height: 2 }, textShadowRadius: 4 }}>
  Shadow
</Text>
```

`tabular-nums` gives every digit the same width, which keeps counters and prices from jittering as they change.

## Truncation

```tsx
<Text numberOfLines={2}>{longDescription}</Text>
<Text numberOfLines={1} ellipsizeMode="middle">{filePath}</Text>
```

| Prop | Description |
|---|---|
| `numberOfLines` | The most lines shown; `0` (default) is unlimited |
| `ellipsizeMode` | Where text that does not fit is cut: `"tail"` (default), `"head"`, `"middle"`, `"clip"` |
| `adjustsFontSizeToFit` | Shrink the font until the text fits its lines and box |
| `minimumFontScale` | How far `adjustsFontSizeToFit` may shrink (0–1) |

```tsx
<Text numberOfLines={1} adjustsFontSizeToFit minimumFontScale={0.5} style={{ fontSize: 48, width: 200 }}>
  {total()}
</Text>
```

The Tailwind classes `truncate` and `line-clamp-*` do the same and lose to the props when both are set.

## System text size

Text follows the user's text size setting (Dynamic Type on iOS, font size on Android) by default.

| Prop | Description |
|---|---|
| `allowFontScaling` | `false` keeps the size fixed. Default `true` |
| `maxFontSizeMultiplier` | The largest scale applied, e.g. `1.5`. `0` is no limit |

`fontScale()` from [`nsh/device`](../modules/nsh-device.md) is the current scale, an accessor that changes when the user changes the setting. `<Input>` takes `allowFontScaling` and `maxFontSizeMultiplier` too.

```tsx
<Text maxFontSizeMultiplier={1.3} style={{ fontSize: 13 }}>Tab label</Text>
```

Prefer limiting the scale over turning it off: large text is an accessibility setting people rely on.

## Selection

`selectable` lets the user select and copy the text, as in the system's own text views: a long press selects the word under the finger, the handles move the selection's ends, and the edit menu offers Copy and Select All (iOS adds Look Up, Translate and Share; Android Share). A selection stays inside one text block. `selectionColor` colors the highlight and the handles. The class `select-text` does the same as `selectable`.

In desktop browsers text is selectable by default, as on a web page, except inside interactive elements (views with press or touch handlers, such as buttons and pressable rows); `selectable={false}` or the class `select-none` keeps a text unselectable there. Phone and tablet browsers follow the native rule.

```tsx
<Text selectable selectionColor="#7c3aed">{orderNumber}</Text>
```

To copy from code (a Copy button), use [`nsh/clipboard`](../modules/nsh-clipboard.md).

## onTextLayout

`onTextLayout` reports the laid-out lines after each layout, in the text's content box:

```tsx
<Text
  onTextLayout={(e) => {
    lines.set(e.lines.length);
    console.log(e.lines.map((l) => `${l.text} (${l.width.toFixed(0)}pt)`));
  }}
>
  {body()}
</Text>
```

Each line has `text`, `x`, `y`, `width`, `height`, `ascender`, `descender`, `capHeight` and `xHeight`. Use it to show a "Read more" button only when text was cut, or to align other content with a baseline.

## Accessibility

A `<Text>` is read by screen readers as its content. Give a role to text that acts as a heading (`accessibilityRole="header"`) and to links (`accessibilityRole="link"`). See [Accessibility](./accessibility.md).

## Related

- [Fonts](./fonts.md): custom fonts and weights.
- [text element](../elements/text.md): every prop.
- [Forms and keyboard](./forms-and-keyboard.md): text input.
