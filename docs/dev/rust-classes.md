# Classes and functions from Rust (design proposal)

Status: proposed; the decisions below are taken.

An app's Rust crate (`nsh.json` `"rust"`) exports components and modules (`#[module]`: functions). This proposes
classes: a Rust type that TypeScript uses as a class, with a constructor, methods, getters and setters, statics and
instances that keep their state in Rust; synchronous functions on the same footing; and how NSH's own standard
library shares the machinery.

```rust
/// A tally kept in Rust.
#[class]
pub struct Tally {
    count: i64,
    log: Vec<String>,
}

#[class]
impl Tally {
    /// Starts at `start` (0 when left out).
    pub fn new(start: Option<i64>) -> Self {
        Tally { count: start.unwrap_or(0), log: Vec::new() }
    }

    /// Adds `by`; the new count.
    pub fn add(&mut self, by: i64) -> i64 {
        self.count += by;
        self.log.push(format!("+{by}"));
        self.count
    }

    /// The count.
    #[getter]
    pub fn count(&self) -> i64 {
        self.count
    }

    #[setter]
    pub fn set_count(&mut self, count: i64) {
        self.count = count;
    }

    /// A tally from `"12"`; throws on anything else.
    pub fn parse(text: String) -> Result<Tally, String> {
        Ok(Tally::new(Some(text.trim().parse().map_err(|_| format!("not a number: {text}"))?)))
    }

    /// Saves the log; resolves when it is written.
    pub async fn save(this: Instance<Tally>, path: String) -> Result<(), String> {
        let lines = this.with(|t| t.log.join("\n"));
        blocking(move || std::fs::write(path, lines)).await.map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
}

#[native_module("app/native")]
fn native(registry: &mut Registry) {
    registry.class::<Tally>();
}
```

```ts
// .nsh/types/app/native.d.ts, generated
/** A tally kept in Rust. */
export declare class Tally {
  /** Starts at `start` (0 when left out). */
  constructor(start?: number);
  /** Adds `by`; the new count. */
  add(by: number): number;
  /** The count. */
  get count(): number;
  set count(count: number);
  /** A tally from `"12"`; throws on anything else. */
  static parse(text: string): Tally;
  /** Saves the log; resolves when it is written. */
  save(path: string): Promise<void>;
  /** Frees the Rust value now (else when the object is collected); the object throws after. */
  dispose(): void;
  [Symbol.dispose](): void;
}
```

```tsx
import { Tally } from "app/native";
const tally = new Tally(3);
tally.add(2);            // 5, synchronously
tally.count = 10;
await tally.save("/tmp/log");
```

## Why synchronous

Module functions are ops: JS queues them and the core runs them after the turn, so they always return a promise.
That is fine for "do something" (show an alert) but not for something that looks like a class: a getter cannot be a
promise, and `tally.add(2) + 1` should be a number. JS and Rust components already run on one thread (the engine's),
so a class's methods can run directly inside the JS call, as the web classes nsh-js implements in Rust (`Blob`,
`Headers`, `URL`) do.

## The model

- **Instances live in Rust, owned by their JS object.** A JS object of the class holds the Rust value as its opaque
  (`Rc<RefCell<T>>`, the web classes' slot, `nsh-js` `web/mod.rs`); the object being collected drops it.
  `dispose()` (and `[Symbol.dispose]`, for `using`) drops it now (a value holding a file or a large buffer); any use
  after throws. `T: 'static`, not
  `Send`: it never leaves the engine thread.
- **Synchronous members run in the JS call.** `&self` / `&mut self` methods, getters and setters, statics and the
  constructor run inside the JS turn, outside the core: they compute and keep state, and cannot use the UI's free
  functions (signals, `dialog`, `theme()`): the reactive graph is not entered there, and calling one panics with a
  message saying so (as on any thread without it). A method borrows its instance for its call; a re-entrant call
  (a method reaching the same instance again) throws instead of aliasing, as the web classes do.
- **`async` members are core tasks.** An `async fn` method gets `this: Instance<Self>` (a handle: `with` /
  `with_mut` borrow the value between awaits) and returns a promise; it runs as a task of the core, where the free
  functions work (`dialog`, `timer`, `blocking`, `background`, the clipboard), as an `async` module function does.
  The call is an op like a module function's (`Command::Native { name: "Tally#save", args: { c, this, a } }`); the
  instance reaches the core through a table both sides share (`Instances`, made by the engine and handed to
  `nsh-js` and the core), keyed by an id the JS object carries. The task keeps the instance alive until it ends.
- **Errors.** `Result<T, E: Display>` throws (`Error` with the message) or rejects; a wrong argument throws a
  `TypeError` naming the parameter; a panic in Rust is caught and thrown as an `Error` (the engine stays up).

## Functions

Rust exports functions today through `#[module]` (an object, `Alert.alert(..)`, or `#[module(flat)]`, top-level
functions): every call is an op ordered with the UI's, so every result is a promise. The class's split applies to
functions too, decided by the signature:

- **`fn` without a context:** synchronous, called directly in the JS call like a class's method (`parse(text)`,
  `checksum(bytes)`): TS gets the value, not a promise.
