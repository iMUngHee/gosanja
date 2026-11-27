//! Written positions used to corroborate compiler call locations.
use super::position::range;
use gosanja_schema::position::Range;
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use syn::{
    AttrStyle, Attribute, Meta, Signature, Token,
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

/// The attribute's name when its path is one identifier, a raw `r#` spelling read as the name
/// it spells, as the compiler reads it.
fn name(meta: &Meta) -> Option<String> {
    meta.path()
        .get_ident()
        .map(|n| syn::ext::IdentExt::unraw(n).to_string())
}

fn allowed(meta: &Meta, inner: bool) -> bool {
    let name = name(meta);
    let is = |n: &str| name.as_deref() == Some(n);
    if is("cfg_attr") {
        let Meta::List(list) = meta else { return false };
        // A group is one token tree, so a comma in all(...), for example, ends no predicate.
        let mut tokens = list.tokens.clone().into_iter().peekable();
        if tokens.peek().is_none_or(comma) {
            return false;
        }
        if !tokens.any(|token| comma(&token)) {
            return false;
        }
        return Punctuated::<Meta, Token![,]>::parse_terminated
            .parse2(tokens.collect())
            .is_ok_and(|attrs| attrs.iter().all(|attr| allowed(attr, inner)));
    }
    if is("unsafe") {
        let Meta::List(list) = meta else { return false };
        return syn::parse2::<Meta>(list.tokens.clone()).is_ok_and(|attr| allowed(&attr, inner));
    }
    if inner && is("feature") {
        let Meta::List(list) = meta else { return false };
        return Punctuated::<syn::Ident, Token![,]>::parse_terminated
            .parse2(list.tokens.clone())
            .is_ok_and(|gates| {
                gates.iter().all(|gate| {
                    matches!(
                        syn::ext::IdentExt::unraw(gate).to_string().as_str(),
                        "doc_cfg" | "doc_notable_trait" | "doc_auto_cfg"
                    )
                })
            });
    }
    name.is_some_and(|name| {
        matches!(
            name.as_str(),
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
        ) || inner
            && matches!(
                name.as_str(),
                "no_std"
                    | "no_main"
                    | "no_implicit_prelude"
                    | "recursion_limit"
                    | "type_length_limit"
                    | "crate_name"
                    | "crate_type"
                    | "windows_subsystem"
            )
    })
}

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

fn comma(token: &TokenTree) -> bool {
    matches!(token, TokenTree::Punct(p) if p.as_char() == ',')
}

impl Facts {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        if text.starts_with('\u{feff}') {
            return None;
        }
        let facts = syn::parse_file(text).ok().and_then(|file| {
            let source = file
                .shebang
                .as_ref()
                .map_or(text, |line| &text[line.len()..]);
            let tokens: TokenStream = source.parse().ok()?;
            let mut facts = Self::default();
            let mut visitor = Owners {
                facts: &mut facts,
                seen: BTreeSet::new(),
                items: Vec::new(),
                macro_inputs: BTreeSet::new(),
            };
            visitor.visit_file(&file);
            visitor.scan(tokens);
            Some(facts)
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
    fn take(&mut self, extent: Range) {
        self.taken.extend(extent.0[0]..=extent.0[2]);
    }
}

struct Owners<'f> {
    facts: &'f mut Facts,
    seen: BTreeSet<(u32, u32)>,
    // Item spans in visit order, so reverse search finds the most specific owner first.
    items: Vec<Range>,
    macro_inputs: BTreeSet<(u32, u32)>,
}

impl Owners<'_> {
    fn is_allowed(&mut self, attr: &Attribute) -> bool {
        self.seen.insert(range(attr.pound_token.span).start());
        allowed(&attr.meta, matches!(attr.style, AttrStyle::Inner(_)))
    }

    fn attributes(&mut self, attrs: &[Attribute], owner: &impl Spanned) {
        for attr in attrs {
            if !self.is_allowed(attr) {
                self.facts.take(range(owner.span()));
            }
        }
    }

    fn verbatim(&mut self, tokens: &TokenStream) {
        if !tokens.is_empty() {
            self.facts.take(range(tokens.span()));
        }
    }

    fn scan(&mut self, stream: TokenStream) {
        let mut tokens = stream.into_iter().peekable();
        while let Some(token) = tokens.next() {
            if let TokenTree::Punct(hash) = &token
                && hash.as_char() == '#'
            {
                let inner = tokens
                    .next_if(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '!'))
                    .is_some();
                if let Some(TokenTree::Group(group)) = tokens.next_if(
                    |t| matches!(t, TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket),
                ) {
                    let extent = super::position::between(hash.span(), group.span_close());
                    if !self.seen.contains(&extent.start())
                        && !syn::parse2::<Meta>(group.stream()).is_ok_and(|m| allowed(&m, inner))
                    {
                        let owner = self.items.iter().rev().find(|i| i.contains(extent));
                        self.facts.take(owner.copied().unwrap_or(extent));
                    }
                }
            } else if let TokenTree::Group(group) = token
                && !self
                    .macro_inputs
                    .contains(&range(group.span_open()).start())
            {
                self.scan(group.stream());
            }
        }
    }
}

