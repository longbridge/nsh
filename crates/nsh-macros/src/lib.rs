//! Derive macros of nsh's Rust components (`nsh_core::ui`):
//!
//! - `#[derive(Props)]` on a component's props struct: decoding the props JS
//!   sends (`$title`), defaults, slots and children, and the TypeScript
//!   declarations of the generated glue (doc comments included).
//! - `#[derive(PropEnum)]` on an enum of unit variants: a string union in
//!   JS (`"primary" | "plain"`).
//! - `#[derive(Event)]` on a component's event enum: the `on<Event>`
//!   handlers and their data types.
//!
//! - `#[methods]` on an `impl` block of a component: its functions become
//!   methods of the component's ref in JS (`ref.setValue(5)`,
//!   `await ref.value()`).
//!
//! - `asset!("logo@2x.png")` / `assets!()`: the files a crate ships in its
//!   `assets/` directory, read at compile time.
//!
//! - `#[native_module("gallery/native")]` on a function of an app's crate:
//!   the UI set it registers, which nsh's generated workspace links.
//!
//! The code they generate names `::nsh_core::ui`. `style_helpers!` is
//! internal to `nsh-core`: the spacing and sizing methods of `Styled`.

mod assets;
mod host_props;
mod js_ops;
mod native_module;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, FnArg, GenericArgument, Ident, ImplItem, ItemFn,
    ItemImpl, LitStr, Pat, PathArguments, ReturnType, Token, Type, bracketed, parse::Parse,
    parse_macro_input, punctuated::Punctuated, spanned::Spanned,
};

/// `text` as declaration text in generated code: left out of builds
/// without declarations (`nsh_core::ui::DECLARATIONS`).
fn decl(text: &str) -> TokenStream2 {
    quote!(::nsh_core::decl!(#text))
}

/// The doc comment of an item, lines joined.
fn docs(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(lit) = &nv.value
            && let syn::Lit::Str(s) = &lit.lit
        {
            lines.push(s.value().trim().to_owned());
        }
    }
    // Rustdoc links (`[`Button`]`) read as code spans in TypeScript.
    lines.join(" ").trim().replace("[`", "`").replace("`]", "`")
}

/// A variant's doc for TypeScript: its first paragraph (the others are for
/// Rust), without the leading JS name (`` `pagingEnabled`: stops at … ``
/// documents `pagingEnabled` as "Stops at …").
fn ts_doc(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(lit) = &nv.value
            && let syn::Lit::Str(s) = &lit.lit
        {
            let line = s.value().trim().to_owned();
            if line.is_empty() {
                break;
            }
            lines.push(line);
        }
    }
    let doc = lines.join(" ").replace("[`", "`").replace("`]", "`");
    if !doc.starts_with('`') {
        return doc;
    }
    // `name`. names the prop and nothing more.
    if doc.ends_with('`') || (doc.ends_with("`.") && doc[1..doc.len() - 2].find('`').is_none()) {
        return String::new();
    }
    // `name` (iOS): … / `a` / `b`: …
    match doc.split_once(": ") {
        Some((name, text)) if !name.contains(". ") => {
            let mut chars = text.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
        _ => doc,
    }
}

/// `accessibility_label` / `AccessibilityLabel` -> `accessibilityLabel`.
fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for (i, c) in name.chars().enumerate() {
        if c == '_' {
            upper = true;
        } else if i == 0 {
            out.push(c.to_ascii_lowercase());
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The options of `#[prop(...)]`.
#[derive(Default)]
struct PropAttr {
    /// `default` (`Some(None)`) or `default = expr`.
    default: Option<Option<Expr>>,
    rename: Option<String>,
    /// More names a `PropEnum` variant is read from.
    aliases: Vec<String>,
    /// `Children` made only while the render places them.
    lazy: bool,
}

fn prop_attr(attrs: &[Attribute]) -> syn::Result<PropAttr> {
    let mut out = PropAttr::default();
    for attr in attrs {
        if !attr.path().is_ident("prop") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") {
                out.default = Some(if meta.input.peek(Token![=]) {
                    Some(meta.value()?.parse()?)
                } else {
                    None
                });
                Ok(())
            } else if meta.path.is_ident("rename") {
                out.rename = Some(meta.value()?.parse::<LitStr>()?.value());
                Ok(())
            } else if meta.path.is_ident("alias") {
                out.aliases.push(meta.value()?.parse::<LitStr>()?.value());
                Ok(())
            } else if meta.path.is_ident("lazy") {
                out.lazy = true;
                Ok(())
            } else {
                Err(meta.error(
                    "expected `default`, `default = …`, `rename = \"…\"`, `alias = \"…\"` or `lazy`",
                ))
            }
        })?;
    }
    Ok(out)
}

/// Whether field `ident` is not the `Children` field found so far.
fn kind_is_not_children(children: &Option<Ident>, ident: &Ident) -> bool {
    children.as_ref() != Some(ident)
}

/// The last segment of a type path (`Slot`, `Option`) and its first
/// generic argument.
fn last_segment(ty: &Type) -> Option<(&Ident, Option<&Type>)> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    let arg = match &seg.arguments {
        PathArguments::AngleBracketed(a) => a.args.iter().find_map(|a| match a {
            GenericArgument::Type(t) => Some(t),
            _ => None,
        }),
        _ => None,
    };
    Some((&seg.ident, arg))
}

