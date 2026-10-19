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
//! - `#[module]` on an `impl` block of a unit struct: functions of a JS
//!   object (`Alert.alert(...)`).
//! - `#[class]` on a struct and on its `impl` block: a JS class backed by
//!   the Rust value (`new Tally(3)`, `tally.add(2)`).
//!
//! - `asset!("logo@2x.png")` / `assets!()`: the files a crate ships in its
//!   `assets/` directory, read at compile time.
//!
//! - `#[native_module("gallery/native")]` on a function of an app's crate:
//!   the UI set it registers, which nsh's generated workspace links.
//!
//! Doc comments become the TypeScript declarations' up to their first
//! heading: a `# Rust` section (or `# Panics`, `# Examples`) is for Rust
//! callers only.
//!
//! The code they generate names `::nsh_core::ui`. `style_helpers!` is
//! internal to `nsh-core`: the spacing and sizing methods of `Styled`.

mod assets;
mod class;
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

/// The doc comment of an item as TypeScript shows it, lines joined: the
/// part before its first heading. The sections under headings (`# Rust`,
/// `# Panics`, `# Examples`, as rustdoc has them) are Rust's: notes about
/// `&self`, `cx` or how the call crosses stay out of the declarations.
fn docs(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    let mut fenced = false;
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(lit) = &nv.value
            && let syn::Lit::Str(s) = &lit.lit
        {
            for line in s.value().lines() {
                let line = line.trim();
                if line.starts_with("```") {
                    fenced = !fenced;
                } else if !fenced && is_heading(line) {
                    // A heading (`# Rust`): the rest is Rust's.
                    return finish_docs(lines);
                }
                lines.push(line.to_owned());
            }
        }
    }
    finish_docs(lines)
}

/// Whether `line` is a Markdown heading: one to six `#` and a space.
fn is_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&hashes) && line[hashes..].starts_with(' ')
}

