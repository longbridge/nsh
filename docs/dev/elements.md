# Elements and interaction

Every tag is a component in scope, imported from its one home (`native-pipeline.md`): `View`, `Text`, `Scroll`,
`Input` from `nsh`, `Image` from `nsh/image`, `List` from `nsh/list`, `Canvas` from `nsh/canvas`, `WebView` from
`nsh/webview`, `Video` from `nsh/video`, `Modal` (and `Stack` / `Screen`) from `nsh/ui/mobile`, `InputAccessoryView` from
`nsh/ios`; only `If` and `Each` are globals.

| JSX | Native (Apple) | Notes |
|---|---|---|
| `<View>` | `NshView` (UIView) | `onPress` / `onLongPress` / `onPressIn` / `onPressOut`, `activeOpacity`, `pan={{x, y}}` + `onPanStart` / `onPanEnd`, `pinch` / `rotation`; `desktop={{ windowDrag }}` (any element: pressing it drags the window; the platforms decide on the main thread from `ViewProps::window_drag`, the nearest view that sets it decides, inputs and views with press / touch handlers nearer than it stay clickable; `nsh/desktop`) |
| `<Text>` | `TextView` (CTFrame) | nested `<Text>` is a span, other elements are inline attachments ([text.md](text.md)); `numberOfLines`, `ellipsizeMode`, `allowFontScaling`, `maxFontSizeMultiplier`, `adjustsFontSizeToFit` + `minimumFontScale`, `selectable` (range selection) + `selectionColor`, `onTextLayout` |
| `<Image src>` | UIView + image CALayer | `src` is a URL or `import img from "./a.png"` (an asset descriptor with its size); `resizeMode`; `defaultSource` (placeholder), `blurRadius`, `fadeDuration`; `onLoadStart` / `onProgress` (`loaded` / `total` bytes) / `onLoad` (the source's pixel size) / `onError` / `onLoadEnd`; `tintColor`; animated GIF / WebP / APNG; `Image.getSize(uri)` / `Image.prefetch(uri)` |
| `<Scroll>` | UIScrollView | `horizontal`, `contentContainerStyle` / `contentContainerClass`, `refreshing` + `onRefresh`, `scrollValue`, `onScrollEnd`, `keyboardDismissMode`, zooming, see [scrolling-and-lists.md](scrolling-and-lists.md) |
| `<Canvas draw>` | `CanvasView` (drawRect replay) | see [canvas.md](canvas.md) |
| `<List each>` | UIScrollView + recycled cells | see [scrolling-and-lists.md](scrolling-and-lists.md) |
| `<Input>` | UITextField / UITextView in a container | `value` / `defaultValue` / `onChangeText` / `onSubmitEditing` / `onFocus` / `onBlur`, `placeholder`, `secureTextEntry`, `keyboardType` (React Native's values: `number-pad` digits only, `decimal-pad`, `numeric` = iOS's decimal pad / Android's signed decimal, `numbers-and-punctuation` = iOS's numbers and punctuation / Android's signed decimal, `email-address`, `phone-pad`, `url`), `ios={{ keyboardAppearance }}` (`default` / `light` / `dark`; Android's keyboards follow the system theme), `returnKeyType`, `autoCapitalize`, `autoCorrect`, `multiline`, `maxLength`, `editable`, `autoFocus`, `selectionColor`, `blurOnSubmit`, `submitBehavior` (`submit` / `blurAndSubmit` / `newline`, resolved with `blurOnSubmit` and `multiline` by `InputConfig::submit()`, which is all platforms read; a multiline input that submits takes no line break from return, but does from Shift / Option / Alt+Return on hardware keyboards, and a return that commits an IME composition never submits), `selection` + `onSelectionChange` (UTF-16 `start` / `end`), `onKeyPress` (`key`: the text, `Backspace`, `Enter`), `autoComplete` (autofill; `ios={{ textContentType }}` for iOS's names), `caretHidden`, `onContentSizeChange` (the text's size, for growing inputs); ref `setSelection(start, end)` |
| `<Stack>` / `<Screen>` | UINavigationController | `createNavigator(route)`: `push` / `pop` / `popToTop` / `replace` / `reset`; screens keep their state while stacked; native header (content below the header UIKit shows: the platform measures it, `Core::set_screen_header`; `title`, `headerLeft` / `headerRight` / `headerTitle` elements, `headerTransparent`, colors; `ios={{ headerLargeTitle, headerBackTitle }}`), swipe back (`gestureEnabled`: the interactive gesture only); `preventBack` + `onBackRequest` keep the user on a screen (no swipe; the header / Android back buttons send the event instead of popping) |
| `<Modal>` | presented UIViewController | `visible`, `animationType`, `transparent`, `onRequestClose` (controlled), `onShow`, `onDismiss`; `dismissible` (iOS sheets swipe away, Android back closes natively, then `onRequestClose` + `onDismiss`; the renderer keeps a removed modal's `onDismiss` until the platform reports it, `dom.rs` `dismissed`); `ios={{ presentationStyle, supportedOrientations }}` (`pageSheet` / `formSheet`), `desktop={{ presentationStyle }}` (a card centered in the window: Linux, macOS, desktop browsers), `web={{ presentationStyle }}` (also in phone browsers: a card from just below the top, as on iPhones; `docs/dev/web.md`), `android={{ statusBarTranslucent }}` (otherwise the content starts below the status bar) |
| `<InputAccessoryView>` | a NshView set as `inputAccessoryView` | `nativeID`; inputs name it with `ios={{ inputAccessoryViewID }}`. A layout root as wide as the window and as tall as its content; the core resolves the id (`InputConfig::accessory`). Renders nothing on Android |
| `<WebView>` | WKWebView (WebKit loaded on first use) | `source` (`uri` / `html` + `baseUrl`), `injectedJavaScript(BeforeContentLoaded)`, `onMessage` (pages call `window.ReactNativeWebView.postMessage`), `onLoad*`, `onNavigationStateChange`; ref: `reload()`, `goBack()`, `goForward()`, `postMessage(data)`, `injectJavaScript(js)` |
| `nativeComponent` (`nsh/native`) | a `NshView` box filled by the registered component's view | see [embedding.md](embedding.md) |
| `<Video>` | AVPlayerLayer / AVPlayerViewController (AVFoundation loaded on first use) | `source`, `paused`, `muted`, `volume`, `rate`, `repeat`, `resizeMode`, `controls`, `onLoad` / `onProgress` / `onEnd` / `onBuffer` / `onError`; ref: `seek(seconds)` |

On Android the same elements map to `Box` (ViewGroup), `TextBox`, `ImageBox`, `ScrollBox` (ScrollView), `InputBox` (EditText),
`StackBox` (screens with a Toolbar, predictive back), `ModalHost` (a Dialog), `WebBox` (android.webkit.WebView) and `VideoBox`
(MediaPlayer on a TextureView, MediaController); both are `WidgetBox`es. Every host element and runtime component is typed in
`.nsh/types/nsh/types.d.ts` and the declarations of the runtime's modules (made from the Rust tables: `tooling.md`).

**Host props are one definition.** A host prop is a variant of `nsh_core::Prop` (or of the enum it nests: `ScrollProp`,
`NativeProp`, `AccessibilityProp`, ...) with `#[prop(js = "name", ...)]`: `#[derive(HostProps)]` (`nsh_core::js::host`)
derives from it how `nsh-js` decodes the JS value (straight from QuickJS values, no JSON), the default clearing the
prop sets (`HostProps::reset`, also used when a Rust component's render stops setting it) and its declaration: one
interface per element group and platform (`InputHostProps`, `IOSInputHostProps`, ...) that the element types of
`types.d.ts` and the runtime's components extend. The payload's type decodes itself (`HostValue`;
`#[derive(PropEnum)]` for string unions); `default`, `min` / `max`, `on` (the element groups), `ios` / `android`
(names inside `ios={{…}}` / `android={{…}}`), `with` (a decoder of the whole variant, `js/host_values.rs`), `once`
(not reset) and `internal` (set by the runtime, untyped) cover the rest. **Events** are a table next to it
(`host::EVENTS`): the event, the element groups whose props declare it and its handler's type, one row per payload
(`onLoad` gives an image its size and a video its duration); the handlers join the same interfaces. An event no
element declares (the root's, the runtime's own) is listed in `INTERNAL_EVENTS`; a test checks every `EventKind` is
in one of them.

## Primitives only

NSH's runtime provides the basic capabilities. Components are the UI sets' (Rust, `crates/nsh-ui-mobile`,
`docs/dev/components.md`), drawn with views, classes, canvases and native animations so they look the same on every platform. Do not
add platform-styled widgets (UISwitch / SeekBar wrappers and the like), and do not add compositions of existing
primitives to the runtime (section lists, touchables, drawers, ...): they belong to a UI set. React Native is a
reference for which capabilities exist, not an API to be compatible with. Components that need core or native support
are primitives of their own (`List`, `Canvas`, `WebView`, `Video`; in the mobile set `Stack`, `Modal`, `SafeAreaView`,
`KeyboardAvoidingView`; `InputAccessoryView` in `nsh/ios`). Tags are PascalCase names of what is in scope: a host
element's primitive is imported from its home (`import { View } from "nsh"`) and compiled in place, a component is
imported (`import { Button } from "nsh/ui/mobile"`) or local; lowercase tags are errors (`native-pipeline.md`).

## The main thread never waits for JS

Some callbacks are questions the system wants answered on the spot (may this
text change, may the back button pop, may the sheet close, may this gesture begin, may the web view navigate): they are
answered natively from props mounted beforehand, never by asking JS (blocking the main thread drops frames, can deadlock
and reorders fast input). Events tell JS afterwards. Existing answers: input `maxLength` (`shouldChangeCharactersIn`,
`InputFilter`), `submitBehavior` (`textFieldShouldReturn`, `textView:shouldChangeTextInRange:`, Android's `OnEditorActionListener` and input connection, AppKit's `doCommandBySelector:`), `gestureEnabled` / `preventBack` (`gestureRecognizerShouldBegin`,
`navigationBar:shouldPopItem:`, Android `wantsBack()` / `OnBackInvokedCallback` registration), `dismissible`
(`presentationControllerShouldDismiss`, the dialog's back), `pan` / `pinch` / `hitSlop` / `pointerEvents` /
`keyboardShouldPersistTaps` / `panActivation` (recognizers), capabilities (`decidePolicyForNavigationAction`, `shouldOverrideUrlLoading`),
`cancelable`. A new capability with such a callback gets a prop (a menu's items, a rule, a flag); what a prop cannot
express is let through and corrected in the event afterwards (a controlled `value` in `onChangeText`). `BackHandler` is
of that kind: with a listener the press is taken, and JS exits the app when no listener handles it.

One exception, bounded: on macOS a window resize waits up to 16 ms for the engine's layout at the new size, so the
window never grows around stale content (AppKit's zoom animation would otherwise show none of it); an engine that
misses the deadline is not waited for again until it catches up (`docs/dev/macos.md`, Architecture).

## Interaction and the environment

- **Press feedback primitives** (native, no JS round trip): `activeOpacity`; `underlayColor` + `underlayOpacity` (default 0.85)
  on any view — the core shows the underlay as the background (in place of the background color and any gradient,
  which would cover it) and fades the children while the platform reports the view pressed; `active:` classes.
  Background colors that would be hidden by a gradient replace it (`docs/dev/styles.md`, Effects). `KeyboardAvoidingView` (Rust; `behavior`: `padding` → inset padding, `height` → inset margin,
  `position` → an inner view moved up; `keyboardVerticalOffset`).
- **Platform modules**: `Platform` (`OS`: `ios` / `android` / `macos` / `windows` / `linux` / `web`, from the platform's
  `Environment::os` (`nsh_engine::Os`; the default is the build's mobile OS, so `nsh trace` is iOS); `isDesktop`,
  `Version`, `isPad`, `isTV`, `isMacCatalyst`, `select` (the OS, then `mobile` / `desktop`, `native` (not on the web), `default`); `isDesktop`, `desktop` and `desktop={{…}}` follow `Environment::desktop()`: a desktop OS, or the web in the desktop idiom), `PixelRatio`, `AccessibilityInfo` (`isScreenReaderEnabled`,
  `isReduceMotionEnabled`, `addEventListener("screenReaderChanged" | "reduceMotionChanged")`, `announceForAccessibility`,
  `setAccessibilityFocus(ref)`: Rust, the node passed as `{ $node: id }`), reactive `screenReaderEnabled()` / `reduceMotion()`. The environment carries `pixelRatio`,
  `screenReader`, `osVersion` and `idiom` (VoiceOver / TalkBack changes are observed natively).
- **`disabled`** on any element applies `disabled:` classes and removes its press listeners from what is mounted (no press,
  no feedback).
- **`onHoverIn` / `onHoverOut`** (any element; macOS, Linux and GPUI) tell JS, after the fact, that the mouse came over a view
  or left it. They follow the same hover tracking as `hover:` classes: the views along the chain under the mouse
  (innermost first) whose classes read `HOVERED` or that listen to either event are the hovered ones, so ancestors
  hear `onHoverIn` too, as with CSS `:hover` and the DOM's `pointerenter`; a handler alone makes its view take part
  (`input.rs` `hoverable`, the view's mounted event mask), and a view that does not listen gets no event. When the set
  changes, the views left hear `onHoverOut` from the inside out, then the views entered `onHoverIn` from the outside
  in. The mouse leaving the window (while no button is down on macOS) unhovers everything; on macOS and GPUI hover is
  picked again after every batch (on macOS also after every scroll), so a view removed or moved from under a still mouse hears `onHoverOut` and one
  that arrives there `onHoverIn`; on Linux that waits for the next mouse move. Touches never hover. Other platforms do
  not raise them yet (iOS, Android, Windows and the web track `hover:` styles only).
- **`onPanEnd` / `onPanStart`** carry `valueX` / `valueY`: the bound pan values at that moment (a drag may grab a value mid
  animation), for snapping.
- **Imperative calls**: nodes have `focus()`, `blur()` and `scrollTo({ x, y, animated })` (use `ref`); lists also
  `scrollToIndex({ index, viewPosition, viewOffset, animated })`, `scrollToEnd()`, `scrollToOffset()`; `Keyboard.dismiss()`.
  Commands run after the commit's other mount items; list scrolls are resolved by the core with the current item sizes.
- **`measure()`** (on any node's ref) resolves, once the turn's changes are laid out, to `{ x, y, width, height, pageX,
  pageY }` or `null` (not in the tree, never shown). `x` / `y` / `width` / `height` are the frame in the parent, as
  `onLayout` reports it; `pageX` / `pageY` the top left corner in the root view the node is in: the surface, or the modal
  it is shown in (screens fill their stack, so a screen's views are in the surface's coordinates). Those are the
  coordinates of touches' `pageX` / `pageY`, so `e.pageX - m.pageX` is a touch's position in the measured view whatever
  view the finger went down on (`locationX` is relative to that one). The core answers it from its layout
  (`Command::Measure`, `Core::measure`: the frames it mounted, summed up the tree), never by asking the platform: the
  scroll views and lists around subtract their content offsets (as the platforms last reported them, `set_scroll_offset`)
  and apply their zoom scales; transforms, native animations included, are not applied, nor is the mirroring of
  right-to-left horizontal lists (scaleX −1). The answer is a `$return` on the root, like a module call's
  (`nsh-js` `components.rs`). There is no `measureInWindow`: the root view is the window for a full-screen surface, and
  an embedding app knows where it put the surface.
- **Touch** (React Native semantics): `pointerEvents`, `hitSlop`, `activeOpacity`, `delayLongPress`, `android={{ ripple }}`
  (`{ color, borderless, radius, foreground }`, Android only); press-in waits ~130ms inside scroll views; only the innermost
  pressable view responds. Children placed outside their parent (`position: "absolute"`, `overflow` visible: a
  dropdown under its field) can be touched where they are drawn: UIKit and Android only look for a touch's target
  inside a view's bounds, so the core mounts, for each view whose descendants reach outside it, the area they cover
  (`MountItem::Overflow`, in the view's coordinates, from the layout frames, only when it changes; clipping views,
  scroll views, lists and stacks have none). `NshView`'s `hitTest` looks in its subviews when the point is outside its
  bounds (and hit slop) but in that area, and never returns the view itself there; Android's `Box` delivers the touch
  (and the rest of the gesture) to the child whose area holds it, which passes it on to its children only, and the touch
  tracker's hit test (`TouchTracker.deepest`) descends the same way. `pointerEvents` keeps its meaning (`box-only` and
  `none` views are not entered). Views whose children stay inside pay nothing; transforms that move a child out are
  not covered. There is no portal / overlay primitive yet: a popover renders inside its anchor's parent (give the
  anchor a `zIndex` over its later siblings) or at the screen's root. Nested `<Text>` spans with press handlers
  are links: the core lists them in `AttributedText::links` (UTF-16 ranges) and the block's press recognizer routes a press to
  the span under the finger (CoreText `CTLineGetStringIndexForPosition` / `Layout.getOffsetForHorizontal`).
- **Raw touches and the responder system**: `onTouchStart/Move/End/Cancel` (+ `Capture`), `onStartShouldSetResponder`,
  `onMoveShouldSetResponder` (+ `Capture`), `onResponderGrant/Reject/Start/Move/End/Release/Terminate/TerminationRequest`, and
  `PanResponder.create` on top. A node with any of them listens to `onTouch` natively; one tracker per root view (surface,
  modals: `touch.rs`, `TouchTracker.java`) observes without recognizing and reports touches whose target or an ancestor
  listens, with the path of listening nodes (`EventPayload::Touch`, sent to the root; moves are merged per engine turn). The
  negotiation runs as in React Native, natively (`nsh-js` `touch.rs`), asking the app's handlers. A grant returning true
  (PanResponder's default) sends `SetJsResponder`: scroll
  views around stop taking the touches (UIKit: their pan recognizer is disabled; Android: `ScrollBox.jsBlocked`); a native
  scroll view that starts dragging first cancels the JS touches. The `…ShouldSetResponder` handlers also get a
  `gestureState` (`ResponderGestureState`: `dx`, `dy`, `vx`, `vy`, `moveX`, `moveY`, `x0`, `y0`, `numberActiveTouches`,
  since the first finger went down; `nsh_core::ui::PanGesture`, the same the core gives Rust claims). Rust components'
  nodes on the path hear the same event first and negotiate their own responder in the core (`claim_touches`,
  `docs/dev/components.md`); the core sends both sides' blocks.
- **Pans that coexist with scroll views** (`panActivation={{ activeOffsetX, activeOffsetY, failOffsetX, failOffsetY }}`,
  react-native-gesture-handler's offsets; Rust: `TouchClaim::on_pan(PanActivation)`): a JS decision to claim a touch
  comes a turn late, when the scroll view may have taken it, and claiming on start blocks the scroll view from the
  first pixel. So the platform decides, on the main thread, from the mounted offsets (`ViewProps::pan_activation`,
  `PanActivation::decide`: a fail offset passed first fails, an active offset passed activates; no active offsets: 10
  points in any direction). iOS: a `PanGate` recognizer on the view (`touch.rs`); the pans of the scroll views around
  it must wait for it to fail (`shouldBeRequiredToFailByGestureRecognizer:`), so a vertical drag scrolls once it passes
  `failOffsetY` and a horizontal one never scrolls. Android: the root's `TouchTracker` decides on each move before the
  views below see it and, on activation, blocks the `ScrollBox`es around (`gateBlocked`) and disallows their
  interception; a scroll view that intercepts first (past its touch slop) wins as before. Web: `nsh.js` defers its
  drag scrolling while the gate is undecided. The activation travels with the touches (`TouchEvent::activated`; the
  iOS gate hands it to the root's tracker, which reports it with these touches or the next), and the gated view
  becomes the responder (its termination request is honored as for any claim). Its own `…ShouldSetResponder`
  handlers (and a Rust element's other claims) are never asked: the view is a responder exactly when its pan is
  active, so `PanResponder`'s `panHandlers` work unchanged next to the prop.
- **Accessibility**: `accessible`, `accessibilityLabel` / `Hint` / `Role` (`role`) / `State` / `Value` / `Actions` +
  `onAccessibilityAction`, `accessibilityElementsHidden`, `importantForAccessibility`, `accessibilityLiveRegion`, `testID`
  (Android: resource id). Views without a label read their texts. The props go as one `MountItem::ViewProps`, except
  a label changing alone on mounted props (one that follows app state: a count, a price), which goes as
  `MountItem::AccessibilityLabel`.
- **Frames**: `requestAnimationFrame` / `cancelAnimationFrame` follow the display (CADisplayLink / Choreographer).
- **Back button** (`nsh/android`): `BackHandler.addEventListener("hardwareBackPress", ...)`, after modals and stacks; the app exits
  when nothing handles it. Rust components listen with `cx.on_back_press(..)`, before JS's listeners; the root wants
  the button while either side listens (`docs/dev/components.md`).
- **Keyboard shortcuts** (`nsh/keymap`): the root's `Prop::Keymap` mounts the surface's whole table
  (`MountItem::Keymap`); the platform matches a key against it on the spot, consumes it and reports the root's
  `KeyCommand` (the binding's id), so the main thread never waits for JS (`docs/dev/js-modules.md`, the macOS and
  Linux pages' "Keyboard"). Rust views' bindings (`on_key`) join JS's in that one table, and the core runs
  their handlers (`docs/dev/components.md`).
- **System UI**: `StatusBar` (component and static functions; the last mounted wins, prop by prop, over what the
  functions set: the core layers them, `docs/dev/components.md`), `Alert.alert` and `ActionSheet.show` (promises of
  the index chosen, -1 when dismissed; a list dialog on Android), `Toast` (`nsh/android`). Answers come back as the
  root's `Dialog` event, which the core resolves. Embedding apps forward their controller's status bar to `nsh_status_bar_style()` / `_hidden()` / `_animation()`.
- **Right to left**: `I18nManager` (`isRTL`, reactive; `allowRTL`, `forceRTL`, applied at once) sets the root's direction;
  `direction` (`inherit` / `ltr` / `rtl`) overrides it per view. The core resolves each node's direction (inherited, also for
  subtrees built detached), sets taffy's `direction` (rows run from the right) and, right to left, swaps the left and right
  properties (margins, paddings, insets, borders, corners — React Native's default `doLeftAndRightSwapInRTL`) and places the
  start / end ones (`marginStart`, `start`, `borderTopEndRadius`, ...). A direction change swaps the current taffy style in place
  (an involution), so authored styles need not be kept. Text: `auto` alignment is the right side right to left, left / right
  swap, `writingDirection` sets the base direction (else the content's). Native widgets mirror (`MountItem::LayoutDirection`:
  `semanticContentAttribute`, `View.setLayoutDirection`; Android apps need `android:supportsRtl`). Horizontal scroll views
  are laid out left to right and their children mirrored in the content by the core (a right-to-left row would overflow to
  negative offsets), starting at the right end and keeping their distance from it; horizontal lists are flipped natively
  (`ScrollConfig::mirrored`: the scroll view and each cell get scaleX −1, composed with their transforms), so the list's
  offset logic stays left to right.
- **Environment** (reactive accessors; `nsh/device`, and `nsh/ui/mobile` for the mobile ones): `windowSize()`, `safeArea()`, `keyboard()` (`{ height, duration }`: how
  far the keyboard reaches into the surface, the bottom safe area under it included, on every platform),
  `colorScheme()`, `fontScale()`. `KeyboardAvoidingView` pads its bottom by `keyboard().height`, animated with the keyboard,
  for content outside scroll views (message bars). Inputs inside a scroll view are kept clear of the keyboard natively (bottom inset + scroll to the
  focused input), with no JS involved.
- **Controlled inputs**: each native edit carries an edit count; a `value` from JS is applied only if it is based on the latest
  edit, so a slow JS round trip can never overwrite newer typing (React Native's eventCount).

Text, styles, scrolling and lists, animation, canvases and images have their own pages.
