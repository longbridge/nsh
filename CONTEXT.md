# NSH

Native Shell renders one app (TSX or Rust) through each platform's own views. This file names the concepts
introduced for the terminal backend; concepts NSH already has keep the names its code and `AGENTS.md` give them
(`MountItem`, `Surface`, `Development host`, `Release runner`, ...) and are not redefined here.

## Language

### The terminal

**Terminal idiom**:
The device form of a surface shown in a terminal: keyboard first, a pointer when the terminal reports one, drawn
on a grid of cells. A peer of the phone, pad and desktop idioms; the operating system is still reported as the
real one.
_Avoid_: TUI mode, console platform

**Cell**:
One character position of the terminal grid, the unit the terminal backend draws in; a fixed number of points
wide and tall.
_Avoid_: pixel, character, block

**Key scheme**:
The set of keys a terminal surface is operated with when the app binds none of them: moving between what can be
pressed or typed in, scrolling, searching, running commands, going back, editing text. The Emacs key scheme is
the first; a vim one may follow.
_Avoid_: keymap (the app's own shortcuts), keybindings, mode

**Minibuffer**:
The one-line input a terminal surface shows at its bottom only while the user searches the screen or runs a
command, and removes when done; the terminal backend has no permanent chrome.
_Avoid_: command line, status line, mode line, echo area

**Page background**:
The fill of a root view, a screen or a modal, and any fill of the same color; drawn as the terminal's own
background so an app follows the user's terminal colors.
_Avoid_: canvas color, window color

### Degradation

**Degradation**:
Showing a mount item, command or event the terminal cannot render as specified in a documented substitute form
that makes clear something was substituted, rather than ignoring it.
_Avoid_: fallback, downgrade, unsupported

**Content feature**:
A feature that carries what the user reads or acts on (images, video, web views, native components); when
degraded it shows a placeholder and is reported once.
_Avoid_: media

**Decorative feature**:
A feature that only changes how something looks (shadows, blur, materials, 3D transforms, blend modes); when
degraded it is left out silently, as listed in the backend's documentation.
_Avoid_: cosmetic, effect
