//! Standard URL utilities share the existing WHATWG constructors and brands.
use rquickjs::{
    Class, Coerced, Ctx, Exception, FromJs, Function, JsLifetime, Object, Result, Value,
    class::{ClassKind, JsCell, JsClass, Readable, Trace, Tracer},
    function::{Constructor, Func, Opt, Params},
    module::{Declarations, Exports, ModuleDef},
};
use url::{Position, Url};

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

fn parse(ctx: &Ctx<'_>, text: &str) -> Result<Url> {
    Url::parse(text).map_err(|_| Exception::throw_type(ctx, "Invalid URL"))
}

fn path_to_file_url<'js>(
    ctx: &Ctx<'js>,
    constructor: &Constructor<'js>,
    path: String,
) -> Result<Object<'js>> {
    #[cfg(not(target_arch = "wasm32"))]
    let url = Url::from_file_path(&path)
        .map_err(|_| Exception::throw_type(ctx, "pathToFileURL requires an absolute path"))?;
    #[cfg(target_arch = "wasm32")]
    let url = {
        if !path.starts_with('/') {
            return Err(Exception::throw_type(
                ctx,
                "pathToFileURL requires an absolute path",
            ));
        }
        let mut url = parse(ctx, "file:///")?;
        url.set_path(&path.replace('%', "%25"));
        url
    };
    constructor.construct((url.as_str(),))
}

fn file_url_to_path(ctx: Ctx<'_>, input: Coerced<String>) -> Result<String> {
    let url = parse(&ctx, &input.0)?;
    if url.scheme() != "file" {
        return Err(Exception::throw_type(
            &ctx,
            "fileURLToPath requires a file: URL",
        ));
    }
    // Encoded separators would otherwise silently change the file hierarchy.
    let lower = url.path().to_ascii_lowercase();
    if lower.contains("%2f") || (cfg!(windows) && lower.contains("%5c")) {
        return Err(Exception::throw_type(
            &ctx,
            "file URL contains an encoded path separator",
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        url.to_file_path()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|_| Exception::throw_type(&ctx, "Invalid file URL for this platform"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        if url.host_str().is_some_and(|host| host != "localhost") {
            return Err(Exception::throw_type(&ctx, "Invalid file URL host"));
        }
        percent_encoding::percent_decode_str(url.path())
            .decode_utf8()
            .map(|text| text.into_owned())
            .map_err(|_| Exception::throw_type(&ctx, "File URL is not UTF-8"))
    }
}

fn flag(options: &Opt<Object<'_>>, name: &str, fallback: bool) -> Result<bool> {
    match &options.0 {
        Some(options) => {
            let value: Value = options.get(name)?;
            if value.is_undefined() {
                Ok(fallback)
            } else {
                bool::from_js(options.ctx(), value)
            }
        }
        None => Ok(fallback),
    }
}

fn format(ctx: Ctx<'_>, input: Coerced<String>, options: Opt<Object<'_>>) -> Result<String> {
    let mut url = parse(&ctx, &input.0)?;
    if !flag(&options, "auth", true)? {
        let _ = url.set_username("");
        let _ = url.set_password(None);
    }
    if !flag(&options, "fragment", true)? {
        url.set_fragment(None);
    }
    if !flag(&options, "search", true)? {
        url.set_query(None);
    }
    if flag(&options, "unicode", false)?
        && let Some(url::Host::Domain(host)) = url.host()
    {
        return Ok(format!(
            "{}{}{}",
            &url[..Position::BeforeHost],
            domain_to_unicode(host),
            &url[Position::AfterHost..]
        ));
    }
    Ok(url.to_string())
}

fn url_to_http_options<'js>(ctx: Ctx<'js>, input: Coerced<String>) -> Result<Object<'js>> {
    let url = parse(&ctx, &input.0)?;
    let result = Object::new(ctx.clone())?;
    let search = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    result.set("protocol", format!("{}:", url.scheme()))?;
    result.set(
        "hostname",
        url.host_str().unwrap_or_default().trim_matches(['[', ']']),
    )?;
    result.set(
        "hash",
        url.fragment().map(|v| format!("#{v}")).unwrap_or_default(),
    )?;
    result.set("search", search.clone())?;
    result.set("pathname", url.path())?;
    result.set("path", format!("{}{search}", url.path()))?;
    result.set("href", url.as_str())?;
    if !url.username().is_empty() || url.password().is_some() {
        let username = percent_encoding::percent_decode_str(url.username()).decode_utf8_lossy();
        let password = percent_encoding::percent_decode_str(url.password().unwrap_or_default())
            .decode_utf8_lossy();
        result.set("auth", format!("{username}:{password}"))?;
    }
    if let Some(port) = url.port() {
        result.set("port", port)?;
    }
    Ok(result)
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
