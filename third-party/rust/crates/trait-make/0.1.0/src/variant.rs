// Copyright (c) 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::iter;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    token::Plus,
    Error, FnArg, GenericParam, Ident, ItemTrait, Pat, PatType, Result, ReturnType, Signature,
    Token, TraitBound, TraitItem, TraitItemConst, TraitItemFn, TraitItemType, Type, TypeGenerics,
    TypeImplTrait, TypeParam, TypeParamBound,
};

struct Attrs {
    variant: MakeVariant,
}

impl Parse for Attrs {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            variant: MakeVariant::parse(input)?,
        })
    }
}

enum MakeVariant {
    // Creates a variant of a trait under a new name with additional bounds while preserving the original trait.
    Create {
        name: Ident,
        _colon: Token![:],
        bounds: Punctuated<TraitBound, Plus>,
    },
    // Rewrites the original trait into a new trait with additional bounds.
    Rewrite {
        bounds: Punctuated<TraitBound, Plus>,
    },
}

impl Parse for MakeVariant {
    fn parse(input: ParseStream) -> Result<Self> {
        let variant = if input.peek(Ident) && input.peek2(Token![:]) {
            MakeVariant::Create {
                name: input.parse()?,
                _colon: input.parse()?,
                bounds: input.parse_terminated(TraitBound::parse, Token![+])?,
            }
        } else {
            MakeVariant::Rewrite {
                bounds: input.parse_terminated(TraitBound::parse, Token![+])?,
            }
        };
        Ok(variant)
    }
}

