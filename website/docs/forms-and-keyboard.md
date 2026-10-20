---
title: Forms and the keyboard
description: Text input with the input element - controlled and uncontrolled values, keyboard types, the return key, focus, selection, growing multiline inputs - and keeping content clear of the software keyboard.
order: 42
group: ui
demo: guide-forms
---

# Forms and the keyboard

Text entry is the `<Input>` element: a UITextField or UITextView on iOS, an EditText on Android. Typing, the caret, selection, autocorrection and autofill are the platform's own; your app gets the text through events and can control it. NSH also keeps focused inputs above the software keyboard on its own, and gives you the keyboard's height as a reactive value for the cases it cannot handle by itself.

`<Input>` takes styles and Tailwind classes like any view, so the fields of a form have your app's own look. A labeled field is a short component of your own:

```tsx
import { Input, signal, Text, View } from "nsh";

function Field(props: { label: string; value: string; onChange: (text: string) => void; masked?: boolean }) {
  const focused = signal(false);
  return (
    <View class="gap-1">
      <Text class="text-sm text-neutral-500">{props.label}</Text>
      <Input
        value={props.value}
        onChange={props.onChange}
        masked={props.masked}
        onFocus={() => focused.set(true)}
        onBlur={() => focused.set(false)}
        class="rounded-lg border px-3 py-2 text-base"
        style={{ borderColor: focused() ? "#0969da" : "#d0d7de" }}
      />
    </View>
  );
}
```

## Controlled and uncontrolled inputs

An input is **controlled** when you pass `value`: it shows what your signal holds, and you update the signal from `onChange`.

```tsx
import { Input, mount, signal, Text, View } from "nsh";

function App() {
  const name = signal("");
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 60, gap: 8 }}>
      <Input
        value={name()}
        onChange={(text) => name.set(text)}
        placeholder="Your name"
        style={{ fontSize: 17, borderWidth: 1, borderColor: "#d0d7de", borderRadius: 10, padding: 12 }}
      />
      <Text>Hello, {name() || "stranger"}</Text>
    </View>
  );
}

mount(App);
```

Because you control the value, you can transform what the user types: `onChange={(t) => code.set(t.replace(/[^0-9]/g, ""))}` keeps digits only, `onChange={(t) => tag.set(t.toUpperCase())}` uppercases.

### Input methods

Chinese, Japanese and Korean input methods compose text before committing it: pinyin turns into 腾 once a character is chosen. `onCompositionChange` reports the composition as it goes, `{ composing: true, text: "teng" }` while the user types and `{ composing: false, text: "" }` when it ends; the committed text reaches `onChange` before the end. On iOS, Android, macOS and the web, `onChange` also reports the text being composed, as those systems keep it in the input; on Windows, Linux and GPUI it reports the committed text. A controlled input keeps the composition going whatever the app does with the value: a `value` set while the user composes is shown once the composition ends, so an input that uppercases or filters its text takes pinyin as typed.

A search box that searches the committed text only:

```tsx
const query = signal("");
let composing = false;
<Input
  value={query()}
  onChange={(text) => {
    query.set(text);
    if (!composing) search(text);
  }}
  onCompositionChange={(e) => {
    composing = e.composing;
    if (!e.composing) search(query());
  }}
/>
```

An input is **uncontrolled** without `value`: it keeps its own text, starting at `defaultValue`, and you read it from `onChange` or `onSubmit` when you need it. Use this for fields you only read on submit, such as a password.

> [!NOTE]
> A controlled input never loses keystrokes. Every native edit carries an edit count, and a `value` coming back from JS is applied only if it answers the latest edit. If JS is slow and the user has typed more in the meantime, the stale value is dropped instead of overwriting the newer text.

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `value` | `string` | | The text, controlled by JS. |
| `defaultValue` | `string` | | The initial text of an uncontrolled input. |
| `placeholder` | `string` | | Shown while the input is empty. |
| `masked` | `boolean` | `false` | Hides the text (passwords). |
| `keyboardType` | `KeyboardType` | `"default"` | The keyboard layout. |
| `returnKeyType` | `ReturnKeyType` | `"default"` | The return key's label. |
| `autoCapitalize` | `"none" \| "sentences" \| "words" \| "characters"` | `"sentences"` | Automatic capitalization. |
| `autoCorrect` | `boolean` | `true` | The system's corrections. |
| `autoComplete` | `AutoComplete` | | What the input holds, for autofill. |
| `multiline` | `boolean` | `false` | Several lines; return inserts a line break unless `submitBehavior` says otherwise. |
| `maxLength` | `number` | | The most characters the user can enter. |
| `readonly` | `boolean` | `false` | The user can select and copy the text but not change it. |
| `autoFocus` | `boolean` | `false` | Focused when it appears. |
| `submitBehavior` | `"submit" \| "blurAndSubmit" \| "newline"` | single line `"blurAndSubmit"`, multiline `"newline"` | What return does: submit and keep the focus, submit and unfocus, or insert a line break (multiline only). |
| `caretHidden` | `boolean` | `false` | No caret is drawn. |
| `ios={{ keyboardAppearance }}` | `"default" \| "light" \| "dark"` | `"default"` | The keyboard's look (iOS). |
| `selectedRange` | `{ start: number; end?: number }` | | The controlled selection, in UTF-16 units. |
| `allowFontScaling` | `boolean` | `true` | Follow the system text size. |
| `maxFontSizeMultiplier` | `number` | | The upper bound of the system scale (0: none). |
| `ios={{ inputAccessoryViewID }}` | `string` | | iOS: the bar shown over the keyboard (see below). |
| `ios={{ textContentType }}` | `TextContentType` | | iOS's names for `autoComplete` values. |

