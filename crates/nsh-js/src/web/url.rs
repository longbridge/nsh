//! `URL` and `URLSearchParams` (URL Standard), on the `url` crate. A URL's
//! `searchParams` is live both ways: changing the parameters rewrites the
//! query, setting `search` (or `href`) parses it again.

use url::{Url, form_urlencoded, quirks};

use super::{
    Class, Key, Object, Spec, data, instance,
    js::{self, Ctx, R, Raw, V, native, trace},
};

pub(crate) struct UrlData {
    url: Url,
    /// Its `URLSearchParams` (made with it).
    params: V,
}

trace!(UrlData { params });

impl Object for UrlData {}

pub(crate) struct Params {
    pub(crate) list: Vec<(String, String)>,
    /// The URL whose query these are (undefined for standalone ones).
    url: V,
}

trace!(Params { list, url });

impl Object for Params {}

fn url_of<'a>(ctx: Ctx, v: Raw) -> R<std::cell::RefMut<'a, UrlData>> {
    data::<UrlData>(ctx, v, "URL")
}

fn params<'a>(ctx: Ctx, v: Raw) -> R<std::cell::RefMut<'a, Params>> {
    data::<Params>(ctx, v, "URLSearchParams")
}

/// Parses `input` against `base`.
fn parse(input: &str, base: Option<&str>) -> Option<Url> {
    match base {
        Some(base) => Url::parse(base).ok()?.join(input).ok(),
        None => Url::parse(input).ok(),
    }
}

/// `input` (and optional `base`) of the URL constructors, as strings.
fn inputs(ctx: Ctx, args: js::Args<'_>) -> R<(String, Option<String>)> {
    let input = js::to_string(ctx, args.get(0))?;
    let base = js::opt_string(ctx, args.get(1))?;
    Ok((input, base))
}

