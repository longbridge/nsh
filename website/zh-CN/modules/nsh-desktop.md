---
title: nsh/desktop
description: 桌面专有 API：窗口（标题、标题栏、最大化、全屏）、带可拖动区域的自定义标题栏、应用菜单、右键菜单和快捷键。
order: 12.5
---

# nsh/desktop

`nsh/desktop` 包含只有桌面才有的东西：窗口和它的标题栏、应用菜单和右键菜单。它在 macOS、Windows 和 Linux 上可用。这个模块在所有平台上都可以导入，因此同一个 bundle 可以在所有平台运行：在 iOS 和 Android 上它的函数什么也不做，`isAvailable` 告诉你当前是否在桌面上。

```ts
import { isAvailable, Menu, titleBar, Window } from "nsh/desktop";
```

## isAvailable

```ts
const isAvailable: boolean;
```

在 macOS、Windows 和 Linux 上为 `true`，其他平台为 `false`。

## Window

```ts
const Window: {
  setTitle(title: string): void;
  setTitleBar(style: "default" | "hidden", height?: number): void;
  minimize(): void;
  toggleMaximize(): void;
  toggleFullScreen(): void;
  close(): void;
};
```

| 成员 | 说明 |
|---|---|
| `setTitle(title)` | 窗口的标题：标题栏、任务栏或 Dock、窗口切换器里显示的名字。 |
| `setTitleBar(style, height?)` | `"hidden"` 去掉系统标题栏：应用一直铺到窗口顶边，自己绘制标题栏。系统的窗口按钮保留（macOS 的红绿灯、Windows 的最小化 / 最大化 / 关闭按钮、GTK 的窗口按钮）。`height` 是你的标题栏高度（单位为点）：窗口按钮在其中垂直居中（Windows 的标题栏按钮撑满这个高度）；省略时使用平台的高度。`"default"` 恢复系统标题栏。 |
| `minimize()` | 最小化窗口。 |
| `toggleMaximize()` | 最大化窗口，已最大化时还原。 |
| `toggleFullScreen()` | 进入或退出全屏。 |
| `close()` | 关闭窗口，应用随之结束。 |

## 自定义标题栏

隐藏标题栏后，应用绘制自己的标题栏，并用 `desktop={{ windowDrag: true }}` 指定哪些部分可以拖动窗口：

```tsx
import { Input, Text, View } from "nsh";
import { titleBar, Window } from "nsh/desktop";

Window.setTitleBar("hidden", 40); // 40 点高的标题栏

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

按住带 `windowDrag` 的视图会像按住系统标题栏一样拖动窗口：双击最大化（或执行用户的设置：macOS 的“连按窗口标题栏以”），在 Windows 和 Linux 上右键打开窗口菜单。

- 由最近的设置了 `windowDrag` 的视图决定：`desktop={{ windowDrag: false }}` 让可拖动视图中的一部分保持可点击。
- 可拖动视图中的文本框，以及带按压或触摸处理（`onPress`、`onTouch*`、responder）的视图，不用设置也保持可点击：上例中的搜索框和其中的按钮照常工作。
- `windowDrag` 在系统标题栏下也能用：侧边栏或工具栏同样可以拖动窗口。

### titleBar

```ts
const titleBar: Accessor<{ height: number; left: number; right: number }>;
```

隐藏的标题栏的区域，单位为点（响应式）：它的 `height`，以及系统窗口按钮在左端（`left`）和右端（`right`）占用的宽度。用它来布局标题栏，避免内容被窗口按钮遮住。系统显示标题栏时（以及全屏时、非桌面平台上）三者都为 0。

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
  popup(items: MenuItem[], at?: { x: number; y: number }): Promise<void>;
};
```

| 成员 | 说明 |
|---|---|
| `setApplicationMenu(items)` | 设置应用菜单：每个顶层项是一个子菜单（`File`、`Edit`……）。在 macOS 上是菜单栏，第一个菜单是应用菜单（AppKit 以应用名作为标题）。在 Windows 和 Linux 上是窗口的菜单栏，隐藏标题栏时不显示；它的快捷键仍然有效，自定义标题栏可以用 `Menu.popup` 打开这些菜单。`null` 恢复平台默认菜单。 |
| `popup(items, at?)` | 显示右键菜单，左上角在 `at`（窗口坐标，单位为点），省略时在指针处。菜单关闭时（无论是否选中了菜单项）返回的 Promise 会 resolve：打开它的按钮可以在此之前一直保持按下样式。 |

再次调用 `setApplicationMenu` 即可修改菜单。放在 `effect` 里时，它会跟随读取的 signal：依赖应用状态的勾选或可用状态始终保持最新。

### 菜单项

```ts
type MenuItem =
  | { type: "separator" }
  | {
      label?: string;
      role?: MenuRole;
      shortcut?: string;
      enabled?: boolean;
      checked?: boolean;
      onSelect?: () => void;
      submenu?: MenuItem[];
    };
```

| 字段 | 说明 |
|---|---|
| `label` | 菜单项显示的文字。带 role 的菜单项省略时使用自己的默认文字。 |
| `onSelect` | 菜单项被选中时调用，无论是点击还是快捷键。 |
| `shortcut` | 快捷键：修饰键和一个按键用 `+` 连接，例如 `"mod+n"`、`"mod+shift+z"`、`"alt+f4"` 或 `"f11"`。修饰键见下表。按键可以是字母、数字和标点，写作它在美式键盘上不按 Shift 时输入的字符（`,`、`/`；`?` 写作 `"shift+/"`），也可以是名称：`plus`（`=` / `+` 键，也可写 `=`）、`minus`（也可写 `-`）、`space`、`enter`（`return`）、`escape`（`esc`）、`tab`、`backspace`、`delete`（`del`）、`insert`、方向键 `up`、`down`、`left`、`right`（`arrowup` 等）、`home`、`end`、`pageup`、`pagedown`、`f1`–`f24`。大小写不影响（`"Mod+Shift+N"` 就是 `"mod+shift+n"`；大写字母表示同一个键，并不意味着 Shift），推荐全小写的写法。无效的快捷键（`"mod-n"`、`"mod++"`、`"mod+?"`）会抛出错误并给出正确的写法。`nsh/keymap` 使用同样的按键写法。菜单按各平台自己的样式显示快捷键（macOS 上是 `⇧⌘N`，Windows 上是 `Ctrl+Shift+N`）。 |
| `enabled` | `false` 时菜单项变灰（默认 `true`）。 |
| `checked` | 设置（为 `true` 或 `false`）后菜单项带勾选标记。 |
| `submenu` | 让菜单项成为子菜单（需要 `label`）。 |
| `role` | 由平台执行的标准菜单项（见下）。 |

`null`、`undefined` 和 `false` 项会被忽略，因此可以在列表中写 `cond && { … }`。

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

菜单快捷键的优先级最高，和原生应用一样：菜单项占用的按键不会再传给获得焦点的输入框，也不会触发 NSH 自己的按键处理。只有 [`nsh/keymap`](./nsh-keymap) 中相同按键组合的绑定排在它们前面。所以编辑快捷键（`mod+c`、`mod+v`、`mod+x`、`mod+a`、`mod+z`）只交给编辑类 role（它们会把按键转给输入框），也不要使用不带修饰键的键（`escape`、方向键、字母），否则输入框和返回导航就收不到它们了。

```tsx
import { effect, signal } from "nsh";
import { Platform } from "nsh/device";
import { Menu } from "nsh/desktop";

const mac = Platform.OS === "macos";
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

```tsx
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
