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

    if let Some(err) = ensure_reserved_fields_are_available(&item_struct) {
        return err.into_compile_error().into();
    }

    let field_id = match parse_named_field(quote! { id: dagrs::NodeId }) {
        Ok(field) => field,
        Err(err) => return err.into_compile_error().into(),
    };

    let field_name = match parse_named_field(quote! { name: String }) {
        Ok(field) => field,
        Err(err) => return err.into_compile_error().into(),
    };

    let field_in_channels = match parse_named_field(quote! { input_channels: dagrs::InChannels }) {
        Ok(field) => field,
        Err(err) => return err.into_compile_error().into(),
    };

    let field_out_channels = match parse_named_field(quote! { output_channels: dagrs::OutChannels })
    {
        Ok(field) => field,
        Err(err) => return err.into_compile_error().into(),
    };

    let field_action = match parse_named_field(quote! { action: Box<dyn dagrs::Action> }) {
        Ok(field) => field,
        Err(err) => return err.into_compile_error().into(),
    };

    let auto_impl = auto_impl_node(
        &item_struct.ident,
        &item_struct.generics,
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

    quote! {
        #item_struct
        #auto_impl
    }
    .into()
}

fn parse_named_field(tokens: proc_macro2::TokenStream) -> syn::Result<Field> {
    syn::Field::parse_named.parse2(tokens)
}

fn ensure_reserved_fields_are_available(item_struct: &ItemStruct) -> Option<syn::Error> {
    const RESERVED_FIELDS: [&str; 5] =
        ["id", "name", "input_channels", "output_channels", "action"];

    match &item_struct.fields {
        syn::Fields::Named(fields) => {
            for field in &fields.named {
                if let Some(ident) = &field.ident {
                    if RESERVED_FIELDS.contains(&ident.to_string().as_str()) {
                        return Some(syn::Error::new_spanned(
                            ident,
                            format!("field `{ident}` is reserved by `auto_node`"),
                        ));
                    }
                }
            }
            None
        }
        syn::Fields::Unit => None,
        syn::Fields::Unnamed(_) => None,
    }
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
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
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
        impl #impl_generics dagrs::Node for #struct_ident #ty_generics #where_clause {
            #impl_tokens
        }
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