/// The pairs of a query (`application/x-www-form-urlencoded`).
pub(crate) fn parse_query(query: &str) -> Vec<(String, String)> {
    let query = query.strip_prefix('?').unwrap_or(query);
    form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// The `application/x-www-form-urlencoded` serialization of `list`.
pub(crate) fn serialize(list: &[(String, String)]) -> String {
    let mut s = form_urlencoded::Serializer::new(String::new());
    for (k, v) in list {
        s.append_pair(k, v);
    }
    s.finish()
}

/// A new URL object for `url`, with its parameters.
pub(crate) fn new_url(ctx: Ctx, new_target: Raw, url: Url) -> R<V> {
    let list = parse_query(url.query().unwrap_or_default());
    let params = instance(
        ctx,
        js::UNDEFINED,
        Class::UrlSearchParams,
        Params {
            list,
            url: V::undefined(),
        },
    )?;
    let obj = instance(
        ctx,
        new_target,
        Class::Url,
        UrlData {
            url,
            params: params.clone_ref(),
        },
    )?;
    self::params(ctx, params.raw())?.url = obj.clone_ref();
    Ok(obj)
}

native!(
    fn url_constructor(ctx, this, args) {
        let (input, base) = inputs(ctx, args)?;
        let url = parse(&input, base.as_deref())
            .ok_or_else(|| js::type_error(ctx, &format!("Invalid URL: {input}")))?;
        new_url(ctx, this, url)
    }
);

native!(
    fn url_can_parse(ctx, this, args) {
        let (input, base) = inputs(ctx, args)?;
        Ok(V::bool(parse(&input, base.as_deref()).is_some()))
    }
);

native!(
    fn url_parse(ctx, this, args) {
        let (input, base) = inputs(ctx, args)?;
        match parse(&input, base.as_deref()) {
            Some(url) => new_url(ctx, js::UNDEFINED, url),
            None => Ok(V::null()),
        }
    }
);

/// After the query changed: the parameters again.
fn reparse(ctx: Ctx, this: Raw) -> R<()> {
    let (params_obj, query) = {
        let u = url_of(ctx, this)?;
        (
            u.params.clone_ref(),
            u.url.query().unwrap_or_default().to_owned(),
        )
    };
    params(ctx, params_obj.raw())?.list = parse_query(&query);
    Ok(())
}

macro_rules! url_getter {
    ($name:ident, $f:expr) => {
        native!(
            fn $name(ctx, this, args) {
                let s: String = {
                    let u = url_of(ctx, this)?;
                    ($f)(&u.url)
                };
                js::string(ctx, &s)
            }
        );
    };
}

url_getter!(url_href, |u: &Url| quirks::href(u).to_owned());
url_getter!(url_origin, |u: &Url| quirks::origin(u));
url_getter!(url_protocol, |u: &Url| quirks::protocol(u).to_owned());
url_getter!(url_username, |u: &Url| quirks::username(u).to_owned());
url_getter!(url_password, |u: &Url| quirks::password(u).to_owned());
url_getter!(url_host, |u: &Url| quirks::host(u).to_owned());
url_getter!(url_hostname, |u: &Url| quirks::hostname(u).to_owned());
url_getter!(url_port, |u: &Url| quirks::port(u).to_owned());
url_getter!(url_pathname, |u: &Url| quirks::pathname(u).to_owned());
url_getter!(url_search, |u: &Url| quirks::search(u).to_owned());
url_getter!(url_hash, |u: &Url| quirks::hash(u).to_owned());

macro_rules! url_setter {
    ($name:ident, |$u:ident, $v:ident| $set:expr) => {
        native!(
            fn $name(ctx, this, args) {
                let $v = js::to_string(ctx, args.get(0))?;
                {
                    let mut guard = url_of(ctx, this)?;
                    let $u = &mut guard.url;
                    // Invalid values leave the URL as it was.
                    let _ = $set;
                }
                Ok(V::undefined())
            }
        );
    };
}

url_setter!(url_set_protocol, |u, v| quirks::set_protocol(u, &v));
url_setter!(url_set_username, |u, v| quirks::set_username(u, &v));
url_setter!(url_set_password, |u, v| quirks::set_password(u, &v));
url_setter!(url_set_host, |u, v| quirks::set_host(u, &v));
url_setter!(url_set_hostname, |u, v| quirks::set_hostname(u, &v));
url_setter!(url_set_port, |u, v| quirks::set_port(u, &v));
url_setter!(url_set_pathname, |u, v| quirks::set_pathname(u, &v));
url_setter!(url_set_hash, |u, v| quirks::set_hash(u, &v));

native!(
    fn url_set_href(ctx, this, args) {
        let v = js::to_string(ctx, args.get(0))?;
        {
            let mut u = url_of(ctx, this)?;
            if quirks::set_href(&mut u.url, &v).is_err() {
                drop(u);
                return Err(js::type_error(ctx, &format!("Invalid URL: {v}")));
            }
        }
        reparse(ctx, this)?;
        Ok(V::undefined())
    }
);

native!(
    fn url_set_search(ctx, this, args) {
        let v = js::to_string(ctx, args.get(0))?;
        quirks::set_search(&mut url_of(ctx, this)?.url, &v);
        reparse(ctx, this)?;
        Ok(V::undefined())
    }
);

native!(
    fn url_search_params(ctx, this, args) {
        Ok(url_of(ctx, this)?.params.clone_ref())
    }
);

/// Whether `v` is a `URLSearchParams`.
pub(crate) fn is_params(ctx: Ctx, v: Raw) -> bool {
    super::is::<Params>(ctx, v)
}

/// The serialization of `URLSearchParams` `v`.
pub(crate) fn params_string(ctx: Ctx, v: Raw) -> R<String> {
    Ok(serialize(&params(ctx, v)?.list))
}

// --- URLSearchParams ------------------------------------------------------

/// After the parameters changed: the URL's query.
fn update(ctx: Ctx, this: Raw) -> R<()> {
    let (url, query) = {
        let p = params(ctx, this)?;
        (p.url.clone_ref(), serialize(&p.list))
    };
    if url.is_undefined() {
        return Ok(());
    }
    let mut u = url_of(ctx, url.raw())?;
    if query.is_empty() {
        u.url.set_query(None);
    } else {
        u.url.set_query(Some(&query));
    }
    Ok(())
}

/// The pairs of a URLSearchParams init (string, pairs, record or
/// another URLSearchParams).
pub(crate) fn init_list(ctx: Ctx, init: Raw) -> R<Vec<(String, String)>> {
    if js::is_undefined(init) {
        return Ok(Vec::new());
    }
    if super::is::<Params>(ctx, init) {
        return Ok(params(ctx, init)?.list.clone());
    }
    if js::is_object(init) {
        if js::is_iterable(ctx, init)? {
            let mut list = Vec::new();
            js::for_each(ctx, init, |pair| {
                let mut items = Vec::new();
                js::for_each(ctx, pair.raw(), |x| {
                    items.push(js::to_string(ctx, x.raw())?);
                    Ok(())
                })?;
                let [k, v]: [String; 2] = items
                    .try_into()
                    .map_err(|_| js::type_error(ctx, "Each pair must have two items"))?;
                list.push((k, v));
                Ok(())
            })?;
            return Ok(list);
        }
        return js::record(ctx, init)?
            .into_iter()
            .map(|(k, v)| Ok((k, js::to_string(ctx, v.raw())?)))
            .collect();
    }
    Ok(parse_query(&js::to_string(ctx, init)?))
}

native!(
    fn params_constructor(ctx, this, args) {
        let list = init_list(ctx, args.get(0))?;
        instance(
            ctx,
            this,
            Class::UrlSearchParams,
            Params {
                list,
                url: V::undefined(),
            },
        )
    }
);

native!(
    fn params_size(ctx, this, args) {
        Ok(V::num(params(ctx, this)?.list.len() as f64))
    }
);

native!(
    fn params_append(ctx, this, args) {
        let (k, v) = (js::to_string(ctx, args.get(0))?, js::to_string(ctx, args.get(1))?);
        params(ctx, this)?.list.push((k, v));
        update(ctx, this)?;
        Ok(V::undefined())
    }
);

native!(
    fn params_delete(ctx, this, args) {
        let k = js::to_string(ctx, args.get(0))?;
        let value = js::opt_string(ctx, args.get(1))?;
        params(ctx, this)?
            .list
            .retain(|(n, v)| *n != k || value.as_ref().is_some_and(|x| x != v));
        update(ctx, this)?;
        Ok(V::undefined())
    }
);

native!(
    fn params_get(ctx, this, args) {
        let k = js::to_string(ctx, args.get(0))?;
        let found = params(ctx, this)?
            .list
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| v.clone());
        match found {
            Some(v) => js::string(ctx, &v),
            None => Ok(V::null()),
        }
    }
);

