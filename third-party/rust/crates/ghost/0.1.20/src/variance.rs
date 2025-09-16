use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::{Attribute, LifetimeParam, Meta, TypeParam};

enum Variance {
    Covariant,
    Contravariant,
    Invariant,
}

pub trait HasVarianceAttribute {
    fn attrs(&mut self) -> &mut Vec<Attribute>;
}

impl HasVarianceAttribute for TypeParam {
    fn attrs(&mut self) -> &mut Vec<Attribute> {
        &mut self.attrs
    }
}

impl HasVarianceAttribute for LifetimeParam {
    fn attrs(&mut self) -> &mut Vec<Attribute> {
        &mut self.attrs
    }
}

pub fn apply(
    param: &mut dyn HasVarianceAttribute,
    base: TokenStream,
    type_param: &Ident,
) -> TokenStream {
    let mut variance = Variance::Covariant;

    let attrs = param.attrs();
    *attrs = attrs
        .drain(..)
        .filter(|attr| {
            if let Meta::Path(attr_path) = &attr.meta {
                if attr_path.is_ident("contra") {
                    variance = Variance::Contravariant;
                    return false;
                } else if attr_path.is_ident("invariant") {
                    variance = Variance::Invariant;
                    return false;
                }
            }
            true
        })
        .collect();

    let phantom = quote!(self::#type_param<#base>);
    match variance {
        Variance::Covariant => base,
        Variance::Contravariant => quote!(fn(#phantom)),
        Variance::Invariant => quote!(fn(#phantom) -> #phantom),
    }
}
