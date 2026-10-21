# API naming

NSH's TSX names follow `gpui-base`'s (`nsh-base` is its counterpart, `architecture.md` "Layers"): the names GPUI Kit
settled, translated to TSX. Before naming anything apps use, look for the term in `gpui-base`
(`gpui-kit/crates/base/src`), then GPUI, then `gpui-component`, then macOS / Windows / UIKit controls; React Native
is a reference for which capabilities exist, never for names. GPUI Kit's own rules are "Vocabulary is part of the
API", "Use precise domain words" and "Callback and event wording" in `gpui-kit/website/docs/coding-guides.md`
(cited below as *guides*). The table at the end lists the renames decided so far; until they land, the code still has
the old names.

## Translation

- `snake_case` → `camelCase`: `on_open_change` → `onOpenChange`, `close_on_escape` → `closeOnEscape`,
  `accessibility_label` → `accessibilityLabel`.
- A builder flag (`.disabled(bool)`, `.open(bool)`) is a boolean prop of the same name; readers (`is_open`) have no
  TSX form.
- A builder taking a part (`Dialog::backdrop(element)`) is a part prop (below); a closure builder of state styles
  (`.active(|s| ..)`, `ButtonStyles::selected(..)`) is a `<state>Style` prop and a `<state>:` class.
- A Rust enum's variants are a string union of the same words (`DialogChangeReason::BackdropPress` →
  `"backdropPress"`).
- One word stays one word: `readonly`, never `readOnly` (*guides* "readonly"; `docs/READONLY-NAMING-RESEARCH.md`).

## Conventions

- **Open state**: `open` + `onOpenChange(open, reason)`, `defaultOpen` when uncontrolled (`Dialog::open`,
  `Dialog::on_open_change`, `Popover::default_open`). The reason is the variant that closed it (`DialogChangeReason`).
  Never `visible` / `onRequestClose`. *open/close* is an overlay's state, *show/hide* a transient request, *expand /
  collapse* structure (*guides*).
- **Value state**: `value` + `onChange(next)`, `defaultValue` when uncontrolled (`Slider::default_value`,
  `Switch::on_change`); a second value of one component is `on<Value>Change` (`onSelectionChange`).
- **Dismissal flags**: `closeOn<Trigger>`, positive, default `true` (`Dialog::close_on_escape`,
  `Dialog::close_on_backdrop_press`). Escape is also Android's back button. A flag is read natively when the user
  acts (the main thread never waits for JS); the trigger is reported either way (next rule), so a flag set to `false`
  lets the app confirm and close itself. A stack's screens use `popOn<Trigger>` the same way.
- **Events**: `on<Event>` with a present-tense verb or noun, never past tense (`on_click`, `on_close`, `on_dismiss`,
  `on_confirm`); *guides*: `on_change` observes, `on_confirm` commits, `on_dismiss` closes a transient surface,
  `on_will_change` may veto (NSH has no veto: the main thread never waits for JS). Lifecycle pairs are `onOpen` /
  `onClose` (`DialogTrigger::on_open`, `Dialog::on_close`). A user's attempt is named after the act (`onCancel`,
  `onBackPress`: `Dialog::on_cancel`, `DialogChangeReason::*Press`), never `on<Noun>Request`.
- **Press, not click**: `onPress`, `onPressDown`, `onPressUp`, `onLongPress` (below, Button). `gpui-base` itself says
  *press* for pointer-independent activation (`DialogChangeReason::TriggerPress`, `close_on_backdrop_press`).
- **States**: `disabled` blocks interaction, `readonly` allows selecting but not editing, `selected` is membership /
  the current item, `checked` / `pressed` the value of a checkbox / toggle (*guides*; `Selectable`, `Disableable`,
  `Toggle::pressed`). Booleans are named after the state, positively (`open`, `hidden`), with no `is` prefix and no
  `Enabled` suffix when a state word exists.
- **Parts**: a component's own `class` / `style` style its main part; every other part gets `<part>Class` /
  `<part>Style`, named after `gpui-base`'s part (`Dialog::backdrop` / `Dialog::popup`; `gpui-component`
  `Settings::header_style`). The part name never repeats the component (`contentStyle`, not `contentContainerStyle`:
  *guides* "Let the enclosing name carry the context").