Events:

| Event | Payload | When |
|---|---|---|
| `onChange` | `text: string` | The text changed. |
| `onSubmit` | `{ text }` | The return key was pressed in an input that submits (single line, or `submitBehavior` `"submit"` / `"blurAndSubmit"`). |
| `onFocus` / `onBlur` | | The input gained / lost focus. |
| `onSelectionChange` | `{ start, end }` | The caret or selection moved (UTF-16 units). |
| `onKeyPress` | `{ key }` | Before the text changes: `key` is the typed text, `"Backspace"` or `"Enter"`. |
| `onContentSizeChange` | `{ width, height }` | The text's size changed (at the input's width when multiline), as it is typed; an empty input reports one empty line, whatever its placeholder. |
| `onCompositionChange` | `{ composing, text }` | An input method's composition started, changed or ended (see [Input methods](#input-methods)). |

An input is styled like text: `fontSize`, `color`, `fontFamily`, `textAlign`, `letterSpacing` and the rest of [text styles](./text.md), plus borders, padding and backgrounds. The `focus:` Tailwind variant (or `focusStyle`) applies while the input has focus. The placeholder's color is the
`placeholderColor` style key (`placeholder:text-*`), and the caret and selection's color is `selectionColor`
(`selection:text-*`):

```tsx
<Input
  class="rounded-lg border border-zinc-300 px-3 py-2 text-base focus:border-blue-500 placeholder:text-zinc-400 selection:text-blue-600"
  placeholder="Search"
/>
```

## Keyboards and autofill

`keyboardType` picks the layout:

