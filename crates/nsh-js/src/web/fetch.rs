//! `Headers`, `Request`, `Response` and `fetch` (Fetch Standard), over the
//! platform's network stack, streaming both ways: a response's body is a
//! byte stream filled as the stack delivers chunks (the request is paused
//! while more than [`QUEUE_LIMIT`] bytes wait unread), and a request body
//! that is a `ReadableStream` is sent chunk by chunk as the stack asks for
//! them. `data:` and `blob:` URLs are answered locally. Relative URLs
//! resolve against `HostOptions::base_url` (the page's URL on the web);
//! hosts without one reject them.
//!
//! Not as in browsers: no CORS (an app talks to any host its capabilities
//! allow), no forbidden request headers (an app may set `Cookie` or
//! `User-Agent`), `redirect: "manual"` delivers the 3xx response itself,
//! and a `FormData` body always gets its `multipart/form-data; boundary=…`
//! type (libraries set a default type, and clear it only where they detect
//! a browser; without the boundary the body cannot be parsed).

use std::{
    cell::{Cell, RefMut},
    rc::Rc,
};

use super::{
    Class, Key, Object, Spec,
    blob::{self, FormValue},
    data, instance, is,
    iterator::{self, Part},
    js::{self, Ctx, Deferred, R, Raw, V, native, trace},
    role,
    streams::{Algo, readable},
    web,
};
use crate::net::{FetchEvent, FetchOp, HttpBody, HttpHead, HttpRequest, RequestId};

/// Response bytes waiting unread before the request is paused.
const QUEUE_LIMIT: f64 = (1 << 20) as f64;

// --- Headers --------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Guard {
    None,
    /// A network response's: no changes.
    Immutable,
}

pub(crate) struct Headers {
    /// Names in lower case, in insertion order.
    list: Vec<(String, String)>,
    guard: Guard,
}

trace!(Headers {});

impl Object for Headers {}

fn headers<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, Headers>> {
    data::<Headers>(ctx, v, "Headers")
}

/// An HTTP token (header names, methods).
fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// A header value, normalized (outer HTTP whitespace removed); `None`
/// when invalid.
fn header_value(v: &str) -> Option<String> {
    let v = v.trim_matches(|c| matches!(c, '\t' | '\n' | '\r' | ' '));
    (!v.contains(['\0', '\n', '\r'])).then(|| v.to_owned())
}

/// A ByteString argument.
fn byte_string(ctx: Ctx, v: Raw) -> R<String> {
    let s = js::to_string(ctx, v)?;
    if s.chars().any(|c| c as u32 > 0xFF) {
        return Err(js::type_error(ctx, "a header must be a ByteString"));
    }
    Ok(s)
}

impl Headers {
    fn append(&mut self, ctx: Ctx, name: &str, value: &str) -> R<()> {
        if !is_token(name) {
            return Err(js::type_error(
                ctx,
                &format!("Invalid header name: \"{name}\""),
            ));
        }
        let value = header_value(value)
            .ok_or_else(|| js::type_error(ctx, &format!("Invalid header value for \"{name}\"")))?;
        if self.guard == Guard::Immutable {
            return Err(js::type_error(ctx, "the headers are immutable"));
        }
        self.list.push((name.to_ascii_lowercase(), value));
        Ok(())
    }

    fn set(&mut self, ctx: Ctx, name: &str, value: &str) -> R<()> {
        if !is_token(name) {
            return Err(js::type_error(
                ctx,
                &format!("Invalid header name: \"{name}\""),
            ));
        }
        let value = header_value(value)
            .ok_or_else(|| js::type_error(ctx, &format!("Invalid header value for \"{name}\"")))?;
        if self.guard == Guard::Immutable {
            return Err(js::type_error(ctx, "the headers are immutable"));
        }
        let name = name.to_ascii_lowercase();
        match self.list.iter().position(|(n, _)| *n == name) {
            Some(i) => {
                self.list[i].1 = value;
                let mut j = 0;
                self.list.retain(|(n, _)| {
                    j += 1;
                    j - 1 <= i || *n != name
                });
            }
            None => self.list.push((name, value)),
        }
        Ok(())
    }

    /// The combined value of `name` (lower case).
    pub(crate) fn get(&self, name: &str) -> Option<String> {
        let values: Vec<&str> = self
            .list
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect();
        (!values.is_empty()).then(|| values.join(", "))
    }

    fn has(&self, name: &str) -> bool {
        self.list.iter().any(|(n, _)| n == name)
    }

    /// Sorted and combined, `set-cookie` values one by one.
    fn sorted(&self) -> Vec<(String, String)> {
        let mut names: Vec<&String> = self.list.iter().map(|(n, _)| n).collect();
        names.sort();
        names.dedup();
        let mut out = Vec::new();
        for n in names {
            if n == "set-cookie" {
                for (k, v) in self.list.iter().filter(|(k, _)| k == n) {
                    out.push((k.clone(), v.clone()));
                }
            } else if let Some(v) = self.get(n) {
                out.push((n.clone(), v));
            }
        }
        out
    }
}

/// Fills `h` from a HeadersInit.
fn fill_headers(ctx: Ctx, h: Raw, init: Raw) -> R<()> {
    if js::is_undefined(init) {
        return Ok(());
    }
    if is::<Headers>(ctx, init) {
        let list = headers(ctx, init)?.list.clone();
        headers(ctx, h)?.list.extend(list);
        return Ok(());
    }
    if !js::is_object(init) {
        return Err(js::type_error(ctx, "HeadersInit must be an object"));
    }
    if js::is_iterable(ctx, init)? {
        let mut pairs = Vec::new();
        js::for_each(ctx, init, |pair| {
            let mut items = Vec::new();
            js::for_each(ctx, pair.raw(), |x| {
                items.push(byte_string(ctx, x.raw())?);
                Ok(())
            })?;
            let [k, v]: [String; 2] = items
                .try_into()
                .map_err(|_| js::type_error(ctx, "Each header pair must have two items"))?;
            pairs.push((k, v));
            Ok(())
        })?;
        for (k, v) in pairs {
            headers(ctx, h)?.append(ctx, &k, &v)?;
        }
        return Ok(());
    }
    for (k, v) in js::record(ctx, init)? {
        let v = byte_string(ctx, v.raw())?;
        headers(ctx, h)?.append(ctx, &k, &v)?;
    }
    Ok(())
}

fn new_headers(ctx: Ctx, list: Vec<(String, String)>, guard: Guard) -> R<V> {
    instance(ctx, js::UNDEFINED, Class::Headers, Headers { list, guard })
}

native!(
    fn headers_constructor(ctx, this, args) {
        let h = instance(
            ctx,
            this,
            Class::Headers,
            Headers {
                list: Vec::new(),
                guard: Guard::None,
            },
        )?;
        fill_headers(ctx, h.raw(), args.get(0))?;
        Ok(h)
    }
);

native!(
    fn headers_append(ctx, this, args) {
        let (k, v) = (byte_string(ctx, args.get(0))?, byte_string(ctx, args.get(1))?);
        headers(ctx, this)?.append(ctx, &k, &v)?;
        Ok(V::undefined())
    }
);

native!(
    fn headers_set(ctx, this, args) {
        let (k, v) = (byte_string(ctx, args.get(0))?, byte_string(ctx, args.get(1))?);
        headers(ctx, this)?.set(ctx, &k, &v)?;
        Ok(V::undefined())
    }
);

native!(
    fn headers_delete(ctx, this, args) {
        let k = byte_string(ctx, args.get(0))?;
        if !is_token(&k) {
            return Err(js::type_error(ctx, &format!("Invalid header name: \"{k}\"")));
        }
        let mut h = headers(ctx, this)?;
        if h.guard == Guard::Immutable {
            return Err(js::type_error(ctx, "the headers are immutable"));
        }
        let k = k.to_ascii_lowercase();
        h.list.retain(|(n, _)| *n != k);
        Ok(V::undefined())
    }
);

native!(
    fn headers_get(ctx, this, args) {
        let k = byte_string(ctx, args.get(0))?;
        if !is_token(&k) {
            return Err(js::type_error(ctx, &format!("Invalid header name: \"{k}\"")));
        }
        let v = headers(ctx, this)?.get(&k.to_ascii_lowercase());
        match v {
            Some(v) => js::string(ctx, &v),
            None => Ok(V::null()),
        }
    }
);

