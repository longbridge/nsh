---
title: nsh/clipboard
description: Clipboard：把文本和图片放到系统剪贴板上，并读回文本、图片和复制的文件。
order: 8.5
---

# nsh/clipboard

剪贴板 API 读写系统剪贴板：各平台都能读写文本（iOS 上是 `UIPasteboard`，Android 上是 `ClipboardManager`，桌面上是 `NSPasteboard`、GDK 和 Win32 剪贴板，Web 上是 `navigator.clipboard`）；iOS、桌面和 Web 还能读写图片，桌面和 Mac Catalyst 还能读到在文件管理器里复制的文件。Rust 代码调用 `nsh_core::ui::prelude` 中的函数（`set_clipboard`、`clipboard_string` 等）；TSX 从 `nsh/clipboard` 导入 `Clipboard`。

```rust tabs
use nsh_core::ui::prelude::*; // set_clipboard、clipboard_string、clipboard_image……
use nsh_core::ui::ClipboardImage;
```

```tsx tabs
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
  width: number; // 像素
  height: number;
}
```

| TS | Rust（`nsh_core::ui::prelude`） |
|---|---|
| `Clipboard.setString(text)` | `set_clipboard(text)` |
| `await Clipboard.getString()` | `clipboard_string().await -> String` |
| `await Clipboard.hasString()` | `clipboard_has_string().await -> bool` |
| `Clipboard.setImage(uri)` | `set_clipboard_image(uri)` |
| `await Clipboard.getImage()` | `clipboard_image().await -> Option<ClipboardImage>` |
| `await Clipboard.hasImage()` | `clipboard_has_image().await -> bool` |
| `await Clipboard.getFiles()` | `clipboard_files().await -> Vec<String>` |

Rust 的读取函数返回一个应答，在任务中（`spawn`，或 `async` 模块函数）await 它；`ClipboardImage` 是 `nsh_core::ui::ClipboardImage`，含 `uri`、`width` 和 `height`（`DevicePixels`）。

用户也可以自己从[可选文本](../elements/text.md)中复制；剪贴板 API 用于在代码中复制，比如“复制”按钮。

## Clipboard.setString

```ts
setString(text: string): void;
```

把 `text` 放到剪贴板上，替换原有内容。Rust：`set_clipboard(text)`。

```rust tabs
use nsh_core::ui::prelude::*;

fn copy_button(code: Signal<String>) -> impl IntoElement {
    let copied = signal(false);
    view()
        .role(AccessibilityRole::Button)
        .on_press(move || {
            set_clipboard(code.get());
            copied.set(true);
        })
        .child(text(move || if copied.get() { "已复制" } else { "复制" }))
}
```

```tsx tabs
import { signal, Text, View } from "nsh";
import { Clipboard } from "nsh/clipboard";

const copied = signal(false);

<View
  onPress={() => {
    Clipboard.setString(code());
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

返回剪贴板上的文本，没有文本时返回 `""`。Rust：`clipboard_string()`。在 iOS 上，读取剪贴板可能会显示系统的粘贴提示（“某应用从……粘贴”）；在 Android 10 及以上，只有前台应用才能读取。请在用户要求粘贴时再读取，而不是每进入一个页面就读。

```rust tabs
spawn(async move {
    let pasted = clipboard_string().await;
    if !pasted.is_empty() {
        code.set(pasted.trim().to_string());
    }
});
```

```tsx tabs
const pasted = await Clipboard.getString();
if (pasted) code.set(pasted.trim());
```

## Clipboard.hasString

```ts
hasString(): Promise<boolean>;
```

返回剪贴板上是否有文本，但不读取它：iOS 上不会出现粘贴提示。Rust：`clipboard_has_string()`。可以用它决定是否显示“粘贴”按钮。在 Web 上，浏览器只能通过读取来判断，因此可能会请求用户授权。

```rust tabs
let can_paste = signal(false);
spawn(async move { can_paste.set(clipboard_has_string().await) });
```

```tsx tabs
const canPaste = signal(false);
Clipboard.hasString().then((has) => canPaste.set(has));
```

## Clipboard.getImage

```ts
getImage(): Promise<ClipboardImage | null>;
```

返回剪贴板上的图片，没有时为 `null`：粘贴的截图，或在浏览器、图片编辑器里复制的图片。Rust：`clipboard_image()`，没有时为 `None`。`uri` 是图片的 `data:` URI（PNG，或剪贴板自带的 JPEG、GIF、WebP、BMP），可直接作为 [`Image`](../elements/image.md) 的 `src`；在 TSX 中用 `fetch(uri)` 拿到它的字节用于上传，在 Rust 中解码逗号之后的 base64 即得到字节。`width` 和 `height` 是像素尺寸。和 `getString` 一样，在用户要求粘贴时再读。可读取的图片最大 48 MB（编码后），这样 `data:` URI 能放进应用的内存；更大的图片 `getImage` 返回 `null`，`hasImage` 返回 `true`。

```rust tabs
use base64::Engine as _;
use nsh_core::ui::{ClipboardImage, prelude::*};

