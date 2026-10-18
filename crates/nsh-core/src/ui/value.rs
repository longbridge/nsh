//! Values crossing between JS and Rust: shared strings, and the direct
//! conversion of props, event data, arguments and results from and to JS
//! values ([`JsValue`]) with their TypeScript types.

use std::{borrow::Borrow, fmt, ops::Deref, sync::Arc};

use super::js::{JsHost, JsIn, JsKind, JsOwned};
use crate::{JsonValue, style::Color};

/// A cheaply cloned string: a static string or a shared one.
#[derive(Clone)]
pub struct SharedString(Repr);

#[derive(Clone)]
enum Repr {
    Static(&'static str),
    Shared(Arc<str>),
}

impl SharedString {
    /// A static string, without allocating.
    pub const fn new_static(s: &'static str) -> Self {
        Self(Repr::Static(s))
    }

    /// The string.
    pub fn as_str(&self) -> &str {
        match &self.0 {
            Repr::Static(s) => s,
            Repr::Shared(s) => s,
        }
    }

    pub(crate) fn to_arc(&self) -> Arc<str> {
        match &self.0 {
            Repr::Static(s) => Arc::from(*s),
            Repr::Shared(s) => s.clone(),
        }
    }
}

impl Default for SharedString {
    fn default() -> Self {
        Self::new_static("")
    }
}

impl Deref for SharedString {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for SharedString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for SharedString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for SharedString {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.as_str(), other.as_str());
        std::ptr::eq(a, b) || a == b
    }
}

impl Eq for SharedString {}

impl std::hash::Hash for SharedString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl PartialEq<str> for SharedString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for SharedString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl fmt::Debug for SharedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for SharedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&'static str> for SharedString {
    fn from(s: &'static str) -> Self {
        Self::new_static(s)
    }
}

impl From<String> for SharedString {
    fn from(s: String) -> Self {
        Self(Repr::Shared(s.into()))
    }
}

impl From<Arc<str>> for SharedString {
    fn from(s: Arc<str>) -> Self {
        Self(Repr::Shared(s))
    }
}

impl From<&SharedString> for SharedString {
    fn from(s: &SharedString) -> Self {
        s.clone()
    }
}

/// A prop value JS sent that does not fit the prop's type.
#[derive(Debug, Clone, PartialEq)]
pub struct PropError(pub String);

impl PropError {
    /// "expected `ty`, got `value`".
    pub fn expected(ty: &str, value: &JsonValue) -> Self {
        let mut got = value.to_string();
        if got.len() > 40 {
            got.truncate(40);
            got.push('…');
        }
        Self(format!("expected {ty}, got {got}"))
    }
}

impl fmt::Display for PropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PropError {}

/// The named TypeScript types the declarations use (`AlertButton`,
/// `BarStyle`), declared once each.
#[derive(Debug, Clone, Default)]
pub struct TsTypes {
    types: std::collections::BTreeMap<&'static str, String>,
}

impl TsTypes {
    /// Type `name`, declared by `declaration` (`interface ... {}` or
    /// `type ... = ...;`, with its doc comment) the first time.
    pub fn declare(
        &mut self,
        name: &'static str,
        declaration: impl FnOnce(&mut TsTypes) -> String,
    ) -> String {
        if !self.types.contains_key(name) {
            // Present while it is built: a type naming itself stops here.
            self.types.insert(name, String::new());
            let text = declaration(self);
            self.types.insert(name, text);
        }
        name.to_owned()
    }

    /// Class `def`'s type: its name (the class is declared with the set's
    /// classes, which include every class a registered signature names:
    /// [`super::Registry::class`]).
    pub fn class(&mut self, def: &'static super::ClassDef) -> String {
        def.name.to_owned()
    }

    /// Adds `other`'s types.
    pub(crate) fn merge(&mut self, other: &TsTypes) {
        for (name, text) in &other.types {
            self.types.entry(name).or_insert_with(|| text.clone());
        }
    }

    pub(crate) fn declarations(&self) -> impl Iterator<Item = &str> {
        self.types.values().map(String::as_str)
    }
}

/// A function giving a TypeScript type (declaring the named types it uses).
pub type TsType = fn(&mut TsTypes) -> String;

/// `T`'s [`JsValue::ts_type`], or one giving nothing in builds without
/// declarations ([`super::DECLARATIONS`]): what generated code calls, so
/// runtimes link none of the TypeScript.
#[doc(hidden)]
pub const fn ts_of<T: JsValue>() -> TsType {
    if super::DECLARATIONS {
        T::ts_type
    } else {
        |_| String::new()
    }
}

/// A type props, event data, and the arguments and results of classes and
/// functions can have: how it converts from and to JS values (directly,
/// in the JS call or when JS receives it) and its TypeScript type.
/// `#[derive(PropEnum)]` implements it for enums of unit variants,
/// `#[derive(JsObject)]` for structs.
pub trait JsValue: Sized {
    /// Its TypeScript type (`string`, `AlertButton`), declaring the named
    /// types it uses in `types`.
    fn ts_type(types: &mut TsTypes) -> String;