/// The TS doc of `lines`: one line, rustdoc links as code spans.
fn finish_docs(lines: Vec<String>) -> String {
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

/// Whether a function of a `#[class]`, `#[module]` or `#[methods]` block
/// is exported: it has a visibility (`pub`, `pub(crate)`, ...). A private
/// one (none, or `pub(self)`) is the block's helper, Rust's only.
fn exported(vis: &syn::Visibility) -> bool {
    match vis {
        syn::Visibility::Inherited => false,
        syn::Visibility::Restricted(r) => !r.path.is_ident("self"),
        syn::Visibility::Public(_) => true,
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

/// The names `#[args(name, amount)]` among `attrs` gives a JS function's
/// arguments (a callback parameter's, an event handler's), in camel case.
fn args_attr(attrs: &[Attribute]) -> syn::Result<Option<(Vec<String>, Span)>> {
    let mut out = None;
    for attr in attrs {
        if !attr.path().is_ident("args") {
            continue;
        }
        if out.is_some() {
            return Err(syn::Error::new(attr.span(), "one `#[args(..)]`"));
        }
        let names = attr.parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)?;
        if names.is_empty() {
            return Err(syn::Error::new(
                attr.span(),
                "`#[args(..)]` names the arguments: `#[args(name, amount)]`",
            ));
        }
        out = Some((
            names.iter().map(|n| camel(&n.to_string())).collect(),
            attr.span(),
        ));
    }
    Ok(out)
}

/// Removes the macros' `#[args(..)]` from the parameters of `sig` (the
/// compiler has no such attribute).
fn strip_args(sig: &mut syn::Signature) {
    for input in &mut sig.inputs {
        if let FnArg::Typed(t) = input {
            t.attrs.retain(|a| !a.path().is_ident("args"));
        }
    }
}

/// The TypeScript type of a parameter of type `ty`: a callback's with its
/// arguments named when `#[args(..)]` names them (`attrs`), checked at
/// compile time to name each.
fn param_ts(ty: &Type, attrs: &[Attribute]) -> syn::Result<TokenStream2> {
    let ui = quote!(::nsh_core::ui);
    let private = quote!(#ui::__private);
    Ok(match args_attr(attrs)? {
        None => quote!(#private::arg_ts::<#ty>()),
        Some((names, span)) => {
            let n = names.len();
            let check = quote::quote_spanned! {span=>
                const {
                    ::core::assert!(
                        <#ty as #private::NamedArgs>::ARITY == #n,
                        "`#[args(..)]` names each argument of the callback, as many names as it has"
                    )
                }
            };
            quote!({
                #check;
                |types: &mut #ui::TsTypes| #private::named_arg_ts::<#ty>(types, &[#(#names),*])
            })
        }
    })
}

/// The options of `#[prop(...)]`.
#[derive(Default)]
struct PropAttr {
    /// `default` (`Some(None)`) or `default = expr`.
    default: Option<Option<Expr>>,
    rename: Option<String>,
    /// More names a `PropEnum` variant is read from.
    aliases: Vec<String>,
    /// `Children` made only while the view places them.
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
fn kind_is_not_children(children: Option<&Ident>, ident: &Ident) -> bool {
    children != Some(ident)
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
///   while the view places them (made again each time it does, released
///   when it stops: a sheet's content starts fresh when it opens).
/// - `Items`: virtualized rows (in JS `each`, a row renderer and `List`'s
///   data props), at most one, and not with `Children`.
///
/// `#[prop(rename = "…")]` sets the JS name. The fields' doc comments
/// document the props in TypeScript (the component's own is its
/// `#[component] fn`'s; the struct's stays Rust's). The props
/// also get `Default` (every field at its default), for Rust views:
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
    let ui = quote!(::nsh_core::ui);
    let mut specs = Vec::new();
    let mut initial = Vec::new();
    let mut set_arms = Vec::new();
    let mut slot_arms = Vec::new();
    let mut children = None;
    let mut lazy_children = false;
    let mut items: Option<Ident> = None;
    let mut items_js: Option<String> = None;
    // Fields of type `Prop<T>`: held in signals by an instance.
    let mut held = Vec::new();
    let mut updates = Vec::new();
    let mut any_held = false;
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named");
        // A `Prop<T>` is a prop of type `T`, held in a signal.
        let (ty, is_held) = match last_segment(&field.ty) {
            Some((id, Some(inner))) if id == "Prop" => (inner, true),
            _ => (&field.ty, false),
        };
        any_held |= is_held;
        if is_held {
            held.push(quote!(#ident: self.#ident.__hold()));
            updates.push(quote!(self.#ident.__update_from(&new.#ident);));
        } else {
            held.push(quote!(#ident: ::core::clone::Clone::clone(&self.#ident)));
            updates.push(quote! {
                if self.#ident != new.#ident {
                    self.#ident = ::core::clone::Clone::clone(&new.#ident);
                    changed = true;
                }
            });
        }
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
        if is_held && matches!(kind, Kind::Items) {
            return Err(syn::Error::new(
                field.span(),
                "`Items` is not a `Prop`: the rows change, not the field",
            ));
        }
        // How a field takes a new value (`value`): a held one writes its
        // signal.
        let assign = |value: TokenStream2| {
            if is_held {
                quote!(self.#ident.__set(#ui::Reactive::Static(#value)))
            } else {
                quote! {{
                    let changed = self.#ident != #value;
                    self.#ident = #value;
                    changed
                }}
            }
        };
        if is_held {
            initial.push(quote!(#ident: #ui::Prop::new(#init)));
        } else {
            initial.push(quote!(#ident: #init));
        }
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
                        decode: ::core::option::Option::Some(|v| {
                            <#ty as #ui::JsValue>::from_js(v).map(#ui::AnyValue::new)
                        }),
                    }
                });
                let take = assign(quote!(value));
                set_arms.push(quote! {
                    #js => {
                        let value: #ty = match value {
                            ::core::option::Option::Some(v) => v.take::<#ty>().ok_or_else(|| {
                                #ui::PropError(::std::format!("`{}`: a value of another type", #js))
                            })?,
                            ::core::option::Option::None => #init,
                        };
                        ::core::result::Result::Ok(#take)
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
                        decode: ::core::option::Option::None,
                    }
                });
                let take = assign(quote!(slot));
                slot_arms.push(quote! {
                    #js => #take,
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
                        decode: ::core::option::Option::None,
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
                        decode: ::core::option::Option::None,
                    }
                });
                children = Some((ident.clone(), assign(quote!(children))));
                lazy_children = attr.lazy;
            }
        }
        if attr.lazy && kind_is_not_children(children.as_ref().map(|c| &c.0), ident) {
            return Err(syn::Error::new(
                field.span(),
                "`#[prop(lazy)]` is for the `Children` field",
            ));
        }
    }
    let set_children = match &children {
        Some((_, take)) => quote!(#take),
        None => quote!({
            let _ = children;
            false
        }),
    };
    let held_fns = if any_held {
        quote! {
            fn held(&self) -> Self {
                Self { #(#held),* }
            }

            fn update_from(&mut self, new: &Self) -> bool {
                let mut changed = false;
                #(#updates)*
                changed
            }
        }
    } else {
        quote!()
    };
    let items_const = match &items_js {
        Some(js) => quote!(::core::option::Option::Some(#js)),
        None => quote!(::core::option::Option::None),
    };
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        /// Every prop at its default (`..Default::default()` when a Rust
        /// view makes the component).
        impl #impl_generics ::core::default::Default for #name #ty_generics #where_clause {
            fn default() -> Self {
                <Self as #ui::Props>::initial()
            }
        }

        impl #impl_generics #ui::Props for #name #ty_generics #where_clause {
            const ITEMS: ::core::option::Option<&'static str> = #items_const;
            const LAZY_CHILDREN: bool = #lazy_children;

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
                value: ::core::option::Option<#ui::AnyValue>,
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

            #held_fns
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

            fn from_js(value: #ui::JsIn<'_>) -> ::core::result::Result<Self, #ui::PropError> {
                ::core::result::Result::Ok(match value.as_string().as_deref() {
                    #(::core::option::Option::Some(#patterns) => Self::#idents,)*
                    _ => return ::core::result::Result::Err(value.expected(#expected)),
                })
            }

            fn to_js<'a>(
                &self,
                host: &'a dyn #ui::JsHost,
            ) -> ::core::result::Result<#ui::JsOwned<'a>, #ui::PropError> {
                let name = <Self as #ui::PropEnum>::js_name(self);
                ::core::result::Result::Ok(#ui::JsOwned::new(host, host.new_string(name)?))
            }
        }

        impl #ui::PropEnum for #name {
            fn js_name(&self) -> &'static str {
                match self {
                    #(Self::#idents => #names,)*
                }
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
/// handlers. The handler's argument is `value` (one field) or `event`
/// (named fields) in TypeScript; `#[args(index)]` on the variant names it
/// (`onSelect?: (index: number) => void`).
#[proc_macro_derive(Event, attributes(args))]
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
        let named = match args_attr(&v.attrs)? {
            None => None,
            Some((names, _)) if names.len() == 1 && !matches!(v.fields, Fields::Unit) => {
                Some(names[0].clone())
            }
            Some((_, span)) => {
                return Err(syn::Error::new(
                    span,
                    "`#[args(name)]` names the data of an event that has some: one name",
                ));
            }
        };
        let arg = named.unwrap_or_else(|| {
            match &v.fields {
                Fields::Named(_) => "event",
                _ => "value",
            }
            .to_owned()
        });
        match &v.fields {
            Fields::Unit => {
                specs.push(quote! {
                    #ui::EventSpec { name: #js, arg: #arg, data: ::std::string::String::from("void"), doc: #doc }
                });
                name_arms.push(quote!(Self::#ident => #js));
                data_arms.push(quote!(Self::#ident => ::core::result::Result::Ok(#ui::JsOwned::new(host, host.undefined()))));
            }
            Fields::Unnamed(f) if f.unnamed.len() == 1 => {
                let ty = &f.unnamed[0].ty;
                specs.push(quote! {
                    #ui::EventSpec { name: #js, arg: #arg, data: #ui::ts_of::<#ty>()(types), doc: #doc }
                });
                name_arms.push(quote!(Self::#ident(..) => #js));
                data_arms.push(quote!(Self::#ident(v) => #ui::JsValue::to_js(v, host)));
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
                        arg: #arg,
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
                        let owned = #ui::JsOwned::new(host, host.new_object()?);
                        let object = owned.get().raw();
                        #(host.set_field(object, #keys, #ui::JsValue::to_js(#idents, host)?.into_raw())?;)*
                        ::core::result::Result::Ok(owned)
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

            fn data_js<'a>(
                &self,
                host: &'a dyn #ui::JsHost,
            ) -> ::core::result::Result<#ui::JsOwned<'a>, #ui::PropError> {
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

/// `#[component]` on a function: a component that runs once
/// (`Component::view`), named after the function.
///
/// ```ignore
/// /// A number with − and + buttons.
/// #[component]
/// pub fn Stepper(props: StepperProps, cx: Scope<StepperEvent>) -> impl IntoElement {
///     let value = signal(0.0);
///     view().child(text(move || format!("{}", value.get())))
/// }
/// ```
///
/// makes `pub struct Stepper;` and its `Component` impl; the body is its
/// view, run once. Without a `Scope` parameter it emits no
/// events (`NoEvent`); without props it has none (`()`). The function's
/// doc comment documents the component in TypeScript (`Component::DOC`),
/// and a `#[module] impl` of the same name (its functions on the
/// component: `StatusBar.setHidden(..)`).
/// `#[component(methods = StepperApi)]` gives its ref in JS the methods of
/// that [`methods`] API, which the view registers with `cx.methods(..)`.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut methods = None;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("methods") {
            methods = Some(meta.value()?.parse::<syn::Path>()?);
            Ok(())
        } else {
            Err(meta.error("`#[component]` takes `methods = Api`"))
        }
    });
    parse_macro_input!(attr with parser);
    let item = parse_macro_input!(item as ItemFn);
    component_fn(item, methods)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn component_fn(item: ItemFn, methods: Option<syn::Path>) -> syn::Result<TokenStream2> {
    let ui = quote!(::nsh_core::ui);
    let sig = &item.sig;
    if !sig.generics.params.is_empty() || sig.asyncness.is_some() {
        return Err(syn::Error::new(
            sig.span(),
            "a component is a plain function: no generics, not async",
        ));
    }
    let name = &sig.ident;
    let vis = &item.vis;
    let attrs = &item.attrs;
    let body = &item.block;
    let output = &sig.output;
    let mut props: Option<(Box<Pat>, Box<Type>)> = None;
    let mut scope: Option<(Box<Pat>, Box<Type>, Type)> = None;
    for input in &sig.inputs {
        let FnArg::Typed(arg) = input else {
            return Err(syn::Error::new(input.span(), "a component takes no `self`"));
        };
        match last_segment(&arg.ty) {
            Some((id, Some(event))) if id == "Scope" => {
                if scope.is_some() {
                    return Err(syn::Error::new(arg.span(), "one `Scope` parameter"));
                }
                scope = Some((arg.pat.clone(), arg.ty.clone(), event.clone()));
            }
            _ => {
                if props.is_some() {
                    return Err(syn::Error::new(
                        arg.span(),
                        "a component takes its props and its `Scope`",
                    ));
                }
                props = Some((arg.pat.clone(), arg.ty.clone()));
            }
        }
    }
    let (props_pat, props_ty) = match &props {
        Some((pat, ty)) => (quote!(#pat), quote!(#ty)),
        None => (quote!(_), quote!(())),
    };
    let (scope_pat, scope_ty, event) = match &scope {
        Some((pat, ty, event)) => (quote!(#pat), quote!(#ty), quote!(#event)),
        None => (
            quote!(_),
            quote!(#ui::Scope<#ui::NoEvent>),
            quote!(#ui::NoEvent),
        ),
    };
    let output = match output {
        ReturnType::Default => {
            return Err(syn::Error::new(
                sig.span(),
                "a component returns its view: `-> impl IntoElement`",
            ));
        }
        ReturnType::Type(_, ty) => quote!(#ty),
    };
    let methods =
        methods.map(|api| quote!(const METHODS: #ui::Methods = #ui::Methods::of::<#api>();));
    let doc = decl(&docs(attrs));
    Ok(quote! {
        #(#attrs)*
        #[derive(::core::fmt::Debug, ::core::default::Default, ::core::clone::Clone, ::core::marker::Copy)]
        #vis struct #name;

        // A `#[module] impl` of the component's name is documented by it.
        impl #ui::__private::ModuleDoc for #name {
            const DOC: &'static str = #doc;
        }

        impl #ui::Component for #name {
            type Props = #props_ty;
            type Event = #event;
            const DOC: &'static str = #doc;
            #methods

            #[allow(unused_variables)]
            fn view(#props_pat: #props_ty, #scope_pat: #scope_ty) -> #output #body
        }
    })
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
/// It also makes `pub fn registry() -> Registry` at the crate's root: the
/// set, for tests and other Rust code (`my_app_native::registry()`, next to
/// `nsh_ui_mobile::registry()`), so the crate's root has no other item
/// named `registry`.
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
            fn #name(mut self, length: impl crate::ui::IntoReactive<crate::ui::Length>) -> Self {
                self.style().bind_value(length, |s, length: crate::ui::Length| {
                    s.lengths(&[#(crate::style::StyleKey::#keys),*], length);
                });
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

/// `#[methods]` on an `impl` block of a component's API (`Api`: a handle
/// its view makes, holding the signals the methods read and write): each
/// function with a visibility (`pub fn`, `pub(crate) fn`) becomes a method
/// of the component's ref in JS, named in camel case (`set_value` is
/// `setValue`), documented by its doc comment; a function without one is a
/// helper, Rust's only. A
/// function takes `&self`, then [`JsValue`] parameters, and returns
/// nothing or a [`JsValue`]: JS gets a `Promise` of it (`Promise<void>`
/// for nothing, settled once the method ran), the call ordered with the
/// props set before it. The view registers the API with
/// `cx.methods(api)`, and the component names it in its spec with
/// `#[component(methods = StepperApi)]`.
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

fn methods_impl(mut item: ItemImpl) -> syn::Result<TokenStream2> {
    let ui = quote!(::nsh_core::ui);
    let private = quote!(#ui::__private);
    let self_ty = item.self_ty.clone();
    let generics = item.generics.clone();
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let mut entries = Vec::new();
    for member in &item.items {
        let ImplItem::Fn(f) = member else { continue };
        if !exported(&f.vis) {
            continue;
        }
        let ident = &f.sig.ident;
        let js = camel(&ident.to_string());
        // The method's name: the host puts its component's before it (an
        // API may serve several components).
        let what = js.clone();
        let doc = decl(&docs(&f.attrs));
        let mut inputs = f.sig.inputs.iter();
        match inputs.next() {
            Some(FnArg::Receiver(r)) if r.reference.is_some() && r.mutability.is_none() => {}
            _ => {
                return Err(syn::Error::new(
                    f.sig.span(),
                    "a JS method takes `&self` (the API holds signals)",
                ));
            }
        }
        let mut params = Vec::new();
        let mut refs = Vec::new();
        let mut decode = Vec::new();
        let mut call_args = Vec::new();
        let typed: Vec<_> = inputs
            .map(|arg| match arg {
                FnArg::Typed(t) => Ok(t),
                FnArg::Receiver(r) => Err(syn::Error::new(r.span(), "one receiver")),
            })
            .collect::<syn::Result<_>>()?;
        for (i, arg) in typed.iter().enumerate() {
            let ty = &arg.ty;
            let Pat::Ident(name) = &*arg.pat else {
                return Err(syn::Error::new(
                    arg.pat.span(),
                    "a JS method's parameters are named",
                ));
            };
            let name = camel(&name.ident.to_string());
            let local = format_ident!("__arg{}", i);
            let ts = param_ts(ty, &arg.attrs)?;
            params.push(quote!((#name, #ts)));
            refs.push(quote!(#private::arg_classes::<#ty>(__out);));
            decode.push(quote! {
                let #local: #ty = #private::arg::<#ty>(host, #i, #what, #name)?;
            });
            call_args.push(quote!(#local));
        }
        let (returns, finish) = match &f.sig.output {
            ReturnType::Default => (
                quote!(::core::option::Option::None),
                quote!(#private::boxed(__result)),
            ),
            ReturnType::Type(_, ty) => match result_ok(ty) {
                Some(ok) => (
                    quote!(::core::option::Option::Some(#private::ret_ts::<#ok>())),
                    quote!(#private::boxed_result(__result)),
                ),
                None => (
                    quote!(::core::option::Option::Some(#private::ret_ts::<#ty>())),
                    quote!(#private::boxed(__result)),
                ),
            },
        };
        if let ReturnType::Type(_, ty) = &f.sig.output {
            let ty = result_ok(ty).unwrap_or(&**ty);
            refs.push(quote!(#private::ret_classes::<#ty>(__out);));
        }
        let cfgs = f.attrs.iter().filter(|a| a.path().is_ident("cfg"));
        entries.push(quote! {
            #(#cfgs)*
            #ui::Method {
                name: #js,
                doc: #doc,
                params: &[#(#params),*],
                returns: #returns,
                refs: |__out| {
                    let _ = &__out;
                    #(#refs)*
                },
                call: |host| {
                    let _ = host;
                    #(#decode)*
                    ::core::result::Result::Ok(::std::boxed::Box::new(move |api: &dyn ::core::any::Any| {
                        let this = api.downcast_ref::<Self>()?;
                        let __result = Self::#ident(this, #(#call_args),*);
                        ::core::option::Option::Some(#finish)
                    }))
                },
            }
        });
    }
    for member in &mut item.items {
        if let ImplItem::Fn(f) = member {
            strip_args(&mut f.sig);
        }
    }
    Ok(quote! {
        #item

        impl #impl_generics #ui::Api for #self_ty #where_clause {
            const METHODS: &'static [#ui::Method] = &[#(#entries),*];
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
    let mut from_js = Vec::new();
    let mut to_js = Vec::new();
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
        from_js.push(quote! {
            #ident: {
                let v = value.field(#js)?;
                let v = v.get();
                if v.kind() != #ui::JsKind::Undefined && (#is_option || !v.is_nullish()) {
                    <#ty as #ui::JsValue>::from_js(v)
                        .map_err(|e| #ui::PropError(::std::format!("`{}`: {}", #js, e)))?
                } else {
                    #missing
                }
            }
        });
        to_js.push(quote! {
            host.set_field(object, #js, #ui::JsValue::to_js(&self.#ident, host)?.into_raw())?;
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

            fn from_js(value: #ui::JsIn<'_>) -> ::core::result::Result<Self, #ui::PropError> {
                if value.kind() != #ui::JsKind::Object {
                    return ::core::result::Result::Err(value.expected("an object"));
                }
                ::core::result::Result::Ok(Self { #(#from_js),* })
            }

            fn to_js<'a>(
                &self,
                host: &'a dyn #ui::JsHost,
            ) -> ::core::result::Result<#ui::JsOwned<'a>, #ui::PropError> {
                let owned = #ui::JsOwned::new(host, host.new_object()?);
                let object = owned.get().raw();
                #(#to_js)*
                ::core::result::Result::Ok(owned)
            }
        }
    })
}

/// Whether `ty` is `&mut ModuleCx`: a module function's context.
fn module_cx(ty: &Type) -> bool {
    matches!(ty, Type::Reference(r) if matches!(last_segment(&r.elem), Some((id, _)) if id == "ModuleCx"))
}

/// `#[module]` on an `impl` block of a unit struct: its functions with a
/// visibility (`pub fn`, `pub(crate) fn`; no receiver) become functions of
/// a JS object named after the struct
/// (`Alert.alert(...)`), documented by their doc comments. The struct
/// takes `#[module]` too, and its doc comment documents the object (a doc
/// comment on the block is an error: TS shows the struct's); a
/// component's struct (`#[component]`) needs none. The signature decides
/// how a call runs:
/// a function without a context runs in the JS call and returns its value
/// (`checksum(bytes)`), as a class's method does; one whose last parameter
/// is `cx: &mut ModuleCx` is an op, run in order with what JS set before it
/// (a dialog, the status bar, focus), a `Promise` (`Promise<void>` when it
/// returns nothing: settled once the op ran);
/// an `async fn` runs as a task of the core with the free functions views
/// use (`dialog(..)`, `clipboard_string()`, ...), a `Promise`. Parameters
/// are [`JsValue`] types (a trailing `Option` is optional), instances
/// (`Instance<T>`) and JS functions (`Callback<A, R>`; `#[args(name,
/// amount)]` on the parameter names the function's arguments in TS); a
/// function returns
/// nothing, a [`JsValue`], an instance, or a `Result` of one (an `Err`
/// throws or rejects with its message). A UI set registers the module
/// (`Registry::module::<Alert>()`); a component exported under the same
/// name gets the functions too (`StatusBar.setHidden(...)`).
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
    let item = parse_macro_input!(item as syn::Item);
    match item {
        syn::Item::Struct(s) if !flat => module_struct(s),
        syn::Item::Impl(i) => module_impl(i, flat),
        syn::Item::Struct(s) => Err(syn::Error::new(
            s.ident.span(),
            "`flat` goes on the `#[module(flat)] impl` block; the struct takes `#[module]`",
        )),
        other => Err(syn::Error::new(
            other.span(),
            "`#[module]` goes on a unit struct and on its `impl` block",
        )),
    }
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}

/// `#[module]` on the struct: its doc comment documents the module in TS.
fn module_struct(item: syn::ItemStruct) -> syn::Result<TokenStream2> {
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new(
            item.generics.span(),
            "a module has no generic parameters",
        ));
    }
    let ui = quote!(::nsh_core::ui);
    let name = &item.ident;
    let doc = decl(&docs(&item.attrs));
    Ok(quote! {
        #item

        impl #ui::__private::ModuleDoc for #name {
            const DOC: &'static str = #doc;
        }
    })
}

/// Refuses a doc comment on an export's `impl` block (`what`: `#[module]`):
/// TS shows the struct's, so one here would be lost.
fn no_block_doc(item: &ItemImpl, what: &str) -> syn::Result<()> {
    match item.attrs.iter().find(|a| a.path().is_ident("doc")) {
        Some(doc) => Err(syn::Error::new(
            doc.span(),
            format!(
                "document the export on its struct, not on the `{what} impl` block: the struct's \
                 doc comment is the one TS shows (a `//` comment here stays in Rust)"
            ),
        )),
        None => Ok(()),
    }
}

/// `#[class]` on a struct and on its `impl` block: a JS class (the
/// struct's name) whose objects hold the Rust value. The block's functions
/// with a visibility (`pub fn`, `pub(crate) fn`) are the class's members;
/// those without one are helpers, Rust's only. In the block, `new` is
/// the constructor (a class without one is made by its members only);
/// `&self` / `&mut self` functions are methods, `#[getter]` / `#[setter]`
/// ones (`count`, `set_count`) the property `count`; functions without a
/// receiver are statics. They run in the JS call: their parameters are
/// [`JsValue`] types (a trailing `Option` is optional), instances
/// (`&Other` / `&mut Other` borrowed for the call, `Instance<Other>` kept)
/// and JS functions (`Callback<A, R>`; `#[args(name, amount)]` on the
/// parameter names the function's arguments in TS);
/// they return nothing, a value, `Self` or another class's value (a new JS
/// object), an `Instance` (its JS object), or a `Result` of one (an `Err`
/// throws). An `async fn` (`this: Instance<Self>` first, for a method)
/// returns a promise and runs as a task of the core, with the free
/// functions (`dialog(..)`, `timer(..)`, ...); its parameters are owned
/// (`Instance<Other>`, not `&Other`). Doc comments document the class and
/// its members. On the struct, the fields whose types name `Callback` or
/// `Instance` report the callbacks they keep to JS's collector.
#[proc_macro_attribute]
pub fn class(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(Span::call_site(), "`#[class]` takes no arguments")
            .into_compile_error()
            .into();
    }
    let item = parse_macro_input!(item as syn::Item);
    class::class(item)
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

fn module_impl(mut item: ItemImpl, flat: bool) -> syn::Result<TokenStream2> {
    let ui = quote!(::nsh_core::ui);
    let private = quote!(#ui::__private);
    let self_ty = &item.self_ty.clone();
    let Some((type_name, _)) = last_segment(self_ty) else {
        return Err(syn::Error::new(
            self_ty.span(),
            "`#[module]` is for a named type",
        ));
    };
    let module_name = type_name.to_string();
    no_block_doc(&item, "#[module]")?;
    let mut entries = Vec::new();
    for member in &item.items {
        let ImplItem::Fn(f) = member else { continue };
        if !exported(&f.vis) {
            continue;
        }
        let ident = &f.sig.ident;
        let js = camel(&ident.to_string());
        let what = if flat {
            js.clone()
        } else {
            format!("{module_name}.{js}")
        };
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
        let takes_cx = typed.last().is_some_and(|t| module_cx(&t.ty));
        let mut params = Vec::new();
        let mut names = Vec::new();
        let mut ts = Vec::new();
        for (i, arg) in typed.iter().enumerate() {
            let ty = &arg.ty;
            if module_cx(ty) {
                if i + 1 != typed.len() || is_async {
                    return Err(syn::Error::new(
                        arg.span(),
                        "the last parameter of a function that is not `async`: `cx: &mut ModuleCx` \
                         (an `async fn` uses the free functions: `dialog(..)`, ...)",
                    ));
                }
                continue;
            }
            if let Type::Reference(_) = &**ty {
                return Err(syn::Error::new(
                    ty.span(),
                    "a module function takes values: an instance as `Instance<T>`",
                ));
            }
            let Pat::Ident(name) = &*arg.pat else {
                return Err(syn::Error::new(
                    arg.pat.span(),
                    "a module function's parameters are named",
                ));
            };
            names.push(camel(&name.ident.to_string()));
            ts.push(param_ts(ty, &arg.attrs)?);
            params.push(ty);
        }
        // An `Option` parameter is optional when the ones after it are too.
        let mut optional = vec![false; params.len()];
        for i in (0..params.len()).rev() {
            if !matches!(last_segment(params[i]), Some((id, _)) if id == "Option") {
                break;
            }
            optional[i] = true;
        }
        let param_specs = ts
            .iter()
            .zip(&names)
            .zip(&optional)
            .map(|((ts, name), optional)| quote!((#name, #ts, #optional)));
        let (returns, fallible) = match &f.sig.output {
            ReturnType::Default => (quote!(::core::option::Option::None), false),
            ReturnType::Type(_, ty) => match result_ok(ty) {
                Some(ok) => (
                    quote!(::core::option::Option::Some(#private::ret_ts::<#ok>())),
                    true,
                ),
                None => (
                    quote!(::core::option::Option::Some(#private::ret_ts::<#ty>())),
                    false,
                ),
            },
        };
        // The classes the signature names, registered with the module.
        let ret_refs = match &f.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => {
                let ty = result_ok(ty).unwrap_or(&**ty);
                Some(quote!(#private::ret_classes::<#ty>(__out);))
            }
        };
        let refs = params
            .iter()
            .map(|ty| quote!(#private::arg_classes::<#ty>(__out);))
            .chain(ret_refs);
        let locals: Vec<_> = (0..params.len())
            .map(|i| format_ident!("__arg{}", i))
            .collect();
        let call = if !is_async && !takes_cx {
            // In the JS call, converting the values directly.
            let decode = params.iter().zip(&names).zip(&locals).enumerate().map(
                |(i, ((ty, name), local))| {
                    quote!(let #local: #ty = #private::arg::<#ty>(host, #i, #what, #name)?;)
                },
            );
            let finish = if fallible {
                quote!(#private::ret_result(host, __result))
            } else {
                quote!(#private::ret(host, __result))
            };
            quote! {
                #ui::FunctionCall::Sync(|host| {
                    #(#decode)*
                    let __result = #self_ty::#ident(#(#locals),*);
                    #finish
                })
            }
        } else {
            // Converted in the JS call; the op carries them.
            let decode = params.iter().zip(&names).zip(&locals).enumerate().map(
                |(i, ((ty, name), local))| {
                    quote!(let #local: #ty = #private::arg::<#ty>(host, #i, #what, #name)?;)
                },
            );
            let finish = if fallible {
                quote!(#private::boxed_result(__result))
            } else {
                quote!(#private::boxed(__result))
            };
            if is_async {
                quote! {
                    #ui::FunctionCall::Async(|host| {
                        let _ = host;
                        #(#decode)*
                        ::core::result::Result::Ok(::std::boxed::Box::new(move || {
                            ::std::boxed::Box::pin(async move {
                                let __result = #self_ty::#ident(#(#locals),*).await;
                                #finish
                            })
                        }))
                    })
                }
            } else {
                quote! {
                    #ui::FunctionCall::Op(|host| {
                        let _ = host;
                        #(#decode)*
                        ::core::result::Result::Ok(::std::boxed::Box::new(move |cx| {
                            let __result = #self_ty::#ident(#(#locals,)* cx);
                            #finish
                        }))
                    })
                }
            }
        };
        // `#[cfg]` of the function applies to its entry too.
        let cfgs = f.attrs.iter().filter(|a| a.path().is_ident("cfg"));
        entries.push(quote! {
            #(#cfgs)*
            #ui::Function {
                name: #js,
                doc: #doc,
                params: &[#(#param_specs),*],
                returns: #returns,
                refs: |__out| {
                    let _ = &__out;
                    #(#refs)*
                },
                call: #call,
            }
        });
    }
    for member in &mut item.items {
        if let ImplItem::Fn(f) = member {
            strip_args(&mut f.sig);
        }
    }
    Ok(quote! {
        #item

        impl #ui::Module for #self_ty {
            const NAME: &'static str = #module_name;
            const DOC: &'static str = <Self as #ui::__private::ModuleDoc>::DOC;
            const FUNCTIONS: &'static [#ui::Function] = &[#(#entries),*];
            const FLAT: bool = #flat;
        }
    })
}

#[cfg(test)]
mod tests {
    /// The TS doc of a doc comment of `lines`.
    fn ts(lines: &[&str]) -> String {
        let attrs: Vec<syn::Attribute> = lines
            .iter()
            .map(|l| syn::parse_quote!(#[doc = #l]))
            .collect();
        super::docs(&attrs)
    }

    #[test]
    fn sections_under_a_heading_stay_rusts() {
        assert_eq!(
            ts(&[
                " Copies `text`; resolves with its length.",
                "",
                " # Rust",
                "",
                " Taking `cx` makes it an op.",
            ]),
            "Copies `text`; resolves with its length."
        );
        assert_eq!(
            ts(&[" Sorts [`Item`]s.", " ## Panics", " Never."]),
            "Sorts `Item`s."
        );
        // Not headings: a `#` in a code block, an attribute, a `#` word.
        assert_eq!(
            ts(&[
                " A list:",
                " ```",
                " # use x;",
                " ```",
                " #[ignore] and #1."
            ]),
            "A list: ``` # use x; ``` #[ignore] and #1."
        );
    }
}
