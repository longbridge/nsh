---
title: nsh/clipboard
description: "Clipboard: put text on the system clipboard and read it back."
order: 8.5
---

# nsh/clipboard

`nsh/clipboard` reads and writes the text on the system clipboard: `UIPasteboard` on iOS, `ClipboardManager` on Android, `navigator.clipboard` on the web.

```ts
import { Clipboard } from "nsh/clipboard";
```

```ts
const Clipboard: {
  setString(text: string): void;
  getString(): Promise<string>;
  hasString(): Promise<boolean>;
};
```

The user can also copy from a [selectable text](../elements/text.md) themselves; `Clipboard` is for copying from code, such as a Copy button.

## Clipboard.setString

```ts
setString(text: string): void;
```

Puts `text` on the clipboard, replacing what was there.

```tsx
import { signal, Text, View } from "nsh";
import { Clipboard } from "nsh/clipboard";

const copied = signal(false);

<View
  onPress={() => {
    Clipboard.setString(code);
    copied.set(true);
  }}
>
  <Text>{copied() ? "Copied" : "Copy"}</Text>
</View>;
```

## Clipboard.getString

```ts
getString(): Promise<string>;
```

Resolves with the clipboard's text, or `""` when it holds none. On iOS, reading the clipboard can show the system's paste notice ("App pasted from …"), and on Android 10 and later only the app in the foreground can read it. Read it when the user asks to paste, not on every screen.

```ts
const pasted = await Clipboard.getString();
if (pasted) code.set(pasted.trim());
```

## Clipboard.hasString

```ts
hasString(): Promise<boolean>;
```

Resolves with whether the clipboard holds text, without reading it: no paste notice on iOS. Use it to decide whether to show a Paste button. On the web, browsers only tell by reading, so it may ask the user for permission.

```ts
const canPaste = signal(false);
Clipboard.hasString().then((has) => canPaste.set(has));
```

> [!NOTE]
> When your app declares `capabilities` in `nsh.json`, the clipboard must be granted with `"clipboard": true`. Without it `setString` does nothing, `getString` resolves `""` and `hasString` `false`. See [Sandbox and capabilities](../docs/sandbox-and-capabilities.md).
