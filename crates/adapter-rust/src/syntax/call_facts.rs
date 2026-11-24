//! Written positions used to corroborate compiler call locations.
use super::position::range;
use gosanja_schema::position::Range;
use std::collections::{BTreeMap, BTreeSet};
use syn::{
    Attribute, Meta, Signature, Token,
    parse::Parser,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
};

#[derive(Debug, Default)]
pub(crate) struct Facts {
    taken: BTreeSet<u32>,
    macros: BTreeSet<u32>,
    signatures: BTreeMap<u32, usize>,
    functions: BTreeMap<(u32, u32), Function>,
    items: Vec<Range>,
}
#[derive(Debug)]
struct Function {
    name: String,
    signature: Range,
    extent: Range,
}

fn inert(meta: &Meta) -> bool {
    let path = meta.path();
    if path.is_ident("cfg_attr") || path.is_ident("unsafe") {
        let Meta::List(list) = meta else { return false };
        let Ok(args) = Punctuated::<Meta, Token![,]>::parse_terminated.parse2(list.tokens.clone())
        else {
            return false;
        };
        return args
            .iter()
            .skip(usize::from(path.is_ident("cfg_attr")))
            .all(inert);
    }
    path.get_ident().is_some_and(|n| {
        matches!(
            n.to_string().as_str(),
            "cfg"
                | "doc"
                | "inline"
                | "cold"
                | "must_use"
                | "deprecated"
                | "allow"
                | "warn"
                | "deny"
                | "forbid"
                | "expect"
                | "track_caller"
                | "no_mangle"
                | "export_name"
                | "link_section"
                | "target_feature"
                | "repr"
                | "path"
                | "automatically_derived"
                | "ignore"
                | "should_panic"
        )
    })
}

impl Facts {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        if text.starts_with('\u{feff}') {
            return None;
        }
        let parsed = syn::parse_file(text).ok();
        let facts = parsed.map(|file| {
            let mut facts = Self::default();
            facts.visit_file(&file);
            facts
        });
        proc_macro2::extra::invalidate_current_thread_spans();
        facts
    }
    pub(crate) fn taken(&self, line: u32) -> bool {
        self.taken.contains(&line)
    }
    pub(crate) fn end(&self, name: &str, pos: (u32, u32)) -> bool {
        self.functions.get(&pos).is_some_and(|f| {
            f.name == name
                && (f.signature.0[0]..=f.signature.0[2]).all(|l| {
                    self.signatures.get(&l) == Some(&1)
                        && !self.taken(l)
                        && !self.macros.contains(&l)
                })
        })
    }
    pub(crate) fn owns(
        &self,
        anchor: (u32, u32),
        site: (u32, u32),
        scope_name: &str,
        scope_line: u32,
    ) -> bool {
        let Some(f) = self.functions.get(&anchor) else {
            return false;
        };
        if !(f.extent.start() <= site && site < f.extent.end())
            || self
                .items
                .iter()
                .any(|i| i.start() <= site && site < i.end() && !i.contains(f.extent))
        {
            return false;
        }
        if [
            "{closure#",
            "{async_fn#",
            "{async_block#",
            "{async_closure#",
            "{coroutine#",
        ]
        .iter()
        .any(|s| scope_name.starts_with(s))
        {
            return f.extent.0[0] <= scope_line
                && scope_line <= f.extent.0[2]
                && !self.taken(scope_line)
                && !self.macros.contains(&scope_line)
                && self
                    .items
                    .iter()
                    .all(|i| scope_line < i.0[0] || i.0[2] < scope_line || i.contains(f.extent));
        }
        scope_name == f.name && scope_line == f.signature.0[0]
    }
    fn function(&mut self, sig: &Signature, extent: Range) {
        let signature = super::position::between(sig.fn_token.span, sig.ident.span());
        for l in signature.0[0]..=signature.0[2] {
            *self.signatures.entry(l).or_default() += 1;
        }
        self.items.push(extent);
        let name = sig.ident.to_string();
        self.functions.insert(
            range(sig.ident.span()).start(),
            Function {
                name: name.trim_start_matches("r#").into(),
                signature,
                extent,
            },
        );
    }
    fn attributes(&mut self, attrs: &[Attribute], item: &impl Spanned) {
        if attrs.iter().any(|a| !inert(&a.meta)) {
            let extent = range(item.span());
            self.taken.extend(extent.0[0]..=extent.0[2]);
        }
    }
}

macro_rules! item_visitor {
    ($method:ident, $kind:ident, [$($other:ident),*], [$($storage:ident),*]) => {
        fn $method(&mut self, item: &'a syn::$kind) {
            match item {
                syn::$kind::Fn(f) => {
                    self.attributes(&f.attrs, f);
                    self.function(&f.sig, range(f.span()));
                }
                $(syn::$kind::$storage(s) => {
                    self.attributes(&s.attrs, s);
                    self.items.push(range(s.span()));
                })*
                $(syn::$kind::$other(v) => self.attributes(&v.attrs, v),)*
                _ => {}
            }
            visit::$method(self, item);
        }
    }
}
impl<'a> Visit<'a> for Facts {
    item_visitor!(
        visit_item,
        Item,
        [
            Enum,
            ExternCrate,
            ForeignMod,
            Impl,
            Macro,
            Mod,
            Struct,
            Trait,
            TraitAlias,
            Type,
            Union,
            Use
        ],
        [Const, Static]
    );
    item_visitor!(visit_impl_item, ImplItem, [Macro, Type], [Const]);
    item_visitor!(visit_trait_item, TraitItem, [Macro, Type], [Const]);
    item_visitor!(visit_foreign_item, ForeignItem, [Macro, Type], [Static]);
    fn visit_file(&mut self, i: &'a syn::File) {
        if i.attrs.iter().any(|attr| !inert(&attr.meta)) {
            for item in &i.items {
                let extent = range(item.span());
                self.taken.extend(extent.0[0]..=extent.0[2]);
            }
        }
        visit::visit_file(self, i);
    }
    fn visit_macro(&mut self, i: &'a syn::Macro) {
        let r = range(i.span());
        self.macros.extend(r.0[0]..=r.0[2]);
        if i.path.is_ident("macro_rules") {
            self.taken.extend(r.0[0]..=r.0[2]);
        }
        visit::visit_macro(self, i);
    }
}
