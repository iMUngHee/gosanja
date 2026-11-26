//! The definitions of one file that get a row, with their anchors, visibility, qualifiers,
//! attributes and outlines.

use gosanja_schema::contract::{ImplFact, ImportFact, SyntaxLine};
use gosanja_schema::position::Range;
use gosanja_schema::snapshot::OutlineNode;
use proc_macro2::Span;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Visibility};

use super::outline::{self, macro_node};
use super::position::{between, range};

/// One row before it is written: the anchor and what the row carries.
#[derive(Debug)]
pub struct Item {
    pub line: u32,
    pub col: u32,
    pub visibility: Option<String>,
    pub qualifiers: Vec<String>,
    pub attributes: Vec<String>,
    pub outline: Option<Vec<OutlineNode>>,
    /// What standard error calls the item.
    pub name: String,
}

/// The rows of a parsed file, the file's module last when a node gives it one.
pub fn of_file(file: &str, syntax: &syn::File) -> (Vec<Item>, Vec<SyntaxLine>) {
    let mut items = Items {
        file,
        ..Items::default()
    };
    items.visit_file(syntax);
    let Items {
        mut rows,
        file_module,
        mut import_sites,
        mut impls,
        ..
    } = items;
    if !file_module.is_empty() {
        rows.push(Item {
            line: 0,
            col: 0,
            visibility: None,
            qualifiers: Vec::new(),
            attributes: Vec::new(),
            outline: Some(file_module),
            name: "the file's module".into(),
        });
    }
    let mut tokens = Vec::new();
    if !import_sites.is_empty() {
        import_sites.sort_unstable();
        import_sites.dedup();
        tokens.push(SyntaxLine::Imports(ImportFact {
            row: "imports".into(),
            file: file.into(),
            sites: import_sites,
        }));
    }
    impls.sort_unstable_by_key(|block| block.extent);
    tokens.extend(impls.into_iter().map(SyntaxLine::Impl));
    (rows, tokens)
}

fn source(span: Span, tokens: &impl ToTokens) -> String {
    span.source_text()
        .unwrap_or_else(|| tokens.to_token_stream().to_string())
}

fn visibility(vis: &Visibility) -> Option<String> {
    match vis {
        Visibility::Inherited => None,
        vis => Some(source(vis.span(), vis)),
    }
}

fn attributes(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| matches!(a.style, syn::AttrStyle::Outer) && !a.path().is_ident("doc"))
        .map(|a| source(a.span(), a))
        .collect()
}

fn signature(default: bool, sig: &syn::Signature) -> Vec<String> {
    let mut q = Vec::new();
    if default {
        q.push("default".to_owned());
    }
    for (present, word) in [
        (sig.constness.is_some(), "const"),
        (sig.asyncness.is_some(), "async"),
        (sig.unsafety.is_some(), "unsafe"),
    ] {
        if present {
            q.push(word.to_owned());
        }
    }
    if let Some(abi) = &sig.abi {
        q.push(match &abi.name {
            Some(name) => format!("extern {}", source(name.span(), name)),
            None => "extern".to_owned(),
        });
    }
    q
}

fn defaultness(default: bool) -> Vec<String> {
    if default {
        vec!["default".to_owned()]
    } else {
        Vec::new()
    }
}

/// Places `node` under the deepest node of `tree` that contains it, keeping source order.
fn insert(tree: &mut Vec<OutlineNode>, node: OutlineNode) {
    if let Some(parent) = tree.iter_mut().find(|n| n.range.contains(node.range)) {
        insert(parent.children.get_or_insert_with(Vec::new), node);
        return;
    }
    tree.push(node);
    tree.sort_by_key(|n| n.range.start());
}

/// Where an item-position macro goes.
#[derive(Clone, Copy)]
enum Owner {
    /// The row with this index.
    Row(usize),
    /// An initializer, which has no outline.
    Nothing,
}

#[derive(Default)]
struct Items<'file> {
    file: &'file str,
    rows: Vec<Item>,
    import_sites: Vec<Range>,
    impls: Vec<ImplFact>,
    /// Innermost last; empty means the file's module.
    owners: Vec<Owner>,
    file_module: Vec<OutlineNode>,
}

