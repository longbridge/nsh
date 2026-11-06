---
title: nsh/desktop
description: "Desktop-only APIs: the window (title, title bar, maximize, full screen, its bounds and display), the displays, a title bar of the app's own with draggable regions, the application menu, context menus and keyboard shortcuts."
order: 12.5
---

# nsh/desktop

`nsh/desktop` holds what only desktops have: the window and its title bar, the application menu and context menus. It works on macOS, Windows and Linux. Rust code finds the same in `nsh_core::element::desktop`: the window's functions (`set_title`, `set_title_bar`, `title_bar`, `minimize`, … `close_window`), its state and the displays (`window_state`, `displays`, `set_window_bounds`), `quit`, the application menu and context menus (`set_application_menu`, `popup_menu`, made of `MenuItem`s), and marks draggable views with `.window_drag(true)`; TSX imports `nsh/desktop`. The module can be imported on every platform, so one bundle runs everywhere: on iOS and Android its functions do nothing, and `isAvailable` tells you whether its window and menus are there.

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
import { Application, displays, isAvailable, Menu, titleBar, Window, windowState } from "nsh/desktop";
```

## isAvailable

```ts
const isAvailable: boolean;
```

`true` where this module's window and menu APIs work: macOS, Windows and Linux; `false` elsewhere. Rust: `os().is_desktop()`.

[`platform.desktop`](./nsh.md#platform) is the broader check for desktop behavior (`desktop={{…}}` props, `platform.select({ desktop })`): it also holds in browsers in the desktop idiom, which run in the browser's own window. Gate desktop layout, density and input on `platform.desktop`, and calls of `Window` and `Menu` on `isAvailable`.

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
  setBounds(bounds: WindowBounds): void;
  setLevel(level: "normal" | "floating" | "desktopIcon"): void;
  setSkipTaskbar(skip: boolean): void;
  orderFront(): void;
  close(): void;
  open(component: () => JSX.Element, options: WindowOpenOptions): AppWindow;
};
```

The Rust column names the functions of `nsh_core::element::desktop`.

