use proc_macro::TokenStream;
use quote::quote;
use std::env;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Path};

#[proc_macro_derive(MessageBox)]
pub fn derive_message_box(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let crate_name = env::var("CARGO_CRATE_NAME").unwrap();

    let crate_path: Path = syn::parse_str(if crate_name == "ipmb" {
        "crate"
    } else {
        "::ipmb"
    })
    .unwrap();

    let indent = input.ident;

    let data_enum = if let Data::Enum(data_enum) = input.data {
        data_enum
    } else {
        panic!("#[derive(MessageBox)] is only defined for enums.");
    };

    let variants_ident: Vec<_> = data_enum
        .variants
        .iter()
        .map(|variant| &variant.ident)
        .collect();

    let variants_ty: Vec<_> = data_enum
        .variants
        .iter()
        .map(|variant| {
            if let Fields::Unnamed(field) = &variant.fields {
                if field.unnamed.len() != 1 {
                    panic!("#[derive(MessageBox)] is only support unnamed single field.");
                } else {
                    &field.unnamed[0].ty
                }
            } else {
                panic!("#[derive(MessageBox)] is only support unnamed single field.");
            }
        })
        .collect();

    let expanded = quote! {
        impl #crate_path::MessageBox for #indent {
            fn decode(uuid: type_uuid::Bytes, data: &[u8]) -> std::result::Result<Self, #crate_path::Error> {
                match uuid {
                    #(<#variants_ty as type_uuid::TypeUuid>::UUID => {
                        let variant: #variants_ty = #crate_path::decode(data)?;
                        Ok(Self::#variants_ident(variant))
                    })*
                    _ => Err(#crate_path::Error::TypeUuidNotFound),
                }
            }

            fn encode(&self) -> std::result::Result<Vec<u8>, #crate_path::Error> {
                match self {
                    #(Self::#variants_ident(t) => #crate_path::encode(t),)*
                }
            }

            fn uuid(&self) -> type_uuid::Bytes {
                match self {
                    #(Self::#variants_ident(_) => <#variants_ty as type_uuid::TypeUuid>::UUID,)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