native!(
    fn headers_get_set_cookie(ctx, this, args) {
        let values: Vec<String> = headers(ctx, this)?
            .list
            .iter()
            .filter(|(n, _)| n == "set-cookie")
            .map(|(_, v)| v.clone())
            .collect();
        let items = values.iter().map(|v| js::string(ctx, v)).collect::<R<Vec<_>>>()?;
        js::new_array(ctx, items)
    }
);

native!(
    fn headers_has(ctx, this, args) {
        let k = byte_string(ctx, args.get(0))?;
        if !is_token(&k) {
            return Err(js::type_error(ctx, &format!("Invalid header name: \"{k}\"")));
        }
        Ok(V::bool(headers(ctx, this)?.has(&k.to_ascii_lowercase())))
    }
);

native!(
    fn headers_for_each(ctx, this, args) {
        let f = args.get(0);
        if !js::is_function(ctx, f) {
            return Err(js::type_error(ctx, "the callback is not a function"));
        }
        let mut i = 0;
        loop {
            let pair = headers(ctx, this)?.sorted().get(i).cloned();
            let Some((k, v)) = pair else { break };
            let (k, v) = (js::string(ctx, &k)?, js::string(ctx, &v)?);
            js::call(ctx, f, args.get(1), &[v.raw(), k.raw(), this])?;
            i += 1;
        }
        Ok(V::undefined())
    }
);

fn headers_iterator(ctx: Ctx, this: Raw, part: Part) -> R<V> {
    let sorted = headers(ctx, this)?.sorted();
    let items = sorted
        .iter()
        .map(|(k, v)| iterator::pair_item(ctx, k, v, part))
        .collect::<R<Vec<_>>>()?;
    iterator::new(ctx, items)
}

native!(
    fn headers_entries(ctx, this, args) {
        headers_iterator(ctx, this, Part::Entries)
    }
);

native!(
    fn headers_keys(ctx, this, args) {
        headers_iterator(ctx, this, Part::Keys)
    }
);

native!(
    fn headers_values(ctx, this, args) {
        headers_iterator(ctx, this, Part::Values)
    }
);

// --- Bodies ---------------------------------------------------------------

/// A body: bytes known up front (until something asks for its stream),
/// or a stream.
#[derive(Default)]
pub(crate) struct Body {
    bytes: Option<Rc<[u8]>>,
    stream: V,
    /// Consumed while still bytes.
    used: bool,
}

trace!(Body { stream });

impl Body {
    fn null() -> Body {
        Body::default()
    }

    fn bytes(bytes: Vec<u8>) -> Body {
        Body {
            bytes: Some(bytes.into()),
            stream: V::undefined(),
            used: false,
        }
    }

    fn is_null(&self) -> bool {
        self.bytes.is_none() && self.stream.is_undefined()
    }
}

fn body<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, Body>> {
    role(ctx, v, "Body", |o| o.body())
}

/// What `extract a body` gives: the body and its content type.
fn extract_body(ctx: Ctx, init: Raw) -> R<(Body, Option<String>)> {
    if let Some(b) = blob::blob_of(ctx, init)? {
        let mime = (!b.mime.is_empty()).then(|| b.mime.clone());
        return Ok((Body::bytes(b.bytes().to_vec()), mime));
    }
    if let Some(bytes) = js::buffer_source(ctx, init) {
        return Ok((Body::bytes(bytes), None));
    }
    if blob::is_form_data(ctx, init) {
        let (bytes, mime) = blob::encode_multipart(ctx, init)?;
        return Ok((Body::bytes(bytes), Some(mime)));
    }
    if super::url::is_params(ctx, init) {
        let s = super::url::params_string(ctx, init)?;
        return Ok((
            Body::bytes(s.into_bytes()),
            Some("application/x-www-form-urlencoded;charset=UTF-8".to_owned()),
        ));
    }
    if readable::is_stream(ctx, init) {
        if readable::is_disturbed(ctx, init)? || readable::is_locked(ctx, init)? {
            return Err(js::type_error(
                ctx,
                "the body stream is disturbed or locked",
            ));
        }
        return Ok((
            Body {
                bytes: None,
                stream: V::dup(ctx, init),
                used: false,
            },
            None,
        ));
    }
    let s = js::to_string(ctx, init)?;
    Ok((
        Body::bytes(s.into_bytes()),
        Some("text/plain;charset=UTF-8".to_owned()),
    ))
}

/// The body's stream (made from its bytes on first use), or null.
fn body_stream(ctx: Ctx, owner: Raw) -> R<V> {
    let (stream, bytes) = {
        let b = body(ctx, owner)?;
        (b.stream.clone_ref(), b.bytes.clone())
    };
    if !stream.is_undefined() {
        return Ok(stream);
    }
    let Some(bytes) = bytes else {
        return Ok(V::null());
    };
    let s = super::streams::bytes_stream(ctx, bytes.to_vec())?;
    let mut b = body(ctx, owner)?;
    if b.used {
        // Consumed: its stream is an empty, disturbed one.
        drop(b);
        readable::cancel_stream(ctx, s.raw(), js::UNDEFINED)?;
        b = body(ctx, owner)?;
    }
    b.stream = s.clone_ref();
    Ok(s)
}

fn body_used(ctx: Ctx, owner: Raw) -> R<bool> {
    let (used, stream) = {
        let b = body(ctx, owner)?;
        (b.used, b.stream.clone_ref())
    };
    Ok(used || (!stream.is_undefined() && readable::is_disturbed(ctx, stream.raw())?))
}

/// The body's bytes (a promise of a Uint8Array); it is used afterwards.
fn consume(ctx: Ctx, owner: Raw) -> R<V> {
    let (bytes, stream, used) = {
        let b = body(ctx, owner)?;
        (b.bytes.clone(), b.stream.clone_ref(), b.used)
    };
    if stream.is_undefined() {
        if used {
            let e = js::type_error_value(ctx, "Body has already been consumed");
            return js::rejected(ctx, e.raw());
        }
        body(ctx, owner)?.used = bytes.is_some();
        let out = js::uint8_array(ctx, bytes.as_deref().unwrap_or_default())?;
        return js::resolved(ctx, out.raw());
    }
    if readable::is_disturbed(ctx, stream.raw())? || readable::is_locked(ctx, stream.raw())? {
        let e = js::type_error_value(ctx, "Body has already been consumed");
        return js::rejected(ctx, e.raw());
    }
    readable::read_all_bytes(ctx, stream.raw())
}

/// The body's MIME type (its `Content-Type`), lower case.
fn body_mime(ctx: Ctx, owner: Raw) -> R<String> {
    let h = if is::<RequestData>(ctx, owner) {
        data::<RequestData>(ctx, owner, "Request")?
            .headers
            .clone_ref()
    } else {
        data::<ResponseData>(ctx, owner, "Response")?
            .headers
            .clone_ref()
    };
    Ok(headers(ctx, h.raw())?
        .get("content-type")
        .unwrap_or_default())
}

/// Consumes the body and converts its bytes with `f`.
fn consume_as(
    ctx: Ctx,
    owner: Raw,
    f: impl Fn(Ctx, &[V], &[u8]) -> R<V> + 'static,
    captures: Vec<V>,
) -> R<V> {
    let p = consume(ctx, owner)?;
    let convert = js::closure(ctx, captures, move |ctx, c, args| {
        // SAFETY: read before any JS runs.
        let bytes = unsafe { js::bytes_of(ctx, args.get(0)) }
            .unwrap_or_default()
            .to_vec();
        f(ctx, c, &bytes)
    })?;
    js::then(ctx, p.raw(), convert.raw(), js::UNDEFINED)
}

