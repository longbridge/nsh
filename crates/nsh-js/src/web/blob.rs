//! `Blob`, `File` (File API), `FormData` (XHR Standard) and blob URLs
//! (`URL.createObjectURL`). A blob's bytes are shared: `slice` makes a view
//! of the same bytes. `FormData` is also encoded as and parsed from
//! `multipart/form-data` here (for `fetch` bodies).

use std::{
    hash::{BuildHasher, Hasher},
    rc::Rc,
};

use super::{
    Class, Key, Object, Spec, data, instance,
    iterator::{self, Part},
    js::{self, Ctx, R, Raw, V, native, trace},
    role,
};

/// A blob's bytes and type.
#[derive(Clone)]
pub(crate) struct Blob {
    bytes: Rc<[u8]>,
    start: usize,
    end: usize,
    pub(crate) mime: String,
}

trace!(Blob {});

impl Object for Blob {
    fn blob(&mut self) -> Option<&mut Blob> {
        Some(self)
    }
}

impl Blob {
    pub(crate) fn new(bytes: Vec<u8>, mime: String) -> Blob {
        let end = bytes.len();
        Blob {
            bytes: bytes.into(),
            start: 0,
            end,
            mime,
        }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes[self.start..self.end]
    }

    fn len(&self) -> usize {
        self.end - self.start
    }

    /// `slice(start, end)` with relative (negative) positions.
    fn slice(&self, start: Option<f64>, end: Option<f64>, mime: String) -> Blob {
        let len = self.len() as f64;
        let clamp = |x: Option<f64>, default: f64| {
            let x = x.unwrap_or(default);
            let x = if x.is_nan() { 0.0 } else { x.trunc() };
            if x < 0.0 {
                (len + x).max(0.0)
            } else {
                x.min(len)
            }
        };
        let (s, e) = (clamp(start, 0.0), clamp(end, len));
        let (s, e) = (s as usize, (e as usize).max(s as usize));
        Blob {
            bytes: self.bytes.clone(),
            start: self.start + s,
            end: self.start + e,
            mime,
        }
    }
}

/// A file: a blob with a name.
pub(crate) struct File {
    pub(crate) blob: Blob,
    pub(crate) name: String,
    last_modified: f64,
}

trace!(File {});

impl Object for File {
    fn blob(&mut self) -> Option<&mut Blob> {
        Some(&mut self.blob)
    }
}

/// The blob part of `v` (a `Blob` or a `File`).
pub(crate) fn blob_of(ctx: Ctx, v: Raw) -> R<Option<Blob>> {
    if !is_blob(ctx, v) {
        return Ok(None);
    }
    Ok(Some(role(ctx, v, "Blob", |o| o.blob())?.clone()))
}

/// Whether `v` is a `Blob` (or a `File`).
pub(crate) fn is_blob(ctx: Ctx, v: Raw) -> bool {
    super::is::<Blob>(ctx, v) || super::is::<File>(ctx, v)
}

/// A type as `Blob` keeps it: lower case, empty unless printable ASCII.
fn normalize_type(t: &str) -> String {
    if t.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        t.to_ascii_lowercase()
    } else {
        String::new()
    }
}

/// The bytes of a `BlobPart` sequence (`endings: "native"` turns line
/// breaks into `\n`, the native one here).
fn blob_parts(ctx: Ctx, parts: Raw, native_endings: bool) -> R<Vec<u8>> {
    let mut out = Vec::new();
    if js::is_undefined(parts) {
        return Ok(out);
    }
    if !js::is_object(parts) {
        return Err(js::type_error(ctx, "blobParts must be a sequence"));
    }
    js::for_each(ctx, parts, |part| {
        if let Some(b) = blob_of(ctx, part.raw())? {
            out.extend_from_slice(b.bytes());
        } else if let Some(bytes) = js::buffer_source(ctx, part.raw()) {
            out.extend_from_slice(&bytes);
        } else {
            let s = js::to_string(ctx, part.raw())?;
            if native_endings {
                out.extend_from_slice(s.replace("\r\n", "\n").replace('\r', "\n").as_bytes());
            } else {
                out.extend_from_slice(s.as_bytes());
            }
        }
        Ok(())
    })?;
    Ok(out)
}