enum Kind {
    Value,
    Slot,
    Children,
    Items,
}

/// `#[derive(Props)]`: a component's props. Every field is a prop named in
/// camel case (`accessibility_label` is `accessibilityLabel`):
///
/// - a field of a [`JsValue`] type; required in TypeScript unless it is an
///   `Option` or has `#[prop(default)]` / `#[prop(default = expr)]`.
///   Fields start with their default (`Default::default()` without one);
///   JS clearing a prop restores it.
/// - `Option<Slot>`: a named slot, JSX the app passes in that prop.
/// - `Children`: the JSX children; with `#[prop(lazy)]` JS makes them only
///   while the render places them (made again each time it does, released
///   when it stops: a sheet's content starts fresh when it opens).
/// - `Items`: virtualized rows (in JS `each`, a row renderer and `List`'s
///   data props), at most one, and not with `Children`.
///
/// `#[prop(rename = "…")]` sets the JS name. The struct's doc comment
/// documents the component in TypeScript, the fields' its props. The props
/// also get `Default` (every field at its default), for Rust renders:
/// `ButtonProps { title: "+1".into(), ..Default::default() }`.
#[proc_macro_derive(Props, attributes(prop))]
pub fn derive_props(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    props(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn props(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`Props` is derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(input.span(), "`Props` needs named fields"));
    };
    let doc = decl(&docs(&input.attrs));
    let ui = quote!(::nsh_core::ui);
    let mut specs = Vec::new();
    let mut initial = Vec::new();
    let mut set_arms = Vec::new();
    let mut slot_arms = Vec::new();
    let mut children = None;
    let mut lazy_children = false;
    let mut items: Option<Ident> = None;
    let mut items_js: Option<String> = None;
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named");
        let ty = &field.ty;
        let attr = prop_attr(&field.attrs)?;
        let js = attr
            .rename
            .clone()
            .unwrap_or_else(|| camel(&ident.to_string()));
        let field_doc = decl(&docs(&field.attrs));
        let kind = match last_segment(ty) {
            Some((id, Some(inner))) if id == "Option" => match last_segment(inner) {
                Some((s, _)) if s == "Slot" => Kind::Slot,
                _ => Kind::Value,
            },
            Some((id, _)) if id == "Children" => Kind::Children,
            Some((id, _)) if id == "Items" => Kind::Items,
            _ => Kind::Value,
        };
        let is_option = matches!(last_segment(ty), Some((id, _)) if id == "Option");
        let init = match (&attr.default, &kind) {
            (_, Kind::Items) => quote!(#ui::Items::__new(#js)),
            (Some(Some(expr)), _) => quote!(#expr),
            _ => quote!(::core::default::Default::default()),
        };
        initial.push(quote!(#ident: #init));
        match kind {
            Kind::Value => {
                let optional = is_option || attr.default.is_some();
                specs.push(quote! {
                    #ui::PropSpec {
                        name: #js,
                        ts: #ui::ts_of::<#ty>()(types),
                        optional: #optional,
                        doc: #field_doc,
                        kind: #ui::PropKind::Value,
                    }
                });
                set_arms.push(quote! {
                    #js => {
                        let value: #ty = match value {
                            ::core::option::Option::Some(v) => <#ty as #ui::JsValue>::from_json(v)
                                .map_err(|e| #ui::PropError(::std::format!("`{}`: {}", #js, e)))?,
                            ::core::option::Option::None => #init,
                        };
                        if self.#ident == value {
                            ::core::result::Result::Ok(false)
                        } else {
                            self.#ident = value;
                            ::core::result::Result::Ok(true)
                        }
                    }
                });
            }
            Kind::Slot => {
                specs.push(quote! {
                    #ui::PropSpec {
                        name: #js,
                        ts: ::std::string::String::from("JSX.Element"),
                        optional: true,
                        doc: #field_doc,
                        kind: #ui::PropKind::Slot,
                    }
                });
                slot_arms.push(quote! {
                    #js => {
                        let changed = self.#ident != slot;
                        self.#ident = slot;
                        changed
                    }
                });
            }
            Kind::Items => {
                if items.is_some() || children.is_some() {
                    return Err(syn::Error::new(
                        field.span(),
                        "one `Items` field at most, and no `Children` with it (the row renderer is the children)",
                    ));
                }
                specs.push(quote! {
                    #ui::PropSpec {
                        name: #js,
                        ts: ::std::string::String::new(),
                        optional: false,
                        doc: #field_doc,
                        kind: #ui::PropKind::Items,
                    }
                });
                items = Some(ident.clone());
                items_js = Some(js.clone());
            }
            Kind::Children => {
                if children.is_some() || items.is_some() {
                    return Err(syn::Error::new(
                        field.span(),
                        "one `Children` field at most, and none with `Items`",
                    ));
                }
                specs.push(quote! {
                    #ui::PropSpec {
                        name: "children",
                        ts: ::std::string::String::from("JSX.Element"),
                        optional: true,
                        doc: #field_doc,
                        kind: #ui::PropKind::Children,
                    }
                });
                children = Some(ident.clone());
                lazy_children = attr.lazy;
            }
        }
        if attr.lazy && kind_is_not_children(&children, ident) {
            return Err(syn::Error::new(
                field.span(),
                "`#[prop(lazy)]` is for the `Children` field",
            ));
        }
    }
    let set_children = match &children {
        Some(ident) => quote! {
            let changed = self.#ident != children;
            self.#ident = children;
            changed
        },
        None => quote!({
            let _ = children;
            false
        }),
    };
    let items_const = match &items_js {
        Some(js) => quote!(::core::option::Option::Some(#js)),
        None => quote!(::core::option::Option::None),
    };
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        /// Every prop at its default (`..Default::default()` when Rust
        /// renders the component).
        impl #impl_generics ::core::default::Default for #name #ty_generics #where_clause {
            fn default() -> Self {
                <Self as #ui::Props>::initial()
            }
        }

        impl #impl_generics #ui::Props for #name #ty_generics #where_clause {
            const ITEMS: ::core::option::Option<&'static str> = #items_const;
            const LAZY_CHILDREN: bool = #lazy_children;

            fn doc() -> &'static str {
                #doc
            }

            fn spec(types: &mut #ui::TsTypes) -> ::std::vec::Vec<#ui::PropSpec> {
                let _ = &types;
                ::std::vec![#(#specs),*]
            }

            fn initial() -> Self {
                Self { #(#initial),* }
            }

            fn set(
                &mut self,
                name: &str,
                value: ::core::option::Option<::nsh_core::JsonValue>,
            ) -> ::core::result::Result<bool, #ui::PropError> {
                match name {
                    #(#set_arms)*
                    _ => {
                        let _ = value;
                        ::core::result::Result::Err(#ui::PropError(::std::format!("unknown prop `{name}`")))
                    }
                }
            }

            fn set_slot(&mut self, name: &str, slot: ::core::option::Option<#ui::Slot>) -> bool {
                match name {
                    #(#slot_arms)*
                    _ => {
                        let _ = slot;
                        false
                    }
                }
            }

            fn set_children(&mut self, children: #ui::Children) -> bool {
                #set_children
            }
        }
    })
}

