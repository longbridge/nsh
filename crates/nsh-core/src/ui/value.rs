//! Values crossing between JS and components: shared strings, and the
//! conversion of prop and event values from and to JSON with their
//! TypeScript types.

use std::{borrow::Borrow, fmt, ops::Deref, sync::Arc};

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
pub struct TsTypes(std::collections::BTreeMap<&'static str, String>);

impl TsTypes {
    /// Type `name`, declared by `declaration` (`interface ... {}` or
    /// `type ... = ...;`, with its doc comment) the first time.
    pub fn declare(
        &mut self,
        name: &'static str,
        declaration: impl FnOnce(&mut TsTypes) -> String,
    ) -> String {
        if !self.0.contains_key(name) {
            // Present while it is built: a type naming itself stops here.
            self.0.insert(name, String::new());
            let text = declaration(self);
            self.0.insert(name, text);
        }
        name.to_owned()
    }

    /// Adds `other`'s types.
    pub(crate) fn merge(&mut self, other: &TsTypes) {
        for (name, text) in &other.0 {
            self.0.entry(name).or_insert_with(|| text.clone());
        }
    }

    pub(crate) fn declarations(&self) -> impl Iterator<Item = &str> {
        self.0.values().map(String::as_str)
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

/// A type props and event data can have: how it converts from and to JSON,
/// and its TypeScript type. `#[derive(PropEnum)]` implements it for enums
/// of unit variants, `#[derive(JsObject)]` for structs.
pub trait JsValue: Sized {
    /// Its TypeScript type (`string`, `AlertButton`), declaring the named
    /// types it uses in `types`.
    fn ts_type(types: &mut TsTypes) -> String;

    /// The value JS sent.
    fn from_json(value: JsonValue) -> Result<Self, PropError>;

    /// The value for JS.
    fn to_json(&self) -> JsonValue;
}

impl JsValue for bool {
    fn ts_type(_types: &mut TsTypes) -> String {
        "boolean".into()
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        value
            .as_bool()
            .ok_or_else(|| PropError::expected("a boolean", &value))
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::Bool(*self)
    }
}

macro_rules! number {
    ($to_json:ident: $($ty:ty),*) => {$(
        impl JsValue for $ty {
            fn ts_type(_types: &mut TsTypes) -> String {
                "number".into()
            }

            fn from_json(value: JsonValue) -> Result<Self, PropError> {
                value
                    .as_f64()
                    .map(|n| n as $ty)
                    .ok_or_else(|| PropError::expected("a number", &value))
            }

            fn to_json(&self) -> JsonValue {
                $to_json(*self)
            }
        }
    )*};
}

fn float(n: impl Into<f64>) -> JsonValue {
    serde_json::Number::from_f64(n.into()).map_or(JsonValue::Null, JsonValue::Number)
}

fn int(n: impl Into<i64>) -> JsonValue {
    JsonValue::Number(n.into().into())
}

fn uint(n: impl TryInto<u64>) -> JsonValue {
    n.try_into()
        .map_or(JsonValue::Null, |n: u64| JsonValue::Number(n.into()))
}

number!(float: f32, f64);
number!(int: i32, i64);
number!(uint: u32, u64, usize);

impl JsValue for SharedString {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string".into()
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        match value {
            JsonValue::String(s) => Ok(s.into()),
            v => Err(PropError::expected("a string", &v)),
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::String(self.as_str().to_owned())
    }
}

impl JsValue for String {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string".into()
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        match value {
            JsonValue::String(s) => Ok(s),
            v => Err(PropError::expected("a string", &v)),
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::String(self.clone())
    }
}

/// A CSS color string from JS (`"#2563eb"`, `"rgb(0 0 0 / 0.5)"`).
impl JsValue for Color {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string".into()
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        value
            .as_str()
            .and_then(Color::parse)
            .ok_or_else(|| PropError::expected("a color", &value))
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::String(format!("#{:08x}", self.0))
    }
}

/// A node JS passes (a `YNode`, a `ref`): the glue sends its id as
/// `{ "$node": id }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeRef(pub crate::NodeId);

impl JsValue for NodeRef {
    fn ts_type(_types: &mut TsTypes) -> String {
        "YNode".into()
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        value
            .get("$node")
            .and_then(JsonValue::as_u64)
            .and_then(|id| crate::NodeId::try_from(id).ok())
            .map(NodeRef)
            .ok_or_else(|| PropError::expected("a node", &value))
    }

    fn to_json(&self) -> JsonValue {
        serde_json::json!({ "$node": self.0 })
    }
}

impl JsValue for JsonValue {
    fn ts_type(_types: &mut TsTypes) -> String {
        "unknown".into()
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        Ok(value)
    }

    fn to_json(&self) -> JsonValue {
        self.clone()
    }
}

/// `null` is `None`.
impl<T: JsValue> JsValue for Option<T> {
    fn ts_type(types: &mut TsTypes) -> String {
        format!("{} | null", T::ts_type(types))
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        match value {
            JsonValue::Null => Ok(None),
            v => T::from_json(v).map(Some),
        }
    }

    fn to_json(&self) -> JsonValue {
        self.as_ref().map_or(JsonValue::Null, T::to_json)
    }
}

impl<T: JsValue> JsValue for Vec<T> {
    fn ts_type(types: &mut TsTypes) -> String {
        format!("({})[]", T::ts_type(types))
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        match value {
            JsonValue::Array(items) => items.into_iter().map(T::from_json).collect(),
            v => Err(PropError::expected("an array", &v)),
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::Array(self.iter().map(T::to_json).collect())
    }
}