/// `{ type, endings }` (and `lastModified` for files).
fn bag(ctx: Ctx, options: Raw) -> R<(String, bool, Option<f64>)> {
    if js::is_nullish(options) {
        return Ok((String::new(), false, None));
    }
    if !js::is_object(options) {
        return Err(js::type_error(ctx, "options must be an object"));
    }
    let endings = js::get(ctx, options, "endings")?;
    let native = match js::opt_string(ctx, endings.raw())?.as_deref() {
        None | Some("transparent") => false,
        Some("native") => true,
        Some(other) => {
            return Err(js::type_error(ctx, &format!("invalid endings `{other}`")));
        }
    };
    let last = js::get(ctx, options, "lastModified")?;
    let last = if last.is_undefined() {
        None
    } else {
        Some(js::to_number(ctx, last.raw())?.trunc())
    };
    let t = js::get(ctx, options, "type")?;
    let t = js::opt_string(ctx, t.raw())?.unwrap_or_default();
    Ok((normalize_type(&t), native, last))
}

/// Milliseconds since the epoch.
pub(crate) fn now_ms() -> f64 {
    nsh_core::time::unix_ms().trunc()
}

/// A new `Blob` object.
pub(crate) fn new_blob(ctx: Ctx, blob: Blob) -> R<V> {
    instance(ctx, js::UNDEFINED, Class::Blob, blob)
}

/// A new `File` object.
pub(crate) fn new_file(ctx: Ctx, blob: Blob, name: String, last_modified: f64) -> R<V> {
    instance(
        ctx,
        js::UNDEFINED,
        Class::File,
        File {
            blob,
            name,
            last_modified,
        },
    )
}

native!(
    fn blob_constructor(ctx, this, args) {
        let (mime, native, _) = bag(ctx, args.get(1))?;
        let bytes = blob_parts(ctx, args.get(0), native)?;
        instance(ctx, this, Class::Blob, Blob::new(bytes, mime))
    }
);

native!(
    fn file_constructor(ctx, this, args) {
        if args.len() < 2 {
            return Err(js::type_error(ctx, "File needs bits and a name"));
        }
        let (mime, native, last) = bag(ctx, args.get(2))?;
        let bytes = blob_parts(ctx, args.get(0), native)?;
        let name = js::to_string(ctx, args.get(1))?;
        let file = File {
            blob: Blob::new(bytes, mime),
            name,
            last_modified: last.unwrap_or_else(now_ms),
        };
        instance(ctx, this, Class::File, file)
    }
);

fn blob<'a>(ctx: Ctx, v: Raw) -> R<std::cell::RefMut<'a, Blob>> {
    role(ctx, v, "Blob", |o| o.blob())
}

native!(
    fn blob_size(ctx, this, args) {
        Ok(V::num(blob(ctx, this)?.len() as f64))
    }
);

native!(
    fn blob_type(ctx, this, args) {
        let t = blob(ctx, this)?.mime.clone();
        js::string(ctx, &t)
    }
);

native!(
    fn blob_slice(ctx, this, args) {
        let num = |v: Raw| -> R<Option<f64>> {
            if js::is_undefined(v) { Ok(None) } else { js::to_number(ctx, v).map(Some) }
        };
        let (start, end) = (num(args.get(0))?, num(args.get(1))?);
        let mime = normalize_type(&js::opt_string(ctx, args.get(2))?.unwrap_or_default());
        let sliced = blob(ctx, this)?.slice(start, end, mime);
        new_blob(ctx, sliced)
    }
);

native!(
    fn blob_array_buffer(ctx, this, args) {
        js::promise_of(ctx, || {
            let b = blob(ctx, this)?.clone();
            js::array_buffer(ctx, b.bytes())
        })
    }
);

native!(
    fn blob_bytes(ctx, this, args) {
        js::promise_of(ctx, || {
            let b = blob(ctx, this)?.clone();
            js::uint8_array(ctx, b.bytes())
        })
    }
);

native!(
    fn blob_text(ctx, this, args) {
        js::promise_of(ctx, || {
            let b = blob(ctx, this)?.clone();
            js::string(ctx, &String::from_utf8_lossy(b.bytes()))
        })
    }
);

native!(
    fn blob_stream(ctx, this, args) {
        let b = blob(ctx, this)?.clone();
        super::streams::bytes_stream(ctx, b.bytes().to_vec())
    }
);

