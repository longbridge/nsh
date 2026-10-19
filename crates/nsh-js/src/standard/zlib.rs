//! LLRT compression with a host-enforced native allocation ceiling.
use rquickjs::{
    Class, Ctx, Exception, FromJs, Function, JsLifetime, Object, Result, Value,
    class::{ClassKind, JsCell, JsClass, Readable, Trace, Tracer},
    function::{Constructor, Params},
    module::{Declarations, Exports, ModuleDef},
};

const LIMIT: usize = 16 * 1024 * 1024;
const METHODS: &[&str] = &[
    "deflate",
    "deflateRaw",
    "gzip",
    "inflate",
    "inflateRaw",
    "gunzip",
    "brotliCompress",
    "brotliDecompress",
    "zstdCompress",
    "zstdDecompress",
];

pub(crate) struct ZlibModule;

/// Callable native state must trace captured JS values. Ordinary Rust closures
/// are opaque to QuickJS's cycle collector and would retain the module/context.
struct CappedFunction<'js> {
    original: Function<'js>,
    sync: bool,
}
// SAFETY: the only lifetime-bearing field is the traced original function.
unsafe impl<'js> JsLifetime<'js> for CappedFunction<'js> {
    type Changed<'to> = CappedFunction<'to>;
}
impl<'js> Trace<'js> for CappedFunction<'js> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        self.original.trace(tracer);
    }
}
impl<'js> JsClass<'js> for CappedFunction<'js> {
    const NAME: &'static str = "NshCappedCompression";
    const KIND: ClassKind = ClassKind::Callable;
    type Mutable = Readable;
    fn prototype(ctx: &Ctx<'js>) -> Result<Option<Object<'js>>> {
        Ok(Some(Function::prototype(ctx.clone())))
    }
    fn constructor(_: &Ctx<'js>) -> Result<Option<Constructor<'js>>> {
        Ok(None)
    }
    fn call<'a>(this: &JsCell<'js, Self>, params: Params<'a, 'js>) -> Result<Value<'js>> {
        let state = this.borrow();
        let ctx = params.ctx();
        let input = params
            .arg(0)
            .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
        let second = params.arg(1);
        if state.sync {
            let opts = options(ctx, second.as_ref())?;
            state.original.call((input, opts))
        } else {
            let (raw_options, callback) =
                if let Some(value) = second.as_ref().filter(|v| v.is_function()) {
                    (None, value.clone())
                } else if let Some(callback) = params.arg(2).filter(|v| v.is_function()) {
                    (second.as_ref(), callback)
                } else {
                    return Err(Exception::throw_type(
                        ctx,
                        "zlib requires a callback function",
                    ));
                };
            let opts = options(ctx, raw_options)?;
            let callback = Function::from_js(ctx, callback)?;
            state.original.call((input, opts, callback))
        }
    }
}

/// Copy options once, then replace the limit with a validated own property.
/// Null-prototype options cannot inherit a guest-controlled maxOutputLength.
fn options<'js>(ctx: &Ctx<'js>, value: Option<&Value<'js>>) -> Result<Object<'js>> {
    let result = Object::new_proto(ctx.clone(), None)?;
    let mut limit = LIMIT;
    if let Some(value) = value.filter(|v| !v.is_undefined() && !v.is_null()) {
        let source = value
            .as_object()
            .ok_or_else(|| Exception::throw_type(ctx, "zlib options must be an object"))?;
        // The limit is read once, including when inherited. Never pass the
        // original object or a getter through to upstream's asynchronous job.
        let value: Value = source.get("maxOutputLength")?;
        if !value.is_undefined() {
            let numeric = value.as_number().ok_or_else(|| {
                Exception::throw_range(ctx, "maxOutputLength must be an integer from 1 to 16777216")
            })?;
            if !numeric.is_finite()
                || numeric < 1.0
                || numeric.fract() != 0.0
                || numeric > LIMIT as f64
            {
                return Err(Exception::throw_range(
                    ctx,
                    "maxOutputLength must be an integer from 1 to 16777216",
                ));
            }
            limit = numeric as usize;
        }
        for key in source.keys::<String>() {
            let key = key?;
            if key != "maxOutputLength" {
                result.set(key.as_str(), source.get::<_, Value>(key.as_str())?)?;
            }
        }
    }
    result.set("maxOutputLength", limit)?;
    Ok(result)
}

impl ModuleDef for ZlibModule {
    fn declare(declarations: &Declarations) -> Result<()> {
        llrt_zlib::ZlibModule::declare(declarations)
    }

    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        llrt_zlib::ZlibModule::evaluate(ctx, exports)?;
        let original: Object = exports.module().get("default")?;
        for key in exports.module().namespace()?.keys::<String>() {
            let key = key?;
            if key != "default" && !METHODS.contains(&key.strip_suffix("Sync").unwrap_or(&key)) {
                return Err(Exception::throw_type(
                    ctx,
                    "Unreviewed upstream zlib export",
                ));
            }
        }
        let safe = Object::new_proto(ctx.clone(), None)?;
        for name in METHODS {
            for sync in [false, true] {
                let export = if sync {
                    format!("{name}Sync")
                } else {
                    (*name).to_owned()
                };
                let function: Function = original.get(export.as_str())?;
                let wrapper = Class::instance(
                    ctx.clone(),
                    CappedFunction {
                        original: function,
                        sync,
                    },
                )?
                .into_inner();
                safe.set(export.as_str(), wrapper.clone())?;
                exports.export(export.as_str(), wrapper)?;
            }
        }
        exports.export("default", safe)?;
        Ok(())
    }
}