/// UTF-8 decode without a BOM (`text()`).
fn utf8(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

native!(
    fn body_get(ctx, this, args) {
        body_stream(ctx, this)
    }
);

native!(
    fn body_used_get(ctx, this, args) {
        Ok(V::bool(body_used(ctx, this)?))
    }
);

native!(
    fn body_array_buffer(ctx, this, args) {
        js::promise_of(ctx, || consume_as(ctx, this, |ctx, _, b| js::array_buffer(ctx, b), Vec::new()))
    }
);

native!(
    fn body_bytes(ctx, this, args) {
        js::promise_of(ctx, || consume_as(ctx, this, |ctx, _, b| js::uint8_array(ctx, b), Vec::new()))
    }
);

native!(
    fn body_text(ctx, this, args) {
        js::promise_of(ctx, || consume_as(ctx, this, |ctx, _, b| js::string(ctx, &utf8(b)), Vec::new()))
    }
);

native!(
    fn body_json(ctx, this, args) {
        js::promise_of(ctx, || {
            consume_as(
                ctx,
                this,
                |ctx, _, b| {
                    let mut text = utf8(b).into_bytes();
                    let len = text.len();
                    text.push(0);
                    // SAFETY: a NUL-terminated buffer of `len` bytes.
                    V::checked(ctx, unsafe {
                        rquickjs::qjs::JS_ParseJSON(ctx, text.as_ptr().cast(), len as _, c"<json>".as_ptr())
                    })
                },
                Vec::new(),
            )
        })
    }
);

native!(
    fn body_blob(ctx, this, args) {
        js::promise_of(ctx, || {
            let mime = body_mime(ctx, this)?.to_ascii_lowercase();
            consume_as(
                ctx,
                this,
                move |ctx, _, b| blob::new_blob(ctx, blob::Blob::new(b.to_vec(), mime.clone())),
                Vec::new(),
            )
        })
    }
);

native!(
    fn body_form_data(ctx, this, args) {
        js::promise_of(ctx, || {
            let mime = body_mime(ctx, this)?;
            consume_as(
                ctx,
                this,
                move |ctx, _, b| {
                    let essence = mime.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
                    let entries = match essence.as_str() {
                        "multipart/form-data" => {
                            let boundary = blob::header_param(&mime, "boundary")
                                .ok_or_else(|| js::type_error(ctx, "the multipart body has no boundary"))?;
                            blob::parse_multipart(ctx, b, &boundary)?
                                .ok_or_else(|| js::type_error(ctx, "the multipart body is malformed"))?
                        }
                        "application/x-www-form-urlencoded" => super::url::parse_query(&String::from_utf8_lossy(b))
                            .into_iter()
                            .map(|(k, v)| (k, FormValue::Str(v)))
                            .collect(),
                        _ => return Err(js::type_error(ctx, "the body is not form data")),
                    };
                    blob::new_form_data(ctx, entries)
                },
                Vec::new(),
            )
        })
    }
);

/// A copy of `owner`'s body for `clone()` (a tee of its stream).
fn clone_body(ctx: Ctx, owner: Raw) -> R<Body> {
    if body_used(ctx, owner)? {
        return Err(js::type_error(ctx, "Body has already been consumed"));
    }
    let (bytes, stream) = {
        let b = body(ctx, owner)?;
        (b.bytes.clone(), b.stream.clone_ref())
    };
    if stream.is_undefined() {
        return Ok(Body {
            bytes,
            stream: V::undefined(),
            used: false,
        });
    }
    if readable::is_locked(ctx, stream.raw())? {
        return Err(js::type_error(ctx, "the body is locked"));
    }
    let (a, b) = readable::tee(ctx, stream.raw())?;
    body(ctx, owner)?.stream = a;
    Ok(Body {
        bytes: None,
        stream: b,
        used: false,
    })
}

// --- Request --------------------------------------------------------------

pub(crate) struct RequestData {
    method: String,
    url: String,
    headers: V,
    body: Body,
    signal: V,
    redirect: String,
    credentials: String,
    cache: String,
    mode: String,
    referrer: String,
    referrer_policy: String,
    integrity: String,
    keepalive: bool,
    priority: String,
}

trace!(RequestData {
    headers,
    body,
    signal
});

impl Object for RequestData {
    fn body(&mut self) -> Option<&mut Body> {
        Some(&mut self.body)
    }
}

fn request<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, RequestData>> {
    data::<RequestData>(ctx, v, "Request")
}

/// An enum member of `init` (`allowed`; undefined: `None`).
fn init_enum(ctx: Ctx, init: Raw, key: &str, allowed: &[&str]) -> R<Option<String>> {
    if !js::is_object(init) {
        return Ok(None);
    }
    let v = js::get(ctx, init, key)?;
    let Some(s) = js::opt_string(ctx, v.raw())? else {
        return Ok(None);
    };
    if !allowed.contains(&s.as_str()) {
        return Err(js::type_error(ctx, &format!("invalid {key} `{s}`")));
    }
    Ok(Some(s))
}

/// Parses a request's (or a redirect's, a referrer's) URL: relative to
/// the host's base URL (the page's on the web), which other hosts lack.
fn parse_url(ctx: Ctx, s: &str) -> Result<url::Url, url::ParseError> {
    url::Url::options()
        .base_url(web(ctx).base_url.as_ref())
        .parse(s)
}

/// The `new Request(input, init)` steps.
fn construct_request(ctx: Ctx, new_target: Raw, input: Raw, init: Raw) -> R<V> {
    if !js::is_nullish(init) && !js::is_object(init) {
        return Err(js::type_error(ctx, "RequestInit must be an object"));
    }
    let base = is::<RequestData>(ctx, input).then(|| V::dup(ctx, input));
    let url = match &base {
        Some(b) => request(ctx, b.raw())?.url.clone(),
        None => {
            let s = js::to_string(ctx, input)?;
            let u = parse_url(ctx, &s)
                .map_err(|_| js::type_error(ctx, &format!("Invalid URL: {s}")))?;
            if !u.username().is_empty() || u.password().is_some() {
                return Err(js::type_error(ctx, "a request URL cannot have credentials"));
            }
            u.to_string()
        }
    };
    let get = |key: &str| -> R<V> {
        if js::is_object(init) {
            js::get(ctx, init, key)
        } else {
            Ok(V::undefined())
        }
    };
    let from = |field: fn(&RequestData) -> String, default: &str| -> R<String> {
        Ok(match &base {
            Some(b) => field(&*request(ctx, b.raw())?),
            None => default.to_owned(),
        })
    };
    let method = get("method")?;
    let method = match js::opt_string(ctx, method.raw())? {
        Some(m) => {
            if !is_token(&m) {
                return Err(js::type_error(ctx, &format!("invalid method `{m}`")));
            }
            let upper = m.to_ascii_uppercase();
            if matches!(upper.as_str(), "CONNECT" | "TRACE" | "TRACK") {
                return Err(js::type_error(ctx, &format!("method `{m}` is forbidden")));
            }
            if matches!(
                upper.as_str(),
                "DELETE" | "GET" | "HEAD" | "OPTIONS" | "POST" | "PUT" | "PATCH"
            ) {
                upper
            } else {
                m
            }
        }
        None => from(|r| r.method.clone(), "GET")?,
    };
    let redirect = match init_enum(ctx, init, "redirect", &["follow", "error", "manual"])? {
        Some(r) => r,
        None => from(|r| r.redirect.clone(), "follow")?,
    };
    let credentials = match init_enum(
        ctx,
        init,
        "credentials",
        &["omit", "same-origin", "include"],
    )? {
        Some(c) => c,
        None => from(|r| r.credentials.clone(), "same-origin")?,
    };
    let cache = match init_enum(
        ctx,
        init,
        "cache",
        &[
            "default",
            "no-store",
            "reload",
            "no-cache",
            "force-cache",
            "only-if-cached",
        ],
    )? {
        Some(c) => c,
        None => from(|r| r.cache.clone(), "default")?,
    };
    let mode = match init_enum(
        ctx,
        init,
        "mode",
        &["same-origin", "no-cors", "cors", "navigate"],
    )? {
        Some(m) if m == "navigate" => {
            return Err(js::type_error(ctx, "mode `navigate` is not allowed"));
        }
        Some(m) => m,
        None => from(|r| r.mode.clone(), "cors")?,
    };
    let referrer_policy = match init_enum(
        ctx,
        init,
        "referrerPolicy",
        &[
            "",
            "no-referrer",
            "no-referrer-when-downgrade",
            "same-origin",
            "origin",
            "strict-origin",
            "origin-when-cross-origin",
            "strict-origin-when-cross-origin",
            "unsafe-url",
        ],
    )? {
        Some(p) => p,
        None => from(|r| r.referrer_policy.clone(), "")?,
    };
    let priority = match init_enum(ctx, init, "priority", &["high", "low", "auto"])? {
        Some(p) => p,
        None => from(|r| r.priority.clone(), "auto")?,
    };
    let referrer = get("referrer")?;
    let referrer = match js::opt_string(ctx, referrer.raw())? {
        Some(r) if r.is_empty() => String::new(),
        Some(r) => match parse_url(ctx, &r) {
            Ok(u) if u.as_str() == "about:client" => "about:client".to_owned(),
            Ok(u) => u.to_string(),
            Err(_) => return Err(js::type_error(ctx, &format!("invalid referrer `{r}`"))),
        },
        None => from(|r| r.referrer.clone(), "about:client")?,
    };
    let integrity = get("integrity")?;
    let integrity = match js::opt_string(ctx, integrity.raw())? {
        Some(i) => i,
        None => from(|r| r.integrity.clone(), "")?,
    };
    let keepalive = get("keepalive")?;
    let keepalive = if keepalive.is_undefined() {
        match &base {
            Some(b) => request(ctx, b.raw())?.keepalive,
            None => false,
        }
    } else {
        js::to_bool(ctx, keepalive.raw())
    };
    // The signal follows the given one (or the input request's).
    let signal = get("signal")?;
    let source_signal = if !signal.is_undefined() {
        if signal.is_nullish() {
            None
        } else if super::event::is_signal(ctx, signal.raw()) {
            Some(signal)
        } else {
            return Err(js::type_error(ctx, "signal is not an AbortSignal"));
        }
    } else {
        match &base {
            Some(b) => Some(request(ctx, b.raw())?.signal.clone_ref()),
            None => None,
        }
    };
    let signal = match source_signal {
        Some(s) => {
            let g = js::global(ctx);
            let abort_signal = js::get(ctx, g.raw(), "AbortSignal")?;
            let list = js::new_array(ctx, vec![s])?;
            js::invoke(ctx, abort_signal.raw(), "any", &[list.raw()])?
        }
        None => super::event::new_signal(ctx)?,
    };
    // Headers: the init's, else the input's.
    let h = new_headers(ctx, Vec::new(), Guard::None)?;
    let init_headers = get("headers")?;
    if !init_headers.is_undefined() {
        fill_headers(ctx, h.raw(), init_headers.raw())?;
    } else if let Some(b) = &base {
        let bh = request(ctx, b.raw())?.headers.clone_ref();
        fill_headers(ctx, h.raw(), bh.raw())?;
    }
    // The body.
    let init_body = get("body")?;
    let has_input_body = match &base {
        Some(b) => !body(ctx, b.raw())?.is_null(),
        None => false,
    };
    let body_given = !init_body.is_nullish();
    if (body_given || (has_input_body && init_body.is_undefined()))
        && matches!(method.as_str(), "GET" | "HEAD")
    {
        return Err(js::type_error(
            ctx,
            "Request with GET/HEAD method cannot have body",
        ));
    }
    let body = if body_given {
        let form = blob::is_form_data(ctx, init_body.raw());
        let (b, mime) = extract_body(ctx, init_body.raw())?;
        if let Some(mime) = mime {
            let mut hd = headers(ctx, h.raw())?;
            if form {
                hd.set(ctx, "content-type", &mime)?;
            } else if !hd.has("content-type") {
                hd.append(ctx, "content-type", &mime)?;
            }
        }
        if !b.stream.is_undefined() {
            let duplex = get("duplex")?;
            if js::opt_string(ctx, duplex.raw())?.as_deref() != Some("half") {
                return Err(js::type_error(
                    ctx,
                    "a ReadableStream body needs duplex: \"half\"",
                ));
            }
        }
        b
    } else if init_body.is_undefined()
        && let Some(b) = &base
    {
        if body_used(ctx, b.raw())? {
            return Err(js::type_error(ctx, "the input request's body was used"));
        }
        // The input's body moves here (the input is used).
        let taken = std::mem::take(&mut *body(ctx, b.raw())?);
        let used = Body {
            bytes: None,
            stream: V::undefined(),
            used: !taken.is_null(),
        };
        *body(ctx, b.raw())? = used;
        taken
    } else {
        Body::null()
    };
    instance(
        ctx,
        new_target,
        Class::Request,
        RequestData {
            method,
            url,
            headers: h,
            body,
            signal,
            redirect,
            credentials,
            cache,
            mode,
            referrer,
            referrer_policy,
            integrity,
            keepalive,
            priority,
        },
    )
}

native!(
    fn request_constructor(ctx, this, args) {
        if args.len() == 0 {
            return Err(js::type_error(ctx, "Request needs an input"));
        }
        construct_request(ctx, this, args.get(0), args.get(1))
    }
);

macro_rules! request_string {
    ($name:ident, $field:ident) => {
        native!(
            fn $name(ctx, this, args) {
                let s = request(ctx, this)?.$field.clone();
                js::string(ctx, &s)
            }
        );
    };
}

request_string!(request_method, method);
request_string!(request_url, url);
request_string!(request_redirect, redirect);
request_string!(request_credentials, credentials);
request_string!(request_cache, cache);
request_string!(request_mode, mode);
request_string!(request_referrer, referrer);
request_string!(request_referrer_policy, referrer_policy);
request_string!(request_integrity, integrity);

native!(
    fn request_headers(ctx, this, args) {
        Ok(request(ctx, this)?.headers.clone_ref())
    }
);

native!(
    fn request_signal(ctx, this, args) {
        Ok(request(ctx, this)?.signal.clone_ref())
    }
);

native!(
    fn request_keepalive(ctx, this, args) {
        Ok(V::bool(request(ctx, this)?.keepalive))
    }
);

native!(
    fn request_destination(ctx, this, args) {
        request(ctx, this)?;
        js::string(ctx, "")
    }
);

native!(
    fn request_duplex(ctx, this, args) {
        request(ctx, this)?;
        js::string(ctx, "half")
    }
);

native!(
    fn request_false(ctx, this, args) {
        request(ctx, this)?;
        Ok(V::bool(false))
    }
);

native!(
    fn request_clone(ctx, this, args) {
        let b = clone_body(ctx, this)?;
        let h = request(ctx, this)?.headers.clone_ref();
        let list = headers(ctx, h.raw())?.list.clone();
        let headers = new_headers(ctx, list, Guard::None)?;
        let signal = request(ctx, this)?.signal.clone_ref();
        let g = js::global(ctx);
        let abort_signal = js::get(ctx, g.raw(), "AbortSignal")?;
        let list = js::new_array(ctx, vec![signal])?;
        let signal = js::invoke(ctx, abort_signal.raw(), "any", &[list.raw()])?;
        let data = {
            let r = request(ctx, this)?;
            RequestData {
                method: r.method.clone(),
                url: r.url.clone(),
                headers,
                body: b,
                signal,
                redirect: r.redirect.clone(),
                credentials: r.credentials.clone(),
                cache: r.cache.clone(),
                mode: r.mode.clone(),
                referrer: r.referrer.clone(),
                referrer_policy: r.referrer_policy.clone(),
                integrity: r.integrity.clone(),
                keepalive: r.keepalive,
                priority: r.priority.clone(),
            }
        };
        instance(ctx, js::UNDEFINED, Class::Request, data)
    }
);

// --- Response -------------------------------------------------------------

pub(crate) struct ResponseData {
    kind: &'static str,
    url: String,
    redirected: bool,
    status: u16,
    status_text: String,
    headers: V,
    body: Body,
}

trace!(ResponseData { headers, body });

impl Object for ResponseData {
    fn body(&mut self) -> Option<&mut Body> {
        Some(&mut self.body)
    }
}

fn response<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, ResponseData>> {
    data::<ResponseData>(ctx, v, "Response")
}

fn is_null_body_status(status: u16) -> bool {
    matches!(status, 101 | 103 | 204 | 205 | 304)
}

fn is_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// A reason phrase: no line breaks.
fn valid_status_text(s: &str) -> bool {
    s.bytes()
        .all(|b| b == b'\t' || (0x20..=0x7E).contains(&b) || b >= 0x80)
}

/// `initialize a response` with `init` and `body`.
fn construct_response(ctx: Ctx, new_target: Raw, body_init: Raw, init: Raw) -> R<V> {
    if !js::is_nullish(init) && !js::is_object(init) {
        return Err(js::type_error(ctx, "ResponseInit must be an object"));
    }
    let get = |key: &str| -> R<V> {
        if js::is_object(init) {
            js::get(ctx, init, key)
        } else {
            Ok(V::undefined())
        }
    };
    let status = get("status")?;
    let status = if status.is_undefined() {
        200.0
    } else {
        js::to_number(ctx, status.raw())?.trunc()
    };
    if !(200.0..=599.0).contains(&status) {
        return Err(js::range_error(ctx, &format!("Invalid status: {status}")));
    }
    let status = status as u16;
    let status_text = get("statusText")?;
    let status_text = js::opt_string(ctx, status_text.raw())?.unwrap_or_default();
    if !valid_status_text(&status_text) {
        return Err(js::type_error(ctx, "invalid statusText"));
    }
    let h = new_headers(ctx, Vec::new(), Guard::None)?;
    let init_headers = get("headers")?;
    fill_headers(ctx, h.raw(), init_headers.raw())?;
    let body = if js::is_nullish(body_init) {
        Body::null()
    } else {
        if is_null_body_status(status) {
            return Err(js::type_error(
                ctx,
                &format!("a {status} response cannot have a body"),
            ));
        }
        let (b, mime) = extract_body(ctx, body_init)?;
        if let Some(mime) = mime
            && !headers(ctx, h.raw())?.has("content-type")
        {
            headers(ctx, h.raw())?.append(ctx, "content-type", &mime)?;
        }
        b
    };
    instance(
        ctx,
        new_target,
        Class::Response,
        ResponseData {
            kind: "default",
            url: String::new(),
            redirected: false,
            status,
            status_text,
            headers: h,
            body,
        },
    )
}

native!(
    fn response_constructor(ctx, this, args) {
        construct_response(ctx, this, args.get(0), args.get(1))
    }
);

native!(
    fn response_error(ctx, this, args) {
        let h = new_headers(ctx, Vec::new(), Guard::Immutable)?;
        instance(
            ctx,
            js::UNDEFINED,
            Class::Response,
            ResponseData {
                kind: "error",
                url: String::new(),
                redirected: false,
                status: 0,
                status_text: String::new(),
                headers: h,
                body: Body::null(),
            },
        )
    }
);

native!(
    fn response_redirect(ctx, this, args) {
        let url = js::to_string(ctx, args.get(0))?;
        let url = parse_url(ctx, &url).map_err(|_| js::type_error(ctx, &format!("Invalid URL: {url}")))?;
        let status = if js::is_undefined(args.get(1)) { 302 } else { js::to_number(ctx, args.get(1))? as u16 };
        if !is_redirect_status(status) {
            return Err(js::range_error(ctx, &format!("Invalid redirect status: {status}")));
        }
        let h = new_headers(ctx, vec![("location".to_owned(), url.to_string())], Guard::Immutable)?;
        instance(
            ctx,
            js::UNDEFINED,
            Class::Response,
            ResponseData {
                kind: "default",
                url: String::new(),
                redirected: false,
                status,
                status_text: String::new(),
                headers: h,
                body: Body::null(),
            },
        )
    }
);

native!(
    fn response_json(ctx, this, args) {
        let g = js::global(ctx);
        let json = js::get(ctx, g.raw(), "JSON")?;
        let text = js::invoke(ctx, json.raw(), "stringify", &[args.get(0)])?;
        if js::is_undefined(text.raw()) {
            return Err(js::type_error(ctx, "the value cannot be serialized to JSON"));
        }
        let r = construct_response(ctx, js::UNDEFINED, text.raw(), args.get(1))?;
        let h = response(ctx, r.raw())?.headers.clone_ref();
        let mut hd = headers(ctx, h.raw())?;
        // `Response.json` sets its own content type over the string's.
        let explicit = js::is_object(args.get(1)) && {
            drop(hd);
            let ih = js::get(ctx, args.get(1), "headers")?;
            let given = !ih.is_undefined() && {
                let probe = new_headers(ctx, Vec::new(), Guard::None)?;
                fill_headers(ctx, probe.raw(), ih.raw())?;
                headers(ctx, probe.raw())?.has("content-type")
            };
            hd = headers(ctx, h.raw())?;
            given
        };
        if !explicit {
            hd.list.retain(|(n, _)| n != "content-type");
            hd.list.push(("content-type".to_owned(), "application/json".to_owned()));
        }
        drop(hd);
        Ok(r)
    }
);

macro_rules! response_getter {
    ($name:ident, |$ctx:ident, $r:ident| $body:expr) => {
        native!(
            fn $name($ctx, this, args) {
                let $r = response($ctx, this)?;
                $body
            }
        );
    };
}

response_getter!(response_type, |ctx, r| {
    let kind = r.kind;
    drop(r);
    js::string(ctx, kind)
});
response_getter!(response_url, |ctx, r| {
    let url = r.url.clone();
    drop(r);
    js::string(ctx, &url)
});
response_getter!(response_redirected, |ctx, r| Ok(V::bool(r.redirected)));
response_getter!(response_status, |ctx, r| Ok(V::num(f64::from(r.status))));
response_getter!(response_ok, |ctx, r| Ok(V::bool(
    (200..300).contains(&r.status)
)));
response_getter!(response_status_text, |ctx, r| {
    let t = r.status_text.clone();
    drop(r);
    js::string(ctx, &t)
});
response_getter!(response_headers, |ctx, r| Ok(r.headers.clone_ref()));

native!(
    fn response_clone(ctx, this, args) {
        let b = clone_body(ctx, this)?;
        let (h, kind, url, redirected, status, status_text) = {
            let r = response(ctx, this)?;
            (r.headers.clone_ref(), r.kind, r.url.clone(), r.redirected, r.status, r.status_text.clone())
        };
        let (list, guard) = {
            let hd = headers(ctx, h.raw())?;
            (hd.list.clone(), hd.guard)
        };
        let headers = new_headers(ctx, list, guard)?;
        instance(
            ctx,
            js::UNDEFINED,
            Class::Response,
            ResponseData {
                kind,
                url,
                redirected,
                status,
                status_text,
                headers,
                body: b,
            },
        )
    }
);

// --- fetch ----------------------------------------------------------------

/// A request in flight (per context, by id).
pub(crate) struct Inflight {
    /// Until the response head: the promise `fetch` returned.
    deferred: Option<Deferred>,
    /// The response body's controller: weakly until someone reads (an
    /// abandoned response is collected and its request canceled), then
    /// strongly.
    weak_controller: V,
    controller: V,
    /// The request body's reader (a streamed body).
    body_reader: V,
    signal: V,
    abort_algorithm: u32,
    /// The response has no body (HEAD, 204, ...).
    null_body: bool,
    redirect_error: bool,
    paused: bool,
    /// Set when the request completed (the guard then does nothing).
    done: Rc<Cell<bool>>,
}

trace!(Inflight {
    deferred,
    weak_controller,
    controller,
    body_reader,
    signal
});

/// Cancels the request when the body stream is collected unfinished.
struct DropGuard {
    id: RequestId,
    shared: Rc<crate::host::Shared>,
    done: Rc<Cell<bool>>,
}

impl Drop for DropGuard {
    fn drop(&mut self) {
        if !self.done.get()
            && let Ok(mut dropped) = self.shared.dropped_fetches.try_borrow_mut()
        {
            dropped.push(self.id);
        }
    }
}

fn push_op(ctx: Ctx, id: RequestId, op: FetchOp) {
    crate::dom::shared(ctx).net.borrow_mut().ops.push((id, op));
}

fn next_id(ctx: Ctx) -> RequestId {
    let w = web(ctx);
    w.next_fetch += 1;
    w.next_fetch
}

/// Cancels and forgets the requests whose bodies were collected unread.
pub(crate) fn forget_dropped(ctx: Ctx) {
    let shared = crate::dom::shared(ctx);
    let ids = std::mem::take(&mut *shared.dropped_fetches.borrow_mut());
    if ids.is_empty() {
        return;
    }
    for id in &ids {
        shared.net.borrow_mut().ops.push((*id, FetchOp::Cancel));
    }
    let gone: Vec<Inflight> = ids
        .iter()
        .filter_map(|id| web(ctx).fetches.remove(id))
        .collect();
    drop(gone);
}

/// The data: URL `url` as a response (`None`: not a valid one).
fn data_url(url: &str) -> Option<(Vec<u8>, String)> {
    let rest = url.strip_prefix("data:")?;
    let rest = rest.split('#').next().unwrap_or(rest);
    let (meta, data) = rest.split_once(',')?;
    let mut mime = meta.trim().to_owned();
    let bytes: Vec<u8> = percent_encoding::percent_decode_str(data).collect();
    let base64 = mime
        .to_ascii_lowercase()
        .trim_end()
        .strip_suffix("base64")
        .is_some_and(|m| m.trim_end().ends_with(';'));
    let bytes = if base64 {
        let cut = mime
            .to_ascii_lowercase()
            .rfind("base64")
            .unwrap_or(mime.len());
        mime.truncate(cut);
        let m = mime.trim_end().trim_end_matches(';').trim_end().to_owned();
        mime = m;
        forgiving_base64(&bytes)?
    } else {
        bytes
    };
    if mime.starts_with(';') {
        mime = format!("text/plain{mime}");
    }
    if mime.is_empty() || !mime.contains('/') {
        mime = "text/plain;charset=US-ASCII".to_owned();
    }
    Some((bytes, mime))
}

/// The infra standard's forgiving-base64 decode.
fn forgiving_base64(input: &[u8]) -> Option<Vec<u8>> {
    let mut data: Vec<u8> = input
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if data.len().is_multiple_of(4) {
        if data.ends_with(b"==") {
            data.truncate(data.len() - 2);
        } else if data.ends_with(b"=") {
            data.truncate(data.len() - 1);
        }
    }
    if data.len() % 4 == 1 {
        return None;
    }
    let value = |b: u8| -> Option<u32> {
        Some(match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for b in data {
        buffer = (buffer << 6) | value(b)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// A local response (data: and blob: URLs).
fn local_response(ctx: Ctx, url: &str, bytes: Vec<u8>, mime: &str) -> R<V> {
    let len = bytes.len().to_string();
    let mut list = vec![("content-type".to_owned(), mime.to_owned())];
    if url.starts_with("blob:") {
        list.push(("content-length".to_owned(), len));
    }
    let h = new_headers(ctx, list, Guard::Immutable)?;
    instance(
        ctx,
        js::UNDEFINED,
        Class::Response,
        ResponseData {
            kind: "basic",
            url: url.to_owned(),
            redirected: false,
            status: 200,
            status_text: "OK".to_owned(),
            headers: h,
            body: Body::bytes(bytes),
        },
    )
}

fn network_error(ctx: Ctx, message: &str) -> V {
    js::type_error_value(ctx, &format!("Network request failed: {message}"))
}

native!(
    fn fetch(ctx, this, args) {
        js::promise_of(ctx, || start_fetch(ctx, args.get(0), args.get(1)))
    }
);

/// The `fetch(input, init)` steps: a promise of the response.
fn start_fetch(ctx: Ctx, input: Raw, init: Raw) -> R<V> {
    forget_dropped(ctx);
    let req = construct_request(ctx, js::UNDEFINED, input, init)?;
    let signal = request(ctx, req.raw())?.signal.clone_ref();
    if super::event::aborted(ctx, signal.raw())? {
        let reason = super::event::reason(ctx, signal.raw())?;
        return js::rejected(ctx, reason.raw());
    }
    let (method, url, redirect) = {
        let r = request(ctx, req.raw())?;
        (r.method.clone(), r.url.clone(), r.redirect.clone())
    };
    let scheme = url
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match scheme.as_str() {
        "data" => {
            let Some((bytes, mime)) = data_url(&url) else {
                return js::rejected(ctx, network_error(ctx, "invalid data: URL").raw());
            };
            let r = local_response(ctx, &url, bytes, &mime)?;
            return js::resolved(ctx, r.raw());
        }
        "blob" => {
            let Some(b) = (method == "GET")
                .then(|| blob::resolve_blob_url(ctx, &url))
                .flatten()
            else {
                return js::rejected(ctx, network_error(ctx, "no such blob").raw());
            };
            let mime = b.mime.clone();
            let r = local_response(ctx, &url, b.bytes().to_vec(), &mime)?;
            return js::resolved(ctx, r.raw());
        }
        "http" | "https" => {}
        other => {
            return js::rejected(
                ctx,
                network_error(ctx, &format!("unsupported URL scheme `{other}`")).raw(),
            );
        }
    }
    // The request as the stack sends it.
    let (mut list, body_bytes, body_stream) = {
        let r = request(ctx, req.raw())?;
        let h = r.headers.clone_ref();
        let (bytes, stream) = (r.body.bytes.clone(), r.body.stream.clone_ref());
        drop(r);
        (headers(ctx, h.raw())?.list.clone(), bytes, stream)
    };
    if !list.iter().any(|(n, _)| n == "accept") {
        list.push(("accept".to_owned(), "*/*".to_owned()));
    }
    let http_body = if !body_stream.is_undefined() {
        if readable::is_disturbed(ctx, body_stream.raw())?
            || readable::is_locked(ctx, body_stream.raw())?
        {
            return Err(js::type_error(ctx, "the request body was used"));
        }
        HttpBody::Stream
    } else {
        match body_bytes {
            Some(b) if !b.is_empty() => HttpBody::Bytes(b.to_vec()),
            _ => HttpBody::Empty,
        }
    };
    let streamed = http_body == HttpBody::Stream;
    let id = next_id(ctx);
    let deferred = Deferred::new(ctx)?;
    let promise = deferred.promise.clone_ref();
    let body_reader = if streamed {
        readable::acquire_default_reader(ctx, body_stream.raw())?
    } else {
        V::undefined()
    };
    // The request's body is used.
    body(ctx, req.raw())?.used = true;
    let abort = js::closure(ctx, vec![signal.clone_ref()], move |ctx, c, _| {
        let reason = super::event::reason(ctx, c[0].raw())?;
        abort_fetch(ctx, id, reason.raw())?;
        Ok(V::undefined())
    })?;
    let abort_algorithm = super::event::add_algorithm(ctx, signal.raw(), abort)?;
    web(ctx).fetches.insert(
        id,
        Inflight {
            deferred: Some(deferred),
            weak_controller: V::undefined(),
            controller: V::undefined(),
            body_reader,
            signal,
            abort_algorithm,
            null_body: method == "HEAD",
            redirect_error: redirect == "error",
            paused: false,
            done: Rc::default(),
        },
    );
    let url = url.split('#').next().unwrap_or(&url).to_owned();
    push_op(
        ctx,
        id,
        FetchOp::Start(HttpRequest {
            method,
            url,
            headers: list,
            body: http_body,
            follow_redirects: redirect == "follow",
        }),
    );
    Ok(promise)
}

/// The request was aborted with `reason`.
fn abort_fetch(ctx: Ctx, id: RequestId, reason: Raw) -> R<()> {
    let Some(entry) = web(ctx).fetches.remove(&id) else {
        return Ok(());
    };
    entry.done.set(true);
    // Not handed to the network yet: dropped from the queue.
    {
        let shared = crate::dom::shared(ctx);
        let mut net = shared.net.borrow_mut();
        let queued = net
            .ops
            .iter()
            .any(|(r, op)| *r == id && matches!(op, FetchOp::Start(_)));
        if queued {
            net.ops.retain(|(r, _)| *r != id);
        } else {
            net.ops.push((id, FetchOp::Cancel));
        }
    }
    if let Some(d) = &entry.deferred {
        d.reject(ctx, reason);
    }
    if let Some(c) = controller(ctx, &entry)? {
        readable::error_controller(ctx, c.raw(), reason)?;
    }
    if !entry.body_reader.is_undefined() {
        let s = readable::reader_stream_of(ctx, entry.body_reader.raw())?;
        if let Some(s) = s {
            let _ = readable::cancel_stream(ctx, s.raw(), reason);
        }
    }
    Ok(())
}

/// The body controller of `entry`, if it is alive.
fn controller(ctx: Ctx, entry: &Inflight) -> R<Option<V>> {
    if !entry.controller.is_undefined() {
        return Ok(Some(entry.controller.clone_ref()));
    }
    if entry.weak_controller.is_undefined() {
        return Ok(None);
    }
    let c = js::invoke(ctx, entry.weak_controller.raw(), "deref", &[])?;
    Ok((!c.is_undefined()).then_some(c))
}

/// Delivers `event` of request `id`.
pub(crate) fn event(ctx: Ctx, id: RequestId, event: FetchEvent) -> R<()> {
    forget_dropped(ctx);
    if !web(ctx).fetches.contains_key(&id) {
        return Ok(());
    }
    match event {
        FetchEvent::BodyWanted => send_body_chunk(ctx, id),
        FetchEvent::Head(head) => on_head(ctx, id, head),
        FetchEvent::Data(bytes) => on_data(ctx, id, &bytes),
        FetchEvent::End => {
            let Some(entry) = finish(ctx, id)? else {
                return Ok(());
            };
            if let Some(c) = controller(ctx, &entry)? {
                readable::close_controller(ctx, c.raw())?;
            }
            Ok(())
        }
        FetchEvent::Failed(message) => {
            let Some(entry) = finish(ctx, id)? else {
                return Ok(());
            };
            let e = network_error(ctx, &message);
            if let Some(d) = &entry.deferred {
                d.reject(ctx, e.raw());
            }
            if let Some(c) = controller(ctx, &entry)? {
                readable::error_controller(ctx, c.raw(), e.raw())?;
            }
            Ok(())
        }
    }
}

/// Removes request `id` (completed): its entry.
fn finish(ctx: Ctx, id: RequestId) -> R<Option<Inflight>> {
    let Some(entry) = web(ctx).fetches.remove(&id) else {
        return Ok(None);
    };
    entry.done.set(true);
    super::event::remove_algorithm(ctx, entry.signal.raw(), entry.abort_algorithm)?;
    Ok(Some(entry))
}

fn on_head(ctx: Ctx, id: RequestId, head: HttpHead) -> R<()> {
    let (deferred, null_body, redirect_error, done) = {
        let Some(e) = web(ctx).fetches.get_mut(&id) else {
            return Ok(());
        };
        (
            e.deferred.take(),
            e.null_body,
            e.redirect_error,
            e.done.clone(),
        )
    };
    let Some(deferred) = deferred else {
        return Ok(());
    };
    if redirect_error && is_redirect_status(head.status) {
        let e = network_error(ctx, "the response was a redirect and redirect is \"error\"");
        finish(ctx, id)?;
        push_op(ctx, id, FetchOp::Cancel);
        deferred.reject(ctx, e.raw());
        return Ok(());
    }
    let list = head
        .headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
        .collect();
    let h = new_headers(ctx, list, Guard::Immutable)?;
    let body = if null_body || is_null_body_status(head.status) {
        Body::null()
    } else {
        // The body's stream: filled as chunks arrive; pulls resume a
        // paused request.
        let guard = Rc::new(DropGuard {
            id,
            shared: crate::dom::shared(ctx),
            done,
        });
        let g = guard.clone();
        let pull = js::closure(ctx, Vec::new(), move |ctx, _, args| {
            let _ = &g;
            on_pull(ctx, id, args.get(0))?;
            Ok(V::undefined())
        })?;
        let cancel = js::closure(ctx, Vec::new(), move |ctx, _, _| {
            let _ = &guard;
            if let Some(entry) = finish(ctx, id)? {
                drop(entry);
                push_op(ctx, id, FetchOp::Cancel);
            }
            Ok(V::undefined())
        })?;
        let (stream, controller) =
            readable::new_byte_stream(ctx, Algo::native(pull), Algo::native(cancel), 0.0)?;
        let g = js::global(ctx);
        let weak_ref = js::get(ctx, g.raw(), "WeakRef")?;
        let weak = js::construct(ctx, weak_ref.raw(), &[controller.raw()])?;
        if let Some(e) = web(ctx).fetches.get_mut(&id) {
            e.weak_controller = weak;
        }
        Body {
            bytes: None,
            stream,
            used: false,
        }
    };
    let r = instance(
        ctx,
        js::UNDEFINED,
        Class::Response,
        ResponseData {
            kind: "basic",
            url: head.url,
            redirected: head.redirected,
            status: head.status,
            status_text: head.status_text,
            headers: h,
            body,
        },
    )?;
    deferred.resolve(ctx, r.raw());
    Ok(())
}

/// Someone reads the body: hold it strongly, and go on reading.
fn on_pull(ctx: Ctx, id: RequestId, c: Raw) -> R<()> {
    let resume = {
        let Some(e) = web(ctx).fetches.get_mut(&id) else {
            return Ok(());
        };
        if e.controller.is_undefined() {
            e.controller = V::dup(ctx, c);
        }
        std::mem::replace(&mut e.paused, false)
    };
    if resume {
        push_op(ctx, id, FetchOp::Pause(false));
    }
    Ok(())
}

fn on_data(ctx: Ctx, id: RequestId, bytes: &[u8]) -> R<()> {
    let entry_controller = {
        let Some(e) = web(ctx).fetches.get(&id) else {
            return Ok(());
        };
        if e.null_body {
            return Ok(());
        }
        controller(ctx, e)?
    };
    let Some(c) = entry_controller else {
        // The response was collected unread.
        if finish(ctx, id)?.is_some() {
            push_op(ctx, id, FetchOp::Cancel);
        }
        return Ok(());
    };
    readable::enqueue_bytes(ctx, c.raw(), bytes)?;
    // Too much waits unread: pause until the next pull.
    let queued = -readable::desired_size(ctx, c.raw())?.unwrap_or(0.0);
    if queued > QUEUE_LIMIT
        && let Some(e) = web(ctx).fetches.get_mut(&id)
        && !e.paused
    {
        e.paused = true;
        push_op(ctx, id, FetchOp::Pause(true));
    }
    Ok(())
}

/// The stack wants the next chunk of a streamed request body.
fn send_body_chunk(ctx: Ctx, id: RequestId) -> R<()> {
    let reader = match web(ctx).fetches.get(&id) {
        Some(e) if !e.body_reader.is_undefined() => e.body_reader.clone_ref(),
        _ => return Ok(()),
    };
    let request = readable::ReadRequest::steps(
        ctx,
        Vec::new(),
        move |ctx, _, chunk| {
            // SAFETY: copied before any JS runs.
            let bytes = (js::view(ctx, chunk).is_some())
                .then(|| unsafe { js::bytes_of(ctx, chunk) }.map(<[u8]>::to_vec))
                .flatten();
            match bytes {
                Some(b) => push_op(ctx, id, FetchOp::Send(b)),
                None => {
                    let e = js::type_error_value(ctx, "a request body chunk must be a Uint8Array");
                    fail_request_body(ctx, id, e.raw())?;
                }
            }
            Ok(())
        },
        move |ctx, _, _| {
            push_op(ctx, id, FetchOp::Finish);
            Ok(())
        },
        move |ctx, _, e| fail_request_body(ctx, id, e),
    )?;
    readable::default_reader_read(ctx, reader.raw(), request)
}

/// The request body failed: the request fails with `e`.
fn fail_request_body(ctx: Ctx, id: RequestId, e: Raw) -> R<()> {
    let Some(entry) = finish(ctx, id)? else {
        return Ok(());
    };
    push_op(ctx, id, FetchOp::Cancel);
    if let Some(d) = &entry.deferred {
        d.reject(ctx, e);
    }
    Ok(())
}

pub(crate) const SPECS: &[Spec] = &[
    Spec {
        class: Class::Headers,
        name: c"Headers",
        parent: None,
        constructor: Some((headers_constructor, 0)),
        methods: &[
            (Key::Name(c"append"), headers_append, 2),
            (Key::Name(c"delete"), headers_delete, 1),
            (Key::Name(c"get"), headers_get, 1),
            (Key::Name(c"getSetCookie"), headers_get_set_cookie, 0),
            (Key::Name(c"has"), headers_has, 1),
            (Key::Name(c"set"), headers_set, 2),
            (Key::Name(c"forEach"), headers_for_each, 1),
            (Key::Name(c"entries"), headers_entries, 0),
            (Key::Name(c"keys"), headers_keys, 0),
            (Key::Name(c"values"), headers_values, 0),
            (Key::Iterator, headers_entries, 0),
        ],
        accessors: &[],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"constructor(init?: HeadersInit);
append(name: string, value: string): void;
delete(name: string): void;
get(name: string): string | null;
getSetCookie(): string[];
has(name: string): boolean;
set(name: string, value: string): void;
forEach(callback: (value: string, key: string, parent: Headers) => void, thisArg?: unknown): void;
entries(): IterableIterator<[string, string]>;
keys(): IterableIterator<string>;
values(): IterableIterator<string>;
[Symbol.iterator](): IterableIterator<[string, string]>;"#,
        },
    },
    Spec {
        class: Class::Request,
        name: c"Request",
        parent: None,
        constructor: Some((request_constructor, 1)),
        methods: &[
            (Key::Name(c"clone"), request_clone, 0),
            (Key::Name(c"arrayBuffer"), body_array_buffer, 0),
            (Key::Name(c"blob"), body_blob, 0),
            (Key::Name(c"bytes"), body_bytes, 0),
            (Key::Name(c"formData"), body_form_data, 0),
            (Key::Name(c"json"), body_json, 0),
            (Key::Name(c"text"), body_text, 0),
        ],
        accessors: &[
            (c"method", request_method, None),
            (c"url", request_url, None),
            (c"headers", request_headers, None),
            (c"destination", request_destination, None),
            (c"referrer", request_referrer, None),
            (c"referrerPolicy", request_referrer_policy, None),
            (c"mode", request_mode, None),
            (c"credentials", request_credentials, None),
            (c"cache", request_cache, None),
            (c"redirect", request_redirect, None),
            (c"integrity", request_integrity, None),
            (c"keepalive", request_keepalive, None),
            (c"isReloadNavigation", request_false, None),
            (c"isHistoryNavigation", request_false, None),
            (c"signal", request_signal, None),
            (c"duplex", request_duplex, None),
            (c"body", body_get, None),
            (c"bodyUsed", body_used_get, None),
        ],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: " implements Body",
            members: r#"constructor(input: string | URL | Request, init?: RequestInit);
readonly method: string;
readonly url: string;
readonly headers: Headers;
readonly destination: string;
readonly referrer: string;
readonly referrerPolicy: string;
readonly mode: string;
readonly credentials: string;
readonly cache: string;
readonly redirect: "follow" | "manual" | "error";
readonly integrity: string;
readonly keepalive: boolean;
readonly isReloadNavigation: boolean;
readonly isHistoryNavigation: boolean;
readonly signal: AbortSignal;
readonly duplex: "half";
readonly body: ReadableStream<Uint8Array> | null;
readonly bodyUsed: boolean;
arrayBuffer(): Promise<ArrayBuffer>;
blob(): Promise<Blob>;
bytes(): Promise<Uint8Array>;
formData(): Promise<FormData>;
json(): Promise<any>;
text(): Promise<string>;
clone(): Request;"#,
        },
    },
    Spec {
        class: Class::Response,
        name: c"Response",
        parent: None,
        constructor: Some((response_constructor, 0)),
        methods: &[
            (Key::Name(c"clone"), response_clone, 0),
            (Key::Name(c"arrayBuffer"), body_array_buffer, 0),
            (Key::Name(c"blob"), body_blob, 0),
            (Key::Name(c"bytes"), body_bytes, 0),
            (Key::Name(c"formData"), body_form_data, 0),
            (Key::Name(c"json"), body_json, 0),
            (Key::Name(c"text"), body_text, 0),
        ],
        accessors: &[
            (c"type", response_type, None),
            (c"url", response_url, None),
            (c"redirected", response_redirected, None),
            (c"status", response_status, None),
            (c"ok", response_ok, None),
            (c"statusText", response_status_text, None),
            (c"headers", response_headers, None),
            (c"body", body_get, None),
            (c"bodyUsed", body_used_get, None),
        ],
        statics: &[
            (c"error", response_error, 0),
            (c"redirect", response_redirect, 1),
            (c"json", response_json, 1),
        ],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: " implements Body",
            members: r#"constructor(body?: BodyInit | null, init?: ResponseInit);
static json(data: unknown, init?: ResponseInit): Response;
static error(): Response;
static redirect(url: string | URL, status?: number): Response;
readonly type: "basic" | "default" | "error";
readonly url: string;
readonly redirected: boolean;
readonly status: number;
readonly ok: boolean;
readonly statusText: string;
readonly headers: Headers;
readonly body: ReadableStream<Uint8Array> | null;
readonly bodyUsed: boolean;
arrayBuffer(): Promise<ArrayBuffer>;
blob(): Promise<Blob>;
bytes(): Promise<Uint8Array>;
formData(): Promise<FormData>;
json(): Promise<any>;
text(): Promise<string>;
clone(): Response;"#,
        },
    },
];

/// The global `fetch`.
pub(crate) const FUNCTIONS: &[super::Function] = &[super::Function {
    name: c"fetch",
    native: fetch,
    length: 1,
    doc: "HTTP through the platform's network stack (NSURLSession,\nHttpURLConnection), streaming both ways; `data:` and `blob:` URLs are\nanswered locally.",
    signature: "(input: string | URL | Request, init?: RequestInit): Promise<Response>",
}];

/// The TypeScript types of this file's classes (`globals.d.ts`).
pub(crate) const TYPES: &str = r#"type HeadersInit = Headers | Record<string, string> | Iterable<[string, string]>;

type BodyInit = ReadableStream<Uint8Array> | Blob | BufferSource | FormData | URLSearchParams | string;

interface Body {
  /** The body as it arrives (a byte stream), or null. */
  readonly body: ReadableStream<Uint8Array> | null;
  readonly bodyUsed: boolean;
  arrayBuffer(): Promise<ArrayBuffer>;
  blob(): Promise<Blob>;
  bytes(): Promise<Uint8Array>;
  formData(): Promise<FormData>;
  json(): Promise<any>;
  text(): Promise<string>;
}

interface RequestInit {
  method?: string;
  headers?: HeadersInit;
  /** A ReadableStream is sent as it is read (chunked), with `duplex: "half"`. */
  body?: BodyInit | null;
  duplex?: "half";
  signal?: AbortSignal | null;
  /** "manual" gives the 3xx response itself. */
  redirect?: "follow" | "manual" | "error";
  credentials?: "omit" | "same-origin" | "include";
  cache?: "default" | "no-store" | "reload" | "no-cache" | "force-cache" | "only-if-cached";
  mode?: "same-origin" | "no-cors" | "cors";
  referrer?: string;
  referrerPolicy?: string;
  integrity?: string;
  keepalive?: boolean;
  priority?: "high" | "low" | "auto";
}

interface ResponseInit {
  status?: number;
  statusText?: string;
  headers?: HeadersInit;
}
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn data_urls_and_base64() {
        assert_eq!(super::forgiving_base64(b"aGk=").unwrap(), b"hi");
        assert_eq!(super::forgiving_base64(b" aG k ").unwrap(), b"hi");
        assert!(super::forgiving_base64(b"a").is_none());
        let (b, m) = super::data_url("data:text/plain;base64,aGk=").unwrap();
        assert_eq!((b.as_slice(), m.as_str()), (&b"hi"[..], "text/plain"));
        let (b, m) = super::data_url("data:,a%20b").unwrap();
        assert_eq!(
            (b.as_slice(), m.as_str()),
            (&b"a b"[..], "text/plain;charset=US-ASCII")
        );
        let (_, m) = super::data_url("data:;charset=utf-8,x").unwrap();
        assert_eq!(m, "text/plain;charset=utf-8");
    }
}
