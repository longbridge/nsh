---
title: nsh/clipboard
description: Clipboard：把文本放到系统剪贴板上，并把它读回来。
order: 8.5
---

# nsh/clipboard

`nsh/clipboard` 读写系统剪贴板上的文本：iOS 上是 `UIPasteboard`，Android 上是 `ClipboardManager`，Web 上是 `navigator.clipboard`。

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

用户也可以自己从[可选文本](../elements/text.md)中复制；`Clipboard` 用于在代码中复制，比如“复制”按钮。

## Clipboard.setString

```ts
setString(text: string): void;
```

把 `text` 放到剪贴板上，替换原有内容。

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
  <Text>{copied() ? "已复制" : "复制"}</Text>
</View>;
```

## Clipboard.getString

```ts
getString(): Promise<string>;
```

返回剪贴板上的文本，没有文本时返回 `""`。在 iOS 上，读取剪贴板可能会显示系统的粘贴提示（“某应用从……粘贴”）；在 Android 10 及以上，只有前台应用才能读取。请在用户要求粘贴时再读取，而不是每进入一个页面就读。

```ts
const pasted = await Clipboard.getString();
if (pasted) code.set(pasted.trim());
```

## Clipboard.hasString

```ts
hasString(): Promise<boolean>;
```

返回剪贴板上是否有文本，但不读取它：iOS 上不会出现粘贴提示。可以用它决定是否显示“粘贴”按钮。在 Web 上，浏览器只能通过读取来判断，因此可能会请求用户授权。

```ts
const canPaste = signal(false);
Clipboard.hasString().then((has) => canPaste.set(has));
```

> [!NOTE]
> 如果应用在 `nsh.json` 中声明了 `capabilities`，必须用 `"clipboard": true` 授予剪贴板。否则 `setString` 不做任何事，`getString` 返回 `""`，`hasString` 返回 `false`。见[沙箱与能力](../docs/sandbox-and-capabilities.md)。