impl Items<'_> {
    fn row(
        &mut self,
        ident: &syn::Ident,
        vis: Option<&Visibility>,
        qualifiers: Vec<String>,
        attrs: &[Attribute],
        outline: Option<Vec<OutlineNode>>,
    ) -> usize {
        let start = range(ident.span());
        self.rows.push(Item {
            line: start.0[0],
            col: start.0[1],
            visibility: vis.and_then(visibility),
            qualifiers,
            attributes: attributes(attrs),
            outline,
            name: ident.to_string(),
        });
        self.rows.len() - 1
    }

    fn opaque(&mut self, mac: &syn::Macro) {
        let node = macro_node(mac);
        match self.owners.last() {
            None => insert(&mut self.file_module, node),
            Some(Owner::Nothing) => {}
            Some(Owner::Row(i)) => insert(self.rows[*i].outline.get_or_insert_with(Vec::new), node),
        }
    }

    fn within(&mut self, owner: Owner, f: impl FnOnce(&mut Self)) {
        self.owners.push(owner);
        f(self);
        self.owners.pop();
    }
}

impl<'a> Visit<'a> for Items<'_> {
    fn visit_use_path(&mut self, i: &'a syn::UsePath) {
        self.import_sites.push(range(i.ident.span()));
        visit::visit_use_path(self, i);
    }

    fn visit_use_name(&mut self, i: &'a syn::UseName) {
        self.import_sites.push(range(i.ident.span()));
    }

    fn visit_use_rename(&mut self, i: &'a syn::UseRename) {
        self.import_sites.push(range(i.ident.span()));
        if i.rename != "_" {
            self.import_sites.push(range(i.rename.span()));
        }
    }

    fn visit_item_impl(&mut self, i: &'a syn::ItemImpl) {
        if let Some(block) = impl_fact(self.file, i) {
            self.impls.push(block);
        }
        visit::visit_item_impl(self, i);
    }

    fn visit_item_fn(&mut self, i: &'a syn::ItemFn) {
        let body = outline::of_block(&i.block);
        let row = self.row(
            &i.sig.ident,
            Some(&i.vis),
            signature(false, &i.sig),
            &i.attrs,
            Some(body),
        );
        self.within(Owner::Row(row), |s| visit::visit_item_fn(s, i));
    }

    fn visit_impl_item_fn(&mut self, i: &'a syn::ImplItemFn) {
        let body = outline::of_block(&i.block);
        let q = signature(i.defaultness.is_some(), &i.sig);
        let row = self.row(&i.sig.ident, Some(&i.vis), q, &i.attrs, Some(body));
        self.within(Owner::Row(row), |s| visit::visit_impl_item_fn(s, i));
    }

    fn visit_trait_item_fn(&mut self, i: &'a syn::TraitItemFn) {
        let body = i.default.as_ref().map(outline::of_block);
        let row = self.row(&i.sig.ident, None, signature(false, &i.sig), &i.attrs, body);
        self.within(Owner::Row(row), |s| visit::visit_trait_item_fn(s, i));
    }

    fn visit_item_struct(&mut self, i: &'a syn::ItemStruct) {
        self.row(&i.ident, Some(&i.vis), Vec::new(), &i.attrs, None);
        self.within(Owner::Nothing, |s| visit::visit_item_struct(s, i));
    }

    fn visit_item_enum(&mut self, i: &'a syn::ItemEnum) {
        self.row(&i.ident, Some(&i.vis), Vec::new(), &i.attrs, None);
        self.within(Owner::Nothing, |s| visit::visit_item_enum(s, i));
    }

    fn visit_item_union(&mut self, i: &'a syn::ItemUnion) {
        self.row(&i.ident, Some(&i.vis), Vec::new(), &i.attrs, None);
        self.within(Owner::Nothing, |s| visit::visit_item_union(s, i));
    }

    fn visit_item_type(&mut self, i: &'a syn::ItemType) {
        self.row(&i.ident, Some(&i.vis), Vec::new(), &i.attrs, None);
        self.within(Owner::Nothing, |s| visit::visit_item_type(s, i));
    }

    fn visit_item_const(&mut self, i: &'a syn::ItemConst) {
        if i.ident != "_" {
            self.row(&i.ident, Some(&i.vis), Vec::new(), &i.attrs, None);
        }
        self.within(Owner::Nothing, |s| visit::visit_item_const(s, i));
    }

    fn visit_item_static(&mut self, i: &'a syn::ItemStatic) {
        let q = match i.mutability {
            syn::StaticMutability::Mut(_) => vec!["mut".to_owned()],
            _ => Vec::new(),
        };
        self.row(&i.ident, Some(&i.vis), q, &i.attrs, None);
        self.within(Owner::Nothing, |s| visit::visit_item_static(s, i));
    }

    fn visit_item_trait(&mut self, i: &'a syn::ItemTrait) {
        let mut q = Vec::new();
        if i.unsafety.is_some() {
            q.push("unsafe".to_owned());
        }
        if i.auto_token.is_some() {
            q.push("auto".to_owned());
        }
        let row = self.row(&i.ident, Some(&i.vis), q, &i.attrs, None);
        self.within(Owner::Row(row), |s| visit::visit_item_trait(s, i));
    }

    fn visit_item_mod(&mut self, i: &'a syn::ItemMod) {
        if i.content.is_none() {
            return;
        }
        let row = self.row(&i.ident, Some(&i.vis), Vec::new(), &i.attrs, None);
        self.within(Owner::Row(row), |s| visit::visit_item_mod(s, i));
    }

    fn visit_item_macro(&mut self, i: &'a syn::ItemMacro) {
        match &i.ident {
            Some(ident) if i.mac.path.is_ident("macro_rules") => {
                self.row(ident, None, Vec::new(), &i.attrs, None);
            }
            _ => self.opaque(&i.mac),
        }
    }

    fn visit_impl_item_macro(&mut self, i: &'a syn::ImplItemMacro) {
        self.opaque(&i.mac);
    }

    fn visit_trait_item_macro(&mut self, i: &'a syn::TraitItemMacro) {
        self.opaque(&i.mac);
    }

    fn visit_foreign_item_macro(&mut self, i: &'a syn::ForeignItemMacro) {
        self.opaque(&i.mac);
    }

    // Items of an extern block have no row.
    fn visit_foreign_item_fn(&mut self, _: &'a syn::ForeignItemFn) {}
    fn visit_foreign_item_static(&mut self, _: &'a syn::ForeignItemStatic) {}
    fn visit_foreign_item_type(&mut self, _: &'a syn::ForeignItemType) {}

    fn visit_trait_item_type(&mut self, i: &'a syn::TraitItemType) {
        self.row(&i.ident, None, Vec::new(), &i.attrs, None);
        self.within(Owner::Nothing, |s| visit::visit_trait_item_type(s, i));
    }

    fn visit_impl_item_type(&mut self, i: &'a syn::ImplItemType) {
        self.row(
            &i.ident,
            Some(&i.vis),
            defaultness(i.defaultness.is_some()),
            &i.attrs,
            None,
        );
        self.within(Owner::Nothing, |s| visit::visit_impl_item_type(s, i));
    }

    fn visit_trait_item_const(&mut self, i: &'a syn::TraitItemConst) {
        if i.ident != "_" {
            self.row(&i.ident, None, Vec::new(), &i.attrs, None);
        }
        self.within(Owner::Nothing, |s| visit::visit_trait_item_const(s, i));
    }

    fn visit_impl_item_const(&mut self, i: &'a syn::ImplItemConst) {
        if i.ident != "_" {
            let q = defaultness(i.defaultness.is_some());
            self.row(&i.ident, Some(&i.vis), q, &i.attrs, None);
        }
        self.within(Owner::Nothing, |s| visit::visit_impl_item_const(s, i));
    }

    // Statement and expression macros belong to the outline of the body they are in.
    fn visit_macro(&mut self, _: &'a syn::Macro) {}
}

fn impl_fact(file: &str, item: &syn::ItemImpl) -> Option<ImplFact> {
    let last = |path: &syn::Path| {
        path.segments
            .last()
            .map(|segment| range(segment.ident.span()))
    };
    let trait_type = match &item.trait_ {
        Some((Some(_), _, _)) => return None,
        Some((None, path, _)) => last(path),
        None => None,
    };
    let self_type = match &*item.self_ty {
        syn::Type::Path(path) if path.qself.is_none() => last(&path.path),
        _ => None,
    };
    if self_type.is_none() && trait_type.is_none() {
        return None;
    }
    let members: Vec<_> = item
        .items
        .iter()
        .filter_map(|member| {
            let name = match member {
                syn::ImplItem::Fn(i) => &i.sig.ident,
                syn::ImplItem::Const(i) => &i.ident,
                syn::ImplItem::Type(i) => &i.ident,
                _ => return None,
            };
            Some(range(name.span()))
        })
        .collect();
    (!members.is_empty()).then(|| ImplFact {
        row: "impl".into(),
        file: file.into(),
        extent: between(item.impl_token.span, item.brace_token.span.close()),
        members,
        self_type,
        trait_type,
    })
}
