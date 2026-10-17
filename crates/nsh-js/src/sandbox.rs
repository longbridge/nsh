//! The VM sandbox: resource limits, per-call time budgets and language
//! hardening.
//!
//! Every surface runs its own QuickJS runtime, so the limits are per VM. The
//! heap and stack ceilings are runtime settings. Time is budgeted per call
//! from Rust into JS (a bundle evaluation, an event, a timer, the microtask
//! drain, ...): QuickJS calls the interrupt handler periodically while it
//! interprets, and returning `true` unwinds the call with an exception no
//! `catch` can swallow. A long-lived app therefore never accumulates time
//! toward a limit; only a single call that does not return in time is cut
//! off.
//!
//! A bundle holds only the app's code (nsh's runtime is native), so
//! nothing in JS is trusted: limits and checks are enforced here, in Rust,
//! never in JS.

use std::{cell::Cell, ffi::c_int, rc::Rc};

use nsh_core::{js::Limits, time::Instant};
use rquickjs::{Ctx, qjs};

/// The kind of call into JS, which decides its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Evaluating the bundle (or a development update).
    Load,
    /// Everything else.
    Call,
}

/// The clock of the call in progress, shared with the interrupt handler.
pub(crate) struct Budget {
    limits: Limits,
    /// When the running call must have returned.
    deadline: Rc<Cell<Option<Instant>>>,
    /// The call that overran, until its error is reported.
    tripped: Rc<Cell<Option<Phase>>>,
    /// The running call's kind.
    phase: Rc<Cell<Phase>>,
}

impl Budget {
    pub(crate) fn new(limits: Limits) -> Self {
        Self {
            limits,
            deadline: Rc::default(),
            tripped: Rc::default(),
            phase: Rc::new(Cell::new(Phase::Call)),
        }
    }

