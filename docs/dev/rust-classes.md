# Classes and functions from Rust

Status: implemented (`nsh_core::ui` `class.rs`, `callback.rs`, `js.rs`; `nsh-macros` `class.rs`; `nsh-js`
`classes.rs`); "Implementation notes" at the end says where it differs from the proposal.

An app's Rust crate (`nsh.json` `"rust"`) exports components, modules (`#[module]`: functions) and classes. This
describes classes: a Rust type that TypeScript uses as a class, with a constructor, methods, getters and setters, statics and
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
  The call is an op like a module function's (`Command::Native { name: "app/native:Tally#save", args: { c, this,
  a, held } }`, named by the class's set, so two sets may have classes of one name); the instance reaches the core
  through a table both sides share (`Instances`, made by the engine and handed to `nsh-js` and the core:
  `HostOptions::class_instances`, `Core::set_class_instances`), held under an id the op carries beside the
  arguments (`this`, `held: [[index, id]]`, never inside the app's data) until the core takes it. The task keeps
  the instance alive until it ends.
- **One object per instance.** While both live, an instance has one JS object (`nsh-js` remembers it by the `Rc`'s
  address): a member returning an instance Rust keeps gives the object it has, so `a.badge === a.badge`.
- **Errors.** `Result<T, E: Display>` throws (`Error` with the message) or rejects; a wrong argument throws a
  `TypeError` naming the member and the parameter (``Tally.add: `by`: expected a number, got "x"``); a panic in
  Rust is caught and thrown as an `Error`, or rejects (the engine stays up) where Rust unwinds: the web runtime is
  built with `panic = "abort"` (`[profile.web]`), so there a panic ends the page's engine. Integer parameters take
  whole numbers in their range (a fraction, `NaN` or a value out of range is a `TypeError`, not rounded or
  clamped), and one call reads at most 2^20 array items and fields, as a JSON conversion does.

## Functions

Rust exports functions through `#[module]` (an object, `Alert.alert(..)`, or `#[module(flat)]`, top-level
functions). Every call used to be an op ordered with the UI's, so every result was a promise. The class's split
applies to functions too, decided by the signature (`FunctionCall::Sync` / `Op` / `Async`):

- **`fn` without a context:** synchronous, called directly in the JS call like a class's method (`parse(text)`,
  `checksum(bytes)`): TS gets the value, not a promise.
- **`fn` taking `cx: &mut ModuleCx`:** an op, run in order with what JS set before it (a dialog, the status bar,
  focus), a promise when it returns a value.
- **`async fn`:** a task of the core, a promise.

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
  conversions and TypeScript types (`Arg` and `Ret` over them); a trailing `Option` parameter is optional.
- **Instances of classes**, by reference: a parameter `&Other` / `&mut Other` borrows another instance for the call
  (TS: `other: Other`), `Instance<Other>` keeps a handle (to store one instance in another), and a result `Other`
  or `Instance<Other>` is a JS object (`Self` from `new` and statics; `Option<Instance<Other>>` for none). An
  instance of another app's or set's class is refused with a `TypeError`. `async` members and module functions keep
  what they take: `Instance<Other>`, not `&Other`.
