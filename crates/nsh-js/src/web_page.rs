//! `nsh/web`: the page around an app on the web: `Head`, the document's
//! title, description and metadata for search engines and link previews.
//! The browser runtime serves the module itself (`web/browser/page.js` in
//! `nsh-platform-web`), writing the page's `<head>`. Here, on the native
//! hosts, there is no page, so `Head` renders and sets nothing: no node,
//! no op, no mount item. Only an app that is a page of a site (its VM has a
//! base URL, `HostOptions::base_url`: `nsh render`, which no host that
//! draws views sets) gets a node for it, a zero-sized native component
//! (`nsh/head`) whose props reach the mount batches as a component's
//! (`MountItem::ComponentProps`), which `nsh render` writes as the page's
//! head (`nsh_html::Document::head`).

use std::ffi::c_int;

use nsh_core::Tag;
use rquickjs::qjs;

use crate::{
    dom,
    modules::{Export, Item, Ts},
    reactive::{self, arg},
};

type JsValue = qjs::JSValue;
type JsContext = qjs::JSContext;

/// The component `Head` renders in an app that is a page.
const HEAD_COMPONENT: &str = "nsh/head";

/// `Head`'s props, each the component's own (`$title`).
const HEAD_PROPS: [&str; 6] = [
    "title",
    "description",
    "canonical",
    "lang",
    "meta",
    "jsonLd",
];

fn string(cx: *mut JsContext, s: &str) -> JsValue {
    // SAFETY: a new string of `s`'s bytes.
    unsafe { qjs::JS_NewStringLen(cx, s.as_ptr().cast(), s.len() as _) }
}

/// `<Head …/>`: nothing where there is no page; in an app that is one, a
/// zero-sized `nsh/head` component carrying its props.
unsafe extern "C" fn head(
    cx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    // No page (every host that draws views): no view to mount and no
    // component the platform would look for.
    if crate::web::web(cx).base_url.is_none() {
        return qjs::JS_NULL;
    }
    let props = arg(argc, argv, 0);
    let node = match dom::create_element(cx, Tag::Native) {
        Ok(node) => node,
        Err(e) => return e,
    };
    let result = (|| -> Result<(), JsValue> {
        for (prop, value) in [("component", HEAD_COMPONENT), ("class", "absolute w-0 h-0")] {
            let v = string(cx, value);
            let r = dom::set_prop_value(cx, node, prop, v);
            // SAFETY: ours.
            unsafe { qjs::JS_FreeValue(cx, v) };
            r?;
        }
        for key in HEAD_PROPS {
            let get = dom::prop_reader(cx, props, key);
            let r = dom::bind_prop_with(cx, node, &format!("${key}"), get);
            // SAFETY: ours.
            unsafe { qjs::JS_FreeValue(cx, get) };
            r?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => reactive::finish_value(cx, node),
        Err(e) => {
            // SAFETY: ours.
            unsafe { qjs::JS_FreeValue(cx, node) };
            e
        }
    }
}

pub(crate) const EXPORTS: &[Item] = &[(
    "Head",
    Export::Function(head, 1),
    Ts::function(
        "The page's `<head>` while it is mounted: its title, description, canonical URL,\nlanguage, metadata and structured data, which search engines index and link\npreviews show. Several may be mounted (the app's, each page's of a stack): the\nlast mounted wins for what it sets, the ones before it for the rest, so a\npage pushed over another titles the document until it is popped. It renders\nnothing; native hosts have no page and ignore it, and a page written ahead\nof time (`nsh render`) is written with it.",
        &["(props: HeadProps): JSX.Element"],
    ),
)];

pub(crate) const TYPES: &str = r#"/** One `<meta>` of the page: `name` (`"robots"`, `"twitter:card"`) or `property`
 * (Open Graph's `"og:image"`) with its `content`. */
export interface HeadMeta {
  name?: string;
  property?: string;
  content: string;
}

/** What a `<Head>` sets in the page's `<head>`; what it leaves out is the
 * `<Head>`s' mounted before it. */
export interface HeadProps {
  /** The document's title (`<title>`, and `og:title` unless `meta` sets one). */
  title?: string;
  /** The page's summary (`<meta name="description">`, and `og:description`). */
  description?: string;
  /** The page's canonical URL, or a path of the site (`"/notes/q3"`, from
   * the directory the site is published to): `<link rel="canonical">`. */
  canonical?: string;
  /** The page's language (`<html lang>`): `"en"`, `"zh-CN"`. */
  lang?: string;
  /** More `<meta>`s: Open Graph, Twitter cards, `robots`. */
  meta?: HeadMeta[];
  /** Structured data (schema.org), as a JSON-LD `<script>`. */
  jsonLd?: object;
}
"#;
