---
title: nsh/keymap
description: 键盘快捷键：把 mod+k 这样的按键组合绑定到处理函数，支持 macOS、Windows、Linux、Web，以及接了硬件键盘的 iOS 和 Android。
order: 8.7
---

# nsh/keymap

快捷键 API 绑定应用的键盘快捷键：一个按键组合（如 `"mod+k"`）和它要运行的函数。Rust 代码调用 `nsh_core::ui::prelude` 中的 `on_key`、`on_key_with` 和 `key_label`；TSX 从 `nsh/keymap` 导入 `keymap`。两者接受同样的按键组合，遵循同样的规则。

```rust tabs
use nsh_core::ui::prelude::*; // on_key、on_key_with、KeyOptions、key_label
```

```tsx tabs
import { keymap } from "nsh/keymap";
```

```ts
const keymap: {
  bind(keystroke: string, handler: (keystroke: string) => void, options?: KeyBindingOptions): () => void;
  bind(bindings: Record<string, (keystroke: string) => void>, options?: KeyBindingOptions): () => void;
  label(keystroke: string): string;
};

interface KeyBindingOptions {
  when?: "always" | "notEditing";
  repeat?: boolean;
}
```

| TS | Rust |
|---|---|
| `keymap.bind(keystroke, handler)` | `on_key(keystroke, handler)` |
| `keymap.bind(keystroke, handler, { when, repeat })` | `on_key_with(keystroke, KeyOptions { when, repeat }, handler)`；`when` 是 `nsh_core::keymap::When`（`Auto`、`Always`、`NotEditing`） |
| `keymap.bind({ … })` | 每个按键组合一次 `on_key` |
| `keymap.label(keystroke)` | `key_label(keystroke)` |

Rust 绑定与创建它的组件（所有者）同生命周期；处理函数收到的是你写下的按键组合。

快捷键在 macOS、Windows、Linux 和浏览器中可用（浏览器自己的按键，比如 Ctrl+T 或 Cmd+W，不会传给页面），在接了硬件键盘的 iOS 和 Android 上也可用：iPad 的键盘、在 Mac 上运行的 iPad 应用、平板或 Chromebook 的键盘，以及模拟器的键盘。同一份代码在所有平台上运行；没有键盘时，不会有按键触发这些绑定。

## keymap.bind

```ts
bind(keystroke: string, handler: (keystroke: string) => void, options?: KeyBindingOptions): () => void;
```

按下 `keystroke` 时运行 `handler`，并返回一个移除该绑定的函数。处理函数收到的是你写下的按键组合字符串，大小写保持原样。Rust：`on_key(keystroke, handler)`。

```rust tabs
use nsh_core::ui::prelude::*;

#[component]
pub fn App() -> impl IntoElement {
    let open = signal(false);
    on_key("mod+k", move |_| open.set(true));
    on_key("escape", move |_| open.set(false));
    text(move || if open.get() { "搜索…" } else { "按 ⌘K" })
}
```

```tsx tabs
import { mount, signal, Text } from "nsh";
import { keymap } from "nsh/keymap";

const open = signal(false);
keymap.bind("mod+k", () => open.set(true));
keymap.bind("escape", () => open.set(false));

mount(() => <Text>{open() ? "搜索…" : "按 ⌘K"}</Text>);
```

传入一个对象可以一次绑定多个按键组合；返回的函数会把它们全部移除。在 Rust 中，每个按键组合调用一次 `on_key`：

```rust tabs
on_key("mod+s", move |_| save());
on_key("mod+shift+p", move |_| open_palette());
on_key("f5", move |_| refresh());
```

```tsx tabs
const unbind = keymap.bind({
  "mod+s": save,
  "mod+shift+p": openPalette,
  f5: refresh,
});
```

组件渲染时创建的绑定会随组件一起移除，所以一个屏幕的快捷键与这个屏幕同生共死：

