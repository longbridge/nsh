//! `#[class]`: a Rust type TypeScript uses as a class. On the struct it
//! makes the struct a result (a new instance); on the `impl` block it
//! makes the class's members (`Class`).

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, FnArg, ImplItem, ImplItemFn, ItemImpl, ItemStruct, Pat, ReturnType, Type,
    spanned::Spanned,
};

use crate::{camel, decl, docs, last_segment, module_cx, result_ok};

pub(crate) fn class(item: syn::Item) -> syn::Result<TokenStream2> {
    match item {
        syn::Item::Struct(s) => class_struct(s),
        syn::Item::Impl(i) => class_impl(i),
        other => Err(syn::Error::new(
            other.span(),
            "`#[class]` goes on a struct and on its `impl` block",
        )),
    }
}

fn class_struct(item: ItemStruct) -> syn::Result<TokenStream2> {
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new(
            item.generics.span(),
            "a class has no generic parameters",
        ));
    }
    let ui = quote!(::nsh_core::ui);
    let name = &item.ident;
    let doc = decl(&docs(&item.attrs));
    Ok(quote! {
        #item

        impl #ui::__private::ClassDoc for #name {
            const DOC: &'static str = #doc;
        }

        /// A new instance (a new JS object).
        impl #ui::Ret for #name {
            fn ts_type(types: &mut #ui::TsTypes) -> ::std::string::String {
                #ui::__private::class_ts::<Self>(types)
            }

            fn into_js<'a>(
                self,
                host: &'a dyn #ui::JsHost,
            ) -> ::core::result::Result<#ui::JsOwned<'a>, #ui::PropError> {
                #ui::__private::class_into_js(self, host)
            }
        }
    })
}

/// Whether `attrs` has `#[name]`.
fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

/// What a function of the block is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Constructor,
    Method { mutable: bool },
    Getter,
    Setter { mutable: bool },
    Static,
}

/// The `&self` / `&mut self` receiver of `f`: `Some(mutable)`.
fn receiver(f: &ImplItemFn) -> syn::Result<Option<bool>> {
    match f.sig.inputs.first() {
        Some(FnArg::Receiver(r)) => {
            if r.reference.is_none() {
                return Err(syn::Error::new(
                    r.span(),
                    "a class's method takes `&self` or `&mut self` (the JS object owns the value)",
                ));
            }
            Ok(Some(r.mutability.is_some()))
        }
        _ => Ok(None),
    }
}

fn kind_of(f: &ImplItemFn) -> syn::Result<Kind> {
    let getter = has_attr(&f.attrs, "getter");
    let setter = has_attr(&f.attrs, "setter");
    if f.sig.asyncness.is_some() {
        return Err(syn::Error::new(
            f.sig.span(),
            "a class's members are not `async` (they run in the JS call)",
        ));
    }
    let receiver = receiver(f)?;
    if getter || setter {
        let Some(mutable) = receiver else {
            return Err(syn::Error::new(
                f.sig.span(),
                "a getter or setter takes `&self` (a setter may take `&mut self`)",
            ));
        };
        let params = f.sig.inputs.len() - 1;
        return if getter {
            if params != 0 || matches!(f.sig.output, ReturnType::Default) {
                Err(syn::Error::new(
                    f.sig.span(),
                    "a getter takes only `&self` and returns its value",
                ))
            } else {
                Ok(Kind::Getter)
            }
        } else if params != 1 || !f.sig.ident.to_string().starts_with("set_") {
            Err(syn::Error::new(
                f.sig.span(),
                "a setter is `set_<name>(&mut self, value: T)` (the property `<name>`)",
            ))
        } else {
            Ok(Kind::Setter { mutable })
        };
    }
    Ok(match receiver {
        Some(mutable) => Kind::Method { mutable },
        None if f.sig.ident == "new" => Kind::Constructor,
        None => Kind::Static,
    })
}

