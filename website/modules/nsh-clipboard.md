---
title: nsh/clipboard
description: "Clipboard: put text and images on the system clipboard, and read text, images and copied files back."
order: 8.5
---

# nsh/clipboard

`nsh/clipboard` reads and writes the system clipboard: text everywhere (`UIPasteboard` on iOS, `ClipboardManager` on Android, `NSPasteboard`, GDK and the Win32 clipboard on desktops, `navigator.clipboard` on the web), images on iOS, the desktops and the web, and the files copied in a file manager on the desktops and Mac Catalyst.

```ts
import { Clipboard } from "nsh/clipboard";
```

```ts
const Clipboard: {
  setString(text: string): void;
  getString(): Promise<string>;
  hasString(): Promise<boolean>;
  setImage(uri: string): void;
  getImage(): Promise<ClipboardImage | null>;
  hasImage(): Promise<boolean>;
  getFiles(): Promise<string[]>;
};

interface ClipboardImage {
  uri: string; // data:image/png;base64,…
  width: number; // pixels
  height: number;
}
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

## Clipboard.getImage

```ts
getImage(): Promise<ClipboardImage | null>;
```

Resolves with the clipboard's image, or `null` when it holds none: a pasted screenshot, or an image copied in a browser
or an image editor. `uri` is a `data:` URI of the image (PNG, or the clipboard's own JPEG, GIF, WebP or BMP), which an
[`Image`](../elements/image.md) shows as its `src`; `fetch(uri)` gives its bytes, for an upload. `width` and `height` are
its size in pixels. As with `getString`, read it when the user asks to paste.

```tsx
import { signal, View } from "nsh";
import { Image } from "nsh/image";
import { Clipboard, type ClipboardImage } from "nsh/clipboard";

const attachments = signal<ClipboardImage[]>([]);

async function paste() {
  const image = await Clipboard.getImage();
  if (image) attachments.set([...attachments(), image]);
}

async function upload(image: ClipboardImage) {
  const body = new FormData();
  body.append("file", await (await fetch(image.uri)).blob(), "screenshot.png");
  await fetch("https://example.com/upload", { method: "POST", body });
}

<View class="flex-row gap-2">
  <Each of={attachments()}>
    {(image) => <Image src={image().uri} style={{ width: 64, height: 64, borderRadius: 8 }} />}
  </Each>
</View>;
```

A chat-style app where users paste screenshots into a composer binds the paste shortcut while its input has the focus
and takes the image when there is one:

```ts
import { keymap } from "nsh/keymap";

keymap.bind("mod+v", async () => {
  if (await Clipboard.hasImage()) return paste();
  insertText(await Clipboard.getString());
}, { when: "always" });
```

## Clipboard.hasImage

```ts
hasImage(): Promise<boolean>;
```

Resolves with whether the clipboard holds an image, without reading it: no paste notice on iOS.

## Clipboard.setImage

```ts
setImage(uri: string): void;
```

Puts the image of `uri`, a base64 `data:` URI such as `getImage` gives, on the clipboard, replacing what it holds. On
Windows the image is a PNG, which browsers, Office and the Snipping Tool read.

```ts
Clipboard.setImage(image.uri);
```

## Clipboard.getFiles

```ts
getFiles(): Promise<string[]>;
```

Resolves with the `file:` URLs of the files and folders copied in a file manager (Finder, Explorer, Files), in their
order, or `[]`: the same URLs a [file drop](../elements/view.md#desktop-file-drops) gives. Parse a URL to get its path.

```ts
const files = await Clipboard.getFiles();
const names = files.map((uri) => decodeURIComponent(new URL(uri).pathname.split("/").pop()!));
```

> [!NOTE]
> When your app declares `capabilities` in `nsh.json`, the clipboard must be granted with `"clipboard": true`. Without it `setString` and `setImage` do nothing, `getString` resolves `""`, `getImage` `null`, `getFiles` `[]` and `hasString` and `hasImage` `false`. See [Sandbox and capabilities](../docs/sandbox-and-capabilities.md).