macro_rules! item_visitor {
    ($method:ident, $kind:ident, [$($other:ident),*], [$($storage:ident),*]) => {
        fn $method(&mut self, item: &'a syn::$kind) {
            if let syn::$kind::Verbatim(tokens) = item {
                self.verbatim(tokens);
                if !tokens.is_empty() {
                    self.items.push(range(tokens.span()));
                }
                return;
            }
            self.items.push(range(item.span()));
            match item {
                syn::$kind::Fn(f) => {
                    self.attributes(&f.attrs, f);
                    self.facts.function(&f.sig, range(f.span()));
                }
                $(syn::$kind::$storage(s) => {
                    self.attributes(&s.attrs, s);
                    self.facts.items.push(range(s.span()));
                })*
                $(syn::$kind::$other(v) => self.attributes(&v.attrs, v),)*
                _ => {}
            }
            visit::$method(self, item);
        }
    }
}

macro_rules! attribute_owners {
    ($($method:ident($kind:ident)),* $(,)?) => {$(
        fn $method(&mut self, node: &'a syn::$kind) {
            self.attributes(&node.attrs, node);
            visit::$method(self, node);
        }
    )*};
}

macro_rules! opaque_variants {
    ($($method:ident($kind:ident)),* $(,)?) => {$(
        fn $method(&mut self, node: &'a syn::$kind) {
            if let syn::$kind::Verbatim(tokens) = node {
                self.verbatim(tokens);
            } else {
                visit::$method(self, node);
            }
        }
    )*};
}
impl<'a> Visit<'a> for Owners<'_> {
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
    opaque_variants!(
        visit_expr(Expr),
        visit_pat(Pat),
        visit_type(Type),
        visit_type_param_bound(TypeParamBound),
    );

    // Nodes visited by syn that can hold attributes independently of their enclosing item.
    attribute_owners!(
        visit_arm(Arm),
        visit_bare_fn_arg(BareFnArg),
        visit_bare_variadic(BareVariadic),
        visit_const_param(ConstParam),
        visit_expr_array(ExprArray),
        visit_expr_assign(ExprAssign),
        visit_expr_async(ExprAsync),
        visit_expr_await(ExprAwait),
        visit_expr_binary(ExprBinary),
        visit_expr_block(ExprBlock),
        visit_expr_break(ExprBreak),
        visit_expr_call(ExprCall),
        visit_expr_cast(ExprCast),
        visit_expr_closure(ExprClosure),
        visit_expr_const(ExprConst),
        visit_expr_continue(ExprContinue),
        visit_expr_field(ExprField),
        visit_expr_for_loop(ExprForLoop),
        visit_expr_group(ExprGroup),
        visit_expr_if(ExprIf),
        visit_expr_index(ExprIndex),
        visit_expr_infer(ExprInfer),
        visit_expr_let(ExprLet),
        visit_expr_lit(ExprLit),
        visit_expr_loop(ExprLoop),
        visit_expr_macro(ExprMacro),
        visit_expr_match(ExprMatch),
        visit_expr_method_call(ExprMethodCall),
        visit_expr_paren(ExprParen),
        visit_expr_path(ExprPath),
        visit_expr_range(ExprRange),
        visit_expr_raw_addr(ExprRawAddr),
        visit_expr_reference(ExprReference),
        visit_expr_repeat(ExprRepeat),
        visit_expr_return(ExprReturn),
        visit_expr_struct(ExprStruct),
        visit_expr_try(ExprTry),
        visit_expr_try_block(ExprTryBlock),
        visit_expr_tuple(ExprTuple),
        visit_expr_unary(ExprUnary),
        visit_expr_unsafe(ExprUnsafe),
        visit_expr_while(ExprWhile),
        visit_expr_yield(ExprYield),
        visit_field(Field),
        visit_field_pat(FieldPat),
        visit_field_value(FieldValue),
        visit_lifetime_param(LifetimeParam),
        visit_local(Local),
        visit_pat_ident(PatIdent),
        visit_pat_or(PatOr),
        visit_pat_paren(PatParen),
        visit_pat_reference(PatReference),
        visit_pat_rest(PatRest),
        visit_pat_slice(PatSlice),
        visit_pat_struct(PatStruct),
        visit_pat_tuple(PatTuple),
        visit_pat_tuple_struct(PatTupleStruct),
        visit_pat_type(PatType),
        visit_pat_wild(PatWild),
        visit_receiver(Receiver),
        visit_stmt_macro(StmtMacro),
        visit_type_param(TypeParam),
        visit_variadic(Variadic),
        visit_variant(Variant),
    );

    fn visit_file(&mut self, i: &'a syn::File) {
        let mut blocked = false;
        for attr in &i.attrs {
            blocked |= !self.is_allowed(attr);
        }
        if blocked {
            for item in &i.items {
                self.facts.take(range(item.span()));
            }
        }
        visit::visit_file(self, i);
    }

    fn visit_macro(&mut self, i: &'a syn::Macro) {
        let r = range(i.span());
        self.facts.macros.extend(r.0[0]..=r.0[2]);
        if i.path.is_ident("macro_rules") {
            self.facts.take(r);
        }
        let delimiters = match &i.delimiter {
            syn::MacroDelimiter::Paren(token) => token.span,
            syn::MacroDelimiter::Brace(token) => token.span,
            syn::MacroDelimiter::Bracket(token) => token.span,
        };
        self.macro_inputs.insert(range(delimiters.open()).start());
        visit::visit_macro(self, i);
    }
}

#[cfg(test)]
mod tests {
    use super::declares_its_macros;

    #[test]
    fn written_declarations_and_inert_attributes_declare_the_macros() {
        let root = "#![warn(missing_docs)]\n#![cfg_attr(docsrs, feature(doc_cfg))]\n\
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