- **`fn` taking `cx: &mut ModuleCx`:** as now: an op, run in order with what JS set before it (a dialog, the status
  bar, focus), a promise when it returns a value.
- **`async fn`:** as now: a task of the core, a promise.

A plain `fn` that returns a value (today a promise in TS) becomes synchronous: no runtime module has one (theirs
take `ModuleCx` or are `async`), and an app crate's are rebuilt with the app, whose types change with them.

## The standard library

What TSX imports from NSH is made two ways today:

- **Runtime modules in Rust** (`crates/nsh-core/src/runtime/`: `nsh/accessibility`, `nsh/android`,
  `nsh/clipboard`, `nsh/desktop`, `nsh/image`'s functions) already use `#[module]` and `Registry`, the app crates'
  mechanism; they take the synchronous functions and classes as they come.
- **Hand-written in `nsh-js`:** the reactive graph and the renderer (`nsh`), `nsh/animated`'s `NativeValue` and
  `NativeNode`, `nsh/canvas`'s `Path2D`, `CanvasGradient` and `CanvasRenderingContext2D`, the keymap, lists, and the
  web globals (`URL`, `Headers`, `Blob`, `fetch`, streams, events). Their classes already sit on one piece of
  machinery: a QuickJS class whose opaque is the Rust value in a `RefCell` (`nsh-js` `web/mod.rs`).

So the classes of this proposal are built on that same machinery: one implementation of "a JS class backed by a
Rust value", with `#[class]` its declarative front. Which standard classes move to `#[class]` depends on what they
need beyond the first version:

| Class | Needs | Moves |
|---|---|---|
| `Path2D`, `CanvasGradient` | plain values, called often | when calls convert QuickJS values directly (below) |
| `CanvasRenderingContext2D` | 17 500 calls in ~1 ms (`docs/dev/performance.md`) | stays recorded natively: a class front adds nothing |
| `NativeValue`, `NativeNode` | values the renderer reads from style props, ops ordered with the UI | stays: it is part of the renderer |
| `URL`, `TextEncoder` | WebIDL coercion, `new.target`, iterators | after subclassing and iterators |
| `Headers`, `Blob`, streams, `EventTarget` | callbacks, events, promises interleaved with JS | after callbacks and events |
| `nsh` itself (signals, `View`, `mount`) | the reactive graph and the JSX compiler's calls | stays: it is the runtime |

For that, two choices of the first version change:

- **Values convert directly**, QuickJS value to Rust type and back, not through `JsonValue`: a synchronous call is
  what makes these classes worth using, and a JSON round trip per call would cost more than the call (the canvas'
  recording is a native list for that reason). `JsValue` gains the direct conversions; modules' ops keep JSON,
  since they cross to the core as data.
- **The machinery is shared from the start**: `#[class]` instances are the web classes' slots, so `instanceof`,
  re-entrancy, finalization and (later) `new.target` behave the same for NSH's classes and an app's.

## What crosses

- **Arguments and results** are the module functions' types (`JsValue`: numbers, `bool`, strings, `Option`, `Vec`,
  `JsonValue`, `#[derive(JsObject)]` structs, `#[derive(PropEnum)]` enums), so classes and modules share one set of
  conversions and TypeScript types; a trailing `Option` parameter is optional.
- **Instances of classes**, by reference: a parameter `&Other` / `&mut Other` borrows another instance for the call
  (TS: `other: Other`), `Instance<Other>` keeps a handle (to store one instance in another), and a result `Other`
  or `Instance<Other>` is a new JS object (`Self` from `new` and statics). An instance of another app's or set's
  class is refused with a `TypeError`.
- **JS functions** (callbacks), below.
- **Not in the first version:** events emitted by an instance (`EventTarget`), iterators, and subclassing in TS (the
  web classes support `new.target`; app classes can follow once something needs it).

## Callbacks

A parameter `Callback<A, R = ()>` takes a JS function: `A` is its argument (a `JsValue` type, or a tuple for
several), `R` what it returns. TS: `Callback<(String, f64), bool>` is `(a: string, b: number) => boolean`.