    pub(crate) fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Starts the clock of a call, unless one is running (nested calls
    /// share the outermost call's budget). Returns whether it started one,
    /// which the caller passes to [`Budget::end`].
    pub(crate) fn begin(&self, phase: Phase) -> bool {
        if self.deadline.get().is_some() {
            return false;
        }
        let budget = match phase {
            Phase::Load => self.limits.load_budget,
            Phase::Call => self.limits.call_budget,
        };
        self.phase.set(phase);
        self.tripped.set(None);
        self.deadline.set(Instant::now().checked_add(budget));
        true
    }

    pub(crate) fn end(&self, started: bool) {
        if started {
            self.deadline.set(None);
        }
    }

    /// Why the last call was cut off, if it overran its budget.
    pub(crate) fn take_trip(&self) -> Option<String> {
        self.tripped.take().map(|phase| {
            let (what, budget) = match phase {
                Phase::Load => ("loading the bundle", self.limits.load_budget),
                Phase::Call => ("a call into JS", self.limits.call_budget),
            };
            format!(
                "the script was interrupted: {what} exceeded its budget of {} ms",
                budget.as_millis()
            )
        })
    }

    /// The runtime's interrupt handler: true once the running call is past
    /// its deadline or the engine is stopping.
    pub(crate) fn interrupt_handler(
        &self,
        stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Box<dyn FnMut() -> bool> {
        let deadline = self.deadline.clone();
        let tripped = self.tripped.clone();
        let phase = self.phase.clone();
        Box::new(move || {
            if stop
                .as_ref()
                .is_some_and(|s| s.load(std::sync::atomic::Ordering::Relaxed))
            {
                return true;
            }
            match deadline.get() {
                Some(d) if Instant::now() > d => {
                    tripped.set(Some(phase.get()));
                    true
                }
                _ => false,
            }
        })
    }
}

/// Removes every way from a string to code (`eval`, the function
/// constructors) and freezes the prototypes all modules share, so one
/// module cannot change `Array.prototype` or `for…in` for the others.
/// Development bundles keep them (REPL-style debugging; hot updates are
/// evaluated from Rust and need neither).
const HARDEN: &str = r#"
(() => {
  const deny = (label) => function () {
    throw new TypeError(label + " is disabled: nsh runs no code from strings (development builds allow it)");
  };
  const replace = (fn, value) => {
    Object.defineProperty(Object.getPrototypeOf(fn), "constructor", {
      value, writable: false, enumerable: false, configurable: false,
    });
  };
  const blocked = deny("the Function constructor");
  blocked.prototype = Function.prototype;
  replace(function () {}, blocked);
  replace(async function () {}, deny("the AsyncFunction constructor"));
  replace(function* () {}, deny("the GeneratorFunction constructor"));
  replace(async function* () {}, deny("the AsyncGeneratorFunction constructor"));
  Object.defineProperty(globalThis, "Function", { value: blocked, writable: false, configurable: false });
  delete globalThis.eval;
  // Assigning a property an object inherits read-only fails (the
  // "override mistake"): `Sub.prototype.constructor = Sub` or
  // `obj.toString = …` would throw once these prototypes are frozen. Their
  // commonly overridden members become accessors whose setter defines the
  // property on the object assigned to, as on an unfrozen prototype (the
  // prototype itself still cannot change).
  const tamed = [
    "constructor", "toString", "toLocaleString", "valueOf", "hasOwnProperty", "isPrototypeOf",
    "propertyIsEnumerable", "toJSON",
  ];
  const tame = (proto) => {
    for (const name of tamed) {
      const d = Object.getOwnPropertyDescriptor(proto, name);
      if (!d || !("value" in d) || !d.writable) continue;
      const value = d.value;
      Object.defineProperty(proto, name, {
        get() { return value; },
        set(v) {
          if (this === proto) throw new TypeError(`Cannot assign to read only property '${name}'`);
          Object.defineProperty(this, name, { value: v, writable: true, enumerable: true, configurable: true });
        },
        enumerable: d.enumerable,
        configurable: false,
      });
    }
  };
  // Not Error.prototype: `this.name = "MyError"` in a subclass constructor
  // assigns a property it has.
  for (const proto of [
    Object.prototype, Array.prototype, Function.prototype, String.prototype, Number.prototype,
    Boolean.prototype, Symbol.prototype, RegExp.prototype, Promise.prototype, Map.prototype,
    Set.prototype, WeakMap.prototype, WeakSet.prototype, Date.prototype,
    Object.getPrototypeOf(Uint8Array.prototype),
  ]) {
    tame(proto);
    Object.freeze(proto);
  }
})();
"#;

/// Installs the hardening. Runs after the host's own globals and before the
/// bundle.
pub(crate) fn harden(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(HARDEN)
}

/// The hardening compiled to bytecode ([`harden_from`]), for runtimes
/// without QuickJS' parser (`bytecode-only`), which cannot compile it.
/// The CLI makes it when it compiles the app, so it always matches the
/// QuickJS the app's bytecode is for ([`crate::BYTECODE_VERSION`]).
pub fn compile_harden() -> Result<Vec<u8>, crate::JsError> {
    let rt = rquickjs::Runtime::new()?;
    let ctx = rquickjs::Context::full(&rt)?;
    ctx.with(|ctx| {
        let raw = ctx.as_raw().as_ptr();
        let source = std::ffi::CString::new(HARDEN).expect("no NUL in the hardening");
        // SAFETY: a live context; the source is NUL-terminated; the
        // function value is freed and the written buffer copied and freed.
        unsafe {
            // As `ctx.eval` runs it: a strict global script.
            let function = qjs::JS_Eval(
                raw,
                source.as_ptr(),
                HARDEN.len() as _,
                c"harden.js".as_ptr(),
                (qjs::JS_EVAL_TYPE_GLOBAL
                    | qjs::JS_EVAL_FLAG_STRICT
                    | qjs::JS_EVAL_FLAG_COMPILE_ONLY) as c_int,
            );
            if qjs::JS_IsException(function) {
                return Err(crate::JsError::from_engine(
                    &ctx,
                    rquickjs::Error::Exception,
                ));
            }
            let mut len = 0;
            let flags = qjs::JS_WRITE_OBJ_BYTECODE
                | qjs::JS_WRITE_OBJ_STRIP_SOURCE
                | qjs::JS_WRITE_OBJ_STRIP_DEBUG;
            let buf = qjs::JS_WriteObject(raw, &mut len, function, flags as c_int);
            qjs::JS_FreeValue(raw, function);
            if buf.is_null() {
                return Err(crate::JsError::from_engine(
                    &ctx,
                    rquickjs::Error::Exception,
                ));
            }
            let bytes = std::slice::from_raw_parts(buf, len as usize).to_vec();
            qjs::js_free(raw, buf.cast());
            Ok(bytes)
        }
    })
}

/// [`harden`] from the bytecode [`compile_harden`] made.
///
/// # Safety
///
/// `bytecode` comes from [`compile_harden`] of the same QuickJS build:
/// QuickJS does not validate it.
pub(crate) unsafe fn harden_from(ctx: &Ctx<'_>, bytecode: &[u8]) -> rquickjs::Result<()> {
    let raw = ctx.as_raw().as_ptr();
    // SAFETY: a live context; trusted bytecode per the contract; the
    // function value is consumed by `JS_EvalFunction`, its result freed.
    unsafe {
        let function = qjs::JS_ReadObject(
            raw,
            bytecode.as_ptr(),
            bytecode.len() as _,
            qjs::JS_READ_OBJ_BYTECODE as c_int,
        );
        if qjs::JS_IsException(function) {
            return Err(rquickjs::Error::Exception);
        }
        let result = qjs::JS_EvalFunction(raw, function);
        if qjs::JS_IsException(result) {
            return Err(rquickjs::Error::Exception);
        }
        qjs::JS_FreeValue(raw, result);
    }
    Ok(())
}
