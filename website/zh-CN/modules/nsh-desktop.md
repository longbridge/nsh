---
title: nsh/desktop
description: 桌面专有 API：窗口（标题、标题栏、最大化、全屏）、带可拖动区域的自定义标题栏、应用菜单、右键菜单和快捷键。
order: 12.5
---

# nsh/desktop

`nsh/desktop` 包含只有桌面才有的东西：窗口和它的标题栏、应用菜单和右键菜单。它在 macOS、Windows 和 Linux 上可用。Rust 代码在 `nsh_core::element::desktop` 中有同样的功能：窗口的函数（`set_title`、`set_title_bar`、`title_bar`、`minimize`……`close_window`）、`quit`、应用菜单和右键菜单（`set_application_menu`、`popup_menu`，由 `MenuItem` 组成），并用 `.window_drag(true)` 标记可拖动的视图；TSX 导入 `nsh/desktop`。这个模块在所有平台上都可以导入，因此同一个 bundle 可以在所有平台运行：在 iOS 和 Android 上它的函数什么也不做，`isAvailable` 告诉你当前是否有它的窗口和菜单。

```rust tabs
use nsh_core::{
    desktop::{MenuRole, TitleBarStyle},
    element::desktop::{
        MenuItem, close_window, popup_menu, quit, set_application_menu, set_title, set_title_bar,
        title_bar,
    },
    prelude::*,
};
```

```tsx tabs
import { Application, isAvailable, Menu, titleBar, Window } from "nsh/desktop";
```

## isAvailable

```ts
const isAvailable: boolean;
```

在这个模块的窗口和菜单 API 可用的地方为 `true`：macOS、Windows 和 Linux；其他平台为 `false`。Rust：`os().is_desktop()`。

