extern crate proc_macro;
use std::str::FromStr;

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{DeriveInput, ExprMacro, Ident, parse_macro_input, parse_quote};

/// Runtime And Compilation Enum
/// Attempt to generate functions that will call the UI API during runtime and generate
/// the exact same Rust code to be used for compilation later
#[proc_macro_derive(RACEnum)]
pub fn rac_code(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = parse_macro_input!(input);
    let enum_name = &ast.ident;

    let mut per_variant = Vec::new();
    let mut per_variant_raw = Vec::new();

    match &ast.data {
        syn::Data::Enum(data) => {
            for var in &data.variants {
                let variant_name = &var.ident;
                println!("Fields: {}", &var.fields.len());
                for field in &var.fields {
                    println!("---> {:?}", field.into_token_stream().to_string());
                }
                let fields = match &var.fields.len() {
                    0 => quote!(),
                    1 => quote!(a),
                    2 => quote!(a, b),
                    _ => quote!(),
                };
                let fields_match = match &var.fields.len() {
                    0 => quote!(),
                    _ => quote!((#fields)),
                };

                let variant_name_lower = Ident::new(
                    variant_name.to_string().as_str().to_lowercase().as_str(),
                    variant_name.span(),
                );
                let per_variant_code = quote! {
                    ui. #variant_name_lower ( #fields );
                };
                // TODO: Doesn't work at all, we need to expand all
                let per_variant_code_raw = per_variant_code.to_string();
                let variant_code = quote! {
                    #enum_name::#variant_name #fields_match => {
                        #per_variant_code
                    }
                };
                let variant_code_raw = quote! {
                    #enum_name::#variant_name #fields_match => String::from(#per_variant_code_raw)
                };
                per_variant.push(variant_code);
                per_variant_raw.push(variant_code_raw);
            }
        }
        _ => {
            panic!("Cannot use this macro on something else than an enum!")
        }
    }

    let all_variants_code = quote! {
        #(#per_variant),*
    };
    let all_variants_code_raw = quote! {
        #(#per_variant_raw),*
    };

    let code = quote! {
        impl #enum_name {
            pub fn to_imui(&self, ui: &mut IMUI) {
                match self {
                    #all_variants_code
                }
            }
            pub fn to_rust(&self) -> String {
                match self {
                    #all_variants_code_raw
                }
            }
        }
    };
    println!("{}", code.to_token_stream().to_string());
    code.into()
}
