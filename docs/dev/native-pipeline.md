# How apps are compiled and run: NSH's JSX compiler and native modules

An app is TSX importing NSH's modules. The bundler compiles each module with NSH's JSX compiler and rolldown bundles
them into one ES module holding only the app's code: imports of NSH's modules stay imports, and `nsh-js` serves them
natively. The reactive graph, the renderer and every API apps import are Rust; nothing of NSH is JS. Apps cannot
import `solid-js` (the bundler refuses it, pointing to `nsh`'s `signal` / `computed` / `effect` / ...).

```tsx
import { computed, mount, signal, Text, View } from "nsh";

function Counter(props: { label: string }) {
  const count = signal(0);
  return (
    <View onPress={() => count.update((c) => c + 1)}>
      <Text>{props.label}: {count()}</Text>
      <If when={count() > 2} else={<Text>keep going</Text>}>
        <Text>done</Text>
      </If>
    </View>
  );
}

mount(() => <Counter label="Taps" />);
```

## Modules

`nsh_core::js::NATIVE_MODULES` names NSH's own, the registries name the rest; the bundler leaves imports of them in the
bundle (`plugin.rs`, an unknown `nsh/…` is an error), `nsh-js` serves them (`modules.rs`, the module table and the
loader). Where each one is implemented and declared: `js-modules.md`.

- `nsh`: `signal` (`count()`, `count.set(v)`, `count.update(f)`), `computed`, `effect`, `batch`, `untracked`, `cleanup`,
  `context` (`Theme()`, `<Theme.Provider value>`), `split(props, keys)` (`[picked, rest]`, keeping getters), `mount`;
  the primitives `View`, `Text`, `Scroll`, `Input`.
- `nsh:rt` (only compiled code imports it): `element`, `text`, `append`, `insert`, `prop`, `bindProp`, `spread`,
  `component`, `cond`, `each`, `merge`, `list`, `setup`, `font`.
- `nsh/device` (`windowSize`, `colorScheme`, `fontScale`, `Platform`, `PixelRatio`, `I18nManager`) and
  `nsh/accessibility` (`reduceMotion`, `screenReaderEnabled`, `AccessibilityInfo`): the environment (`env.rs`), one
  signal per part written when the platform reports a change, read through read-only accessors.
- `nsh/list`, `nsh/gesture`, `nsh/animated`, `nsh/canvas`, `nsh/webview`, `nsh/video`, `nsh/native`, `nsh/ios`,
  `nsh/image` (`Image`: the image element's component with its registry's functions on it) and the hand-written part
  of `nsh/android` (below).
- The modules of registries (`Registry::specifier`): the UI sets a host serves (`HostOptions::ui_sets`,
  `nsh/ui/mobile`) and the runtime's (the registry parts of `nsh/android`, `nsh/accessibility` and `nsh/image`). Their
  components, modules and re-exports become exports (`components.rs`): a component is a host view naming it, with its
  props sent as `$name` by one render effect, events delivered to `on<Event>` handlers, children and slot holders; a
  module function sends a call to the core and, when it has a result, returns a promise settled by the core's
  `$return` event. Values a registry names natively (`Registry::native_values`: the mobile UI's navigation) and
  re-exports (`safeArea`, `useSafeAreaInsets`) come from the runtime. NSH's own export wins over a registry's of the
  same name.

QuickJS-NG resolves imports when it compiles a module, so the bytecode compiler installs the same loader
(`compile_bytecode_with` with the UI sets); names are checked when the module links (evaluation). The exports are made
on first import.

## The compiler (`crates/nsh-bundler/src/jsx.rs`)

One Oxc pass removes TypeScript and lowers JSX (`plugin.rs` `compile_native`):

- **Tags** are PascalCase and name bindings by normal scoping: every tag is imported or local, except `<If when else>`
  and `<Each of key>`, globals (`dom.rs` `install_globals`) a binding of the same name shadows. A tag naming one of
  `jsx::PRIMITIVES` imported from its module (alias-aware: `import { View as V } from "nsh"`) compiles to what it is,
  with no component call: `View`, `Text`, `Scroll`, `Input` (`nsh`), `Image` (`nsh/image`), `WebView`, `Video` to
  their host elements made in place, `List` to `nsh:rt`'s `list`, `If` / `Each` to `cond` / `each`; `Canvas`,
  `Modal` and `InputAccessoryView` are native components that do more than make their element (the drawing,
  `visible` and the insets, iOS only) and are called. The exports are callable components too (`dom.rs`
  `host_component`; `If` and `Each` are real globals), so `const C = cond ? View : Text; <C />` works through the
  component path. Lowercase tags are errors with the import to write (`<view>`: write `<View>` and `import { View }
  from "nsh"`; for another, the closest binding, primitive or UI set component). Why: TypeScript checks a lowercase
  tag against one global type per name, so generic components (`List`, `Each`) and local ones would lose their types.
  Props keep their camelCase names.
- **Host elements** become one arrow function making the element and its host children: static props and children
  are set once (`prop`, `append`), changing ones are functions the renderer follows natively (`bindProp`, `insert`).
  An expression changes when it calls something or reads a member outside nested functions (Solid's rule). Event
  handlers are set once when they are a function or a name, followed otherwise. `ref={el}` assigns the variable,
  `ref={fn}` calls it. Consecutive dynamic children share one `insert`, before the next static child.
- **Components** are called untracked with a props object: static props as values, the others as getters, children as
  the `children` getter; spreads go through `merge`, which keeps getters.
- **Text** follows Babel's algorithm (what React renders): lines are trimmed except at the text's ends, whitespace-only
  lines dropped, the rest joined with a space; whitespace on one line is kept (`{a} {b}` keeps its space).
- Code is generated as JS text with holes, parsed, and the app's expressions are moved into the holes: they keep their
  spans (the source map points at the app), generated nodes take their JSX element's span.
- The entry's prologue gives the renderer the app's themes (`setup`: the Tailwind theme and the components' `uiTheme`,
  before anything renders). A font import (`import Inter from "./Inter.ttf"`) compiles to `import { font } from
  "nsh:rt"; export default font(uri, family)`: its family, registered by the host before the first frame.

## The renderer (`crates/nsh-js/src/dom.rs`)

- A node is an object of a native class holding its id, its children's objects and its event handlers, reported by its
  GC mark: the tree and what handlers capture are one heap QuickJS collects. The root view is held by the renderer;
  a node nothing refers to is collected and its view released (its finalizer queues the release separately, as the op
  queue may be borrowed when a finalizer runs).
- Every change is an `Op` pushed straight into the host's queue (`Shared.ops`, at most `Limits::max_op_values` at
  once); the engine takes them after the JS turn for one `Core::commit`. Props are decoded from the JS value by
  `protocol::decode_prop`. Events go from `Host::dispatch_event` to the handler the node holds (`dom::handler`);
  touches to the responder system (`touch.rs`).
- Dynamic children are a render effect whose value is the array of nodes it inserted, reconciled with Solid's
  `reconcileArrays` against the renderer's mirror of the tree; a node that comes again later is moved, never removed
  and inserted (the core would remake its view), and an insert that would not move a node is skipped.
  A run of dynamic children (one `insert` of an array, `{spans()}{tail()}`) gives each function in it a memo of its
  own (`memoize_run`): a change in one re-runs only that one, and the others' nodes stay where they are.
- `each` keys rows (`key(item, index)`, the index for `key="index"`, else the item; objects by identity, primitives by
  value) and updates the item and index signals of kept rows (rows get accessors: `item()`, `index()`); each row has
  its own owner. `cond` renders a
  branch per change of truthiness.
- Platform-only props (`ios={{…}}`, `android={{…}}`, `desktop={{…}}`): only the current platform's object is sent, each
  key as a prop of its own (`platform_props`).
- The reactive graph and the renderer share one per-context state (`state.rs`); natives reach each part by shared or
  short exclusive references, never across a call into JS or a free of a JS value.

## Lists and node methods

- `List` (`nsh/list`) is the renderer's (`list.rs`; its tags compile to a component call of `nsh:rt`'s `list`, the
  same function). The core asks for rows (`Host::bind_list`), bound natively (`list::bind`); a recycled cell gets a
  new index and its item accessor follows; header, footer, empty and separator cells, viewability tokens (with
  `minimumViewTime`), `initialScrollIndex`. Rust components with rows (`ListView`) get a list of the app's data as
  their items slot.
- Nodes have the methods of `YNode` (`_core.d.ts`: `focus`, `blur`, `setSelection`, `command`, `scrollTo`, `zoomTo`,
  `scrollToOffset`, `scrollToIndex`, `scrollToEnd`, the web view's and video's): each sends a command
  (`protocol::decode_command`); `measure` returns a promise the core settles after the turn's layout
  (`Command::Measure`, `elements.md`).

## Navigation

`createNavigator`, `Stack`, `Screen` and `Modal` are natives (`navigation.rs`), which the mobile UI exports
(`Registry::native_values`). A stack renders each route (by identity) with the app's function into a screen, the app's
`<Screen>` or one around what it rendered (also when a hot component returns an accessor); a screen the user pops
natively removes its route; screens and modals provide the insets they report to what they render
(`useSafeAreaInsets`).

## Runtime modules

- `nsh/gesture`: the responder system and `PanResponder` (`touch.rs`), in Rust as it runs on every touch move.
- `nsh/animated` (`animated.rs`): operations are encoded as a few values (`ANIMATION_*`) and decoded into `AnimationOp`s
  (`protocol::decode_animation_op`); nodes and composites are released by their finalizers; a started animation's promise
  settles when the core reports its end (`Host::animations_finished`).
- `nsh/webview`, `nsh/video`, `nsh/native`, `nsh/ios` and the hand-written part of `nsh/android` (`isAvailable`,
  `BackHandler`'s listeners): `platform_modules.rs`.
- `nsh/canvas` (`canvas.rs`): a drawing makes thousands of calls, so the context records into a Rust list instead of a
  JS array, and the core's decoder (`nsh_core::js::canvas::decode`) reads it into one `Op::Draw`. The recording methods
  and properties are made from the core's specs (`CanvasOp::methods` / `properties`), so a new operation needs no
  change here. Paths and gradients are taken as they are when the drawing uses them.

## Types

`.nsh/types` holds the declarations of everything NSH serves, made from its Rust sources (`nsh-js`
`declarations.rs`, `tooling.md`), the same for every app: the JSX namespace (`nsh/jsx-runtime.d.ts`, `JSX` of
`nsh/types.d.ts`) has no intrinsic elements, so TypeScript checks every tag as the value it names (a lowercase tag is
an error, as in the compiler) and infers generic components (`<List each={rows}>{(row) => …}</List>`, `If` and `Each`
declared as global functions in `nsh/globals.d.ts`). The bundler's part is only the asset imports
(`nsh/assets.d.ts`, `crates/nsh-bundler/src/types.rs`).

TypeScript's import elision runs after the JSX is compiled (`plugin.rs` `elide_unused_imports`): an import only
primitives' tags use (`View` for `<View>`, made in place) goes, as does one only types use (even without `type`).

## Hot module replacement

Development bundles are compiled by the same compiler, after the bundler's hot transform (`crates/nsh-bundler/src/hot.rs`)
has wrapped the components, the functions passed to top-level `mount()` and the top-level `context()`, `signal()` and
`createNavigator()` calls, one CommonJS chunk per module, and run by the native hot
runtime (`crates/nsh-js/src/hot.rs`, see `tooling.md`). The script is an ES module: it imports `nsh:hot` (which makes the
`__nsh_hot` global; only development bundles import it) and the namespaces of the native modules the app imports, which
factories `require` by specifier; an edit that imports a native module the running script did not makes the host
reload.