fn attachments() -> impl IntoElement {
    let attachments = signal(Vec::<ClipboardImage>::new());
    let paste = move || {
        spawn(async move {
            if let Some(image) = clipboard_image().await {
                attachments.update(|list| list.push(image));
            }
        });
    };
    view()
        .flex_row()
        .gap_2()
        .child(text("粘贴").on_press(paste))
        .child(each(
            attachments,
            |shot: &ClipboardImage| shot.uri.clone(),
            |shot, _| image(move || shot.with(|s| s.uri.clone())).size(px(64.0)).rounded(px(8.0)),
        ))
}

async fn upload(image: &ClipboardImage) -> Result<HttpResponse, String> {
    let (_, data) = image.uri.split_once(',').ok_or("not a data: URI")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| e.to_string())?;
    let request = HttpRequest::post("https://example.com/upload", bytes).header("Content-Type", "image/png");
    fetch(request).await
}
```

```tsx tabs
import { Image, signal, View } from "nsh";
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

聊天类应用要让用户把截图粘贴进输入框，可以在输入框有焦点时绑定粘贴快捷键，剪贴板有图片时取图片：

```rust tabs
use nsh_core::keymap::When;

on_key_with("mod+v", KeyOptions { when: When::Always, ..KeyOptions::default() }, move |_| {
    spawn(async move {
        if clipboard_has_image().await {
            paste().await;
        } else {
            insert_text(&clipboard_string().await);
        }
    });
});
```

```tsx tabs
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

返回剪贴板上是否有图片，不读取内容：iOS 上不会出现粘贴提示。Rust：`clipboard_has_image()`。

## Clipboard.setImage

```ts
setImage(uri: string): void;
```

把 `uri`（base64 `data:` URI，例如 `getImage` 给的）的图片放到剪贴板上，替换原有内容。Rust：`set_clipboard_image(uri)`。Windows 上接受 PNG，这是浏览器、Office 和截图工具读取的格式。

```rust tabs
set_clipboard_image(image.uri.clone());
```

```tsx tabs
Clipboard.setImage(image.uri);
```

## Clipboard.getFiles

```ts
getFiles(): Promise<string[]>;
```

返回在文件管理器（访达、资源管理器、Files）里复制的文件和文件夹的 `file:` URL，按原顺序，没有时为 `[]`：与[文件拖放](../elements/view.md#桌面文件拖放)给出的 URL 相同。Rust：`clipboard_files()`。解析 URL 得到路径。

```rust tabs
let paths: Vec<std::path::PathBuf> = clipboard_files()
    .await
    .iter()
    .filter_map(|uri| url::Url::parse(uri).ok()?.to_file_path().ok())
    .collect();
```

```tsx tabs
const files = await Clipboard.getFiles();
const names = files.map((uri) => decodeURIComponent(new URL(uri).pathname.split("/").pop()!));
```

> [!NOTE]
> 如果应用在 `nsh.json` 中声明了 `capabilities`，必须用 `"clipboard": true` 授予剪贴板。否则 `setString` 和 `setImage` 不做任何事，`getString` 返回 `""`，`getImage` 返回 `null`，`getFiles` 返回 `[]`，`hasString` 和 `hasImage` 返回 `false`，Rust 与 TSX 都是如此。见[沙箱与能力](../docs/sandbox-and-capabilities.md)。
