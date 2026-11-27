//! Whether a procedural-macro crate's dependencies can change which macros it exports.

use super::call_facts::{allowed, name};
use syn::{AttrStyle, Attribute};

/// Whether the root file of a procedural-macro crate writes every macro it exports.
///
/// The compiler takes those declarations only from items at the crate root, so the
/// dependencies of such a crate can change what it exports only through a macro there: an
/// invocation other than a `macro_rules!` definition, or an attribute that is not inert. A
/// `#[macro_use]` only widens which macros the root may invoke. A file that does not parse
/// declares nothing known.
#[must_use]
pub fn declares_its_macros(text: &str) -> bool {
    let Ok(file) = syn::parse_file(text) else {
        return false;
    };
    let inert = |attr: &Attribute| {
        allowed(&attr.meta, matches!(attr.style, AttrStyle::Inner(_)))
            || matches!(
                name(&attr.meta).as_deref(),
                Some("proc_macro" | "proc_macro_derive" | "proc_macro_attribute" | "macro_use")
            )
    };
    let written = file.attrs.iter().all(inert)
        && file.items.iter().all(|item| {
            let attrs = match item {
                syn::Item::Macro(m) if m.mac.path.is_ident("macro_rules") => &m.attrs,
                syn::Item::Const(i) => &i.attrs,
                syn::Item::Enum(i) => &i.attrs,
                syn::Item::ExternCrate(i) => &i.attrs,
                syn::Item::Fn(i) => &i.attrs,
                syn::Item::ForeignMod(i) => &i.attrs,
                syn::Item::Impl(i) => &i.attrs,
                syn::Item::Mod(i) => &i.attrs,
                syn::Item::Static(i) => &i.attrs,
                syn::Item::Struct(i) => &i.attrs,
                syn::Item::Trait(i) => &i.attrs,
                syn::Item::TraitAlias(i) => &i.attrs,
                syn::Item::Type(i) => &i.attrs,
                syn::Item::Union(i) => &i.attrs,
                syn::Item::Use(i) => &i.attrs,
                _ => return false,
            };
            attrs.iter().all(inert)
        });
    proc_macro2::extra::invalidate_current_thread_spans();
    written
}

#[cfg(test)]
mod tests {
    use super::declares_its_macros;

    #[test]
    fn written_declarations_and_inert_attributes_declare_the_macros() {
        let root = "#![doc = include_str!(\"../README.md\")]\n#![warn(missing_docs)]\n#![cfg_attr(docsrs, feature(doc_cfg))]\n\
            #[macro_use]\nextern crate quote;\nuse proc_macro::TokenStream;\nmod expand;\n\
            macro_rules! local { () => {} }\n\
            /// Derives a thing.\n#[proc_macro_derive(Thing, attributes(thing))]\n\
            pub fn thing(input: TokenStream) -> TokenStream { input }\n\
            #[proc_macro]\npub fn make(input: TokenStream) -> TokenStream { input }\n\
            #[proc_macro_attribute]\n#[allow(unused)]\npub fn keep(_: TokenStream, item: TokenStream) -> TokenStream { item }\n";
        assert!(declares_its_macros(root));
    }

    #[test]
    fn a_macro_at_the_root_may_write_the_declarations() {
        for root in [
            "use proc_macro::TokenStream;\nsynstructure::decl_derive!([Thing] => thing);\n",
            "include!(\"generated.rs\");\n",
            "#[dependency::expand]\nmod hidden {}\n",
            "#[derive(Debug)]\nstruct Options;\n",
            "#![dependency::crate_attr]\n",
            "pub fn broken( {\n",
        ] {
            assert!(!declares_its_macros(root), "{root}");
        }
    }
}