```rust tabs
#[component]
fn Editor() -> impl IntoElement {
    on_key("mod+enter", move |_| send());
    input().multiline(true)
}
```

```tsx tabs
function Editor() {
  keymap.bind("mod+enter", send);
  return <Input multiline />;
}
```

使用开发服务器时，修改一个模块会在它重新运行之前移除它顶层代码创建的绑定，因此热重载不会留下过期的快捷键。

## keymap.label

```ts
label(keystroke: string): string;
```

返回 `keystroke` 在应用所运行的平台上，菜单和提示中的显示方式：对 `"mod+shift+l"`，在 macOS 和 iOS 上（以及 Mac、iPhone、iPad 上的浏览器里）是 `"⇧⌘L"`，在 Windows、Linux 和其他浏览器里是 `"Ctrl+Shift+L"`。Rust：`key_label(keystroke)`。用它来显示应用的快捷键，显示的就是实际可用的按键：

```rust tabs
text(format!("搜索 {}", key_label("mod+k")))
```

```tsx tabs
<Text>搜索 {keymap.label("mod+k")}</Text>
```

按键组合无效时，它和 `bind` 一样抛出 `TypeError`；`key_label` 和 `on_key` 以同样的信息 panic。

## 按键组合

按键组合由修饰键和一个键用 `+` 连接而成：`"mod+k"`、`"ctrl+shift+p"`、`"alt+enter"`、`"escape"`、`"f5"`、`"cmd+-"`（Command 加减号）。

| 修饰键 | macOS | Windows | Linux |
|---|---|---|---|
| `mod`（也可写 `cmdorctrl`） | Command（⌘） | Ctrl | Ctrl |
| `cmd`（也可写 `command`、`meta`、`super`、`win`） | Command（⌘） | Windows 键 | Super |
| `ctrl`（也可写 `control`） | Control（⌃） | Ctrl | Ctrl |
| `alt`（也可写 `option`、`opt`） | Option（⌥） | Alt | Alt |
| `shift` | Shift（⇧） | Shift | Shift |

iOS 与 macOS 一致（`mod` 是 Command），Android 与 Windows、Linux 一致（`mod` 是 Ctrl，`cmd` 是 Meta 键）。在浏览器里，按键遵循浏览器所在的平台：在 Mac、iPhone 或 iPad 上 `mod` 是 Command，其他地方是 Ctrl。

应用自己的快捷键请用 `mod`：它就是各平台应用习惯使用的那个键（macOS 上是 ⌘K，Windows 和 Linux 上是 Ctrl+K），一种写法到处适用。Windows 键和 Super 基本归系统使用，所以在 macOS 以外很少该用 `cmd`。

键可以是字母、数字或标点键，写作它在美式键盘上不按 Shift 时输入的字符（`k`、`1`、`,`、`/`、`[`；`?` 写作 `"shift+/"`，Shift+K 写作 `"shift+k"`），也可以是名称：`plus`（`=` / `+` 键，也可写 `=`）、`minus`（也可写 `-`）、`space`、`enter`（`return`）、`escape`（`esc`）、`tab`、`backspace`、`delete`（`del`）、`insert`、方向键 `up`、`down`、`left`、`right`（`arrowup` 等）、`home`、`end`、`pageup`、`pagedown`、`f1` 到 `f24`。大小写不影响：`"Mod+Shift+K"` 就是 `"mod+shift+k"`，大写字母表示同一个键，并不意味着 Shift（`"mod+K"` 就是 `"mod+k"`）。推荐的写法是全小写，本页示例都这样写。`"mod-k"` 和 `"mod++"` 会报错，错误信息给出正确的写法。`nsh/desktop` 的菜单快捷键使用同样的写法。

快捷键适用于任何键盘布局：在西里尔或希腊字母布局下，`mod+k` 是美式布局中输入 K 的那个键，与菜单快捷键一致。