pub fn make(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let attrs = parse_macro_input!(attr as Attrs);
    let item = parse_macro_input!(item as ItemTrait);

    match attrs.variant {
        MakeVariant::Create { name, bounds, .. } => {
            let maybe_allow_async_lint = if bounds
                .iter()
                .any(|b| b.path.segments.last().unwrap().ident == "Send")
            {
                quote! { #[allow(async_fn_in_trait)] }
            } else {
                quote! {}
            };

            let variant = mk_variant(&name, bounds, &item);
            let blanket_impl = mk_blanket_impl(&name, &item);

            quote! {
                #maybe_allow_async_lint
                #item

                #variant

                #blanket_impl
            }
            .into()
        }
        MakeVariant::Rewrite { bounds, .. } => {
            let variant = mk_variant(&item.ident, bounds, &item);
            quote! {
                #variant
            }
            .into()
        }
    }
}

fn mk_variant(
    variant: &Ident,
    with_bounds: Punctuated<TraitBound, Plus>,
    tr: &ItemTrait,
) -> TokenStream {
    let bounds: Vec<_> = with_bounds
        .into_iter()
        .map(|b| TypeParamBound::Trait(b.clone()))
        .collect();
    let variant = ItemTrait {
        ident: variant.clone(),
        supertraits: tr.supertraits.iter().chain(&bounds).cloned().collect(),
        items: tr
            .items
            .iter()
            .map(|item| transform_item(item, &bounds))
            .collect(),
        ..tr.clone()
    };
    quote! { #variant }
}

// Transforms one item declaration within the definition if it has `async fn` and/or `-> impl Trait` return types by adding new bounds.
fn transform_item(item: &TraitItem, bounds: &Vec<TypeParamBound>) -> TraitItem {
    let TraitItem::Fn(fn_item @ TraitItemFn { sig, .. }) = item else {
        return item.clone();
    };
    let (arrow, output) = if sig.asyncness.is_some() {
        let orig = match &sig.output {
            ReturnType::Default => quote! { () },
            ReturnType::Type(_, ty) => quote! { #ty },
        };
        let future = syn::parse2(quote! { ::core::future::Future<Output = #orig> }).unwrap();
        let ty = Type::ImplTrait(TypeImplTrait {
            impl_token: syn::parse2(quote! { impl }).unwrap(),
            bounds: iter::once(TypeParamBound::Trait(future))
                .chain(bounds.iter().cloned())
                .collect(),
        });
        (syn::parse2(quote! { -> }).unwrap(), ty)
    } else {
        match &sig.output {
            ReturnType::Type(arrow, ty) => match &**ty {
                Type::ImplTrait(it) => {
                    let ty = Type::ImplTrait(TypeImplTrait {
                        impl_token: it.impl_token,
                        bounds: it.bounds.iter().chain(bounds).cloned().collect(),
                    });
                    (*arrow, ty)
                }
                _ => return item.clone(),
            },
            ReturnType::Default => return item.clone(),
        }
    };

    let default_impl = fn_item.default.as_ref().map(|default_impl| {
        // only wrap async fn block with async move {}
        if sig.asyncness.is_some() {
            syn::parse2(quote! {
                {
                    async move {
                        #default_impl
                    }
                }
            })
            .unwrap()
        } else {
            default_impl.clone()
        }
    });

    TraitItem::Fn(TraitItemFn {
        sig: Signature {
            asyncness: None,
            output: ReturnType::Type(arrow, Box::new(output)),
            ..sig.clone()
        },
        default: default_impl,
        ..fn_item.clone()
    })
}

fn mk_blanket_impl(variant: &Ident, tr: &ItemTrait) -> TokenStream {
    let orig = &tr.ident;
    let (_impl, orig_ty_generics, _where) = &tr.generics.split_for_impl();
    let items = tr
        .items
        .iter()
        .map(|item| blanket_impl_item(item, variant, orig_ty_generics));
    let blanket_bound: TypeParam =
        parse_quote!(TraitVariantBlanketType: #variant #orig_ty_generics);
    let blanket = &blanket_bound.ident.clone();
    let mut blanket_generics = tr.generics.clone();
    blanket_generics
        .params
        .push(GenericParam::Type(blanket_bound));
    let (blanket_impl_generics, _ty, blanket_where_clause) = &blanket_generics.split_for_impl();
    quote! {
        impl #blanket_impl_generics #orig #orig_ty_generics for #blanket #blanket_where_clause
        {
            #(#items)*
        }
    }
}

fn blanket_impl_item(
    item: &TraitItem,
    variant: &Ident,
    trait_ty_generics: &TypeGenerics<'_>,
) -> TokenStream {
    // impl<T> IntFactory for T where T: SendIntFactory {
    //     const NAME: &'static str = <Self as SendIntFactory>::NAME;
    //     type MyFut<'a> = <Self as SendIntFactory>::MyFut<'a> where Self: 'a;
    //     async fn make(&self, x: u32, y: &str) -> i32 {
    //         <Self as SendIntFactory>::make(self, x, y).await
    //     }
    // }
    match item {
        TraitItem::Const(TraitItemConst {
            ident,
            generics,
            ty,
            ..
        }) => {
            quote! {
                const #ident #generics: #ty = <Self as #variant #trait_ty_generics>::#ident;
            }
        }
        TraitItem::Fn(TraitItemFn { sig, .. }) => {
            let ident = &sig.ident;
            let args = sig.inputs.iter().map(|arg| match arg {
                FnArg::Receiver(_) => quote! { self },
                FnArg::Typed(PatType { pat, .. }) => match &**pat {
                    Pat::Ident(arg) => quote! { #arg },
                    _ => Error::new_spanned(pat, "patterns are not supported in arguments")
                        .to_compile_error(),
                },
            });
            let maybe_await = if sig.asyncness.is_some() {
                quote! { .await }
            } else {
                quote! {}
            };
            quote! {
                #sig {
                    <Self as #variant #trait_ty_generics>::#ident(#(#args),*)#maybe_await
                }
            }
        }
        TraitItem::Type(TraitItemType {
            ident, generics, ..
        }) => {
            let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
            quote! {
                type #ident #impl_generics = <Self as #variant #trait_ty_generics>::#ident #ty_generics #where_clause;
            }
        }
        _ => Error::new_spanned(item, "unsupported item type").into_compile_error(),
    }
}
