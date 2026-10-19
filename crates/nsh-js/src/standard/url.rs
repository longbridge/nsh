//! Standard URL utilities share the existing WHATWG constructors and brands.
#[cfg(target_arch = "wasm32")]
use rquickjs::Exception;
use rquickjs::{
    Class, Coerced, Ctx, FromJs, Function, JsLifetime, Object, Result, Value,
    class::{ClassKind, JsCell, JsClass, Readable, Trace, Tracer},
    function::{Constructor, Func, Opt, Params},
    module::{Declarations, Exports, ModuleDef},
};
#[cfg(target_arch = "wasm32")]
use url::Url;

fn domain_to_ascii(domain: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        llrt_url::domain_to_ascii(domain)
    }
    #[cfg(target_arch = "wasm32")]
    {
        url::quirks::domain_to_ascii(domain)
    }
}
fn domain_to_unicode(domain: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        llrt_url::domain_to_unicode(domain)
    }
    #[cfg(target_arch = "wasm32")]
    {
        url::quirks::domain_to_unicode(domain)
    }
}

pub(crate) struct UrlModule;

struct FileUrlConstructor<'js>(Constructor<'js>);
// SAFETY: the only lifetime-bearing field is the traced constructor handle.
unsafe impl<'js> JsLifetime<'js> for FileUrlConstructor<'js> {
    type Changed<'to> = FileUrlConstructor<'to>;
}
impl<'js> Trace<'js> for FileUrlConstructor<'js> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        self.0.trace(tracer);
    }
}
impl<'js> JsClass<'js> for FileUrlConstructor<'js> {
    const NAME: &'static str = "NshFileUrlConstructor";
    const KIND: ClassKind = ClassKind::Callable;
    type Mutable = Readable;
    fn prototype(ctx: &Ctx<'js>) -> Result<Option<Object<'js>>> {
        Ok(Some(Function::prototype(ctx.clone())))
    }
    fn constructor(_: &Ctx<'js>) -> Result<Option<Constructor<'js>>> {
        Ok(None)
    }
    fn call<'a>(this: &JsCell<'js, Self>, params: Params<'a, 'js>) -> Result<Value<'js>> {
        let ctx = params.ctx();
        let input = params
            .arg(0)
            .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
        let path = String::from_js(ctx, input)?;
        path_to_file_url(ctx, &this.borrow().0, path).map(Value::from_object)
    }
}
const METHODS: &[&str] = &[
    "URL",
    "URLSearchParams",
    "domainToASCII",
    "domainToUnicode",
    "fileURLToPath",
    "pathToFileURL",
    "format",
    "urlToHttpOptions",
];

#[cfg(target_arch = "wasm32")]
fn parse(ctx: &Ctx<'_>, text: &str) -> Result<Url> {
    Url::parse(text).map_err(|_| Exception::throw_type(ctx, "Invalid URL"))
}

fn path_to_file_url<'js>(
    ctx: &Ctx<'js>,
    constructor: &Constructor<'js>,
    path: String,
) -> Result<Object<'js>> {
    #[cfg(not(target_arch = "wasm32"))]
    let href = llrt_url::path_to_file_url(ctx.clone(), path, Opt(None))?.href();
    #[cfg(target_arch = "wasm32")]
    let href = {
        if !path.starts_with('/') {
            return Err(Exception::throw_type(ctx, "Path is not absolute"));
        }
        let mut url = parse(ctx, "file:///")?;
        url.set_path(&path.replace('%', "%25"));
        url.to_string()
    };
    constructor.construct((href,))
}

fn file_url_to_path<'js>(ctx: Ctx<'js>, input: Coerced<String>) -> Result<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rquickjs::IntoJs;
        llrt_url::file_url_to_path(ctx.clone(), input.0.into_js(&ctx)?)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        // Match LLRT's utility contract, including preserving percent escapes.
        Ok(input.0.trim_start_matches("file://").to_owned())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_url<'js>(
    ctx: &Ctx<'js>,
    input: &str,
) -> Result<Class<'js, llrt_url::url_class::URL<'js>>> {
    Class::instance(
        ctx.clone(),
        llrt_url::url_class::URL::from_str(ctx.clone(), input)?,
    )
}