对于无效的按键组合，`bind` 会抛出一个说明问题所在的 `TypeError`（`keymap.bind: invalid keystroke "cmd+kk": unknown key "kk"`），并拒绝系统保留的按键组合，应用因此永远无法让它们失效：

| 平台 | 保留 |
|---|---|
| macOS、iOS | Cmd+Q（退出）、Cmd+H（隐藏）、Option+Cmd+H（隐藏其他）、Cmd+M（最小化）、Ctrl+Cmd+F（全屏） |
| Linux、Windows、Android | Ctrl+Q（退出）、Alt+F4（关闭）、F11（全屏） |
| Web | 无（浏览器保留它自己的按键） |

## 输入时

拥有键盘焦点的文本输入框需要它自己的按键：输入、移动光标、删除，以及全选、剪切、复制、粘贴、撤销和重做。因此默认情况下，当文本输入框（或网页视图）拥有焦点时，按键组合属于这些的绑定不会触发：普通键、只带 Shift 或 Option 的键、方向键、Home、End、Page Up / Down、Return、Tab、Escape、Backspace、Delete，以及 `mod+a`、`mod+c`、`mod+v`、`mod+x`、`mod+z`。其他带 Command 或 Control 的按键组合，以及功能键，无论焦点在哪里都会触发。

`when` 可以改变这一点（Rust：`KeyOptions { when: When::Always, .. }`）：

| `when` | 触发 |
|---|---|
| （默认） | 如上 |
| `"always"` | 在文本输入框中也触发，先于输入框看到按键 |
| `"notEditing"` | 文本输入框拥有焦点时从不触发 |

```rust tabs
use nsh_core::keymap::When;

// 在消息输入框中也能发送。
on_key_with("mod+enter", KeyOptions { when: When::Always, ..KeyOptions::default() }, move |_| send());
// 只在用户没有输入时。
on_key_with("mod+e", KeyOptions { when: When::NotEditing, ..KeyOptions::default() }, move |_| archive());
```

```tsx tabs
// 在消息输入框中也能发送。
keymap.bind("mod+enter", send, { when: "always" });
// 只在用户没有输入时。
keymap.bind("mod+e", archive, { when: "notEditing" });
```

在 iOS 和 Android 上，输入法正在组字时用到的按键（选候选词、上屏）仍归输入法。在 GPUI 后端上，拥有键盘焦点的原生组件按文本输入框对待，因为它可能是文本编辑器。它先看到每个按键：它自己处理的按键不会到达绑定，它放过的按键交给在编辑文本时也会触发的绑定。

## 运行哪个绑定

触发了绑定的按键会被消耗：它不会被输入，系统不会发出提示音，NSH 自己的按键处理也看不到它（Escape 返回、Tab 移动焦点、方向键滚动）。当多个绑定的按键组合相同时，运行最新的那个可以触发的绑定，所以对话框可以把 Escape 绑定在屏幕的绑定之上，对话框的绑定移除后屏幕的绑定又会生效。按下按键的那一刻，绑定就在原生代码中完成匹配；处理函数随后运行，Rust 在引擎线程上，TSX 在 JS 线程上。

按住一个键只触发一次绑定，直到松开为止；设置 `repeat: true` 后，每次自动重复都会再次触发（例如在列表中逐项移动）：

```rust tabs
on_key_with("alt+down", KeyOptions { repeat: true, ..KeyOptions::default() }, move |_| next_item());
```

```tsx tabs
keymap.bind("alt+down", nextItem, { repeat: true });
```

菜单快捷键会让位给相同按键组合的绑定：所有桌面平台上的应用菜单（[`nsh/desktop`](./nsh-desktop)），以及 macOS 上的内置菜单（Cmd+W、Cmd+C 等）。使用 `nsh run` 开发时，即使应用绑定了 `mod+r`，Cmd+R / Ctrl+R 也总是重新加载应用；打包后的应用可以绑定它。
