---
title: nsh/keymap
description: "Keyboard shortcuts: bind keystrokes such as mod+k to handlers, on macOS, Windows and Linux."
order: 8.7
---

# nsh/keymap

`nsh/keymap` binds the app's keyboard shortcuts: a keystroke such as `"mod+k"` and the function it runs.

```ts
import { keymap } from "nsh/keymap";
```

```ts
const keymap: {
  bind(keystroke: string, handler: (keystroke: string) => void, options?: KeyBindingOptions): () => void;
  bind(bindings: Record<string, (keystroke: string) => void>, options?: KeyBindingOptions): () => void;
};

interface KeyBindingOptions {
  when?: "always" | "notEditing";
  repeat?: boolean;
}
```

Shortcuts work on macOS, Windows and Linux. On iOS, Android and the web, `bind` keeps the bindings but no key fires them yet, so the same code runs everywhere.

## keymap.bind

```ts
bind(keystroke: string, handler: (keystroke: string) => void, options?: KeyBindingOptions): () => void;
```

Runs `handler` when `keystroke` is pressed and returns a function that removes the binding. The handler gets the keystroke as you wrote it, in its case.

```tsx
import { mount, signal, Text } from "nsh";
import { keymap } from "nsh/keymap";

const open = signal(false);
keymap.bind("mod+k", () => open.set(true));
keymap.bind("escape", () => open.set(false));

mount(() => <Text>{open() ? "Search…" : "Press ⌘K"}</Text>);
```

Pass an object to bind several keystrokes at once; the function returned removes them all:

```ts
const unbind = keymap.bind({
  "mod+s": save,
  "mod+shift+p": openPalette,
  f5: refresh,
});
```

A binding made while a component renders is removed with the component, so a screen's shortcuts live as long as the screen:

```tsx
function Editor() {
  keymap.bind("mod+enter", send);
  return <Input multiline />;
}
```

With the dev server, editing a module removes the bindings its top-level code made before it runs again, so hot reload never leaves stale shortcuts behind.

## Keystrokes

A keystroke is modifiers and a key joined by `+`: `"mod+k"`, `"ctrl+shift+p"`, `"alt+enter"`, `"escape"`, `"f5"`, `"cmd+-"` (Command and minus).

| Modifier | macOS | Windows | Linux |
|---|---|---|---|
| `mod` (also `cmdorctrl`) | Command (⌘) | Ctrl | Ctrl |
| `cmd` (also `command`, `meta`, `super`, `win`) | Command (⌘) | Windows key | Super |
| `ctrl` (also `control`) | Control (⌃) | Ctrl | Ctrl |
| `alt` (also `option`, `opt`) | Option (⌥) | Alt | Alt |
| `shift` | Shift (⇧) | Shift | Shift |

Use `mod` for an app's own shortcuts: it is the key each platform's apps use for them (⌘K on macOS, Ctrl+K on Windows and Linux), so one spelling fits everywhere. The Windows key and Super mostly belong to the system, so `cmd` is rarely right outside macOS.

The key is a letter, digit or punctuation key, written as the character it types on a US keyboard without Shift (`k`, `1`, `,`, `/`, `[`; `"shift+/"` for `?`, `"shift+k"` for Shift+K), or a name: `plus` (the `=` / `+` key, also written `=`), `minus` (also `-`), `space`, `enter` (`return`), `escape` (`esc`), `tab`, `backspace`, `delete` (`del`), `insert`, the arrows `up`, `down`, `left`, `right` (`arrowup`, …), `home`, `end`, `pageup`, `pagedown`, `f1` to `f24`. Case does not matter: `"Mod+Shift+K"` is `"mod+shift+k"`, and a capital letter is the same key, not Shift (`"mod+K"` is `"mod+k"`). Lowercase is the recommended style, which these pages use. `"mod-k"` and `"mod++"` are errors whose messages give the right spelling. Menu shortcuts in `nsh/desktop` take the same keystrokes.

Shortcuts work with any keyboard layout: on a Cyrillic or Greek layout, `mod+k` is the key that types K on a US layout, as menu shortcuts are.

`bind` throws a `TypeError` that says what is wrong with an invalid keystroke (`keymap.bind: invalid keystroke "cmd+kk": unknown key "kk"`), and refuses the keystrokes the system keeps for itself, so an app can never break them:

| Platform | Reserved |
|---|---|
| macOS, iOS | Cmd+Q (quit), Cmd+H (hide), Option+Cmd+H (hide others), Cmd+M (minimize), Ctrl+Cmd+F (full screen) |
| Linux, Windows, Android | Ctrl+Q (quit), Alt+F4 (close), F11 (full screen) |

## While typing

A text input with the keyboard focus needs its keys: typing, moving the caret, deleting, and select all, cut, copy, paste, undo and redo. So by default a binding does not fire while a text input (or a web view) has the focus if its keystroke is one of these: plain keys and keys with only Shift or Option, the arrows, Home, End, Page Up / Down, Return, Tab, Escape, Backspace, Delete, and `mod+a`, `mod+c`, `mod+v`, `mod+x`, `mod+z`. Other keystrokes with Command or Control, and the function keys, fire wherever the focus is.

`when` changes that:

| `when` | Fires |
|---|---|
| (default) | As above |
| `"always"` | Also in text inputs, before the input sees the key |
| `"notEditing"` | Never while a text input has the focus |

```ts
// Sends from the message field too.
keymap.bind("mod+enter", send, { when: "always" });
// Only when the user is not typing.
keymap.bind("mod+e", archive, { when: "notEditing" });
```

## Which binding runs

A key that fires a binding is consumed: it is not typed, the system does not beep, and NSH's own keys do not see it (Escape going back, Tab moving the focus, the arrows scrolling). When several bindings have the same keystroke, the newest one that may fire runs, so a dialog can bind Escape over the screen's binding and the screen's comes back when the dialog's goes. Bindings are checked natively the moment the key is pressed; the handler runs right after, on the JS thread.

Holding a key down fires its binding once, until the key is released; with `repeat: true` it fires again on every auto-repeat (to step through a list, say):

```ts
keymap.bind("alt+down", nextItem, { repeat: true });
```

Menu shortcuts give way to a binding of the same keystroke: the application menu's ([`nsh/desktop`](./nsh-desktop)) on every desktop, and on macOS the built-in menus' too (Cmd+W, Cmd+C, ...). While you develop with `nsh run`, Cmd+R / Ctrl+R always reloads the app, even if it binds `mod+r`; a packaged app can bind it.