fn file<'a>(ctx: Ctx, v: Raw) -> R<std::cell::RefMut<'a, File>> {
    data::<File>(ctx, v, "File")
}

native!(
    fn file_name(ctx, this, args) {
        let n = file(ctx, this)?.name.clone();
        js::string(ctx, &n)
    }
);

native!(
    fn file_last_modified(ctx, this, args) {
        Ok(V::num(file(ctx, this)?.last_modified))
    }
);

native!(
    fn file_relative_path(ctx, this, args) {
        file(ctx, this)?;
        js::string(ctx, "")
    }
);

// --- Blob URLs ------------------------------------------------------------

/// A random hex string (boundaries, blob URL ids).
pub(crate) fn random_hex() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::hash::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    let a = h.finish();
    h.write_u64(a);
    format!("{a:016x}{:016x}", h.finish())
}

/// The blobs of `blob:` URLs, per context.
#[derive(Default)]
pub(crate) struct BlobUrls {
    map: Vec<(String, Blob)>,
}

fn blob_urls<'a>(ctx: Ctx) -> &'a mut BlobUrls {
    // SAFETY: a projection of the state's field, briefly.
    unsafe { &mut (*crate::state::state(ctx)).web.blob_urls }
}

/// The blob `url` names (`blob:…`), if registered.
pub(crate) fn resolve_blob_url(ctx: Ctx, url: &str) -> Option<Blob> {
    let url = url.split('#').next().unwrap_or(url);
    blob_urls(ctx)
        .map
        .iter()
        .find(|(u, _)| u == url)
        .map(|(_, b)| b.clone())
}

native!(
    fn create_object_url(ctx, this, args) {
        let b = blob_of(ctx, args.get(0))?
            .ok_or_else(|| js::type_error(ctx, "createObjectURL needs a Blob"))?;
        let url = format!("blob:nsh://app/{}", random_hex());
        blob_urls(ctx).map.push((url.clone(), b));
        js::string(ctx, &url)
    }
);

native!(
    fn revoke_object_url(ctx, this, args) {
        let url = js::to_string(ctx, args.get(0))?;
        blob_urls(ctx).map.retain(|(u, _)| *u != url);
        Ok(V::undefined())
    }
);

// --- FormData -------------------------------------------------------------

/// A value of a form entry: a string or a `File` object.
pub(crate) enum FormValue {
    Str(String),
    File(V),
}

impl js::Trace for FormValue {
    fn trace(&self, mark: &mut dyn FnMut(Raw)) {
        if let FormValue::File(f) = self {
            f.trace(mark);
        }
    }
}

impl FormValue {
    fn to_js(&self, ctx: Ctx) -> R<V> {
        match self {
            FormValue::Str(s) => js::string(ctx, s),
            FormValue::File(f) => Ok(f.clone_ref()),
        }
    }

    fn clone_ref(&self) -> FormValue {
        match self {
            FormValue::Str(s) => FormValue::Str(s.clone()),
            FormValue::File(f) => FormValue::File(f.clone_ref()),
        }
    }
}

pub(crate) struct FormData {
    pub(crate) entries: Vec<(String, FormValue)>,
}

trace!(FormData { entries });

impl Object for FormData {}

fn form<'a>(ctx: Ctx, v: Raw) -> R<std::cell::RefMut<'a, FormData>> {
    data::<FormData>(ctx, v, "FormData")
}

/// Whether `v` is a `FormData`.
pub(crate) fn is_form_data(ctx: Ctx, v: Raw) -> bool {
    super::is::<FormData>(ctx, v)
}

/// The entry `append` / `set` make of `value` (and `filename`).
fn form_value(ctx: Ctx, value: Raw, filename: Raw) -> R<FormValue> {
    let Some(b) = blob_of(ctx, value)? else {
        return Ok(FormValue::Str(js::to_string(ctx, value)?));
    };
    let is_file = super::is::<File>(ctx, value);
    if is_file && js::is_undefined(filename) {
        return Ok(FormValue::File(V::dup(ctx, value)));
    }
    let (name, last) = if is_file {
        let f = file(ctx, value)?;
        (f.name.clone(), f.last_modified)
    } else {
        ("blob".to_owned(), now_ms())
    };
    let name = js::opt_string(ctx, filename)?.unwrap_or(name);
    Ok(FormValue::File(new_file(ctx, b, name, last)?))
}