- **JS functions** (callbacks), below.
- **Not in the first version:** events emitted by an instance (`EventTarget`) and iterators. Constructors honor
  `new.target` as the web classes' do, so a TS class may extend a Rust one.

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
  next JS turn, in order with the other posts: `post` forgets the result, `call_async` waits for it (and settles a
  promise the function returns). The calls wait in the shared table (`Instances`), which the engine hands to the
  script at each commit round before the core's events (`Host::run_posted`), so a task's posts reach JS before its
  result and cost one turn, not a thread hop. A post made inside a synchronous call from JS is parked until a
  `$post` op JS sends after it is applied, so posts keep program order with the ops before them (an op function
  posting, then a synchronous one). Once the VM stops, queued calls and those posted later are dropped and their
  replies say so. `call` rethrows an uncatchable exception (the CPU budget's interrupt, running out of memory)
  when the synchronous call around it returns, whatever Rust made of its `Err`.
- **Keeping it:** a `Callback` can be stored (in an instance's field, a `Vec`, a global) and called any number of
  times; dropping the last clone releases the JS function. A callback an instance keeps is reported to QuickJS's
  collector through the class's mark hook, as the web classes report the JS values they hold. `#[class]` traces
  every field whose type implements the `unsafe trait Trace` (`Callback`, `Instance`, and `Option`, `Box`,
  `RefCell`, `Vec`, `VecDeque`, map values and pairs of them; chosen by type, so aliases count and other types are
  skipped) and reports them while the object is the instance's only owner and the callback has no other clone,
  so a callback that captures the object owning the instance does not keep both alive forever. Callbacks held
  elsewhere (an `Rc` shared by two instances included) are roots until dropped. The functions belong to the VM:
  freed with it, a callback Rust keeps longer does nothing. When the collector frees an object that owns its
  instance alone, the instance's callbacks are released before its value drops, so a `Drop` calling or posting
  one finds it gone instead of a function the same collection frees.
- **Functions take them too:** `fn watch(path: String, on_change: Callback<String>)` (`on_change` kept and posted
  later), and `async` functions and members can `call_async`.


## The pieces

- **`nsh-macros`:** `#[class]` on the struct (a marker; the type's name is the JS name) and on its `impl`: `new`
  is the constructor (a class without one cannot be made from JS, only returned); `&self` / `&mut self` methods,
  `#[getter]` / `#[setter]` (one name: `count` and `set_count` are the property `count`), functions without `self`
  are statics, `async fn` with `this: Instance<Self>` are async methods. Doc comments become the declarations'.
  It implements `ui::Class` (`Class::DEF`, a `ClassDef`: name, doc, members, each a `Member` with its
  `MemberCall`), as `#[module]` implements `Module`; on the struct it implements `Trace` (the callbacks the fields
  keep) and `Ret` (a value of the struct is a new instance).
- **`nsh-core`:** `ui::Class`, `ClassSpec` (for the declarations), `Instance<T>`, `Callback<A, R>`, the
  `Instances` table, `Registry::class::<C>()`; the direct conversions (`JsHost`, `JsIn`, `JsOwned`; `JsValue`'s
  `from_js` / `to_js`); the core runs async members like async module functions (`call_member`, `run_call`).
- **`nsh-js`:** `classes.rs` makes each class's constructor and prototype from its `ClassDef` when a bundle imports
  the set or a member first returns one of its values: C closures calling the members through `CallHost` (its
  `JsHost`); async members push the op and return the promise; the object is the web classes' (`web/mod.rs`), its
  slot holding the instance, and its mark hook reports the instance's callbacks.
- **Declarations:** `nsh_core::ui::declarations` writes `export declare class` for each, with the types the members
  use, next to the set's components and modules; the declarations test checks that what exists is declared.
- **The CLI** needs no change: classes are part of the app's set like its modules.

## Costs

A synchronous call converts its arguments from QuickJS values to the Rust types directly (no `JsonValue`, no op, no
commit, no promise), as the web classes' methods do. `bench_calls` (`docs/dev/performance.md`): a class method with
a number costs what a JS method call does (~34ns), with a `{ x, y }` object ~137ns, a module function without a
context ~27ns; an awaited op, async function or async member ~1.6–2.1µs.

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

## Implementation notes

Where the implementation differs from the proposal above, and why:

- **Posted calls go through the shared table, not a core action.** `Callback::post` / `call_async` queue a `JsCall`
  in `Instances` (it wakes the engine), and the engine runs the queue at each commit round before dispatching the
  core's events. A callback may be posted from a synchronous member, where the core is not running, so the queue
  belongs to the table both sides already share rather than to the core's actions; the order with the core's events
  is the same.
- **The table holds what crosses, not every object.** Each op carries the ids of its `this`, instances and callbacks;
  the core takes them, and what a failed call did not take is released (`Instances::release_call`). Objects carry no
  id of their own.
- **Results by value.** `Self` and another class's value are results through the `Ret` impl `#[class]` writes for
  the struct; `Option<Other>` by value cannot be one (Rust's coherence rules), `Option<Instance<Other>>` can.
- **Subclassing works already**: constructors are QuickJS constructors (called only with `new`) and honor
  `new.target`, so a TS subclass's objects are its own.
- **A class without `new`** is declared with `private constructor()`; JS calling it throws a `TypeError`. Members
  named `dispose` or `constructor`, and statics named `prototype`, `name` or `length`, are compile errors (the
  object has those already).
- **Ids stay out of the app's data.** An op lists its held ids beside the arguments and a reply its instance beside
  the value (`h`), so data that looks like `{ $instance: 1 }` is data; ops the engine drops when the VM stops
  release what they hold (`Core::discard_ops`).
- **Panics in `async` module functions** now reject their promise too (they used to fail the root and leave JS
  waiting), as an `async` member's do.
- **Module functions take instances and callbacks** in every kind: an op's and a task's cross in the table like an
  async member's.
- **The standard library's classes** (the table above) have not moved yet; the machinery is shared, so each moves
  when it needs nothing more.

## Known limits

- **Deep chains of instances.** Dropping an instance that holds another (`Instance<Node>` in a field, as a linked
  list) drops the chain recursively, and the mark hook traces it recursively: a chain of very many instances can
  overflow the stack. Keep long sequences in a `Vec`.
- **A promise that never settles.** `call_async` on a function returning a promise that never settles waits for
  ever; a cycle through such a promise (the promise's reactions holding the callback whose reply holds it) keeps
  both alive until the VM goes.
