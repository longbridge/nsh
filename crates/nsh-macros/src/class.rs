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

            fn into_wire(self, instances: &#ui::Instances) -> #ui::Wire {
                #ui::__private::class_into_wire(self, instances)
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
    AsyncMethod,
    AsyncStatic,
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

/// Whether `pat: ty` is `this: Instance<..>` (an `async` method's).
fn is_this(arg: &FnArg) -> bool {
    let FnArg::Typed(t) = arg else { return false };
    let named_this = matches!(&*t.pat, Pat::Ident(p) if p.ident == "this");
    named_this && matches!(last_segment(&t.ty), Some((id, _)) if id == "Instance")
}

fn kind_of(f: &ImplItemFn) -> syn::Result<Kind> {
    let getter = has_attr(&f.attrs, "getter");
    let setter = has_attr(&f.attrs, "setter");
    let receiver = receiver(f)?;
    if f.sig.asyncness.is_some() {
        if receiver.is_some() {
            return Err(syn::Error::new(
                f.sig.span(),
                "an `async` method takes `this: Instance<Self>` (a handle: borrow the value \
                 between awaits with `this.with(..)` / `this.with_mut(..)`)",
            ));
        }
        if getter || setter {
            return Err(syn::Error::new(
                f.sig.span(),
                "getters and setters are not `async`",
            ));
        }
        return Ok(match f.sig.inputs.first() {
            Some(arg) if is_this(arg) => Kind::AsyncMethod,
            _ => Kind::AsyncStatic,
        });
    }
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

/// A parameter: how the generated code gets it.
enum Param<'a> {
    /// An `Arg`.
    Value(&'a Type),
    /// `&Other` / `&mut Other`: an instance borrowed for the call.
    Borrowed { elem: &'a Type, mutable: bool },
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
    // The parameters JS passes (not the receiver nor `this`).
    let skip = match kind {
        Kind::Method { .. } | Kind::Getter | Kind::Setter { .. } | Kind::AsyncMethod => 1,
        _ => 0,
    };
    let is_async = matches!(kind, Kind::AsyncMethod | Kind::AsyncStatic);
    let mut params = Vec::new();
    let mut names = Vec::new();
    for arg in f.sig.inputs.iter().skip(skip) {
        let FnArg::Typed(t) = arg else {
            return Err(syn::Error::new(arg.span(), "one receiver"));
        };
        if module_cx(&t.ty) {
            return Err(syn::Error::new(
                t.span(),
                "a class's member takes no `ModuleCx` (a synchronous member computes; an `async` \
                 one uses the free functions: `dialog(..)`, ...)",
            ));
        }
        let Pat::Ident(name) = &*t.pat else {
            return Err(syn::Error::new(
                t.pat.span(),
                "a class member's parameters are named",
            ));
        };
        names.push(camel(&name.ident.to_string()));
        params.push(match &*t.ty {
            Type::Reference(_) if is_async => {
                return Err(syn::Error::new(
                    t.ty.span(),
                    "an `async` member keeps its arguments: take `Instance<T>`, not a reference",
                ));
            }
            Type::Reference(r) => Param::Borrowed {
                elem: &r.elem,
                mutable: r.mutability.is_some(),
            },
            ty => Param::Value(ty),
        });
    }
    // An `Option` parameter is optional when the ones after it are too (a
    // setter's value never is: TypeScript requires it).
    let mut optional = vec![false; params.len()];
    for i in (0..params.len()).rev() {
        if matches!(kind, Kind::Setter { .. }) {
            break;
        }
        let is_option = matches!(&params[i], Param::Value(ty) if matches!(last_segment(ty), Some((id, _)) if id == "Option"));
        if !is_option {
            break;
        }
        optional[i] = true;
    }
    let param_specs: Vec<TokenStream2> = params
        .iter()
        .zip(&names)
        .zip(&optional)
        .map(|((p, name), optional)| {
            let ts = match p {
                Param::Value(ty) => quote!(#private::arg_ts::<#ty>()),
                Param::Borrowed { elem, .. } => quote!(#private::arg_ts::<#ui::Instance<#elem>>()),
            };
            quote!((#name, #ts, #optional))
        })
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
        Kind::Method { .. } | Kind::AsyncMethod => quote!(Method),
        Kind::Getter => quote!(Getter),
        Kind::Setter { .. } => quote!(Setter),
        Kind::Static | Kind::AsyncStatic => quote!(Static),
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
    let call = if is_async {
        async_call(f, kind, &params, &names, fallible)
    } else {
        sync_call(f, kind, &params, &names, &what, fallible)
    };
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
    params: &[Param<'_>],
    names: &[String],
    what: &str,
    fallible: bool,
) -> TokenStream2 {
    let ui = quote!(::nsh_core::ui);
    let private = quote!(#ui::__private);
    let ident = &f.sig.ident;
    let mut decode = Vec::new();
    let mut borrow = Vec::new();
    let mut release = Vec::new();
    let mut args = Vec::new();
    for (i, (p, name)) in params.iter().zip(names).enumerate() {
        let local = format_ident!("__arg{}", i);
        match p {
            Param::Value(ty) => {
                decode.push(quote! {
                    let #local: #ty = #private::arg::<#ty>(host, #i, #what, #name)?;
                });
                args.push(quote!(#local));
            }
            Param::Borrowed { elem, mutable } => {
                let held = format_ident!("__held{}", i);
                decode.push(quote! {
                    let #held: #ui::Instance<#elem> =
                        #private::arg::<#ui::Instance<#elem>>(host, #i, #what, #name)?;
                });
                if *mutable {
                    borrow.push(quote!(let mut #local = #private::borrow_mut::<#elem>(&#held)?;));
                    args.push(quote!(&mut *#local));
                } else {
                    borrow.push(quote!(let #local = #private::borrow::<#elem>(&#held)?;));
                    args.push(quote!(&*#local));
                }
                release.push(quote!(::core::mem::drop(#local);));
            }
        }
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
    if matches!(
        kind,
        Kind::Method { .. } | Kind::Setter { .. } | Kind::Getter
    ) {
        release.push(quote!(::core::mem::drop(__value);));
    }
    quote! {
        #ui::MemberCall::Sync(|host| {
            #(#decode)*
            #this
            #(#borrow)*
            let __result = Self::#ident(#(#call_args),*);
            #(#release)*
            #finish
        })
    }
}

fn async_call(
    f: &ImplItemFn,
    kind: Kind,
    params: &[Param<'_>],
    names: &[String],
    fallible: bool,
) -> TokenStream2 {
    let ui = quote!(::nsh_core::ui);
    let private = quote!(#ui::__private);
    let ident = &f.sig.ident;
    let mut decode = Vec::new();
    let mut args = Vec::new();
    for (i, (p, name)) in params.iter().zip(names).enumerate() {
        let local = format_ident!("__arg{}", i);
        if let Param::Value(ty) = p {
            decode.push(quote! {
                let #local: #ty = __args.next::<#ty>(__instances, #name)?;
            });
            args.push(quote!(#local));
        }
    }
    let this = if kind == Kind::AsyncMethod {
        args.insert(0, quote!(__this));
        quote!(let __this = #private::async_this::<Self>(__this)?;)
    } else {
        quote!(let _ = __this;)
    };
    let args_iter = if decode.is_empty() {
        quote!(let _ = __args;)
    } else {
        quote!(let mut __args = __args;)
    };
    let finish = if fallible {
        quote!(#private::wire_ret_result(__result, &__instances))
    } else {
        quote!(#private::wire_ret(__result, &__instances))
    };
    quote! {
        #ui::MemberCall::Async(|__this, __args, __instances| {
            #this
            #args_iter
            #(#decode)*
            let __instances = ::core::clone::Clone::clone(__instances);
            ::core::result::Result::Ok(::std::boxed::Box::pin(async move {
                let __result = Self::#ident(#(#args),*).await;
                #finish
            }))
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