native!(
    fn form_constructor(ctx, this, args) {
        if !js::is_undefined(args.get(0)) {
            return Err(js::type_error(ctx, "FormData has no form elements here"));
        }
        instance(ctx, this, Class::FormData, FormData { entries: Vec::new() })
    }
);

/// A new `FormData` with `entries`.
pub(crate) fn new_form_data(ctx: Ctx, entries: Vec<(String, FormValue)>) -> R<V> {
    instance(ctx, js::UNDEFINED, Class::FormData, FormData { entries })
}

fn two_or_three(ctx: Ctx, args: js::Args<'_>) -> R<()> {
    if args.len() < 2 {
        return Err(js::type_error(ctx, "a name and a value are required"));
    }
    if args.len() > 2 && !js::is_undefined(args.get(2)) && !is_blob(ctx, args.get(1)) {
        return Err(js::type_error(ctx, "a filename needs a Blob value"));
    }
    Ok(())
}

native!(
    fn form_append(ctx, this, args) {
        two_or_three(ctx, args)?;
        let name = js::to_string(ctx, args.get(0))?;
        let value = form_value(ctx, args.get(1), args.get(2))?;
        form(ctx, this)?.entries.push((name, value));
        Ok(V::undefined())
    }
);

native!(
    fn form_set(ctx, this, args) {
        two_or_three(ctx, args)?;
        let name = js::to_string(ctx, args.get(0))?;
        let value = form_value(ctx, args.get(1), args.get(2))?;
        let removed = {
            let mut f = form(ctx, this)?;
            match f.entries.iter().position(|(n, _)| *n == name) {
                Some(i) => {
                    let old = std::mem::replace(&mut f.entries[i].1, value);
                    let mut j = 0;
                    let mut rest = Vec::new();
                    let entries = std::mem::take(&mut f.entries);
                    for (n, v) in entries {
                        j += 1;
                        if j - 1 > i && n == name {
                            rest.push(v);
                        } else {
                            f.entries.push((n, v));
                        }
                    }
                    rest.push(old);
                    rest
                }
                None => {
                    f.entries.push((name, value));
                    Vec::new()
                }
            }
        };
        drop(removed);
        Ok(V::undefined())
    }
);

native!(
    fn form_delete(ctx, this, args) {
        let name = js::to_string(ctx, args.get(0))?;
        let removed: Vec<_> = {
            let mut f = form(ctx, this)?;
            let (gone, kept) = std::mem::take(&mut f.entries)
                .into_iter()
                .partition(|(n, _)| *n == name);
            f.entries = kept;
            gone
        };
        drop::<Vec<(String, FormValue)>>(removed);
        Ok(V::undefined())
    }
);

native!(
    fn form_get(ctx, this, args) {
        let name = js::to_string(ctx, args.get(0))?;
        let found = form(ctx, this)?
            .entries
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone_ref());
        match found {
            Some(v) => v.to_js(ctx),
            None => Ok(V::null()),
        }
    }
);

native!(
    fn form_get_all(ctx, this, args) {
        let name = js::to_string(ctx, args.get(0))?;
        let all: Vec<FormValue> = form(ctx, this)?
            .entries
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v.clone_ref())
            .collect();
        let items = all.iter().map(|v| v.to_js(ctx)).collect::<R<Vec<_>>>()?;
        js::new_array(ctx, items)
    }
);

native!(
    fn form_has(ctx, this, args) {
        let name = js::to_string(ctx, args.get(0))?;
        Ok(V::bool(form(ctx, this)?.entries.iter().any(|(n, _)| *n == name)))
    }
);

native!(
    fn form_for_each(ctx, this, args) {
        let f = args.get(0);
        if !js::is_function(ctx, f) {
            return Err(js::type_error(ctx, "the callback is not a function"));
        }
        let mut i = 0;
        loop {
            let entry = form(ctx, this)?
                .entries
                .get(i)
                .map(|(n, v)| (n.clone(), v.clone_ref()));
            let Some((n, v)) = entry else { break };
            let (n, v) = (js::string(ctx, &n)?, v.to_js(ctx)?);
            js::call(ctx, f, args.get(1), &[v.raw(), n.raw(), this])?;
            i += 1;
        }
        Ok(V::undefined())
    }
);