native!(
    fn params_get_all(ctx, this, args) {
        let k = js::to_string(ctx, args.get(0))?;
        let all: Vec<String> = params(ctx, this)?
            .list
            .iter()
            .filter(|(n, _)| *n == k)
            .map(|(_, v)| v.clone())
            .collect();
        let items = all.iter().map(|v| js::string(ctx, v)).collect::<R<Vec<_>>>()?;
        js::new_array(ctx, items)
    }
);

native!(
    fn params_has(ctx, this, args) {
        let k = js::to_string(ctx, args.get(0))?;
        let value = js::opt_string(ctx, args.get(1))?;
        let has = params(ctx, this)?
            .list
            .iter()
            .any(|(n, v)| *n == k && value.as_ref().is_none_or(|x| x == v));
        Ok(V::bool(has))
    }
);

native!(
    fn params_set(ctx, this, args) {
        let (k, v) = (js::to_string(ctx, args.get(0))?, js::to_string(ctx, args.get(1))?);
        {
            let mut p = params(ctx, this)?;
            match p.list.iter().position(|(n, _)| *n == k) {
                Some(i) => {
                    p.list[i].1 = v;
                    let mut j = 0;
                    p.list.retain(|(n, _)| {
                        j += 1;
                        j - 1 <= i || *n != k
                    });
                }
                None => p.list.push((k, v)),
            }
        }
        update(ctx, this)?;
        Ok(V::undefined())
    }
);

native!(
    fn params_sort(ctx, this, args) {
        // Stable, by UTF-16 code units of the names.
        params(ctx, this)?
            .list
            .sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
        update(ctx, this)?;
        Ok(V::undefined())
    }
);

native!(
    fn params_to_string(ctx, this, args) {
        let s = serialize(&params(ctx, this)?.list);
        js::string(ctx, &s)
    }
);

native!(
    fn params_for_each(ctx, this, args) {
        let f = args.get(0);
        if !js::is_function(ctx, f) {
            return Err(js::type_error(ctx, "the callback is not a function"));
        }
        let mut i = 0;
        loop {
            let pair = params(ctx, this)?.list.get(i).cloned();
            let Some((k, v)) = pair else { break };
            let (k, v) = (js::string(ctx, &k)?, js::string(ctx, &v)?);
            js::call(ctx, f, args.get(1), &[v.raw(), k.raw(), this])?;
            i += 1;
        }
        Ok(V::undefined())
    }
);