fn class_impl(mut item: ItemImpl) -> syn::Result<TokenStream2> {
    let ui = quote!(::nsh_core::ui);
    if !item.generics.params.is_empty() || item.trait_.is_some() {
        return Err(syn::Error::new(
            item.span(),
            "`#[class]` goes on the class's own `impl` block, without generics",
        ));
    }
    let self_ty = item.self_ty.clone();
    let Some((type_name, _)) = last_segment(&self_ty) else {
        return Err(syn::Error::new(
            self_ty.span(),
            "`#[class]` is for a named type",
        ));
    };
    let class_name = type_name.to_string();
    let mut members = Vec::new();
    for member in &item.items {
        let ImplItem::Fn(f) = member else { continue };
        members.push(member_def(f, &class_name)?);
    }
    // The getters' and setters' attributes are the macro's.
    for member in &mut item.items {
        if let ImplItem::Fn(f) = member {
            f.attrs
                .retain(|a| !a.path().is_ident("getter") && !a.path().is_ident("setter"));
        }
    }
    Ok(quote! {
        #item

        impl #ui::Class for #self_ty {
            const DEF: &'static #ui::ClassDef = &#ui::ClassDef {
                name: #class_name,
                doc: <Self as #ui::__private::ClassDoc>::DOC,
                type_id: #ui::__private::type_id::<Self>,
                members: &[#(#members),*],
            };
        }
    })
}