    /// The value of JS value `value`.
    fn from_js(value: JsIn<'_>) -> Result<Self, PropError>;

    /// The value as a JS value of `host`.
    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError>;
}

impl JsValue for bool {
    fn ts_type(_types: &mut TsTypes) -> String {
        "boolean".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        value.as_bool().ok_or_else(|| value.expected("a boolean"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(host, host.new_bool(*self)))
    }
}

/// Whether `n` is a whole number in the range of a `bits`-bit integer: what
/// an integer parameter, prop or field takes (a fraction, `NaN`,
/// `Infinity` or a value out of range is a mistake, not something to round
/// or clamp).
pub(crate) fn fits(n: f64, signed: bool, bits: u32) -> bool {
    if !n.is_finite() || n.fract() != 0.0 {
        return false;
    }
    let (lo, hi) = if signed {
        let half = 2f64.powi(bits as i32 - 1);
        (-half, half)
    } else {
        (0.0, 2f64.powi(bits as i32))
    };
    n >= lo && n < hi
}

macro_rules! number {
    ($check:ident: $($ty:ty),*) => {$(
        impl JsValue for $ty {
            fn ts_type(_types: &mut TsTypes) -> String {
                "number".into()
            }

            fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
                let n = value.as_number().ok_or_else(|| value.expected("a number"))?;
                number!(@$check $ty, n, value);
                Ok(n as $ty)
            }

            /// Above 2^53 an integer is the nearest number JS has.
            fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
                Ok(JsOwned::new(host, host.new_number(*self as f64)))
            }
        }
    )*};
    // A float takes any number.
    (@any $ty:ty, $n:ident, $value:ident) => {};
    (@whole $ty:ty, $n:ident, $value:ident) => {
        if !fits($n, <$ty>::MIN != 0, <$ty>::BITS) {
            return Err($value.expected(concat!("an integer that fits ", stringify!($ty))));
        }
    };
}

number!(any: f32, f64);
number!(whole: i32, i64, u32, u64, usize);

impl JsValue for SharedString {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        value
            .as_string()
            .map(Into::into)
            .ok_or_else(|| value.expected("a string"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(host, host.new_string(self.as_str())?))
    }
}

impl JsValue for String {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        value.as_string().ok_or_else(|| value.expected("a string"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(host, host.new_string(self)?))
    }
}

/// A CSS color string from JS (`"#2563eb"`, `"rgb(0 0 0 / 0.5)"`).
impl JsValue for Color {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        value
            .as_string()
            .as_deref()
            .and_then(Color::parse)
            .ok_or_else(|| value.expected("a color"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(
            host,
            host.new_string(&format!("#{:08x}", self.0))?,
        ))
    }
}

/// A node JS passes (a `YNode`, a `ref`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeRef(pub crate::NodeId);

impl JsValue for NodeRef {
    fn ts_type(_types: &mut TsTypes) -> String {
        "YNode".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        value
            .host()
            .node(value.raw())
            .map(NodeRef)
            .ok_or_else(|| value.expected("a node"))
    }

    /// The node's JS object (`null` once JS let it go).
    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(host, host.node_value(self.0)))
    }
}

/// Any value, as JSON (a real JSON API's data: `JSON.stringify`'s view of
/// the value).
impl JsValue for JsonValue {
    fn ts_type(_types: &mut TsTypes) -> String {
        "unknown".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        match value.kind() {
            JsKind::Undefined => Ok(JsonValue::Null),
            _ => value.to_json(),
        }
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(host, host.json(self)?))
    }
}

/// `null` (and `undefined`) is `None`.
impl<T: JsValue> JsValue for Option<T> {
    fn ts_type(types: &mut TsTypes) -> String {
        format!("{} | null", T::ts_type(types))
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if value.is_nullish() {
            Ok(None)
        } else {
            T::from_js(value).map(Some)
        }
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            Some(v) => v.to_js(host),
            None => Ok(JsOwned::new(host, host.null())),
        }
    }
}

impl<T: JsValue> JsValue for Vec<T> {
    fn ts_type(types: &mut TsTypes) -> String {
        format!("({})[]", T::ts_type(types))
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        let len = value
            .array_len()?
            .ok_or_else(|| value.expected("an array"))?;
        // The length is JS's (the host limits what it reads): reserve a
        // bounded amount up front.
        let mut out = Vec::with_capacity(len.min(1024));
        for i in 0..len {
            let item = value.index(i)?;
            out.push(T::from_js(item.get()).map_err(|e| PropError(format!("[{i}]: {e}")))?);
        }
        Ok(out)
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        // Owned until the array takes them: an item failing frees the
        // ones made before.
        let mut items = Vec::with_capacity(self.len());
        for item in self {
            items.push(item.to_js(host)?);
        }
        let raws = items.into_iter().map(JsOwned::into_raw).collect();
        Ok(JsOwned::new(host, host.new_array(raws)?))
    }
}