/// An iterator over the pairs: `[name, value]`, names or values.
fn params_iterator(ctx: Ctx, this: Raw, part: Part) -> R<V> {
    let list = params(ctx, this)?.list.clone();
    let items = list
        .iter()
        .map(|(k, v)| super::iterator::pair_item(ctx, k, v, part))
        .collect::<R<Vec<_>>>()?;
    super::iterator::new(ctx, items)
}

use super::iterator::Part;

native!(
    fn params_entries(ctx, this, args) {
        params_iterator(ctx, this, Part::Entries)
    }
);

native!(
    fn params_keys(ctx, this, args) {
        params_iterator(ctx, this, Part::Keys)
    }
);

native!(
    fn params_values(ctx, this, args) {
        params_iterator(ctx, this, Part::Values)
    }
);

pub(crate) const SPECS: &[Spec] = &[
    Spec {
        class: Class::UrlSearchParams,
        name: c"URLSearchParams",
        parent: None,
        constructor: Some((params_constructor, 0)),
        methods: &[
            (Key::Name(c"append"), params_append, 2),
            (Key::Name(c"delete"), params_delete, 1),
            (Key::Name(c"get"), params_get, 1),
            (Key::Name(c"getAll"), params_get_all, 1),
            (Key::Name(c"has"), params_has, 1),
            (Key::Name(c"set"), params_set, 2),
            (Key::Name(c"sort"), params_sort, 0),
            (Key::Name(c"toString"), params_to_string, 0),
            (Key::Name(c"forEach"), params_for_each, 1),
            (Key::Name(c"entries"), params_entries, 0),
            (Key::Name(c"keys"), params_keys, 0),
            (Key::Name(c"values"), params_values, 0),
            (Key::Iterator, params_entries, 0),
        ],
        accessors: &[(c"size", params_size, None)],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"constructor(init?: string | Record<string, string> | Iterable<[string, string]> | URLSearchParams);
readonly size: number;
append(name: string, value: string): void;
delete(name: string, value?: string): void;
get(name: string): string | null;
getAll(name: string): string[];
has(name: string, value?: string): boolean;
set(name: string, value: string): void;
sort(): void;
forEach(callback: (value: string, key: string, parent: URLSearchParams) => void, thisArg?: unknown): void;
entries(): IterableIterator<[string, string]>;
keys(): IterableIterator<string>;
values(): IterableIterator<string>;
[Symbol.iterator](): IterableIterator<[string, string]>;
toString(): string;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::Url,
        name: c"URL",
        parent: None,
        constructor: Some((url_constructor, 1)),
        methods: &[
            (Key::Name(c"toString"), url_href, 0),
            (Key::Name(c"toJSON"), url_href, 0),
        ],
        accessors: &[
            (c"href", url_href, Some(url_set_href)),
            (c"origin", url_origin, None),
            (c"protocol", url_protocol, Some(url_set_protocol)),
            (c"username", url_username, Some(url_set_username)),
            (c"password", url_password, Some(url_set_password)),
            (c"host", url_host, Some(url_set_host)),
            (c"hostname", url_hostname, Some(url_set_hostname)),
            (c"port", url_port, Some(url_set_port)),
            (c"pathname", url_pathname, Some(url_set_pathname)),
            (c"search", url_search, Some(url_set_search)),
            (c"searchParams", url_search_params, None),
            (c"hash", url_hash, Some(url_set_hash)),
        ],
        statics: &[
            (c"canParse", url_can_parse, 1),
            (c"parse", url_parse, 1),
            (c"createObjectURL", super::blob::create_object_url, 1),
            (c"revokeObjectURL", super::blob::revoke_object_url, 1),
        ],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"constructor(url: string | URL, base?: string | URL);
static canParse(url: string | URL, base?: string | URL): boolean;
static parse(url: string | URL, base?: string | URL): URL | null;
/** A `blob:` URL `fetch` reads `blob` from, until revoked. */
static createObjectURL(blob: Blob): string;
static revokeObjectURL(url: string): void;
href: string;
readonly origin: string;
protocol: string;
username: string;
password: string;
host: string;
hostname: string;
port: string;
pathname: string;
search: string;
hash: string;
readonly searchParams: URLSearchParams;
toString(): string;
toJSON(): string;"#,
        }
        .declared(),
    },
];