fn form_iterator(ctx: Ctx, this: Raw, part: Part) -> R<V> {
    let entries: Vec<(String, FormValue)> = form(ctx, this)?
        .entries
        .iter()
        .map(|(n, v)| (n.clone(), v.clone_ref()))
        .collect();
    let items = entries
        .iter()
        .map(|(n, v)| iterator::pair_value(ctx, n, v.to_js(ctx)?, part))
        .collect::<R<Vec<_>>>()?;
    iterator::new(ctx, items)
}

native!(
    fn form_entries(ctx, this, args) {
        form_iterator(ctx, this, Part::Entries)
    }
);

native!(
    fn form_keys(ctx, this, args) {
        form_iterator(ctx, this, Part::Keys)
    }
);

native!(
    fn form_values(ctx, this, args) {
        form_iterator(ctx, this, Part::Values)
    }
);

// --- multipart/form-data --------------------------------------------------

/// Escapes a name or filename in a part header (HTML's rule).
fn escape_header(s: &str) -> String {
    s.replace('\n', "%0A")
        .replace('\r', "%0D")
        .replace('"', "%22")
}

/// Encodes `form` as `multipart/form-data`: the body and its content type.
pub(crate) fn encode_multipart(ctx: Ctx, form_obj: Raw) -> R<(Vec<u8>, String)> {
    let entries: Vec<(String, FormValue)> = form(ctx, form_obj)?
        .entries
        .iter()
        .map(|(n, v)| (n.clone(), v.clone_ref()))
        .collect();
    let boundary = format!("----nshFormBoundary{}", &random_hex()[..24]);
    let mut out = Vec::new();
    for (name, value) in entries {
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        // Line breaks in names become CRLF before escaping, as browsers do.
        let name = escape_header(&normalize_newlines(&name));
        match value {
            FormValue::Str(s) => {
                out.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
                );
                out.extend_from_slice(normalize_newlines(&s).as_bytes());
            }
            FormValue::File(f) => {
                let (bytes, mime, filename) = {
                    let file = file(ctx, f.raw())?;
                    (
                        file.blob.bytes().to_vec(),
                        file.blob.mime.clone(),
                        file.name.clone(),
                    )
                };
                let mime = if mime.is_empty() {
                    "application/octet-stream".to_owned()
                } else {
                    mime
                };
                out.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{}\"\r\nContent-Type: {mime}\r\n\r\n",
                        escape_header(&filename)
                    )
                    .as_bytes(),
                );
                out.extend_from_slice(&bytes);
            }
        }
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((out, format!("multipart/form-data; boundary={boundary}")))
}

/// `\n` and lone `\r` as `\r\n` (form values are sent with CRLF).
fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

/// A parameter of a header value (`boundary=…`, `name="…"`).
pub(crate) fn header_param(value: &str, key: &str) -> Option<String> {
    for part in value.split(';').skip(1) {
        let (k, v) = part.split_once('=')?;
        if k.trim().eq_ignore_ascii_case(key) {
            let v = v.trim();
            return Some(
                v.strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .unwrap_or(v)
                    .to_owned(),
            );
        }
    }
    None
}

/// Parses a `multipart/form-data` body with `boundary`: its entries
/// (`None`: malformed).
pub(crate) fn parse_multipart(
    ctx: Ctx,
    body: &[u8],
    boundary: &str,
) -> R<Option<Vec<(String, FormValue)>>> {
    let delimiter = format!("--{boundary}");
    let d = delimiter.as_bytes();
    let find = |hay: &[u8], needle: &[u8], from: usize| {
        hay.get(from..)?
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + from)
    };
    let Some(mut at) = find(body, d, 0) else {
        return Ok(None);
    };
    let mut entries = Vec::new();
    loop {
        at += d.len();
        if body[at..].starts_with(b"--") {
            return Ok(Some(entries));
        }
        if !body[at..].starts_with(b"\r\n") {
            return Ok(None);
        }
        at += 2;
        let Some(head_end) = find(body, b"\r\n\r\n", at) else {
            return Ok(None);
        };
        let head = String::from_utf8_lossy(&body[at..head_end]).into_owned();
        let content = head_end + 4;
        let Some(next) = find(body, format!("\r\n{delimiter}").as_bytes(), content) else {
            return Ok(None);
        };
        let value = &body[content..next];
        let (mut name, mut filename, mut mime) = (None, None, None);
        for line in head.split("\r\n") {
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            if k.trim().eq_ignore_ascii_case("content-disposition") {
                let v = format!("x;{}", v.split_once(';').map_or("", |(_, rest)| rest));
                name = header_param(&v, "name");
                filename = header_param(&v, "filename");
            } else if k.trim().eq_ignore_ascii_case("content-type") {
                mime = Some(v.trim().to_owned());
            }
        }
        let Some(name) = name else {
            return Ok(None);
        };
        let value = match filename {
            Some(filename) => {
                let blob = Blob::new(value.to_vec(), normalize_type(&mime.unwrap_or_default()));
                FormValue::File(new_file(ctx, blob, filename, now_ms())?)
            }
            None => FormValue::Str(String::from_utf8_lossy(value).into_owned()),
        };
        entries.push((name, value));
        at = next + 2;
    }
}

