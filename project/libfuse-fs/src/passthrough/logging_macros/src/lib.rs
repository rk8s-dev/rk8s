extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemImpl};
use quote::quote;

#[proc_macro]
pub fn impl_filesystem_logging(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemImpl);

    let struct_name = &input.self_ty;
    let trait_name = &input.trait_;

    let trait_path = match trait_name {
        Some((_, path, _)) => path,
        None => panic!("Trait name cannot be None"),
    };

    let methods = input.items.iter().map(|item| {
        match item {
            syn::ImplItem::Method(method) => {
                let method_name = &method.sig.ident;
                let method_inputs = &method.sig.inputs;
                let method_output = &method.sig.output;

                quote! {
                    fn #method_name(&self, #method_inputs) -> #method_output {
                        println!("{}::{} called", stringify!(#struct_name), stringify!(#method_name));
                        let result = <#struct_name as #trait_path>::#method_name(self, #method_inputs);
                        println!("{}::{} returned: {:?}", stringify!(#struct_name), stringify!(#method_name), result);
                        result
                    }
                }
            },
            _ => quote! {},
        }
    });

    let expanded = quote! {
        impl #trait_path for #struct_name {
            #(#methods)*
        }
    };

    TokenStream::from(expanded)
}