/// `#[derive(JsOps)]` on an enum of operations JS records into flat lists
/// (`nsh_core::js::ops`): `#[op(code = CV_…)]` on a variant reads its
/// fields in order after that code (each an `OpArg`; `min` / `max` clamp,
/// `with = path` reads the whole variant), `nested` delegates to another
/// enum of operations. `method = "name", params = "x, y = 0"` describes the
/// generated method recording it as is, `property = "name", initial =
/// "…"` (and `ts = "…"`) the property; their docs are the variant's.
/// `#[ops(cx = Type)]` on the enum is what reading needs.
#[proc_macro_derive(JsOps, attributes(op, ops))]
pub fn derive_js_ops(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    js_ops::js_ops(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// `#[derive(HostProps)]` on an enum of host element props
/// (`nsh_core::js::host`): `#[prop(js = "name", …)]` on a variant gives
/// its JS name (several for aliases; `ios = …` / `android = …` inside
/// `ios={{…}}` / `android={{…}}`), how its payload decodes (its
/// `HostValue`; `default = …`, `min` / `max`, `required`, or a whole
/// decoder `with = path`), where it applies (`on = TEXT | INPUT`), its
/// TypeScript type (`ts = "…"` over the payload's) and its reset
/// (`reset = …`); `nested` delegates to an enum of the same kind,
/// `internal` keeps it out of the element types. Doc comments document
/// the props.
#[proc_macro_derive(HostProps, attributes(prop))]
pub fn derive_host_props(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    host_props::host_props(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// `#[derive(PropEnum)]`: an enum of unit variants as a string union in JS,
/// each variant in camel case (`SemiBold` is `"semiBold"`;
/// `#[prop(rename = "…")]` sets it, `#[prop(alias = "…")]` adds names it
/// is read from). It is a `JsValue` for components' props and a
/// `HostValue` for host props.
#[proc_macro_derive(PropEnum, attributes(prop))]
pub fn derive_prop_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    prop_enum(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn prop_enum(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`PropEnum` is derived for enums",
        ));
    };
    let ui = quote!(::nsh_core::ui);
    let mut names = Vec::new();
    let mut idents = Vec::new();
    // Every name a variant is read from (its own, then its aliases).
    let mut read = Vec::new();
    let mut accepted = Vec::new();
    let mut patterns = Vec::new();
    for v in &data.variants {
        if !matches!(v.fields, Fields::Unit) {
            return Err(syn::Error::new(
                v.span(),
                "`PropEnum` variants have no fields",
            ));
        }
        let attr = prop_attr(&v.attrs)?;
        let own = attr.rename.unwrap_or_else(|| camel(&v.ident.to_string()));
        let mut all = vec![own.clone()];
        all.extend(attr.aliases);
        names.push(own);
        let ident = &v.ident;
        read.push(quote!(#(#all)|* => Self::#ident));
        patterns.push(quote!(#(#all)|*));
        accepted.extend(all);
        idents.push(&v.ident);
    }
    // The aliases are part of the type: JS may use them.
    let ts = accepted
        .iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let expected = format!("one of {ts}");
    let type_name = name.to_string();
    let doc = docs(&input.attrs);
    let declaration = if doc.is_empty() {
        format!("export type {type_name} = {ts};")
    } else {
        format!("/** {doc} */\nexport type {type_name} = {ts};")
    };
    Ok(quote! {
        impl #ui::JsValue for #name {
            fn ts_type(types: &mut #ui::TsTypes) -> ::std::string::String {
                types.declare(#type_name, |_| ::std::string::String::from(#declaration))
            }

            fn from_json(value: ::nsh_core::JsonValue) -> ::core::result::Result<Self, #ui::PropError> {
                ::core::result::Result::Ok(match value.as_str() {
                    #(::core::option::Option::Some(#patterns) => Self::#idents,)*
                    _ => return ::core::result::Result::Err(#ui::PropError::expected(#expected, &value)),
                })
            }

            fn to_json(&self) -> ::nsh_core::JsonValue {
                ::nsh_core::JsonValue::String(::std::string::String::from(match self {
                    #(Self::#idents => #names,)*
                }))
            }
        }

        impl ::nsh_core::js::host::HostValue for #name {
            /// One of the names; anything else is none (the prop's
            /// default).
            fn decode<I: ::nsh_core::js::host::HostInput>(
                input: &I,
                _cx: &mut ::nsh_core::js::host::HostCx,
            ) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(match input.as_string()?.as_str() {
                    #(#read,)*
                    _ => return ::core::option::Option::None,
                })
            }

            fn ts_type(types: &mut #ui::TsTypes) -> ::std::string::String {
                <Self as #ui::JsValue>::ts_type(types)
            }
        }
    })
}

/// `#[derive(Event)]`: a component's events. Each variant is an event in
/// camel case (`Press` is `onPress` in JS): a unit variant carries no
/// data, a variant with named fields an object of them (camel case), a
/// variant with one unnamed field that value. Doc comments document the
/// handlers.
#[proc_macro_derive(Event)]
pub fn derive_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    event(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn event(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`Event` is derived for enums",
        ));
    };
    let ui = quote!(::nsh_core::ui);
    let mut specs = Vec::new();
    let mut name_arms = Vec::new();
    let mut data_arms = Vec::new();
    for v in &data.variants {
        let ident = &v.ident;
        let js = camel(&ident.to_string());
        let doc = decl(&docs(&v.attrs));
        match &v.fields {
            Fields::Unit => {
                specs.push(quote! {
                    #ui::EventSpec { name: #js, data: ::std::string::String::from("void"), doc: #doc }
                });
                name_arms.push(quote!(Self::#ident => #js));
                data_arms.push(quote!(Self::#ident => ::nsh_core::JsonValue::Null));
            }
            Fields::Unnamed(f) if f.unnamed.len() == 1 => {
                let ty = &f.unnamed[0].ty;
                specs.push(quote! {
                    #ui::EventSpec { name: #js, data: #ui::ts_of::<#ty>()(types), doc: #doc }
                });
                name_arms.push(quote!(Self::#ident(..) => #js));
                data_arms.push(quote!(Self::#ident(v) => #ui::JsValue::to_json(v)));
            }
            Fields::Named(f) => {
                let idents: Vec<_> = f
                    .named
                    .iter()
                    .map(|f| f.ident.clone().expect("named"))
                    .collect();
                let keys: Vec<_> = idents.iter().map(|i| camel(&i.to_string())).collect();
                let tys: Vec<_> = f.named.iter().map(|f| &f.ty).collect();
                specs.push(quote! {
                    #ui::EventSpec {
                        name: #js,
                        data: if #ui::DECLARATIONS {
                            let fields: ::std::vec::Vec<::std::string::String> = ::std::vec![
                                #(::std::format!("{}: {}", #keys, <#tys as #ui::JsValue>::ts_type(types))),*
                            ];
                            ::std::format!("{{ {} }}", fields.join("; "))
                        } else {
                            ::std::string::String::new()
                        },
                        doc: #doc,
                    }
                });
                name_arms.push(quote!(Self::#ident { .. } => #js));
                data_arms.push(quote! {
                    Self::#ident { #(#idents),* } => {
                        let mut map = ::nsh_core::ui::__private::JsonMap::new();
                        #(map.insert(::std::string::String::from(#keys), #ui::JsValue::to_json(#idents));)*
                        ::nsh_core::JsonValue::Object(map)
                    }
                });
            }
            Fields::Unnamed(f) => {
                return Err(syn::Error::new(
                    f.span(),
                    "an event variant has at most one unnamed field",
                ));
            }
        }
    }
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #ui::Event for #name #ty_generics #where_clause {
            fn spec(types: &mut #ui::TsTypes) -> ::std::vec::Vec<#ui::EventSpec> {
                let _ = &types;
                ::std::vec![#(#specs),*]
            }

            fn name(&self) -> &'static str {
                match self {
                    #(#name_arms,)*
                }
            }

            fn data(&self) -> ::nsh_core::JsonValue {
                match self {
                    #(#data_arms,)*
                }
            }
        }
    })
}

/// One `name => [Key, ...]` line of `style_helpers!`.
struct Helper {
    doc: String,
    name: Ident,
    keys: Vec<Ident>,
}

impl Parse for Helper {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let name: Ident = input.parse()?;
        input.parse::<Token![=>]>()?;
        let content;
        bracketed!(content in input);
        let keys = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Ok(Helper {
            doc: docs(&attrs),
            name,
            keys,
        })
    }
}

/// Tailwind's spacing scale: method suffix and points (4pt steps).
const STEPS: &[(&str, f32)] = &[
    ("0", 0.0),
    ("0p5", 2.0),
    ("1", 4.0),
    ("1p5", 6.0),
    ("2", 8.0),
    ("2p5", 10.0),
    ("3", 12.0),
    ("3p5", 14.0),
    ("4", 16.0),
    ("5", 20.0),
    ("6", 24.0),
    ("7", 28.0),
    ("8", 32.0),
    ("9", 36.0),
    ("10", 40.0),
    ("11", 44.0),
    ("12", 48.0),
    ("14", 56.0),
    ("16", 64.0),
    ("20", 80.0),
    ("24", 96.0),
    ("28", 112.0),
    ("32", 128.0),
    ("36", 144.0),
    ("40", 160.0),
    ("44", 176.0),
    ("48", 192.0),
    ("52", 208.0),
    ("56", 224.0),
    ("60", 240.0),
    ("64", 256.0),
    ("72", 288.0),
    ("80", 320.0),
    ("96", 384.0),
];

/// A file of the crate's `assets/` directory (`asset!("logo@2x.png")`, the
/// path relative to it), as TS imports one: an image is an `ImageAsset`
/// with its natural size in points (`@2x` / `@3x` in the name divide its
/// pixels), a font (`.ttf`, `.otf`) a `FontAsset` with its family name,
/// anything else a `FileAsset`. Read at compile time: a missing or broken
/// file fails the build, and a changed one builds the crate again. The
/// registry ships the files (`Registry::assets(assets!())`).
#[proc_macro]
pub fn asset(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    assets::asset(lit)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Every file of the crate's `assets/` directory, as `&'static
/// [AssetFile]`: what `Registry::assets` ships with the app (`nsh run`
/// serves them, `nsh package` copies them; the engine registers the
/// fonts).
#[proc_macro]
pub fn assets(input: TokenStream) -> TokenStream {
    if !input.is_empty() {
        return syn::Error::new(
            Span::call_site(),
            "`assets!()` takes no arguments: it lists the crate's `assets/` directory",
        )
        .into_compile_error()
        .into();
    }
    assets::assets(Span::call_site())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// `#[native_module("gallery/native")]` on `fn native(registry: &mut
/// Registry)` at the root of an app's crate (`nsh.json` `"rust"`): the UI
/// set the app's TSX imports by that specifier. The function registers the
/// crate's components, modules and assets into the set's registry
/// (`Registry::ui_set("gallery/native")`), which the command line, hosts
/// and Android library nsh builds for the app serve:
///
/// ```ignore
/// #[native_module("gallery/native")]
/// fn native(registry: &mut Registry) {
///     registry.register::<Gallery>();
///     registry.assets(assets!());
/// }
/// ```
///
/// The specifier is checked at compile time: a bare import specifier
/// (`my-app/native`, `@acme/ui`), not a path, and not nsh's own (`nsh`,
/// `nsh/...`). A crate has one, at its root: it generates a hidden entry
/// of a fixed name the generated workspace calls, so a second one fails as
/// that name defined twice, one in a module as not at the root, and a
/// crate without one fails the generated workspace's build saying what to
/// add.
#[proc_macro_attribute]
pub fn native_module(attr: TokenStream, item: TokenStream) -> TokenStream {
    let specifier = match syn::parse::<LitStr>(attr) {
        Ok(lit) => lit,
        Err(_) => {
            return syn::Error::new(
                Span::call_site(),
                "`#[native_module]` takes the specifier the app imports the set by: `#[native_module(\"my-app/native\")]`",
            )
            .into_compile_error()
            .into();
        }
    };
    let item = parse_macro_input!(item as ItemFn);
    native_module::expand(specifier, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Internal to `nsh-core`: `name => [StyleKey, ...]` lines become a method
/// `name(length)` and one method per spacing step (`p_4()` is 16pt) of
/// the `Styled` trait.
#[proc_macro]
pub fn style_helpers(input: TokenStream) -> TokenStream {
    let helpers = parse_macro_input!(input with Punctuated::<Helper, Token![,]>::parse_terminated);
    let mut out = TokenStream2::new();
    for h in helpers {
        let Helper { doc, name, keys } = h;
        out.extend(quote! {
            #[doc = #doc]
            fn #name(mut self, length: impl ::core::convert::Into<crate::ui::Length>) -> Self {
                self.style().lengths(&[#(crate::style::StyleKey::#keys),*], length.into());
                self
            }
        });
        for (suffix, points) in STEPS {
            let method = format_ident!("{}_{}", name, suffix, span = Span::call_site());
            let step_doc = format!("{}: {points}pt.", doc.trim_end_matches('.'));
            out.extend(quote! {
                #[doc = #step_doc]
                fn #method(mut self) -> Self {
                    self.style().lengths(
                        &[#(crate::style::StyleKey::#keys),*],
                        crate::ui::Length::Points(#points),
                    );
                    self
                }
            });
        }
    }
    out.into()
}

/// Whether `ty` is `&mut Context<..>` (a method's context parameter).
fn is_context(ty: &Type) -> bool {
    match ty {
        Type::Reference(r) => {
            r.mutability.is_some()
                && matches!(last_segment(&r.elem), Some((id, _)) if id == "Context")
        }
        _ => false,
    }
}

/// `#[methods]` on an `impl` block of a component: each function becomes a
/// method of the component's ref in JS, named in camel case (`set_value`
/// is `setValue`), documented by its doc comment. A function takes `&self`
/// or `&mut self`, then [`JsValue`] parameters, and optionally a last
/// `cx: &mut Context<Self>`; it returns nothing (JS calls it and moves on;
/// the call is ordered with the props set before it) or a [`JsValue`] (JS
/// gets a `Promise`). The block's functions are JS's: keep helpers in
/// another `impl`. The component lists them with
/// `const METHODS: &'static [Method<Self>] = Self::JS_METHODS;`.
#[proc_macro_attribute]
pub fn methods(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(Span::call_site(), "`#[methods]` takes no arguments")
            .into_compile_error()
            .into();
    }
    let item = parse_macro_input!(item as ItemImpl);
    methods_impl(item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn methods_impl(item: ItemImpl) -> syn::Result<TokenStream2> {
    let ui = quote!(::nsh_core::ui);
    let self_ty = &item.self_ty;
    let (impl_generics, _, where_clause) = item.generics.split_for_impl();
    let mut entries = Vec::new();
    for member in &item.items {
        let ImplItem::Fn(f) = member else { continue };
        let ident = &f.sig.ident;
        let js = camel(&ident.to_string());
        let doc = decl(&docs(&f.attrs));
        let mut inputs = f.sig.inputs.iter();
        match inputs.next() {
            Some(FnArg::Receiver(r)) if r.reference.is_some() => {}
            _ => {
                return Err(syn::Error::new(
                    f.sig.span(),
                    "a JS method takes `&self` or `&mut self`",
                ));
            }
        }
        let mut params = Vec::new();
        let mut decode = Vec::new();
        let mut call_args = Vec::new();
        let mut takes_cx = false;
        let typed: Vec<_> = inputs
            .map(|arg| match arg {
                FnArg::Typed(t) => Ok(t),
                FnArg::Receiver(r) => Err(syn::Error::new(r.span(), "one receiver")),
            })
            .collect::<syn::Result<_>>()?;
        for (i, arg) in typed.iter().enumerate() {
            let ty = &arg.ty;
            if is_context(ty) {
                if i + 1 != typed.len() {
                    return Err(syn::Error::new(
                        arg.span(),
                        "the context is the last parameter",
                    ));
                }
                takes_cx = true;
                continue;
            }
            let Pat::Ident(name) = &*arg.pat else {
                return Err(syn::Error::new(
                    arg.pat.span(),
                    "a JS method's parameters are named",
                ));
            };
            let name = camel(&name.ident.to_string());
            let local = format_ident!("__arg{}", i);
            params.push(quote!((#name, #ui::ts_of::<#ty>())));
            decode.push(quote! {
                let #local: #ty = <#ty as #ui::JsValue>::from_json(
                    args.next().unwrap_or(::nsh_core::JsonValue::Null),
                )
                .map_err(|e| #ui::PropError(::std::format!("`{}`: {}", #name, e)))?;
            });
            call_args.push(quote!(#local));
        }
        if takes_cx {
            call_args.push(quote!(cx));
        }
        let (returns, result) = match &f.sig.output {
            ReturnType::Default => (
                quote!(::core::option::Option::None),
                quote! {
                    this.#ident(#(#call_args),*);
                    ::core::result::Result::Ok(::nsh_core::JsonValue::Null)
                },
            ),
            ReturnType::Type(_, ty) => (
                quote!(::core::option::Option::Some(#ui::ts_of::<#ty>())),
                quote! {
                    let value: #ty = this.#ident(#(#call_args),*);
                    ::core::result::Result::Ok(#ui::JsValue::to_json(&value))
                },
            ),
        };
        let unused_cx = (!takes_cx).then(|| quote!(let _ = cx;));
        let args_iter = if decode.is_empty() {
            quote!(let _ = args;)
        } else {
            quote!(let mut args = args.into_iter();)
        };
        entries.push(quote! {
            #ui::Method {
                name: #js,
                doc: #doc,
                params: &[#(#params),*],
                returns: #returns,
                call: |this, args, cx| {
                    #unused_cx
                    #args_iter
                    #(#decode)*
                    #result
                },
            }
        });
    }
    Ok(quote! {
        #item

        impl #impl_generics #self_ty #where_clause {
            /// The methods JS calls on the component's ref (`#[methods]`).
            #[doc(hidden)]
            pub const JS_METHODS: &'static [#ui::Method<Self>] = &[#(#entries),*];
        }
    })
}

/// `#[derive(JsObject)]` on a struct with named fields: a plain object in
/// JS (fields in camel case). A field is optional in TypeScript when it is
/// an `Option` or has `#[prop(default)]` / `#[prop(default = expr)]`;
/// missing, it takes its default. Doc comments document the fields.
#[proc_macro_derive(JsObject, attributes(prop))]
pub fn derive_js_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    js_object(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn js_object(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`JsObject` is derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            input.span(),
            "`JsObject` needs named fields",
        ));
    };
    let ui = quote!(::nsh_core::ui);
    let mut ts = Vec::new();
    let mut from = Vec::new();
    let mut to = Vec::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named");
        let ty = &field.ty;
        let attr = prop_attr(&field.attrs)?;
        let js = attr
            .rename
            .clone()
            .unwrap_or_else(|| camel(&ident.to_string()));
        let doc = docs(&field.attrs);
        let is_option = matches!(last_segment(ty), Some((id, _)) if id == "Option");
        let optional = is_option || attr.default.is_some();
        let mark = if optional { "?" } else { "" };
        let doc_ts = if doc.is_empty() {
            String::new()
        } else {
            format!("/** {doc} */\n  ")
        };
        ts.push(quote!(
            ::std::format!("  {}{}{}: {};", #doc_ts, #js, #mark, <#ty as #ui::JsValue>::ts_type(types))
        ));
        let missing = match (&attr.default, is_option) {
            (Some(Some(expr)), _) => quote!(#expr),
            (Some(None), _) | (None, true) => quote!(::core::default::Default::default()),
            (None, false) => quote! {
                return ::core::result::Result::Err(#ui::PropError(::std::format!("`{}` is missing", #js)))
            },
        };
        from.push(quote! {
            #ident: match object.remove(#js) {
                ::core::option::Option::Some(v) if !v.is_null() || #is_option => <#ty as #ui::JsValue>::from_json(v)
                    .map_err(|e| #ui::PropError(::std::format!("`{}`: {}", #js, e)))?,
                _ => { #missing }
            }
        });
        to.push(quote! {
            object.insert(::std::string::String::from(#js), #ui::JsValue::to_json(&self.#ident));
        });
    }
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let type_name = name.to_string();
    let doc = docs(&input.attrs);
    let head = if doc.is_empty() {
        format!("export interface {type_name} {{")
    } else {
        format!("/** {doc} */\nexport interface {type_name} {{")
    };
    Ok(quote! {
        impl #impl_generics #ui::JsValue for #name #ty_generics #where_clause {
            fn ts_type(types: &mut #ui::TsTypes) -> ::std::string::String {
                types.declare(#type_name, |types| {
                    let fields: ::std::vec::Vec<::std::string::String> = ::std::vec![#(#ts),*];
                    ::std::format!("{}\n{}\n}}", #head, fields.join("\n"))
                })
            }

            fn from_json(value: ::nsh_core::JsonValue) -> ::core::result::Result<Self, #ui::PropError> {
                let ::nsh_core::JsonValue::Object(mut object) = value else {
                    return ::core::result::Result::Err(#ui::PropError::expected("an object", &value));
                };
                ::core::result::Result::Ok(Self { #(#from),* })
            }

            fn to_json(&self) -> ::nsh_core::JsonValue {
                let mut object = ::nsh_core::ui::__private::JsonMap::new();
                #(#to)*
                ::nsh_core::JsonValue::Object(object)
            }
        }
    })
}

/// Whether `ty` is `AsyncCx` / `&mut ModuleCx`: a module function's
/// context.
fn module_cx(ty: &Type) -> Option<bool> {
    match ty {
        Type::Reference(r) if matches!(last_segment(&r.elem), Some((id, _)) if id == "ModuleCx") => {
            Some(false)
        }
        t if matches!(last_segment(t), Some((id, _)) if id == "AsyncCx") => Some(true),
        _ => None,
    }
}

/// `#[module]` on an `impl` block of a unit struct: its functions (no
/// receiver) become functions of a JS object named after the struct
/// (`Alert.alert(...)`), documented by their doc comments (the block's doc
/// comment documents the object). A function takes [`JsValue`] parameters
/// (an `Option` one is optional), then optionally `cx: &mut ModuleCx`, or
/// for an `async fn` `cx: AsyncCx`; it returns nothing (JS calls it and
/// moves on) or a [`JsValue`] (JS gets a `Promise`), or a `Result` of one
/// (an `Err` rejects the `Promise` with its message). A UI set registers
/// the module (`Registry::module::<Alert>()`); a component exported under
/// the same name gets the functions too (`StatusBar.setHidden(...)`).
/// `#[module(flat)]` exports the functions one by one instead of as an
/// object.
#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    let flat = match attr.to_string().as_str() {
        "" => false,
        "flat" => true,
        _ => {
            return syn::Error::new(Span::call_site(), "`#[module]` takes `flat` or nothing")
                .into_compile_error()
                .into();
        }
    };
    let item = parse_macro_input!(item as ItemImpl);
    module_impl(item, flat)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// `Result<T, E>` -> `T`.
fn result_ok(ty: &Type) -> Option<&Type> {
    match last_segment(ty)? {
        (id, ok) if id == "Result" => ok,
        _ => None,
    }
}

fn module_impl(item: ItemImpl, flat: bool) -> syn::Result<TokenStream2> {
    let ui = quote!(::nsh_core::ui);
    let self_ty = &item.self_ty;
    let Some((type_name, _)) = last_segment(self_ty) else {
        return Err(syn::Error::new(
            self_ty.span(),
            "`#[module]` is for a named type",
        ));
    };
    let module_name = type_name.to_string();
    let module_doc = decl(&docs(&item.attrs));
    let mut entries = Vec::new();
    for member in &item.items {
        let ImplItem::Fn(f) = member else { continue };
        let ident = &f.sig.ident;
        let js = camel(&ident.to_string());
        let doc = decl(&docs(&f.attrs));
        let is_async = f.sig.asyncness.is_some();
        let typed: Vec<_> = f
            .sig
            .inputs
            .iter()
            .map(|arg| match arg {
                FnArg::Typed(t) => Ok(t),
                FnArg::Receiver(r) => Err(syn::Error::new(
                    r.span(),
                    "a module function has no receiver (its state is a global)",
                )),
            })
            .collect::<syn::Result<_>>()?;
        let mut params = Vec::new();
        let mut decode = Vec::new();
        let mut call_args = Vec::new();
        for (i, arg) in typed.iter().enumerate() {
            let ty = &arg.ty;
            if let Some(async_cx) = module_cx(ty) {
                if i + 1 != typed.len() || async_cx != is_async {
                    return Err(syn::Error::new(
                        arg.span(),
                        "the last parameter: `cx: &mut ModuleCx`, or `cx: AsyncCx` for an `async fn`",
                    ));
                }
                call_args.push(quote!(cx));
                continue;
            }
            let Pat::Ident(name) = &*arg.pat else {
                return Err(syn::Error::new(
                    arg.pat.span(),
                    "a module function's parameters are named",
                ));
            };
            let name = camel(&name.ident.to_string());
            let local = format_ident!("__arg{}", i);
            let optional = matches!(last_segment(ty), Some((id, _)) if id == "Option");
            params.push(quote!((#name, #ui::ts_of::<#ty>(), #optional)));
            decode.push(quote! {
                let #local: #ty = <#ty as #ui::JsValue>::from_json(
                    args.next().unwrap_or(::nsh_core::JsonValue::Null),
                )
                .map_err(|e| #ui::PropError(::std::format!("`{}`: {}", #name, e)))?;
            });
            call_args.push(quote!(#local));
        }
        let takes_cx = call_args.len() > decode.len();
        let unused_cx = (!takes_cx).then(|| quote!(let _ = cx;));
        let args_iter = if decode.is_empty() {
            quote!(let _ = args;)
        } else {
            quote!(let mut args = args.into_iter();)
        };
        // The result as `Result<JsonValue, String>`.
        let (returns, to_json) = match &f.sig.output {
            ReturnType::Default => (
                quote!(::core::option::Option::None),
                quote!({
                    let _ = value;
                    ::core::result::Result::Ok(::nsh_core::JsonValue::Null)
                }),
            ),
            ReturnType::Type(_, ty) => match result_ok(ty) {
                Some(ok) => (
                    quote!(::core::option::Option::Some(#ui::ts_of::<#ok>())),
                    quote!(value
                        .map(|v| #ui::JsValue::to_json(&v))
                        .map_err(|e| ::std::string::ToString::to_string(&e))),
                ),
                None => (
                    quote!(::core::option::Option::Some(#ui::ts_of::<#ty>())),
                    quote!(::core::result::Result::Ok(#ui::JsValue::to_json(&value))),
                ),
            },
        };
        let call = if is_async {
            quote! {
                #ui::FunctionCall::Async(|args, cx| {
                    #unused_cx
                    #args_iter
                    #(#decode)*
                    ::core::result::Result::Ok(::std::boxed::Box::pin(async move {
                        let value = #self_ty::#ident(#(#call_args),*).await;
                        #to_json
                    }))
                })
            }
        } else {
            quote! {
                #ui::FunctionCall::Sync(|args, cx| {
                    #unused_cx
                    #args_iter
                    #(#decode)*
                    let value = #self_ty::#ident(#(#call_args),*);
                    let result: ::core::result::Result<::nsh_core::JsonValue, ::std::string::String> = #to_json;
                    result.map_err(#ui::PropError)
                })
            }
        };
        entries.push(quote! {
            #ui::Function {
                name: #js,
                doc: #doc,
                params: &[#(#params),*],
                returns: #returns,
                call: #call,
            }
        });
    }
    Ok(quote! {
        #item

        impl #ui::Module for #self_ty {
            const NAME: &'static str = #module_name;
            const DOC: &'static str = #module_doc;
            const FUNCTIONS: &'static [#ui::Function] = &[#(#entries),*];
            const FLAT: bool = #flat;
        }
    })
}