```rust
#[class]
impl Downloader {
    /// Calls `progress` with each fraction done; resolves with the bytes' count.
    pub async fn fetch(this: Instance<Downloader>, url: String, progress: Callback<f64>) -> Result<u64, String> {
        // ...
        progress.post(0.5);                       // runs in JS after this step, in order
        progress.call_async(1.0).await?;          // runs in JS, its result back here
        Ok(bytes)
    }

    /// Sorts with the app's comparison, at once.
    pub fn sort_by(&mut self, compare: Callback<(String, String), f64>) -> Result<(), String> {
        self.items.sort_by(|a, b| {
            let order = compare.call((a.clone(), b.clone())).unwrap_or(0.0);
            order.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(())
    }
}
```

- **Calling it now** (`call(args) -> Result<R, String>`): inside a synchronous call from JS (a method, a function,
  the constructor), the JS function runs on the spot and its result comes back, as a JS callback passed to
  `Array.prototype.sort` does. Its exception is the `Err` (its message). The instance stays borrowed meanwhile:
  the callback reaching the same instance again throws, as any re-entrant call does. `call` outside a call from
  JS (in a task) panics with a message saying to use `post` or `call_async` there.
- **Calling it later** (`post(args)`, `call_async(args).await -> Result<R, String>`): from anywhere on the engine
  thread (an async member's task, a timer, a stored callback), the call is queued for JS and runs in the engine's
  next JS turn, in order with the other posts: `post` forgets the result, `call_async` waits for it. The core hands
  the calls to the script as it hands it events (`Action::CallJs`), so they cost one turn, not a thread hop.
- **Keeping it:** a `Callback` can be stored (in an instance's field, a `Vec`, a global) and called any number of
  times; dropping the last clone releases the JS function. A callback an instance keeps is reported to QuickJS's
  collector through the class's mark hook, as the web classes report the JS values they hold. `#[class]` finds
  the fields of type `Callback` (also in `Option`, `Vec` and `HashMap` values), so a callback that captures the
  object owning the instance does not keep both alive forever. Callbacks held elsewhere are roots until dropped.
- **Functions take them too:** `fn watch(path: String, on_change: Callback<String>)` (`on_change` kept and posted
  later), and `async` functions and members can `call_async`.


## The pieces

- **`nsh-macros`:** `#[class]` on the struct (a marker; the type's name is the JS name) and on its `impl`: `new`
  is the constructor (a class without one cannot be made from JS, only returned); `&self` / `&mut self` methods,
  `#[getter]` / `#[setter]` (one name: `count` and `set_count` are the property `count`), functions without `self`
  are statics, `async fn` with `this: Instance<Self>` are async methods. Doc comments become the declarations'.
  It implements `ui::Class` (name, doc, members, each member a decoder of its arguments and a call), as `#[module]`
  implements `Module`.
- **`nsh-core`:** `ui::Class`, `ClassSpec` (for the JS face and the declarations), `Instance<T>`, the `Instances`
  table, `Registry::class::<C>()`; the core runs async members like async module functions (`call_function`).
- **`nsh-js`:** makes each registered class a QuickJS class when a bundle imports the set (as it makes modules'
  objects in `components.rs`): its constructor, prototype methods, accessors and statics call the spec's members
  with the converted arguments; async members push the op and return the promise; the finalizer drops the slot.
- **Declarations:** `nsh_core::ui::declarations` writes `export declare class` for each, with the types the members
  use, next to the set's components and modules; the declarations test checks that what exists is declared.
- **The CLI** needs no change: classes are part of the app's set like its modules.

## Costs

A synchronous call converts its arguments from QuickJS values to the Rust types directly (no `JsonValue`, no op, no
commit, no promise), as the web classes' methods do. The benchmark comes with the implementation
(`docs/dev/performance.md`): a method call against a module call, and `Path2D` built through `#[class]` against the
hand-written one.

## Decisions

1. **Synchronous or a promise is the Rust signature's `async`**: a member or function that is not `async` runs in
   the JS call and returns its value (it computes and keeps state, without the UI's free functions); an `async`
   one returns a promise and runs as a core task, with them. The one exception, for functions only: one taking
   `cx: &mut ModuleCx` stays an op ordered with the UI (a toast, the status bar, focus), as now.
2. **`dispose()` and `[Symbol.dispose]`** besides collection.
3. **Callbacks in the first version** (above); events and iterators later.
4. **Functions follow the same rule**, so a plain `fn` returning a value becomes synchronous in TS.
5. **The standard library's classes move to `#[class]`** as the mechanism gains what each needs (the table above);
   the renderer's own classes stay hand-written. Their TS interfaces stay as they are: names, members and
   signatures are generated alike; arguments are checked by type rather than coerced (`moveTo("10", 0)` throws
   instead of reading 10), and errors read as the generated ones do.