| Value | Keyboard |
|---|---|
| `"default"` | Text. |
| `"email-address"` | Text with `@` and `.` at hand. |
| `"url"` | Text with `/` and `.com`. |
| `"number-pad"` | Digits only: no sign, no decimal separator (codes, PINs). |
| `"decimal-pad"` | Digits and a decimal separator. |
| `"numeric"` | As React Native: iOS's decimal pad; on Android digits with a sign and a decimal separator. |
| `"numbers-and-punctuation"` | Digits, a minus sign and punctuation (iOS's numbers and punctuation keyboard; on Android digits with a sign and a decimal separator). Use it for negative numbers on iOS. |
| `"phone-pad"` | A phone dial pad. |

On iOS, `ios={{ keyboardAppearance: "dark" }}` (or `"light"`, `"default"`) gives the keyboard a light or dark look regardless of the system's appearance, for a dark app in light mode. Android's keyboards always follow the system's theme.

`autoComplete` tells the system what the field holds, so it can offer saved names, addresses, passwords and one-time codes from messages. It takes these values: `"name"`, `"given-name"`, `"family-name"`, `"email"`, `"tel"`, `"username"`, `"current-password"`, `"new-password"`, `"one-time-code"`, `"street-address"`, `"postal-code"`, `"country"`, `"cc-number"`, `"cc-exp"`, `"cc-csc"`, `"url"`, `"birthdate-full"` and others, or `"off"`. iOS's own names (`"emailAddress"`, `"oneTimeCode"`, ...) are accepted too, in `autoComplete` or `ios={{ textContentType }}`.

```tsx
<Input keyboardType="email-address" autoCapitalize="none" autoCorrect={false} autoComplete="email" placeholder="E-mail" />
<Input masked autoComplete="new-password" placeholder="Password" />
<Input keyboardType="number-pad" autoComplete="one-time-code" maxLength={6} placeholder="Code" />
```

## The return key and moving between fields

`returnKeyType` labels the return key: `"default"`, `"done"`, `"go"`, `"next"`, `"search"` or `"send"`. `submitBehavior` says what pressing it does:

| `submitBehavior` | Return | Default for |
|---|---|---|
| `"blurAndSubmit"` | fires `onSubmit`, then the input loses focus | single-line inputs |
| `"submit"` | fires `onSubmit`; the input keeps focus | |
| `"newline"` | inserts a line break (multiline inputs only) | multiline inputs |

A multiline input with `"submit"` or `"blurAndSubmit"` sends on return, as chat composers do, and still wraps its lines; on a hardware keyboard (desktops, a Mac, a tablet's keyboard) Shift+Return or Option / Alt+Return inserts a line break. A return that commits an input method's composition (Chinese or Japanese input, say) only commits it and never submits.

To move to the next field, keep refs to the inputs and call `focus()`:

```tsx
import { Input, mount, signal, View, type YNode } from "nsh";

function SignIn() {
  const email = signal("");
  let password!: YNode;
  const submit = () => console.log("sign in", email());
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 60, gap: 12 }}>
      <Input
        value={email()}
        onChange={(t) => email.set(t)}
        keyboardType="email-address"
        autoCapitalize="none"
        returnKeyType="next"
        submitBehavior="submit"
        onSubmit={() => password.focus()}
        placeholder="E-mail"
      />
      <Input ref={password} masked returnKeyType="go" onSubmit={submit} placeholder="Password" />
    </View>
  );
}

mount(SignIn);
```

`submitBehavior="submit"` on the first field keeps the keyboard up while focus moves, so it does not drop and rise again. A chat input that sends on return and stays focused uses `submitBehavior="submit"` (see the composer [below](#content-outside-scroll-views-keyboardavoidingview)).

Both `maxLength` and `submitBehavior` are enforced natively: the platform asks "may this character be added?" and "should return end editing?" as the user types, and NSH answers from the props without waiting for JS.

## Focus from code

An input's ref has:

| Method | Description |
|---|---|
| `focus()` | Focuses the input and shows the keyboard. |
| `blur()` | Unfocuses it. |
| `setSelection(start, end?)` | Selects `start..end` (UTF-16 units); a caret without `end`. |

`Keyboard.dismiss()` from `nsh/base` unfocuses whichever input has focus:

```tsx
import { View } from "nsh";
import { Keyboard } from "nsh/base";

<View onPress={() => Keyboard.dismiss()}>…</View>
```

## Selection

`onSelectionChange` reports the caret and selection; `selectedRange` sets them. Positions are UTF-16 code units, as JavaScript string indices are, so `text.slice(start, end)` is the selected text.

```tsx
const text = signal("Select part of this text");
const range = signal({ start: 0, end: 0 });
const selection = signal<{ start: number; end?: number } | undefined>(undefined);

<Input
  value={text()}
  onChange={(t) => text.set(t)}
  selectedRange={selection()}
  onSelectionChange={(r) => range.set(r)}
/>
<Text>Selected: "{text().slice(range().start, range().end)}"</Text>
<View onPress={() => selection.set({ start: 0, end: text().length })}><Text>Select all</Text></View>
```

## Multiline inputs that grow

A `multiline` input scrolls its text inside its own height. To grow it with its content, feed `onContentSizeChange`'s height back into its style, with a limit:

```tsx
const height = signal(0);

<Input
  multiline
  placeholder="Write a comment"
  onContentSizeChange={(e) => height.set(Math.min(e.height, 120))}
  style={{ fontSize: 16, padding: 10, borderWidth: 1, borderColor: "#d0d7de", borderRadius: 10, height: Math.max(height(), 20) + 20 }}
/>
```

The reported height is the text's (at the input's width), so add the vertical padding yourself, as above.

## Keeping inputs above the keyboard

### Inside scroll views: automatic

When the keyboard appears over a focused input that is inside a `<Scroll>` or `List`, NSH gives the scroll view a bottom inset as tall as the keyboard and scrolls the input into view. This is native and needs no code. A long form is simply a scroll view of inputs:

```tsx
<Scroll style={{ flex: 1 }} keyboardDismissMode="on-drag" keyboardShouldPersistTaps="handled">
  <View style={{ padding: 16, gap: 12 }}>
    {/* inputs */}
  </View>
</Scroll>
```

`keyboardDismissMode` (`"none"`, `"on-drag"`, `"interactive"`) lets dragging the form hide the keyboard; `keyboardShouldPersistTaps="handled"` lets a tap on a button go through while the keyboard is up instead of only dismissing it. See [Scrolling](./scrolling.md#scroll-views-and-the-keyboard).

### Content outside scroll views: KeyboardAvoidingView

A message bar pinned to the bottom of a screen is not in a scroll view, so nothing moves it. Wrap the screen in `KeyboardAvoidingView` from `nsh/base`: it makes room at the bottom as tall as the keyboard, animated with the keyboard.

```tsx
import { Input, Scroll, signal, View } from "nsh";
import { keyboardFrame, KeyboardAvoidingView, safeAreaInsets } from "nsh/base";

function ChatScreen() {
  const insets = safeAreaInsets();
  const draft = signal("");
  const send = () => {
    if (draft().trim()) console.log("send", draft());
    draft.set("");
  };
  return (
    <KeyboardAvoidingView style={{ flex: 1 }}>
      <Scroll style={{ flex: 1 }} keyboardDismissMode="interactive">
        {/* messages */}
      </Scroll>
      <View
        style={{
          flexDirection: "row",
          padding: 8,
          // Above the home indicator, unless the keyboard covers it.
          paddingBottom: 8 + (keyboardFrame().height > 0 ? 0 : insets().bottom),
        }}
      >
        <Input
          value={draft()}
          onChange={(t) => draft.set(t)}
          multiline
          submitBehavior="submit"
          returnKeyType="send"
          onSubmit={send}
          placeholder="Message"
          style={{ flex: 1, maxHeight: 120 }}
        />
      </View>
    </KeyboardAvoidingView>
  );
}
```

| Prop | Type | Default | Description |
|---|---|---|---|
| `behavior` | `"padding" \| "height"` | `"padding"` | `padding` pads the bottom by the keyboard's height; `height` shrinks the view (its background ends at the keyboard). |
| `enabled` | `boolean` | `true` | Avoid the keyboard. |
| `offset` | `number` | `0` | Added to the keyboard's height while it shows (a tab bar below the view, say). |

It assumes it reaches the bottom of the surface, as a screen's content does. On an Android host whose window resizes for the keyboard, the window itself shrinks and `KeyboardAvoidingView` has nothing to do. See [KeyboardAvoidingView](../modules/nsh-base.md#keyboardavoidingview).

### The keyboard accessor

`keyboardFrame()` from `nsh/base` is a reactive `{ height, duration }`: how far the keyboard reaches into the surface (0 when hidden; the bottom safe area under it included) and its animation's duration in seconds. Read it for anything custom: hide a floating button while typing, or size a panel.

```tsx
import { View } from "nsh";
import { keyboardFrame } from "nsh/base";

<If when={keyboardFrame().height === 0}>
  <View style={{ position: "absolute", right: 16, bottom: 16 }}>…</View>
</If>
```

## A bar over the keyboard (iOS): InputAccessoryView

`InputAccessoryView` from `nsh/ios` shows a bar attached to the top of the keyboard while an input that names it has focus: iOS's `inputAccessoryView`. It moves with the keyboard natively, including during interactive dismissal. It renders nothing where it is placed in the tree; inputs refer to it by `nativeID`:

```tsx
import { Input, signal, Text, View, type YNode } from "nsh";
import { InputAccessoryView } from "nsh/ios";

function Note() {
  const text = signal("");
  let input!: YNode;
  return (
    <View style={{ padding: 16 }}>
      <Input
        ref={input}
        value={text()}
        onChange={(t) => text.set(t)}
        ios={{ inputAccessoryViewID: "note-bar" }}
        placeholder="A note"
      />
      <InputAccessoryView nativeID="note-bar" style={{ backgroundColor: "#f6f8fa" }}>
        <View style={{ flexDirection: "row", alignItems: "center", padding: 8 }}>
          <Text style={{ flex: 1, color: "#57606a" }}>{text().length} characters</Text>
          <Text onPress={() => input.blur()} style={{ color: "#0969da", fontWeight: "600" }}>Done</Text>
        </View>
      </InputAccessoryView>
    </View>
  );
}
```

The bar is as wide as the window and as tall as its content. Android has no input accessories: there `InputAccessoryView` renders nothing, and `isAvailable` from `nsh/ios` is `false`, so plan another place for those controls (for example above the input) when `Platform.OS === "android"`.

## Related

- [`Input`](../elements/input.md): the element reference.
- [`KeyboardAvoidingView`](../modules/nsh-base.md#keyboardavoidingview) and [`Keyboard`](../modules/nsh-base.md#keyboard).
- [`nsh/ios`](../modules/nsh-ios.md): `InputAccessoryView`.
- [Scrolling](./scrolling.md#scroll-views-and-the-keyboard): keyboard dismissal and taps in scroll views.