fn format<'js>(ctx: Ctx<'js>, input: Coerced<String>, options: Opt<Value<'js>>) -> Result<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        llrt_url::url_format(native_url(&ctx, &input.0)?, options)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let url = parse(&ctx, &input.0)?;
        let flag = |name: &str, fallback| {
            options
                .0
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|options| options.get::<_, bool>(name).ok())
                .unwrap_or(fallback)
        };
        let mut text = format!("{}//", url::quirks::protocol(&url));
        if flag("auth", true) && !url.username().is_empty() {
            text.push_str(url.username());
            if let Some(password) = url.password().filter(|p| !p.is_empty()) {
                text.push(':');
                text.push_str(password);
            }
            text.push('@');
        }
        let host = url::quirks::host(&url);
        text.push_str(&if flag("unicode", false) {
            domain_to_unicode(host)
        } else {
            host.to_owned()
        });
        text.push_str(url::quirks::pathname(&url));
        if flag("search", true) {
            text.push_str(url::quirks::search(&url));
        }
        if flag("fragment", true) {
            text.push_str(url::quirks::hash(&url));
        }
        Ok(text)
    }
}

fn url_to_http_options<'js>(ctx: Ctx<'js>, input: Coerced<String>) -> Result<Object<'js>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        llrt_url::url_class::url_to_http_options(ctx.clone(), native_url(&ctx, &input.0)?)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let url = parse(&ctx, &input.0)?;
        let result = Object::new(ctx.clone())?;
        let search = url::quirks::search(&url);
        result.set("protocol", url::quirks::protocol(&url))?;
        result.set("hostname", url::quirks::hostname(&url))?;
        if let Some(hash) = url.fragment().filter(|h| !h.is_empty()) {
            result.set("hash", hash)?;
        }
        if !search.is_empty() {
            result.set("search", search)?;
        }
        result.set("pathname", url::quirks::pathname(&url))?;
        result.set("path", format!("{}{search}", url::quirks::pathname(&url)))?;
        result.set("href", url.as_str())?;
        if !url.username().is_empty() {
            result.set(
                "auth",
                format!("{}:{}", url.username(), url.password().unwrap_or_default()),
            )?;
        }
        if let Some(port) = url.port() {
            result.set("port", port.to_string())?;
        }
        Ok(result)
    }
}

impl ModuleDef for UrlModule {
    fn declare(declarations: &Declarations) -> Result<()> {
        declarations.declare("default")?;
        for name in METHODS {
            declarations.declare(*name)?;
        }
        Ok(())
    }
    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        let module = Object::new(ctx.clone())?;
        module.set("URL", ctx.globals().get::<_, Value>("URL")?)?;
        module.set(
            "URLSearchParams",
            ctx.globals().get::<_, Value>("URLSearchParams")?,
        )?;
        module.set(
            "domainToASCII",
            Func::from(|domain: String| domain_to_ascii(&domain)),
        )?;
        module.set(
            "domainToUnicode",
            Func::from(|domain: String| domain_to_unicode(&domain)),
        )?;
        let constructor: Constructor = ctx.globals().get("URL")?;
        module.set(
            "pathToFileURL",
            Class::instance(ctx.clone(), FileUrlConstructor(constructor))?.into_inner(),
        )?;
        module.set("fileURLToPath", Func::from(file_url_to_path))?;
        module.set("format", Func::from(format))?;
        module.set("urlToHttpOptions", Func::from(url_to_http_options))?;
        for name in METHODS {
            exports.export(*name, module.get::<_, Value>(*name)?)?;
        }
        exports.export("default", module)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn llrt_initialization_preserves_native_web_constructor_identity() {
        let host = crate::Host::new().unwrap();
        host.with_raw(|raw| {
            // SAFETY: with_raw holds this live runtime's context lock, and no
            // value from the temporary Ctx escapes the closure.
            let ctx = unsafe { rquickjs::Ctx::from_raw(std::ptr::NonNull::new(raw).unwrap()) };
            let globals = ctx.globals();
            let names = [
                "Blob",
                "File",
                "URL",
                "URLSearchParams",
                "Event",
                "ReadableStream",
                "AbortController",
            ];
            let before = names.map(|name| globals.get::<_, rquickjs::Value>(name).unwrap());
            super::super::install(&ctx).unwrap();
            for (name, before) in names.into_iter().zip(before) {
                let after = globals.get::<_, rquickjs::Value>(name).unwrap();
                assert_eq!(
                    before, after,
                    "LLRT replaced the existing {name} constructor"
                );
            }
        });
    }
}
