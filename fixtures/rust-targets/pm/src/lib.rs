extern crate proc_macro;
use proc_macro::TokenStream;
#[proc_macro_derive(Gen)]
pub fn gen(_input: TokenStream) -> TokenStream {
    "#[path = \"from_derive.rs\"] pub mod from_derive;".parse().unwrap()
}