- **Slots**: named after what they are, as `gpui-kit`'s slots are (`header`, `footer`, `trigger`, `content`, `empty`,
  `separator`: *guides*, `gpui-component` `Command::empty` / `header` / `footer`); an element or a component. Never
  React Native's `*Component` suffix.
- **State styles**: `active`, `hover`, `focus`, `disabled`, `selected` as `<state>:` classes and `<state>Style`
  objects (GPUI `InteractiveElement::active` / `hover`, `ButtonStyles::selected` / `disabled`). Later states win, and
  `disabled` wins over all (`state_style.rs` `resolve_style`). No per-feedback props (`activeOpacity`).
- **Roles, not looks**: a choice that means something says what (`role: "cancel" | "destructive"`, as SwiftUI's
  `ButtonRole`); a look is the app's (`gpui-component`'s `ok_variant` has no `nsh-base` counterpart). An element's
  accessibility role is `role` (GPUI `.role(Role::…)`, `Button::role`), its name `accessibilityLabel`
  (`Button::accessibility_label`).
- **Colors are styles**: a color of a part or a pseudo-element is a style key with its class (`placeholderColor`,
  `selectionColor`), not a prop.
- **Platform props only for platform-only things**: something two platforms do is a top-level prop, ignored (and
  documented as such) where it does not apply. One name per concept, never the same prop under `ios` and `desktop`.
- **Accessors** are named after what they read (`safeArea()`, `windowSize()`, `titleBar()`); they never differ from
  a module's name by case only.
- **Navigation verbs** are `gpui-base`'s: `push`, `pop`, `popToRoot`, `replace` (`NavStackState`); replacing all is
  `set<Field>` (*guides* "In-place mutation: `set_<field>`").

## When `gpui-base` is not followed

