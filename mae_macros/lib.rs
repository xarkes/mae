extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro]
pub fn rac_code(item: TokenStream) -> TokenStream {
    item
}
