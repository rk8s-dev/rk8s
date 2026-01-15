use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::{parse, parse_macro_input, Field, Generics, Ident, ItemStruct};

/// Generate fields & implements of `Node` trait.
///
/// Step 1: generate fields (`id`, `name`, `input_channel`, `output_channel`, `action`)
///
/// Step 2: generates methods for `Node` implementation.
///
/// Step 3: append the generated fields to the input struct.
///
/// Step 4: return tokens of the input struct & the generated methods.
pub(crate) fn auto_node(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_struct = parse_macro_input!(input as ItemStruct);
    let _ = parse_macro_input!(args as parse::Nothing);

    let generics = &item_struct.generics;

    let field_id = syn::Field::parse_named
        .parse2(quote! {
            id: dagrs::NodeId
        })
        .unwrap();

    let field_name = syn::Field::parse_named
        .parse2(quote! {
            name: String
        })
        .unwrap();

    let field_in_channels = syn::Field::parse_named
        .parse2(quote! {
            input_channels: dagrs::InChannels
        })
        .unwrap();

    let field_out_channels = syn::Field::parse_named
        .parse2(quote! {
            output_channels: dagrs::OutChannels
        })
        .unwrap();

    let field_action = syn::Field::parse_named
        .parse2(quote! {
            action: Box<dyn dagrs::Action>
        })
        .unwrap();

    let auto_impl = auto_impl_node(
        &item_struct.ident,
        generics,
        &field_id,
        &field_name,
        &field_in_channels,
        &field_out_channels,
        &field_action,
    );

    match item_struct.fields {
        syn::Fields::Named(ref mut fields) => {
            fields.named.push(field_id);
            fields.named.push(field_name);
            fields.named.push(field_in_channels);
            fields.named.push(field_out_channels);
            fields.named.push(field_action);
        }
        syn::Fields::Unit => {
            item_struct.fields = syn::Fields::Named(syn::FieldsNamed {
                named: [
                    field_id,
                    field_name,
                    field_in_channels,
                    field_out_channels,
                    field_action,
                ]
                .into_iter()
                .collect(),
                brace_token: Default::default(),
            });
        }
        _ => {
            return syn::Error::new_spanned(
                item_struct.ident,
                "`auto_node` macro can only be annotated on named struct or unit struct.",
            )
            .into_compile_error()
            .into()
        }
    };

    return quote! {
        #item_struct
        #auto_impl
    }
    .into();
}

fn auto_impl_node(
    struct_ident: &Ident,
    generics: &Generics,
    field_id: &Field,
    field_name: &Field,
    field_in_channels: &Field,
    field_out_channels: &Field,
    field_action: &Field,
) -> proc_macro2::TokenStream {
    let mut impl_tokens = proc_macro2::TokenStream::new();
    impl_tokens.extend([
        impl_id(field_id),
        impl_name(field_name),
        impl_in_channels(field_in_channels),
        impl_out_channels(field_out_channels),
        impl_run(field_action, field_in_channels, field_out_channels),
    ]);

    quote::quote!(
        #[dagrs::async_trait::async_trait]
        impl #generics dagrs::Node for #struct_ident #generics {
            #impl_tokens
        }
        unsafe impl #generics Send for #struct_ident #generics{}
        unsafe impl #generics Sync for #struct_ident #generics{}
    )
}

fn impl_id(field: &Field) -> proc_macro2::TokenStream {
    let ident = &field.ident;
    quote::quote!(
        fn id(&self) -> dagrs::NodeId {
            self.#ident
        }
    )
}

fn impl_name(field: &Field) -> proc_macro2::TokenStream {
    let ident = &field.ident;
    quote::quote!(
        fn name(&self) -> dagrs::NodeName {
            self.#ident.clone()
        }
    )
}

fn impl_in_channels(field: &Field) -> proc_macro2::TokenStream {
    let ident = &field.ident;
    quote::quote!(
        fn input_channels(&mut self) -> &mut dagrs::InChannels {
            &mut self.#ident
        }
    )
}

fn impl_out_channels(field: &Field) -> proc_macro2::TokenStream {
    let ident = &field.ident;
    quote::quote!(
        fn output_channels(&mut self) -> &mut dagrs::OutChannels {
            &mut self.#ident
        }
    )
}

fn impl_run(
    field: &Field,
    field_in_channels: &Field,
    field_out_channels: &Field,
) -> proc_macro2::TokenStream {
    let ident = &field.ident;
    let in_channels_ident = &field_in_channels.ident;
    let out_channels_ident = &field_out_channels.ident;
    quote::quote!(
        async fn run(&mut self, env: std::sync::Arc<dagrs::EnvVar>) -> dagrs::Output {
            self.#ident
                .run(&mut self.#in_channels_ident, &mut self.#out_channels_ident, env)
                .await
        }
    )
}
