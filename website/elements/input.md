---
title: Input
description: "The native text field: controlled and uncontrolled text, keyboards, autofill, selection, multiline inputs and focus."
order: 6
demo: el-input
---

# Input

`<Input>` is a native text field: a UITextField (single line) or UITextView (`multiline`) on iOS and Mac Catalyst, an
EditText on Android. Typing, the caret, selection, autocorrection and autofill are the platform's own.

```tsx
import { Input, signal } from "nsh";

function NameField() {
  const name = signal("");
  return (
    <Input
      value={name()}
      onChange={(text) => name.set(text)}
      placeholder="Your name"
      autoCapitalize="words"
      returnKeyType="done"
      style={{ fontSize: 16, padding: 12, borderWidth: 1, borderColor: "#d0d7de", borderRadius: 8 }}
    />
  );
}
```

## Controlled and uncontrolled

- **Controlled**: pass `value` and update it in `onChange`. JS can rewrite what the user typed (trim it, format it):
  the text shows the edit at once and is corrected when the new `value` arrives. Each native edit carries a count, and a
  `value` from JS is applied only if it is based on the latest edit, so a slow JS turn never overwrites newer typing.
- **Uncontrolled**: pass `defaultValue` (or nothing) and read the text from `onChange` or `onSubmit`.

Rules that must hold while the user types are props, answered natively without asking JS: `maxLength` limits the length
and `submitBehavior` decides what return does. See [forms and the keyboard](../docs/forms-and-keyboard.md).

## Props

Besides the [common props](./index.md#common-props) (with `style` typed as `TextStyle`: font, color, alignment and the
view styles apply):

### Text

| Prop | Type | Default | Description |
|---|---|---|---|
| `value` | `string` | | The text, controlled by JS. |
| `defaultValue` | `string` | | The initial text. |
| `placeholder` | `string` | | Shown while the input is empty. |
| `multiline` | `boolean` | `false` | Several lines; return inserts a line break unless `submitBehavior` says otherwise. |
| `maxLength` | `number` | | The most characters the user can enter. |
| `readonly` | `boolean` | `false` | The user can select and copy the text but not change it. |
| `masked` | `boolean` | `false` | The text is hidden (passwords). |
| `allowFontScaling` | `boolean` | `true` | Follow the system text size. |
| `maxFontSizeMultiplier` | `number` | `0` | The upper bound of the system scale (0 is none). |

### Keyboard

| Prop | Type | Default | Description |
|---|---|---|---|
| `keyboardType` | `"default" \| "number-pad" \| "decimal-pad" \| "numeric" \| "numbers-and-punctuation" \| "email-address" \| "phone-pad" \| "url"` | `"default"` | The keyboard layout ([the values](../docs/forms-and-keyboard.md#keyboards-and-autofill); `numbers-and-punctuation` has a minus sign on iOS). |
| `returnKeyType` | `"default" \| "done" \| "go" \| "next" \| "search" \| "send"` | `"default"` | The label of the return key. |
| `autoCapitalize` | `"none" \| "sentences" \| "words" \| "characters"` | `"sentences"` | Automatic capitalization. |
| `autoCorrect` | `boolean` | `true` | The system's corrections. |
| `autoComplete` | `AutoComplete \| TextContentType` | | What the input holds, for autofill: `"username"`, `"current-password"`, `"new-password"`, `"one-time-code"`, `"email"`, `"tel"`, `"postal-code"`, `"cc-number"`, `"off"`, … |
| `submitBehavior` | `"submit" \| "blurAndSubmit" \| "newline"` | `"blurAndSubmit"` (single line), `"newline"` (multiline) | What return does: `"submit"` reports `onSubmit` and keeps the focus, `"blurAndSubmit"` also unfocuses, `"newline"` inserts a line break (multiline only; a single line submits). A multiline input that submits still takes line breaks from Shift+Return or Option / Alt+Return on hardware keyboards. `"submit"` keeps the keyboard up (e.g. to send several messages). |
| `autoFocus` | `boolean` | `false` | Focused when it appears. |

### Caret and selection

| Prop | Type | Description |
|---|---|---|
| `caretHidden` | `boolean` | No caret is drawn. |
| `selectedRange` | `{ start: number; end?: number }` | The controlled selection, in UTF-16 units; `end` defaults to `start` (a caret). |

### Colors

The placeholder's color and the color of the caret and the selection are style keys, with their Tailwind variants:

| Style key | Class | Description |
|---|---|---|
| `placeholderColor` | `placeholder:text-*` | The placeholder's color. |
| `selectionColor` | `selection:text-*` | The color of the caret and the selection. |

```tsx
<Input placeholder="Search" class="text-zinc-900 placeholder:text-zinc-400 selection:text-blue-600" />
<Input placeholder="Search" style={{ placeholderColor: "#9ca3af", selectionColor: "#2563eb" }} />
```

### iOS

| `ios` key | Type | Description |
|---|---|---|
| `textContentType` | `AutoComplete \| TextContentType` | iOS's names for `autoComplete` (`"oneTimeCode"`, `"newPassword"`, …). |
| `inputAccessoryViewID` | `string` | The `nativeID` of an `InputAccessoryView` ([`nsh/ios`](../modules/nsh-ios.md)) shown over the keyboard while the input has focus. |
| `keyboardAppearance` | `"default" \| "light" \| "dark"` | The keyboard's look; `"default"` follows the system's appearance. Android's keyboards follow the system's theme. |

```tsx
<Input autoComplete="one-time-code" keyboardType="number-pad" ios={{ textContentType: "oneTimeCode" }} />
```

## Events

| Event | Payload | When |
|---|---|---|
| `onChange` | `(text: string)` | The user changed the text. |
| `onSubmit` | `{ text }` | The return key was pressed in an input that submits (single line, or `submitBehavior` `"submit"` / `"blurAndSubmit"`). |
| `onFocus` / `onBlur` | none | The input gained / lost focus. |
| `onSelectionChange` | `{ start, end }` | The caret or selection moved (UTF-16 units). |
| `onKeyPress` | `{ key }` | Before the text changes: `key` is the typed text, `"Backspace"` or `"Enter"`. |
| `onContentSizeChange` | `{ width, height }` | The size of the text changed (at the input's width when multiline), as it is typed; an empty input reports one empty line, whatever its placeholder. |

A multiline input that grows with its text:

```tsx
const height = signal(40);
<Input
  multiline
  onContentSizeChange={(e) => height.set(Math.min(120, Math.max(40, e.height + 16)))}
  style={{ height: height(), paddingHorizontal: 12, paddingVertical: 8 }}
/>;
```

## Ref methods

| Method | Description |
|---|---|
| `focus()` | Focuses the input and shows the keyboard. |
| `blur()` | Unfocuses it. |
| `setSelection(start, end?)` | Selects `start..end` (UTF-16 units); a caret without `end`. |

Moving focus to the next field on return:

```tsx
let password!: YNode;
<>
  <Input returnKeyType="next" onSubmit={() => password.focus()} />
  <Input ref={password} masked autoComplete="current-password" />
</>;
```

`Keyboard.dismiss()` from [`nsh/base`](../modules/nsh-base.md#keyboard) hides the keyboard whichever input has it.

## Platform notes

- Inputs inside a [`<Scroll>`](./scroll.md#keyboard) are kept clear of the keyboard natively. Elsewhere use
  `KeyboardAvoidingView`.
- `disabled` disables the input (no focus) as well as applying `disabled:` classes; a `readonly` input keeps its
  focus and selection.
- `inputAccessoryViewID` has no effect on Android, which has no keyboard accessory bar.