| Member | Rust | Description |
|---|---|---|
| `setTitle(title)` | `set_title(title)` | The window's title: the title bar, the taskbar or Dock, the window switcher. |
| `setTitleBar({ style, height? })` | `set_title_bar(TitleBarStyle::Hidden, Some(px(height)))` | `style: "hidden"` removes the system's title bar: the app fills the window up to its top edge and draws a title bar of its own. The system's window controls stay (macOS's traffic lights, Windows' minimize / maximize / close buttons, GTK's window buttons). `height` is your bar's height in points: the controls are centered in it (Windows' caption buttons fill it; on macOS `height` selects compact chrome up to 44 points and regular chrome above; size your bar using the actual height from `titleBar().height`, and AppKit places the traffic lights during live resizing. An embedding app's existing toolbar keeps its own style and height); without it they take the platform's height. `style: "default"` brings the system's title bar back (Rust: `set_title_bar(TitleBarStyle::Default, None)`). |
| `setMenuBar(mode)` | `set_menu_bar_visibility(mode)` | On Windows and Linux in GPUI apps, where the window draws the application menu as a bar above the app: `"shown"` (the default) shows it always; `"autoHide"` shows it after Alt is pressed and released alone, until Alt alone hides it again, as in Windows and GNOME apps. F10 and Alt with a menu's initial open a menu either way, and the bar shows while it is open. macOS's menu bar is the system's. |
| `minimize()` | `minimize()` | Minimizes the window. |
| `toggleMaximize()` | `toggle_maximize()` | Maximizes the window, or restores it when it is maximized. |
| `toggleFullScreen()` | `toggle_full_screen()` | Enters or leaves full screen. |
| `setFrame(frame)` | `set_frame(WindowFrame { .. })` | Moves and sizes the window in points; omitted fields keep their values. On macOS and Windows, `size` (`width`, `height`) includes window decorations; on Linux it sets GTK's default window size. `origin` (`x`, `y`) is the window's top left corner, measured from the top left of the primary screen (on macOS the one with the menu bar) with `y` growing down, the same on every platform; a `height` without a `y` keeps the top edge where it is, so the window grows and shrinks at its bottom edge. `animated: true` enables macOS's resize animation (default: false; GPUI apps ignore it). Use it to move or resize the window as it is; `setBounds` restores a saved window. |
| `setBounds(bounds)` | `set_window_bounds(bounds)` | Restores a window saved earlier: puts it in `bounds.mode` (`"windowed"`, `"maximized"` or `"fullscreen"`) with its windowed bounds `bounds.bounds`, restored to them, then maximized or put in full screen when the mode says so. It takes what [`windowState()`](#the-windows-state-and-the-displays) reports as `windowBounds`, to open the window as the user left it. The bounds first go onto the displays there are now, as the display they were saved on may be unplugged since: they stay when at least 100 × 20 points of the window's top 40 points are on a display's visible bounds, where the user can take the window by its title bar; otherwise they are centered on the primary display's visible bounds, no larger than them. Calling it again with the same bounds changes nothing. Use `setFrame` to move or resize the window as it is. |
| `setLevel(level)` | `set_level(level)` | On macOS and Windows, `"floating"` keeps the window above normal windows and `"normal"` restores the normal level. `"desktopIcon"` selects macOS's desktop icon level (the window also sits on every space, out of Mission Control and Cmd+`); on Windows it moves the window to the bottom of the current z-order, where later activation can raise it. |
| `setSkipTaskbar(skip)` | `set_skip_taskbar(skip)` | On Windows, `true` keeps the window out of the taskbar (Electron's and Tauri's `skipTaskbar`), for overlays, pets and pop-ups; `false` puts it back. macOS's Dock and Linux's docks list apps rather than windows, so other platforms leave it as it is. |
| `orderFront()` | `order_front()` | Brings the window to the front of its level and focuses it (the app activates; macOS deminiaturizes, Windows restores minimized windows and sets foreground, GTK presents). The window keeps its level. On macOS, desktop-level surfaces stay behind other windows. GPUI activates its window; the window manager controls foreground requests on Linux. |
| `close()` | `close_window()` | Closes the window; the app ends with it. |

Window sizing is supported on macOS, Windows and Linux; positioning and levels on macOS and Windows. Linux's window manager controls placement and stacking. In GPUI apps, `setFrame` sets the size of the window's content (each side at least 1 point) and the platform places it; GPUI's host chooses its position and level when opening the window. A maximized or full-screen GPUI window keeps its size, and on a tiling compositor (Hyprland, Sway) the compositor decides a tiled window's size: GPUI cannot tell a tiled window apart when the compositor draws the decorations.

When sizing a window from content measured with `onLayout`, allow room for window decorations on macOS and Windows. GPUI apps on Linux with the window manager's decorations need none: the decorations come around the size you set. `width` and `height` accept positive finite numbers; `x` and `y` accept finite numbers. Invalid fields keep their previous values.

### Window.open

`Window.open(Component, options)` opens a window of the app's own and renders `Component` in it. The window runs in
the app's one JS runtime, as part of the same app: it reads and writes the same signals, stores and modules as the main
window, so there is no messaging between windows. Opening an `id` that is open returns that window.

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

| Option | Description |
|---|---|
| `id` | The window's id: opening the same id again returns the open window. |
| `title` | Its title (the main window's when left out). |
| `size` | Its content size in points, `{ width, height }` (480×360 by default). |
| `position` | Its top left corner on the screen, `{ x, y }`. GTK always places windows itself. |
| `level` | `"normal"`, `"floating"` or `"desktopIcon"`, as `setLevel`. Not on Linux. |
| `frame` | `"none"`: no title bar or border; the app draws everything. |

The window it returns has `id`, `focus()` (shows it, also after `hide()`, and brings it to the front), `hide()` and
`close()` (its content unmounts). When the user closes it, it closes as `close()` would; closing the main window closes
it too. `Window.open` runs on Linux and Windows; on other platforms it throws.

## Application

```ts
const Application: {
  quit(): void;
};
```

| Member | Rust | Description |
|---|---|---|
| `quit()` | `quit()` | Quits the app, as the application menu's Quit item (`{ role: "quit" }`) and the platform's quit key do. |

`Application.quit()` is for a Quit of the app's own (a menu item with its own label, a button) and for restarting after an
update. The app stops as when its last window closes, then its run ends, also when closing the window alone would
leave it running: a window kept hidden for the Dock, a tray icon. Closing goes through the same steps as the Quit
item: the standalone app on macOS returns from `run_app` as with Cmd+Q; in a GPUI app that owns the window, the request
reaches its `AppView::on_close_request` handler as `CloseRequest::Quit`, which quits when the app is ready; elsewhere
the quit goes ahead as the Quit item's does. A view
embedded in another app leaves the app's lifecycle to that app. In the terminal the app ends, as with a quit role.

## A title bar of your own

With the title bar hidden, the app draws its own bar and says which parts of it move the window, with `.window_drag(true)` in Rust and `desktop={{ windowDrag: true }}` in TSX:

```rust tabs
use nsh_core::{
    desktop::TitleBarStyle,
    element::{desktop::{set_title_bar, title_bar}}, prelude::*,
};

#[component]
fn TitleBar() -> impl IntoElement {
    set_title_bar(TitleBarStyle::Hidden, Some(px(40.0))); // a bar 40 points tall
    view()
        .window_drag(true)
        .h(move || match title_bar().top {
            top if top > px(0.0) => top,
            _ => px(40.0),
        })
        .pl(move || title_bar().left + px(8.0)) // macOS's traffic lights
        .pr(move || title_bar().right + px(8.0)) // the caption buttons on Windows, GTK's buttons
        .flex_row()
        .items_center()
        .child(text("My App"))
        .child(view().flex_1())
        .child(input().placeholder("Search").w(px(180.0)))
}
```

```tsx tabs
import { Text, View } from "nsh";
import { Input } from "nsh/base";
import { titleBar, Window } from "nsh/desktop";

Window.setTitleBar({ style: "hidden", height: 40 }); // a bar 40 points tall

function TitleBar() {
  return (
    <View
      desktop={{ windowDrag: true }}
      style={{
        height: titleBar().height || 40,
        paddingLeft: titleBar().left + 8, // macOS's traffic lights
        paddingRight: titleBar().right + 8, // the caption buttons on Windows, GTK's buttons
        flexDirection: "row",
        alignItems: "center",
      }}
    >
      <Text>My App</Text>
      <View style={{ flex: 1 }} />
      <Input placeholder="Search" style={{ width: 180 }} />
    </View>
  );
}
```

Pressing a view with `windowDrag` drags the window as the system's title bar would: a double click maximizes it (or does what the user set: on macOS, "Double-click a window's title bar to"), and on Windows and Linux a right click opens the window menu. In GPUI apps on Linux, a right click goes to the app instead when the draggable view under it, or a view inside that one, listens to `onContextMenu`; listeners on views around it, the root included, do not take the window menu away.

- The nearest view that sets `windowDrag` decides: `.window_drag(false)` (`desktop={{ windowDrag: false }}`) keeps a part of a draggable view clickable.
- Text fields and views with press or touch handlers (`onPress`, `onTouch*`, responders) inside a draggable view stay clickable without it: the search field and any buttons in the example above work as usual.
- `windowDrag` also works with the system's title bar: a sidebar or toolbar can move the window too.

### titleBar

```ts
const titleBar: Accessor<{ height: number; left: number; right: number }>;
```

The hidden title bar's area, in points (reactive): its `height`, and the widths the system's window controls take at its `left` and `right` ends. Lay your bar out with it so that nothing goes under the controls. All three are 0 while the system shows its title bar (and in full screen, and off desktops). Rust: `title_bar() -> Edges<Points>`, whose `top` is the height and `left` and `right` the controls' widths; reading it subscribes to changes.

| | `height` | `left` | `right` |
|---|---|---|---|
| macOS | `height`, else the title bar's (28 or 32, by macOS version) | The traffic lights' width | 0 |
| Windows | `height`, else 32 | 0 | The three caption buttons (138) |
| Linux | `height`, else the header bar's | The window buttons at the start (the desktop's button layout) | The window buttons at the end |

On Windows the caption buttons are drawn by NSH in Windows 11's style and follow the app's appearance; hovering the maximize button shows Windows 11's snap layouts. The window keeps its resize borders, shadow and rounded corners on every platform.

## The window's state and the displays

```ts
const windowState: Accessor<WindowState | null>;
const displays: Accessor<Display[]>;

interface WindowState {
  bounds: Bounds;              // its outer frame now, decorations included
  windowBounds: WindowBounds;  // its mode and the bounds it is restored to
  minimized: boolean;
  hidden: boolean;             // off the screen, not minimized: the app hidden, the window not shown
  display: Display | null;     // the display with most of it
}
interface WindowBounds {
  mode: "windowed" | "maximized" | "fullscreen";
  bounds: Bounds;
}
interface Display {
  id: string;                  // the same in the app's next launch
  bounds: Bounds;
  visibleBounds: Bounds;       // without the menu bar, the Dock or the taskbar
  primary: boolean;
}
```

`windowState()` is the app's window as the platform reports it, and `displays()` the displays, the primary one first; both are reactive. Every rectangle is in points, in screen coordinates: from the primary display's top left corner, with `y` growing down, as `setFrame` takes them. A change of mode, of display, or of being minimized or hidden is reported at once; a move or a resize once the window stops for a moment, so what reads them runs once per move rather than at every step. Rust: `window_state() -> Option<WindowState>` and `displays() -> Vec<Display>` (`nsh_core::desktop`'s types); reading them subscribes to changes.

`minimized` and `hidden` tell whether the window is on the screen: it is when neither is set. `hidden` is the app hidden (macOS's Hide) or the window not shown yet or any more. While the window is on the screen, [`lifecycle()`](./nsh.md#device) reporting `"background"` means other windows cover it.

`windowBounds` is what to save to open the window as the user left it: a maximized or full screen window carries the bounds it returns to. The window and the displays are there when the app starts, so the root can restore the saved bounds as it first renders; the same entry serves a Rust root and a TSX one, as `WindowBounds`' serde form is the object TSX saves.

```rust tabs
#[component]
fn App() -> impl IntoElement {
    if let Some(saved) = Storage::get_item("window")
        .and_then(|json| serde_json::from_str::<WindowBounds>(&json).ok())
    {
        set_window_bounds(saved); // fitted on today's displays
    }
    effect(|| {
        if let Some(state) = window_state() {
            let json = serde_json::to_string(&state.window_bounds).unwrap();
            let _ = Storage::set_item("window", json);
        }
    });
    view() // ...
}
```

```tsx tabs
import { effect, Storage } from "nsh";
import { Window, windowState } from "nsh/desktop";

const saved = await Storage.getItem("window");
if (saved) Window.setBounds(JSON.parse(saved)); // fitted on today's displays

effect(() => {
  const state = windowState();
  if (state) Storage.setItem("window", JSON.stringify(state.windowBounds));
});
```

macOS reports the window's state and the displays. On iOS, Android and the web, `windowState()` is `null`, `displays()` is empty and `setBounds` does nothing.

## Menu

```ts
const Menu: {
  setApplicationMenu(items: MenuItem[] | null): void;
  popup(items: MenuItem[], at?: Point): Promise<void>;
};
```

| Member | Description |
|---|---|
| `setApplicationMenu(items)` | Sets the application menu: one submenu per top-level item (`File`, `Edit`, ...). On macOS it is the menu bar, whose first menu is the app menu (AppKit titles it with the app's name). On Windows and Linux it is the window's menu bar, which is not shown while the title bar is hidden; its shortcuts still work, and a title bar of the app's own can open its menus with `Menu.popup`. `null` restores the platform's default menu. |
| `popup(items, at?)` | Shows a context menu with its top left corner at `at` (the window's coordinates, in points: an `onContextMenu`'s `e.pagePosition`), or at the pointer. The promise resolves once the menu closes, whether an item was chosen or not: a button that opened it can stay pressed until then. |

In Rust, `set_application_menu(items)` takes a `Vec<MenuItem>` (`None` restores the platform's default menu), and `popup_menu(items)` (at the pointer) and `popup_menu_at(items, at)` return an `Answer<()>` that comes once the menu closes (awaiting it is optional). An item's handler runs under the owner that set the menu, while that owner lives.

Call `setApplicationMenu` again to change the menu. Inside an `effect` it follows the signals it reads: a check mark or a disabled state that depends on app state stays current.

### Menu items

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

| Field | Description |
|---|---|
| `label` | What the item shows. A role's item has a label of its own without it. |
| `onSelect` | Called when the item is chosen, by a click or its shortcut. |
| `shortcut` | Its keyboard shortcut: modifiers and one key joined by `+`, such as `"mod+n"`, `"mod+shift+z"`, `"alt+f4"` or `"f11"`. The modifiers are in the table below. Keys are letters, digits and punctuation, written as the character the key types on a US keyboard without Shift (`,`, `/`; `"shift+/"` for `?`), or a name: `plus` (the `=` / `+` key, also `=`), `minus` (also `-`), `space`, `enter` (`return`), `escape` (`esc`), `tab`, `backspace`, `delete` (`del`), `insert`, the arrows `up`, `down`, `left`, `right` (`arrowup`, …), `home`, `end`, `pageup`, `pagedown`, `f1`–`f24`. Case does not matter (`"Mod+Shift+N"` is `"mod+shift+n"`; a capital letter is the same key, not Shift), and lowercase is the recommended style. An invalid shortcut (`"mod-n"`, `"mod++"`, `"mod+?"`) throws an error that gives the right spelling. `keymap` takes the same keystrokes. Menus show it in the platform's own style (`⇧⌘N` on macOS, `Ctrl+Shift+N` on Windows). |
| `when` | When the shortcut chooses the item while a text field has the keyboard focus, on the GPUI backend: `"always"` (the default), `"auto"` (unless the field uses the key itself: keys that type, move the caret or delete, and the editing shortcuts, as with [`keymap`](./nsh.md#keymap)'s bindings) or `"notEditing"`. A key the item leaves goes to the field. |
| `disabled` | `true` grays the item out (default `false`). |
| `checked` | Set (to `true` or `false`) for an item with a check mark. |
| `submenu` | Makes the item a submenu (it needs a `label`). |
| `role` | A standard item the platform carries out (below). |

`null`, `undefined` and `false` items are left out, so `cond && { … }` works in a list.

In Rust, an item is `MenuItem::new(label)`, `MenuItem::role(role)`, `MenuItem::separator()` or `MenuItem::submenu(label, items)`, with the fields as methods: `.shortcut("mod+n")` (it panics on an invalid shortcut, with the right spelling), `.shortcut_when(When::Auto)`, `.disabled(true)`, `.checked(on)`, `.on_select(f)`, and `.label(..)` for a role's item.

Modifiers:

| Modifier | macOS | Windows | Linux |
|---|---|---|---|
| `mod` (also `cmdorctrl`) | Command (⌘) | Ctrl | Ctrl |
| `cmd` (also `command`, `meta`, `super`, `win`) | Command (⌘) | Windows key | Super |
| `ctrl` (also `control`) | Control (⌃) | Ctrl | Ctrl |
| `alt` (also `option`, `opt`) | Option (⌥) | Alt | Alt |
| `shift` | Shift (⇧) | Shift | Shift |

Use `mod` for an app's own shortcuts: it is the key each platform's apps use for them (⌘N on macOS, Ctrl+N on Windows and Linux), so one spelling fits everywhere. The Windows key and Super mostly belong to the system (Win+E, the GNOME overview), so `cmd` is rarely right outside macOS.

Roles:

| Role | Does | Platforms |
|---|---|---|
| `about`, `hide`, `hideOthers`, `showAll` | The app menu's standard items | macOS (left out elsewhere) |
| `quit` | Quits the app | All |
| `undo`, `redo`, `cut`, `copy`, `paste`, `selectAll` | Edit the focused text field (its own shortcuts stay its own) | All |
| `minimize`, `zoom`, `toggleFullScreen`, `close` | The window | All |

A role's item has the platform's label and shortcut unless it sets its own (macOS adds the app's name to `about`, `hide` and `quit`).

Menu shortcuts come first, as in native apps: a key an item takes never reaches the focused text field or NSH's own keys. Only a binding of [`keymap`](./nsh.md#keymap) of the same keystroke comes before them. So give the editing shortcuts (`mod+c`, `mod+v`, `mod+x`, `mod+a`, `mod+z`) only to the edit roles, which pass them to the text field, and avoid keys without a modifier (`escape`, the arrows, letters), which would take them from text fields and from back navigation. On the GPUI backend, `when: "auto"` lets an item share a key with text fields: `delete` or `mod+z` choose the item while no field has the focus and edit the text while one has it.

On the GPUI backend, a native component with the keyboard focus sees each key first. A key it handles itself never reaches the keymap or the menu. A key it lets through reaches the keymap as if a text field were focused, then the menu.

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

A context menu, from a button's press:

```rust tabs
text("More").on_press(|| {
    let _ = popup_menu(vec![
        MenuItem::new("Rename").on_select(rename),
        MenuItem::new("Duplicate").on_select(duplicate),
        MenuItem::separator(),
        MenuItem::new("Delete").on_select(remove),
    ]);
})
```

```tsx tabs
<View
  onPress={() =>
    Menu.popup([
      { label: "Rename", onSelect: rename },
      { label: "Duplicate", onSelect: duplicate },
      { type: "separator" },
      { label: "Delete", onSelect: remove },
    ])
  }
>
  <Text>More</Text>
</View>
```

> [!NOTE]
> Menus are the system's (NSMenu, Win32 menus, GTK popover menus): they look and behave like every other app's, follow the system's (or the app's) appearance and are keyboard accessible. `examples/desktop` is an app with a title bar of its own, an application menu and context menus.

## Tray

```ts
interface TrayModule {
  set(options: TrayOptions | null): void;
}
```

`Tray.set` shows the app's icon in the system tray, or changes it; `null` removes it. There is one per app. The tray is the notification area on Windows, the right side of the menu bar on macOS, and a StatusNotifierItem on Linux, which Plasma, Waybar and most panels show (GNOME needs the AppIndicator extension).

| Option | Description |
| --- | --- |
| `icon` | The icon, as `<Image>`'s `src` takes it. Panels draw it at 16 to 24 pixels, so keep it simple and square. On macOS and Windows an SVG is a template drawn in the color of the menu bar's or taskbar's text. Without it, Windows shows the app's icon. |
| `tooltip` | Shown when the pointer rests on the icon; the app's name without it. |
| `menu` | Its menu, made of the same items as `Menu`'s. A secondary click opens it, and so does a primary one when there is no `onClick`. |
| `onClick` | Called when the icon is clicked with the primary button (on Windows, Enter or Space on the icon too). |

Call `set` again to change it; inside an `effect` it follows the signals it reads.

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

TSX only for now; Rust components have no tray yet.

## GlobalShortcut

```ts
interface GlobalShortcutModule {
  register(shortcut: string, onPress: () => void): Promise<boolean>;
  unregister(shortcut: string): void;
  unregisterAll(): void;
}
```

A global shortcut reaches the app whichever app has the keyboard. `register` takes a keystroke written as a menu item's `shortcut` (`"mod+shift+k"`) and resolves with whether the system took it: `false` when another app holds it, or off desktops. Registering a keystroke again replaces its function.

On macOS they ask the user for no permission. A letter is the key that types it on the keyboard layout at registration, so `k` stays `k` on Dvorak.

On Windows a keystroke is the app's alone: one another app holds, or one the system keeps (Win+L), resolves `false`. Holding the keys down presses once.

On Linux, shortcuts go through the desktop portal (`org.freedesktop.portal.GlobalShortcuts`), the only way on Wayland. The keystroke is only a preference: the desktop may ask the user to confirm it or pick another. Hyprland binds them in its own configuration, by the app's id and the keystroke written in lowercase: `bind = CTRL ALT, K, global, com.example.mail:ctrl+alt+k`.

```tsx
import { GlobalShortcut, Window } from "nsh/desktop";

const ok = await GlobalShortcut.register("mod+shift+space", () => Window.orderFront());
```