pub(crate) const SPECS: &[Spec] = &[
    Spec {
        class: Class::Blob,
        name: c"Blob",
        parent: None,
        constructor: Some((blob_constructor, 0)),
        methods: &[
            (Key::Name(c"slice"), blob_slice, 0),
            (Key::Name(c"arrayBuffer"), blob_array_buffer, 0),
            (Key::Name(c"bytes"), blob_bytes, 0),
            (Key::Name(c"text"), blob_text, 0),
            (Key::Name(c"stream"), blob_stream, 0),
        ],
        accessors: &[(c"size", blob_size, None), (c"type", blob_type, None)],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"constructor(blobParts?: Iterable<BlobPart>, options?: BlobPropertyBag);
readonly size: number;
readonly type: string;
slice(start?: number, end?: number, contentType?: string): Blob;
arrayBuffer(): Promise<ArrayBuffer>;
bytes(): Promise<Uint8Array>;
text(): Promise<string>;
stream(): ReadableStream<Uint8Array>;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::File,
        name: c"File",
        parent: Some(Class::Blob),
        constructor: Some((file_constructor, 2)),
        methods: &[],
        accessors: &[
            (c"name", file_name, None),
            (c"lastModified", file_last_modified, None),
            (c"webkitRelativePath", file_relative_path, None),
        ],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: " extends Blob",
            members: r#"constructor(fileBits: Iterable<BlobPart>, fileName: string, options?: BlobPropertyBag & { lastModified?: number });
readonly name: string;
readonly lastModified: number;
readonly webkitRelativePath: string;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::FormData,
        name: c"FormData",
        parent: None,
        constructor: Some((form_constructor, 0)),
        methods: &[
            (Key::Name(c"append"), form_append, 2),
            (Key::Name(c"delete"), form_delete, 1),
            (Key::Name(c"get"), form_get, 1),
            (Key::Name(c"getAll"), form_get_all, 1),
            (Key::Name(c"has"), form_has, 1),
            (Key::Name(c"set"), form_set, 2),
            (Key::Name(c"forEach"), form_for_each, 1),
            (Key::Name(c"entries"), form_entries, 0),
            (Key::Name(c"keys"), form_keys, 0),
            (Key::Name(c"values"), form_values, 0),
            (Key::Iterator, form_entries, 0),
        ],
        accessors: &[],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"constructor();
append(name: string, value: string | Blob, fileName?: string): void;
delete(name: string): void;
get(name: string): FormDataEntryValue | null;
getAll(name: string): FormDataEntryValue[];
has(name: string): boolean;
set(name: string, value: string | Blob, fileName?: string): void;
forEach(callback: (value: FormDataEntryValue, key: string, parent: FormData) => void, thisArg?: unknown): void;
entries(): IterableIterator<[string, FormDataEntryValue]>;
keys(): IterableIterator<string>;
values(): IterableIterator<FormDataEntryValue>;
[Symbol.iterator](): IterableIterator<[string, FormDataEntryValue]>;"#,
        }
        .declared(),
    },
];

/// The TypeScript types of this file's classes (`globals.d.ts`).
pub(crate) const TYPES: &str = r#"type BlobPart = BufferSource | Blob | string;

interface BlobPropertyBag {
  type?: string;
  endings?: "transparent" | "native";
}

type FormDataEntryValue = File | string;
"#;