fn member_def(f: &ImplItemFn, class: &str) -> syn::Result<TokenStream2> {
    let ui = quote!(::nsh_core::ui);
    let private = quote!(#ui::__private);
    let kind = kind_of(f)?;
    let ident = &f.sig.ident;
    let rust_name = ident.to_string();
    let js = match kind {
        Kind::Setter { .. } => camel(rust_name.trim_start_matches("set_")),
        _ => camel(&rust_name),
    };
    let what = match kind {
        Kind::Constructor => format!("new {class}"),
        _ => format!("{class}.{js}"),
    };
    // Names the class's object has already, or must keep.
    let reserved = match kind {
        Kind::Constructor => None,
        Kind::Static if matches!(js.as_str(), "prototype" | "name" | "length") => Some(
            "a static named `prototype`, `name` or `length` would replace the constructor's own property",
        ),
        _ if js == "dispose" => Some("`dispose` is every class's own (it frees the Rust value)"),
        _ if js == "constructor" => Some("`constructor` is the prototype's own (the class)"),
        _ => None,
    };
    if let Some(why) = reserved {
        return Err(syn::Error::new(
            ident.span(),
            format!("rename `{rust_name}`: {why}"),
        ));
    }
    let doc = decl(&docs(&f.attrs));
    // `#[cfg]` of the function applies to its entry too.
    let cfgs: Vec<_> = f
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("cfg"))
        .collect();
    // The parameters JS passes (not the receiver).
    let skip = match kind {
        Kind::Method { .. } | Kind::Getter | Kind::Setter { .. } => 1,
        _ => 0,
    };
    let mut params: Vec<&Type> = Vec::new();
    let mut names = Vec::new();
    for arg in f.sig.inputs.iter().skip(skip) {
        let FnArg::Typed(t) = arg else {
            return Err(syn::Error::new(arg.span(), "one receiver"));
        };
        if module_cx(&t.ty) {
            return Err(syn::Error::new(
                t.span(),
                "a class's member takes no `ModuleCx` (it computes in the JS call)",
            ));
        }
        let Pat::Ident(name) = &*t.pat else {
            return Err(syn::Error::new(
                t.pat.span(),
                "a class member's parameters are named",
            ));
        };
        names.push(camel(&name.ident.to_string()));
        if let Type::Reference(_) = &*t.ty {
            return Err(syn::Error::new(
                t.ty.span(),
                "a class member's parameters are values (`JsValue` types)",
            ));
        }
        params.push(&t.ty);
    }
    // An `Option` parameter is optional when the ones after it are too (a
    // setter's value never is: TypeScript requires it).
    let mut optional = vec![false; params.len()];
    for i in (0..params.len()).rev() {
        if matches!(kind, Kind::Setter { .. }) {
            break;
        }
        let is_option = matches!(last_segment(params[i]), Some((id, _)) if id == "Option");
        if !is_option {
            break;
        }
        optional[i] = true;
    }
    let param_specs: Vec<TokenStream2> = params
        .iter()
        .zip(&names)
        .zip(&optional)
        .map(|((ty, name), optional)| quote!((#name, #private::arg_ts::<#ty>(), #optional)))
        .collect();
    let (returns, ok_ty, fallible) = match &f.sig.output {
        ReturnType::Default => (quote!(::core::option::Option::None), None, false),
        ReturnType::Type(_, ty) => match result_ok(ty) {
            Some(ok) => (
                quote!(::core::option::Option::Some(#private::ret_ts::<#ok>())),
                Some(ok.clone()),
                true,
            ),
            None => (
                quote!(::core::option::Option::Some(#private::ret_ts::<#ty>())),
                Some((**ty).clone()),
                false,
            ),
        },
    };
    let member_kind = match kind {
        Kind::Constructor => quote!(Constructor),
        Kind::Method { .. } => quote!(Method),
        Kind::Getter => quote!(Getter),
        Kind::Setter { .. } => quote!(Setter),
        Kind::Static => quote!(Static),
    };
    if kind == Kind::Constructor {
        let ok_is_self = matches!(&ok_ty, Some(ty) if matches!(last_segment(ty), Some((id, _)) if id == "Self" || id == class));
        if !ok_is_self {
            return Err(syn::Error::new(
                f.sig.span(),
                "the constructor `new` returns `Self` (or `Result<Self, E>`)",
            ));
        }
    }
    let call = sync_call(f, kind, &params, &names, &what, fallible);
    Ok(quote! {
        #(#cfgs)*
        #ui::Member {
            name: #js,
            doc: #doc,
            kind: #ui::MemberKind::#member_kind,
            params: &[#(#param_specs),*],
            returns: #returns,
            call: #call,
        }
    })
}

fn sync_call(
    f: &ImplItemFn,
    kind: Kind,
    params: &[&Type],
    names: &[String],
    what: &str,
    fallible: bool,
) -> TokenStream2 {
    let ui = quote!(::nsh_core::ui);
    let private = quote!(#ui::__private);
    let ident = &f.sig.ident;
    let mut decode = Vec::new();
    let mut args = Vec::new();
    for (i, (ty, name)) in params.iter().zip(names).enumerate() {
        let local = format_ident!("__arg{}", i);
        decode.push(quote! {
            let #local: #ty = #private::arg::<#ty>(host, #i, #what, #name)?;
        });
        args.push(quote!(#local));
    }
    let (this, receiver) = match kind {
        Kind::Method { mutable } | Kind::Setter { mutable } => {
            let borrow_fn = if mutable {
                quote!(borrow_mut)
            } else {
                quote!(borrow)
            };
            let by = if mutable {
                quote!(&mut *__value)
            } else {
                quote!(&*__value)
            };
            (
                quote! {
                    let __this = #private::this::<Self>(host, #what)?;
                    #[allow(unused_mut)]
                    let mut __value = #private::#borrow_fn::<Self>(&__this)?;
                },
                Some(by),
            )
        }
        Kind::Getter => (
            quote! {
                let __this = #private::this::<Self>(host, #what)?;
                let __value = #private::borrow::<Self>(&__this)?;
            },
            Some(quote!(&*__value)),
        ),
        _ => (quote!(), None),
    };
    let call_args = receiver.into_iter().chain(args);
    let finish = match (kind, fallible) {
        (Kind::Constructor, false) => quote!(#private::construct(host, __result)),
        (Kind::Constructor, true) => quote!(#private::construct_result(host, __result)),
        (_, false) => quote!(#private::ret(host, __result)),
        (_, true) => quote!(#private::ret_result(host, __result)),
    };
    // The borrows end before the result reaches JS (making its object may
    // run the collector, which reads the instance).
    let release = match kind {
        Kind::Method { .. } | Kind::Setter { .. } | Kind::Getter => {
            quote!(::core::mem::drop(__value);)
        }
        _ => quote!(),
    };
    quote! {
        #ui::MemberCall::Sync(|host| {
            #(#decode)*
            #this
            let __result = Self::#ident(#(#call_args),*);
            #release
            #finish
        })
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    /// The error `#[class]` gives `item`.
    fn error(item: proc_macro2::TokenStream) -> String {
        match super::class(syn::parse2(item).unwrap()) {
            Ok(_) => "no error".to_owned(),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn names_the_object_has_are_refused() {
        assert!(
            error(quote!(impl T { fn dispose(&self) {} }))
                .contains("`dispose` is every class's own")
        );
        assert!(error(quote!(impl T { fn constructor(&self) {} })).contains("`constructor`"));
        for name in ["prototype", "name", "length"] {
            let ident = quote::format_ident!("{name}");
            assert!(
                error(quote!(impl T { fn #ident() -> u32 { 0 } })).contains("a static named"),
                "{name}"
            );
        }
        // A method may be called `name` (it is on the prototype).
        assert_eq!(
            error(quote!(impl T { fn name(&self) -> u32 { 0 } })),
            "no error"
        );
    }
}