`gpui-base` is the reference, not a rule above judgment: a name is not taken when it reads worse for TSX apps on
every platform, as the user decided (2026-10-20) for `onSubmit` (a phone's return key is not Enter), `animation`
(`transition` is a style key), `multiline` (one word) and a page's `onDismiss`. Each such row says "not followed" and why.

## Renames

Decided with the user (2026-10-20): components take `gpui-base`'s names too, the dismissal flags default to `true`,
and `Input`'s React Native names follow the rules as well.

| Now | New | Precedent | Notes |
|---|---|---|---|
| **Components** | | | |
| `Stack`, `StackProps` (`navigator`) | `NavStack`, `NavStackProps` (`state`) | `nav_stack.rs` `NavStack`, `NavStackState` | |
| `Screen`, `ScreenProps` | `NavPage`, `NavPageProps` | `nav_stack.rs` `NavPage` | |
| `createNavigator`, `Navigator<R>` | `createNavStack(routes: R[])`, `NavStackState<R>` | `NavStackState::new` | |
| `Modal`, `ModalProps` | `Dialog`, `DialogProps` | `dialog.rs` `Dialog` ("a modal surface") | One component, `presentation` picks the native form; `Sheet` stays the name of a future edge panel drawn with primitives (`sheet.rs`) |
| `Alert`, `ActionSheet` | `AlertDialog` | `alert_dialog.rs` `AlertDialog` | Below |
| **Modal** | | | |
| `visible` | `open` (default `true`) | `dialog.rs` `Dialog::open` (default `true` in `Dialog::new`) | |
| `onRequestClose` | `onCancel()` (the attempt) + `onOpenChange(false, reason)` (it closed) | `Dialog::on_cancel`, `Dialog::on_open_change`, `DialogChangeReason` | `reason`: `"backdropPress" \| "cancel" \| "swipe"`; `"swipe"` (an iOS / phone browser sheet pulled down) has no precedent |
| `dismissible` | `closeOnEscape`, `closeOnBackdropPress`, `closeOnSwipe` (default `true` each) | `Dialog::close_on_escape`, `Dialog::close_on_backdrop_press` | `closeOnSwipe` by analogy. Default flips: today nothing closes natively |
| `onShow` | `onOpen` | `DialogTrigger::on_open` | After the entrance transition |
| `onDismiss` | `onClose` | `Dialog::on_close`, `Sheet::on_close` | Also when the app removed it |
| `animationType` | `animation` | none (`NavStack::transition` not followed) | Same prop as NavPage's; values stay. `transition` is a style key (style transitions), and `gpui-base`'s `Transition` is timing, not a preset |
| `transparent` | (dropped) | `Dialog::backdrop` / `Dialog::popup` own no look (*AGENTS* "Base controls are no-style") | Nothing NSH draws has a background; the app styles the popup and the backdrop |
| `ios` / `desktop` / `web` `presentationStyle` | `presentation` | none (UIKit `modalPresentationStyle`) | Top-level; `Style` is reserved for style objects |
| (new) | `class` / `style` style the popup; `backdropClass` / `backdropStyle` | `Dialog::popup`, `Dialog::backdrop` (`DialogPopup`, `DialogBackdrop`) | `Sheet`'s `overlay` / `surface` are the same parts; `Dialog`'s names win as the primary modal host |
| **Screen** | | | |
| `headerTintColor`, `headerBackgroundColor`, `headerTitleColor` | `headerStyle={{ backgroundColor, color, tintColor }}`, `headerClass` | `gpui-component` `Settings::header_style` | `color` is the title's, `tintColor` the buttons'; only these keys |
| `ios` / `desktop` `headerLargeTitle` | `headerLargeTitle` | none | Top-level |
| `ios` / `desktop` `headerBackTitle` | `headerBackTitle` | none | Top-level |
| `onDismiss` | unchanged | `NavStackEvent::Popped` (not followed) | The user popped it (back button, swipe). Decided by the user: a page leaving the stack is dismissed, as UIKit says |
| `preventBack` | `popOnBack` (default `true`; `false` = `preventBack`) | `Dialog::close_on_escape` | By analogy: `popOn<Trigger>` as `closeOn<Trigger>`; covers every back (header, Android back, swipe) |
| `onBackRequest` | `onBackPress` | `DialogChangeReason::*Press`; `nsh/android` `"hardwareBackPress"` | Fires on every back press, before the pop when `popOnBack` allows it |
| `gestureEnabled` | `popOnSwipe` (default `true`) | `Dialog::close_on_backdrop_press` | By analogy; the interactive gesture only, the buttons still pop |
| `animation` | unchanged | | As Dialog's |
| (new, desktop / web header) | `header` | *guides* named slots (`header`, `footer`) | Top-level slot: the whole bar where NSH draws it |
| **Navigator** | | | |
| `push`, `pop(count?)`, `replace` | unchanged | `NavStackState::push` / `pop` / `replace` | |
| `popToTop` | `popToRoot` | `NavStackState::pop_to_root` | |
| `reset(routes)` | `setRoutes(routes)` | *guides* `set_<field>` (`set_items`) | `NavStackState::clear` empties, which a stack of screens cannot |
| `remove(route)` | unchanged | none | |
| `createNavigator(initial: R \| R[])` | `createNavigator(routes: R[])` | `NavStackState::new` then `push` | One shape |
| **Alert, ActionSheet** | | | |
| `Alert.alert(title, message, buttons, options)`, `ActionSheet.show(options)` | `AlertDialog.show({ title, description, actions, presentation, closeOnEscape, closeOnBackdropPress })` → `Promise<string \| null>` | `alert_dialog.rs` `AlertDialog`, `AlertDialogTitle`, `AlertDialogDescription`, `AlertDialogAction`, `AlertDialogCancel` | Resolves with the action's `id` (its `label` without one), `null` when closed without an action |
| `message` | `description` | `AlertDialogDescription`, `DialogDescription` | |
| `buttons` / `options` (+ `cancelButtonIndex`, `destructiveButtonIndexes`) | `actions: { label, role?, id? }[]` | `AlertDialogAction`; *guides* fluent property `label` | One shape for both |
| `AlertButton.text` | `label` | `gpui-component` `Button::label` | |
| `AlertButton.style: AlertButtonStyle` | `role: "default" \| "cancel" \| "destructive"` | `AlertDialogCancel`; GPUI `Role`; SwiftUI `ButtonRole` | A role, not a look |
| `ActionSheet` (a module) | `presentation: "alert" \| "actionSheet"` | none (UIKit `preferredStyle`) | Default `"alert"`; same word as Modal's `presentation` |
| `AlertOptions.cancelable` | `closeOnEscape` + `closeOnBackdropPress` | `AlertDialog::close_on_escape`; `Dialog::close_on_backdrop_press` | Default `false` for `"alert"` (`AlertDialog` has no backdrop dismissal), `true` for `"actionSheet"` |
| **StatusBar** | | | |
| `StatusBar` (a component; the most recently mounted wins) | `statusBar?: StatusBar` on `NavPage` and `Dialog`; `nsh.json` `"statusBar"` for the default | UIKit: each view controller's `preferredStatusBarStyle` / `prefersStatusBarHidden`, the topmost deciding; Info.plist `UIStatusBarStyle` / `UIStatusBarHidden`; `gpui-base` has none | Decided by the user (2026-10-20): the status bar follows UIKit; pages and dialogs are NSH's view controllers. Rust: `Screen::status_bar`, `Modal::status_bar`, `Root::status_bar` |
| `foreground: "auto" \| "light" \| "dark"` (`Foreground`) | `foreground` of `StatusBar` (`StatusBarForeground`) | none; `gpui-kit` theme `foreground` (the content's color) | Not `style` (the inline style every component takes); `"light"` is light content, for dark backgrounds |
| `hidden`, `animated` | fields of `StatusBar` | `NavMotion::Animated` (UIKit `animated:`) | `nsh.json` takes `foreground` and `hidden` |
| `StatusBar.setBarStyle`, `StatusBar.setHidden`, `BarAnimation` | (dropped) | | The pages and dialogs are the API |
| `android.backgroundColor`; `nsh/android` `setStatusBarBackgroundColor` | (dropped) | | Edge-to-edge (API 35) has no bar color |
| **KeyboardAvoidingView** | | | |
| `behavior="position"`, `contentContainerStyle` | (dropped) | | `behavior: "padding" \| "height"` |
| `keyboardVerticalOffset` | `offset` | `popup.rs` `Popup::offset` | The enclosing name says keyboard and vertical |
| `enabled` | unchanged | *guides* "Name predicates positively" | `disabled` means no presses on a view |
| **Keyboard** | | | |
| `keyboard` (accessor) | `keyboardFrame` | none (UIKit's `keyboardFrameEndUserInfoKey` and animation duration arrive together) | Differed from `Keyboard` by case only; `Keyboard.dismiss()` unchanged |
| **List, Scroll** | | | |
| `ListHeaderComponent`, `ListFooterComponent` | `header`, `footer` | `gpui-component` `Command::header` / `footer`; *guides* named slots | Also in `ListItemsProps` |
| `ListEmptyComponent` | `empty` | `Command::empty` | |
| `ItemSeparatorComponent` | `separator` | `Command::separator` | |
| List `contentContainerStyle` | `contentStyle` (padding and gaps only), `contentClass` (`p-*`, `gap-*`) | *guides* "Let the enclosing name carry the context" | Same names as Scroll's, a narrower type |
| Scroll `contentContainerStyle`, `contentContainerClass` | `contentStyle`, `contentClass` | as above; *guides* slot `content` | |
| **Input** | | | |
| `blurOnSubmit` | (dropped) | | `submitBehavior` covers it |
| `placeholderTextColor` | style key `placeholderColor`, class `placeholder:text-*` | *guides* `value` vs `presentation`; CSS `::placeholder` | |
| `selectionColor` (Input, Text) | style key `selectionColor`, class `selection:text-*` | CSS `::selection` | |
| `editable` | `readonly` (`editable={false}` = `readonly`) | *guides* "readonly"; `gpui-component` `Input::readonly` | `disabled` (no focus, `disabled:` classes) stays separate |
| `onChangeText(text)` | `onChange(text)` | `InputEvent::Change`; `on_change` observes (*guides*) | |
| `onSubmitEditing` | `onSubmit` | `InputEvent::PressEnter` (not followed) | The return key, whatever `returnKeyType` shows: on a phone it says Done / Search / Go, not Enter; HTML's `submit` |
| `secureTextEntry` | `masked` | `gpui-component` `Input::masked` | |
| `multiline` | unchanged | `Input::multi_line` (not followed) | One word, as `readonly` |
| `selection` | `selectedRange` | `Input::selected_range` | `onSelectionChange` stays |
| `autoFocus`, `maxLength`, `keyboardType`, `returnKeyType`, `submitBehavior`, `autoCapitalize`, `autoCorrect`, `autoComplete`, `caretHidden`, `onKeyPress`, `onContentSizeChange`, `allowFontScaling`, `maxFontSizeMultiplier`, `ios.textContentType` | unchanged | none | No `gpui-kit` term differs; `textContentType` stays iOS's |
| **View** | | | |
| `accessibilityRole` | (dropped; `role` stays) | GPUI `.role(Role::…)`; `Button::role` | |
| `activeOpacity` | `activeStyle={{ opacity }}` or `active:opacity-*` | GPUI `InteractiveElement::active` | |
| `underlayColor`, `underlayOpacity` | `activeStyle={{ backgroundColor }}`; children `group-active:opacity-*` | as above | Native while pressed, as today |
| (new) | `hoverStyle`, `focusStyle`, `disabledStyle` | GPUI `hover` / `focus`; `ButtonStyles::disabled` | As the `<state>:` classes |
| **Button** (`nsh/base`) | | | |
| `disabled`, `selected`, `accessibilityLabel` | unchanged | `button.rs` `Button::disabled`, `selected`, `accessibility_label` | Add `role` (`Button::role`) |
| `onPress` | unchanged | `gpui-base`'s *press* (`DialogChangeReason::TriggerPress`, `close_on_backdrop_press`) over `Button::on_click` | Every view has them: one word for one concept across `nsh` and `nsh/base` (*guides*); touch, pen, mouse and keyboard activation are presses, *click* is a mouse's (*guides* "Use `on_click` only for a genuine click-level contract") |
| `onPressIn`, `onPressOut` (every view, Button) | `onPressDown`, `onPressUp` | UIKit `.touchDown` / `.touchUp*`; GPUI `on_mouse_down` / `on_mouse_up` | Decided by the user: *down* / *up* are both frameworks' words, *press* any input's; `onPressUp` is the press's end (released, moved away or cancelled). Rust `on_press_down` / `on_press_up` |
| `selected` styles | `selectedStyle`, `selected:` classes | `ButtonStyles::selected` | |
| `checked` styles (new) | `checkedStyle`, `checked:` classes | `CheckboxStyles::checked`, `SwitchStyles::checked`, `RadioStyles::checked` | From `accessibilityState.checked`; mixed is not checked |
| **Overlays and popups** (new) | | | |
| `<Overlay>` (`nsh`) | | none (GPUI's `deferred` / `anchored`, the web's top layer) | A primitive: what shows above the root view, not a component with a look |
| `anchor`, `placement`, `align`, `offset`, `margin` | | `positioner.rs` `Positioner::placement` / `align` / `offset` / `margin`, `Placement`, `Align` | `placement`'s values are `Placement`'s (`"top"`, `"bottom"`, `"left"`, `"right"`), `align`'s `Align`'s (`"start"`, `"center"`, `"end"`) |
| `onPosition` | | `Positioner::on_position` (`ResolvedPosition`) | The payload is `OverlayPosition`: the side it went on, its frame, the room on that side |
| `onPressOutside` | | none (`DialogChangeReason::*Press`) | The act, a press, named after where it went down |
| `onCancel` | | `Dialog::on_cancel`, `actions::Cancel` (Escape in `Popover`) | Escape and the Android back button, as for dialogs |
| `Popover`, `Tooltip`, `Sheet` (`nsh/base`) | | `popover.rs`, `tooltip.rs`, `sheet.rs` | Parts as `Dialog`'s: `popupClass` / `popupStyle`, `backdropClass` / `backdropStyle`; `trigger` and `content` slots |
| `Menu`, `ContextMenu`, `MenuItem`, `MenuSeparator`, `Submenu` (`nsh/base`) | | `gpui-component` `PopupMenu`, `ContextMenu` (`gpui-base` has no menu) | `MenuItem::on_select` as `PopupMenu`'s items; `Submenu`'s `back` slot is the row a sheet shows above its items |
| `onOpenChange(open, reason)` of the popups | | `Popover::on_open_change`, `DialogChangeReason` | `OpenChangeReason`: `"triggerPress"`, `"pressOutside"`, `"backdropPress"`, `"cancel"`, `"select"`, `"hover"`, `"longPress"`, `"contextMenu"` |