[`platform.desktop`](./nsh.md#platform) 是更宽的桌面行为判断（`desktop={{…}}` 属性、`platform.select({ desktop })`）：桌面形态的浏览器也算在内，它们运行在浏览器自己的窗口中。桌面布局、密度和输入方式用 `platform.desktop` 判断，调用 `Window` 和 `Menu` 用 `isAvailable` 判断。

## Window

```ts
const Window: {
  setTitle(title: string): void;
  setTitleBar(options: { style: "default" | "hidden"; height?: number }): void;
  setMenuBar(mode: "shown" | "autoHide"): void;
  minimize(): void;
  toggleMaximize(): void;
  toggleFullScreen(): void;
  setFrame(frame: {
    origin?: Partial<Point>;
    size?: Partial<Size>;
    animated?: boolean;
  }): void;
  setLevel(level: "normal" | "floating" | "desktopIcon"): void;
  setSkipTaskbar(skip: boolean): void;
  orderFront(): void;
  close(): void;
};
```

Rust 列给出 `nsh_core::element::desktop` 中的函数。

| 成员 | Rust | 说明 |
|---|---|---|
| `setTitle(title)` | `set_title(title)` | 窗口的标题：标题栏、任务栏或 Dock、窗口切换器里显示的名字。 |
| `setTitleBar({ style, height? })` | `set_title_bar(TitleBarStyle::Hidden, Some(px(height)))` | `style: "hidden"` 去掉系统标题栏：应用一直铺到窗口顶边，自己绘制标题栏。系统的窗口按钮保留（macOS 的红绿灯、Windows 的最小化 / 最大化 / 关闭按钮、GTK 的窗口按钮）。`height` 是你的标题栏高度（单位为点）：窗口按钮在其中垂直居中（Windows 的标题栏按钮撑满这个高度；在 macOS 上，`height` 选择紧凑样式（44 点及以下）或标准样式（44 点以上）；应用根据 `titleBar().height` 返回的实际高度布局，实时缩放时由 AppKit 布置红绿灯。嵌入应用已有的工具栏保留自身的样式和高度）；省略时使用平台的高度。`style: "default"` 恢复系统标题栏（Rust：`set_title_bar(TitleBarStyle::Default, None)`）。 |
| `setMenuBar(mode)` | `set_menu_bar_visibility(mode)` | 在 GPUI 应用的 Windows 和 Linux 上，窗口把应用菜单绘制成应用上方的菜单栏：`"shown"`（默认）始终显示；`"autoHide"` 在单独按下并松开 Alt 后显示，再次单独按 Alt 时隐藏，与 Windows 和 GNOME 应用一致。两种模式下 F10 和 Alt 加菜单首字母都会打开菜单，菜单打开期间菜单栏显示。macOS 的菜单栏是系统的。 |
| `minimize()` | `minimize()` | 最小化窗口。 |
| `toggleMaximize()` | `toggle_maximize()` | 最大化窗口，已最大化时还原。 |
| `toggleFullScreen()` | `toggle_full_screen()` | 进入或退出全屏。 |
| `setFrame(frame)` | `set_frame(WindowFrame { .. })` | 移动窗口并调整大小，单位为点；省略的字段保持原值。macOS 和 Windows 上的 `size`（`width`、`height`）包含窗口装饰；Linux 上设置 GTK 的默认窗口尺寸。`origin`（`x`、`y`）是窗口的左上角，从主屏幕（macOS 上是带菜单栏的那块屏幕）的左上角量起，`y` 向下增长，各平台相同；只给 `height` 而不给 `y` 时顶边保持不动，窗口从底边伸缩。`animated: true` 启用 macOS 的窗口调整动画（默认为 false）。 |
| `setLevel(level)` | `set_level(level)` | 在 macOS 和 Windows 上，`"floating"` 让窗口保持在普通窗口之上，`"normal"` 恢复普通层级。`"desktopIcon"` 在 macOS 上选择桌面图标层级（窗口同时存在于所有空间，不进入 Mission Control 和 Cmd+` 循环）；在 Windows 上将窗口移到当前 z 序底部，之后激活窗口仍可将其提升。 |
| `setSkipTaskbar(skip)` | `set_skip_taskbar(skip)` | 在 Windows 上，`true` 让窗口不出现在任务栏（对应 Electron 和 Tauri 的 `skipTaskbar`），适合浮层、桌宠和弹出窗口；`false` 让它回到任务栏。macOS 的 Dock 和 Linux 的 Dock 按应用而不是按窗口列出，其他平台不做处理。 |
| `orderFront()` | `order_front()` | 把窗口提升到其所在层级的最前并聚焦（应用随之激活；macOS 会解除最小化，Windows 会恢复最小化的窗口并置于前台，GTK 会 present）。窗口保持原有层级。macOS 的桌面层级窗口保持在其他窗口之后。GPUI 激活其窗口；Linux 上的前台请求由窗口管理器决定。 |
| `close()` | `close_window()` | 关闭窗口，应用随之结束。 |

窗口尺寸调整适用于 macOS、Windows 和 Linux；窗口定位和层级控制适用于 macOS 和 Windows。Linux 的窗口位置和层级由窗口管理器决定。在 GPUI 应用中，`setFrame` 设置窗口尺寸，窗口位置由平台决定；GPUI 窗口的位置和层级由宿主在创建窗口时配置。

使用 `onLayout` 测量内容后调整窗口大小时，在 macOS 和 Windows 上应为窗口装饰预留空间。`width` 和 `height` 接受正有限数值；`x` 和 `y` 接受有限数值。无效字段保留原值。

### Window.open

`Window.open(Component, options)` 打开一个应用自己的窗口，并在其中渲染 `Component`。这个窗口运行在应用唯一的 JS 运行时中，
是同一个应用的一部分：它和主窗口读写相同的 signal、store 和模块，窗口之间不需要传递消息。打开一个已经打开的 `id` 会返回那个窗口。

```tsx
import { signal, Text, View } from "nsh";
import { Window } from "nsh/desktop";

const unread = signal(0);

function Chat() {
  return <View><Text>{`${unread()} unread`}</Text></View>;
}

const chat = Window.open(Chat, { id: "chat", title: "Chat", size: { width: 360, height: 480 } });
chat.focus();
```

| 选项 | 说明 |
|---|---|
| `id` | 窗口的 id：再次打开同一个 id 会返回已打开的窗口。 |
| `title` | 窗口标题（省略时使用主窗口的标题）。 |
| `size` | 内容尺寸，单位为点，`{ width, height }`（默认 480×360）。 |
| `position` | 窗口左上角在屏幕上的位置，`{ x, y }`。GTK 总是自行放置窗口。 |
| `level` | `"normal"`、`"floating"` 或 `"desktopIcon"`，同 `setLevel`。Linux 上不适用。 |
| `frame` | `"none"`：没有标题栏和边框，由应用绘制全部内容。 |

返回的窗口有 `id`、`focus()`（显示窗口，`hide()` 之后也可以，并把它提到最前）、`hide()` 和 `close()`（其内容随之卸载）。
用户关闭它时，效果和 `close()` 相同；关闭主窗口也会关闭它。`Window.open` 在 Linux 和 Windows 上运行；在其他平台上会抛出异常。

## Application

```ts
const Application: {
  quit(): void;
};
```

| 成员 | Rust | 说明 |
|---|---|---|
| `quit()` | `quit()` | 退出应用，与应用菜单的「退出」项（`{ role: "quit" }`）和平台的退出快捷键相同。 |

`Application.quit()` 用于应用自己的「退出」（自定义文字的菜单项、按钮），以及更新后重新启动。应用像最后一个窗口关闭时那样停止，然后结束运行；只关窗口会让应用继续运行时（为 Dock 保留的隐藏窗口、托盘图标）也一样。关闭的过程与「退出」菜单项相同：macOS 上的独立应用像按下 Cmd+Q 一样从 `run_app` 返回；在拥有窗口的 GPUI 应用中，请求以 `CloseRequest::Quit` 交给它的 `AppView::on_close_request` 回调，由应用在准备好后退出；其他平台上退出照常进行，与「退出」菜单项相同。嵌入在其他应用中的视图把应用的生命周期交给那个应用。在终端中应用结束，与退出角色相同。

## 自定义标题栏

隐藏标题栏后，应用绘制自己的标题栏，并指定哪些部分可以拖动窗口：Rust 用 `.window_drag(true)`，TSX 用 `desktop={{ windowDrag: true }}`：

```rust tabs
use nsh_core::{
    desktop::TitleBarStyle,
    element::{desktop::{set_title_bar, title_bar}}, prelude::*,
};

#[component]
fn TitleBar() -> impl IntoElement {
    set_title_bar(TitleBarStyle::Hidden, Some(px(40.0))); // 40 点高的标题栏
    view()
        .window_drag(true)
        .h(move || match title_bar().top {
            top if top > px(0.0) => top,
            _ => px(40.0),
        })
        .pl(move || title_bar().left + px(8.0)) // macOS 的红绿灯
        .pr(move || title_bar().right + px(8.0)) // Windows 的标题栏按钮、GTK 的窗口按钮
        .flex_row()
        .items_center()
        .child(text("My App"))
        .child(view().flex_1())
        .child(input().placeholder("搜索").w(px(180.0)))
}
```

```tsx tabs
import { Text, View } from "nsh";
import { Input } from "nsh/base";
import { titleBar, Window } from "nsh/desktop";

Window.setTitleBar({ style: "hidden", height: 40 }); // 40 点高的标题栏

function TitleBar() {
  return (
    <View
      desktop={{ windowDrag: true }}
      style={{
        height: titleBar().height || 40,
        paddingLeft: titleBar().left + 8, // macOS 的红绿灯
        paddingRight: titleBar().right + 8, // Windows 的标题栏按钮、GTK 的窗口按钮
        flexDirection: "row",
        alignItems: "center",
      }}
    >
      <Text>My App</Text>
      <View style={{ flex: 1 }} />
      <Input placeholder="搜索" style={{ width: 180 }} />
    </View>
  );
}
```

按住带 `windowDrag` 的视图会像按住系统标题栏一样拖动窗口：双击最大化（或执行用户的设置：macOS 的“连按窗口标题栏以”），在 Windows 和 Linux 上右键打开窗口菜单。在 Linux 的 GPUI 应用中，若点击处的可拖动视图本身或其内部的视图监听 `onContextMenu`，右键点击交给应用；外层视图（包括根视图）上的监听不会取代窗口菜单。

- 由最近的设置了 `windowDrag` 的视图决定：`.window_drag(false)`（`desktop={{ windowDrag: false }}`）让可拖动视图中的一部分保持可点击。
- 可拖动视图中的文本框，以及带按压或触摸处理（`onPress`、`onTouch*`、responder）的视图，不用设置也保持可点击：上例中的搜索框和其中的按钮照常工作。
- `windowDrag` 在系统标题栏下也能用：侧边栏或工具栏同样可以拖动窗口。

### titleBar

```ts
const titleBar: Accessor<{ height: number; left: number; right: number }>;
```

隐藏的标题栏的区域，单位为点（响应式）：它的 `height`，以及系统窗口按钮在左端（`left`）和右端（`right`）占用的宽度。用它来布局标题栏，避免内容被窗口按钮遮住。系统显示标题栏时（以及全屏时、非桌面平台上）三者都为 0。Rust：`title_bar() -> Edges<Points>`，其中 `top` 是高度，`left` 和 `right` 是窗口按钮的宽度；读取它会订阅变化。

| | `height` | `left` | `right` |
|---|---|---|---|
| macOS | `height`，否则为标题栏的高度（28 或 32，视 macOS 版本而定） | 红绿灯的宽度 | 0 |
| Windows | `height`，否则为 32 | 0 | 三个标题栏按钮（138） |
| Linux | `height`，否则为标题栏（header bar）的高度 | 开头的窗口按钮（按桌面的按钮布局） | 末尾的窗口按钮 |

在 Windows 上，标题栏按钮由 NSH 按 Windows 11 的样式绘制，跟随应用的外观；鼠标停在最大化按钮上会出现 Windows 11 的贴靠布局。在所有平台上窗口都保留调整大小的边框、阴影和圆角。

## Menu

```ts
const Menu: {
  setApplicationMenu(items: MenuItem[] | null): void;
  popup(items: MenuItem[], at?: Point): Promise<void>;
};
```

| 成员 | 说明 |
|---|---|
| `setApplicationMenu(items)` | 设置应用菜单：每个顶层项是一个子菜单（`File`、`Edit`……）。在 macOS 上是菜单栏，第一个菜单是应用菜单（AppKit 以应用名作为标题）。在 Windows 和 Linux 上是窗口的菜单栏，隐藏标题栏时不显示；它的快捷键仍然有效，自定义标题栏可以用 `Menu.popup` 打开这些菜单。`null` 恢复平台默认菜单。 |
| `popup(items, at?)` | 显示右键菜单，左上角在 `at`（窗口坐标，单位为点：`onContextMenu` 的 `e.pagePosition`），省略时在指针处。菜单关闭时（无论是否选中了菜单项）返回的 Promise 会 resolve：打开它的按钮可以在此之前一直保持按下样式。 |

在 Rust 中，`set_application_menu(items)` 接受 `Vec<MenuItem>`（`None` 恢复平台默认菜单），`popup_menu(items)`（在指针处）和 `popup_menu_at(items, at)` 返回一个 `Answer<()>`，菜单关闭时得到结果（可以不 await）。菜单项的处理函数在设置菜单的 owner 下运行，只在该 owner 存活期间有效。

再次调用 `setApplicationMenu` 即可修改菜单。放在 `effect` 里时，它会跟随读取的 signal：依赖应用状态的勾选或可用状态始终保持最新。

### 菜单项

```ts
type MenuItem =
  | { type: "separator" }
  | {
      label?: string;
      role?: MenuRole;
      shortcut?: string;
      when?: "always" | "auto" | "notEditing";
      disabled?: boolean;
      checked?: boolean;
      onSelect?: () => void;
      submenu?: MenuItem[];
    };
```

| 字段 | 说明 |
|---|---|
| `label` | 菜单项显示的文字。带 role 的菜单项省略时使用自己的默认文字。 |
| `onSelect` | 菜单项被选中时调用，无论是点击还是快捷键。 |
| `shortcut` | 快捷键：修饰键和一个按键用 `+` 连接，例如 `"mod+n"`、`"mod+shift+z"`、`"alt+f4"` 或 `"f11"`。修饰键见下表。按键可以是字母、数字和标点，写作它在美式键盘上不按 Shift 时输入的字符（`,`、`/`；`?` 写作 `"shift+/"`），也可以是名称：`plus`（`=` / `+` 键，也可写 `=`）、`minus`（也可写 `-`）、`space`、`enter`（`return`）、`escape`（`esc`）、`tab`、`backspace`、`delete`（`del`）、`insert`、方向键 `up`、`down`、`left`、`right`（`arrowup` 等）、`home`、`end`、`pageup`、`pagedown`、`f1`–`f24`。大小写不影响（`"Mod+Shift+N"` 就是 `"mod+shift+n"`；大写字母表示同一个键，并不意味着 Shift），推荐全小写的写法。无效的快捷键（`"mod-n"`、`"mod++"`、`"mod+?"`）会抛出错误并给出正确的写法。`keymap` 使用同样的按键写法。菜单按各平台自己的样式显示快捷键（macOS 上是 `⇧⌘N`，Windows 上是 `Ctrl+Shift+N`）。 |
| `when` | 在 GPUI 后端上，输入框获得键盘焦点时快捷键何时选中该菜单项：`"always"`（默认）、`"auto"`（除非输入框自己使用这个按键：输入字符、移动光标或删除的键，以及编辑快捷键，和 [`keymap`](./nsh.md#keymap) 的绑定相同）或 `"notEditing"`。菜单项让出的键交给输入框。 |
| `disabled` | `true` 时菜单项变灰（默认 `false`）。 |
| `checked` | 设置（为 `true` 或 `false`）后菜单项带勾选标记。 |
| `submenu` | 让菜单项成为子菜单（需要 `label`）。 |
| `role` | 由平台执行的标准菜单项（见下）。 |

`null`、`undefined` 和 `false` 项会被忽略，因此可以在列表中写 `cond && { … }`。

在 Rust 中，菜单项是 `MenuItem::new(label)`、`MenuItem::role(role)`、`MenuItem::separator()` 或 `MenuItem::submenu(label, items)`，字段对应同名方法：`.shortcut("mod+n")`（快捷键无效时 panic，并给出正确写法）、`.shortcut_when(When::Auto)`、`.disabled(true)`、`.checked(on)`、`.on_select(f)`，以及给 role 菜单项设置文字的 `.label(..)`。

修饰键：

| 修饰键 | macOS | Windows | Linux |
|---|---|---|---|
| `mod`（也可写 `cmdorctrl`） | Command（⌘） | Ctrl | Ctrl |
| `cmd`（也可写 `command`、`meta`、`super`、`win`） | Command（⌘） | Windows 键 | Super |
| `ctrl`（也可写 `control`） | Control（⌃） | Ctrl | Ctrl |
| `alt`（也可写 `option`、`opt`） | Option（⌥） | Alt | Alt |
| `shift` | Shift（⇧） | Shift | Shift |

应用自己的快捷键请用 `mod`：它就是各平台应用习惯使用的那个键（macOS 上是 ⌘N，Windows 和 Linux 上是 Ctrl+N），一种写法到处适用。Windows 键和 Super 基本归系统使用（Win+E、GNOME 的活动视图），所以在 macOS 以外很少该用 `cmd`。

Role：

| Role | 作用 | 平台 |
|---|---|---|
| `about`、`hide`、`hideOthers`、`showAll` | 应用菜单的标准项 | macOS（其他平台忽略） |
| `quit` | 退出应用 | 全部 |
| `undo`、`redo`、`cut`、`copy`、`paste`、`selectAll` | 编辑获得焦点的文本框（快捷键仍归文本框自己） | 全部 |
| `minimize`、`zoom`、`toggleFullScreen`、`close` | 窗口操作 | 全部 |

带 role 的菜单项默认使用平台的文字和快捷键，除非自己设置（macOS 会在 `about`、`hide` 和 `quit` 中加上应用名）。

菜单快捷键的优先级最高，和原生应用一样：菜单项占用的按键不会再传给获得焦点的输入框，也不会触发 NSH 自己的按键处理。只有 [`keymap`](./nsh.md#keymap) 中相同按键组合的绑定排在它们前面。所以编辑快捷键（`mod+c`、`mod+v`、`mod+x`、`mod+a`、`mod+z`）只交给编辑类 role（它们会把按键转给输入框），也不要使用不带修饰键的键（`escape`、方向键、字母），否则输入框和返回导航就收不到它们了。在 GPUI 后端上，`when: "auto"` 让菜单项和输入框共用按键：没有输入框获得焦点时，`delete` 或 `mod+z` 选中菜单项；有输入框获得焦点时，它们编辑文本。

在 GPUI 后端上，拥有键盘焦点的原生组件先看到每个按键。它自己处理的按键不会到达快捷键表或菜单。它放过的按键按输入框获得焦点时的规则进入 `keymap` 的快捷键，然后再到菜单。

```rust tabs
use nsh_core::{
    desktop::MenuRole,
    element::desktop::{MenuItem, set_application_menu},
    platform::Os,
    prelude::*,
};

#[component]
fn AppMenu() -> impl IntoElement {
    let mac = os() == Os::MacOs;
    let wrap = signal(true);
    effect(move || {
        let mut menus = Vec::new();
        if mac {
            menus.push(MenuItem::submenu("My App", [
                MenuItem::role(MenuRole::About),
                MenuItem::separator(),
                MenuItem::role(MenuRole::Hide),
                MenuItem::role(MenuRole::HideOthers),
                MenuItem::separator(),
                MenuItem::role(MenuRole::Quit),
            ]));
        }
        let mut file = vec![
            MenuItem::new("New").shortcut("mod+n").on_select(create_document),
            MenuItem::new("Open…").shortcut("mod+o").on_select(open_document),
        ];
        if !mac {
            file.push(MenuItem::separator());
            file.push(MenuItem::role(MenuRole::Quit).label("Exit"));
        }
        menus.push(MenuItem::submenu("File", file));
        menus.push(MenuItem::submenu("Edit", [
            MenuItem::role(MenuRole::Undo),
            MenuItem::role(MenuRole::Redo),
            MenuItem::separator(),
            MenuItem::role(MenuRole::Cut),
            MenuItem::role(MenuRole::Copy),
            MenuItem::role(MenuRole::Paste),
        ]));
        menus.push(MenuItem::submenu("View", [
            MenuItem::new("Word Wrap")
                .checked(wrap.get())
                .on_select(move || wrap.update(|w| *w = !*w)),
            MenuItem::role(MenuRole::ToggleFullScreen),
        ]));
        set_application_menu(menus);
    });
    view()
}
```

```tsx tabs
import { effect, signal } from "nsh";
import { platform } from "nsh";
import { Menu } from "nsh/desktop";

const mac = platform.os === "macos";
const wrap = signal(true);

effect(() => {
  Menu.setApplicationMenu([
    mac && {
      label: "My App",
      submenu: [{ role: "about" }, { type: "separator" }, { role: "hide" }, { role: "hideOthers" }, { type: "separator" }, { role: "quit" }],
    },
    {
      label: "File",
      submenu: [
        { label: "New", shortcut: "mod+n", onSelect: () => createDocument() },
        { label: "Open…", shortcut: "mod+o", onSelect: () => openDocument() },
        !mac && { type: "separator" },
        !mac && { role: "quit", label: "Exit" },
      ],
    },
    {
      label: "Edit",
      submenu: [{ role: "undo" }, { role: "redo" }, { type: "separator" }, { role: "cut" }, { role: "copy" }, { role: "paste" }],
    },
    {
      label: "View",
      submenu: [
        { label: "Word Wrap", checked: wrap(), onSelect: () => wrap.update((w) => !w) },
        { role: "toggleFullScreen" },
      ],
    },
  ]);
});
```

在按钮按下时弹出右键菜单：

```rust tabs
text("更多").on_press(|| {
    let _ = popup_menu(vec![
        MenuItem::new("重命名").on_select(rename),
        MenuItem::new("复制一份").on_select(duplicate),
        MenuItem::separator(),
        MenuItem::new("删除").on_select(remove),
    ]);
})
```

```tsx tabs
<View
  onPress={() =>
    Menu.popup([
      { label: "重命名", onSelect: rename },
      { label: "复制一份", onSelect: duplicate },
      { type: "separator" },
      { label: "删除", onSelect: remove },
    ])
  }
>
  <Text>更多</Text>
</View>
```

> [!NOTE]
> 菜单是系统原生的（NSMenu、Win32 菜单、GTK popover 菜单）：外观和行为与其他应用一致，跟随系统（或应用）的外观，并支持键盘操作。`examples/desktop` 是一个带自定义标题栏、应用菜单和右键菜单的示例应用。

## Tray

```ts
interface TrayModule {
  set(options: TrayOptions | null): void;
}
```

`Tray.set` 在系统托盘显示应用图标，或修改它；传 `null` 移除。每个应用只有一个。托盘在 Windows 上是通知区域，在 macOS 上是菜单栏右侧，在 Linux 上是 StatusNotifierItem，Plasma、Waybar 和大多数面板都能显示（GNOME 需要 AppIndicator 扩展）。

| 选项 | 说明 |
| --- | --- |
| `icon` | 图标，写法与 `<Image>` 的 `src` 相同。面板按 16 到 24 像素绘制，图标应简单、方形。在 macOS 和 Windows 上 SVG 是模板图标，用菜单栏或任务栏的文字颜色绘制。不设时 Windows 显示应用图标。 |
| `tooltip` | 指针停在图标上时显示；不设时显示应用名。 |
| `menu` | 图标的菜单，菜单项与 `Menu` 的相同。右键打开；没有 `onClick` 时左键也打开。 |
| `onClick` | 左键点击图标时调用（在 Windows 上，图标获得焦点时按 Enter 或空格也会调用）。 |

再次调用 `set` 即可修改；放在 `effect` 里时会跟随它读取的 signal 更新。

```tsx
import { Tray, Window } from "nsh/desktop";
import icon from "./tray.svg";

Tray.set({
  icon,
  tooltip: "Mail",
  onClick: () => Window.orderFront(),
  menu: [
    { label: "New Message", onSelect: compose },
    { type: "separator" },
    { role: "quit" },
  ],
});
```

目前只有 TSX 可用，Rust 组件还没有托盘 API。

## GlobalShortcut

```ts
interface GlobalShortcutModule {
  register(shortcut: string, onPress: () => void): Promise<boolean>;
  unregister(shortcut: string): void;
  unregisterAll(): void;
}
```

全局快捷键不论哪个应用持有键盘都会送到本应用。`register` 的按键写法与菜单项的 `shortcut` 相同（`"mod+shift+k"`），返回系统是否接受：被其他应用占用、或不在桌面平台时为 `false`。同一按键再次注册会替换回调。

在 macOS 上不需要用户授予权限。字母对应注册时键盘布局上输入该字母的键，所以 Dvorak 布局下 `k` 仍是 `k`。

在 Windows 上一个按键只属于一个应用：已被其他应用占用或由系统保留（Win+L）时返回 `false`。按住不放只触发一次。

在 Linux 上通过桌面 portal（`org.freedesktop.portal.GlobalShortcuts`）注册，这是 Wayland 下唯一的方式。按键只是建议值：桌面可能请用户确认或改选其他按键。Hyprland 在自己的配置里绑定，按应用 id 和小写的按键名：`bind = CTRL ALT, K, global, com.example.mail:ctrl+alt+k`。

```tsx
import { GlobalShortcut, Window } from "nsh/desktop";

const ok = await GlobalShortcut.register("mod+shift+space", () => Window.orderFront());
```
