use proc_macro2::Span;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Meta, Token};

use super::position;

type Args = Punctuated<Meta, Token![,]>;

fn args(list: &syn::MetaList) -> Option<Args> {
    list.parse_args_with(Args::parse_terminated).ok()
}

fn name(meta: &Meta) -> Option<&syn::Ident> {
    meta.path().get_ident()
}

fn value(meta: &Meta) -> Option<[bool; 2]> {
    match meta {
        Meta::Path(path) if path.is_ident("test") => Some([false, true]),
        Meta::List(list) if list.path.is_ident("not") => {
            let operands = args(list)?;
            if operands.len() != 1 {
                return None;
            }
            let [prod, test] = value(operands.first()?)?;
            Some([!prod, !test])
        }
        Meta::List(list) if list.path.is_ident("all") || list.path.is_ident("any") => {
            let all = list.path.is_ident("all");
            args(list)?
                .iter()
                .try_fold([all; 2], |[prod, test], operand| {
                    let [left, right] = value(operand)?;
                    Some(if all {
                        [prod && left, test && right]
                    } else {
                        [prod || left, test || right]
                    })
                })
        }
        _ => None,
    }
}

fn cfg_attr(meta: &Meta, check: fn(&Meta) -> bool) -> bool {
    match meta {
        Meta::List(list) if list.path.is_ident("cfg_attr") => {
            args(list).is_none_or(|args| args.iter().skip(1).any(check))
        }
        _ => false,
    }
}

fn yields_cfg(meta: &Meta) -> bool {
    meta.path().is_ident("cfg") || cfg_attr(meta, yields_cfg)
}

fn rewrites(meta: &Meta) -> bool {
    let Some(ident) = name(meta) else { return true };
    if ident == "cfg_attr" {
        return cfg_attr(meta, rewrites);
    }
    !matches!(
        ident.to_string().as_str(),
        "cfg"
            | "doc"
            | "allow"
            | "warn"
            | "deny"
            | "forbid"
            | "expect"
            | "inline"
            | "cold"
            | "track_caller"
            | "must_use"
            | "repr"
            | "non_exhaustive"
            | "deprecated"
            | "path"
            | "macro_use"
            | "macro_export"
            | "ignore"
            | "should_panic"
            | "automatically_derived"
            | "link_section"
            | "export_name"
            | "used"
            | "target_feature"
            | "feature"
            | "no_std"
            | "no_main"
            | "no_implicit_prelude"
            | "recursion_limit"
            | "type_length_limit"
            | "windows_subsystem"
            | "crate_type"
            | "crate_name"
    )
}

#[derive(Clone, Copy)]
struct Context {
    gate: Option<[bool; 2]>,
    rewritable: bool,
}

impl Context {
    fn enter(self, attrs: &[Attribute], is_item: bool) -> Self {
        let mut gate = self.gate;
        if is_item {
            for attr in attrs {
                let meta = &attr.meta;
                if let Meta::List(list) = meta
                    && list.path.is_ident("cfg")
                {
                    gate = gate
                        .zip(
                            (!self.rewritable)
                                .then(|| list.parse_args::<Meta>().ok().and_then(|m| value(&m)))
                                .flatten(),
                        )
                        .map(|([a, b], [c, d])| [a && c, b && d]);
                } else if cfg_attr(meta, yields_cfg) {
                    gate = None;
                }
            }
        }
        Self {
            gate,
            rewritable: self.rewritable || attrs.iter().any(|attr| rewrites(&attr.meta)),
        }
    }
}

struct Items {
    context: Context,
    positions: Vec<(u32, u32)>,
}

impl Items {
    fn node(
        &mut self,
        attrs: &[Attribute],
        item: bool,
        ident: Option<Span>,
        nested: impl FnOnce(&mut Self),
    ) {
        let parent = self.context;
        self.context = parent.enter(attrs, item);
        if self.context.gate == Some([false, true])
            && let Some(ident) = ident
        {
            let range = position::range(ident);
            self.positions.push((range.0[0], range.0[1]));
        }
        nested(self);
        self.context = parent;
    }
}

macro_rules! named {
    ($($visit:ident : $type:ident => $($ident:ident).+),* $(,)?) => {$(
        fn $visit(&mut self, node: &'ast syn::$type) {
            self.node(&node.attrs, true, Some(node.$($ident).+.span()), |this| visit::$visit(this, node));
        }
    )*};
}

macro_rules! nested {
    ($item:expr; $($visit:ident : $type:ident),* $(,)?) => {$(
        fn $visit(&mut self, node: &'ast syn::$type) {
            self.node(&node.attrs, $item, None, |this| visit::$visit(this, node));
        }
    )*};
}

impl<'ast> Visit<'ast> for Items {
    named! {
        visit_item_fn: ItemFn => sig.ident,
        visit_impl_item_fn: ImplItemFn => sig.ident,
        visit_trait_item_fn: TraitItemFn => sig.ident,
        visit_item_struct: ItemStruct => ident,
        visit_item_enum: ItemEnum => ident,
        visit_item_union: ItemUnion => ident,
        visit_item_type: ItemType => ident,
        visit_item_const: ItemConst => ident,
        visit_item_static: ItemStatic => ident,
        visit_item_trait: ItemTrait => ident,
        visit_item_trait_alias: ItemTraitAlias => ident,
        visit_item_mod: ItemMod => ident,
        visit_impl_item_const: ImplItemConst => ident,
        visit_impl_item_type: ImplItemType => ident,
        visit_trait_item_const: TraitItemConst => ident,
        visit_trait_item_type: TraitItemType => ident,
        visit_foreign_item_fn: ForeignItemFn => sig.ident,
        visit_foreign_item_static: ForeignItemStatic => ident,
        visit_foreign_item_type: ForeignItemType => ident,
        visit_variant: Variant => ident,
    }

    nested! {
        true;
        visit_file: File,
        visit_item_impl: ItemImpl,
        visit_item_foreign_mod: ItemForeignMod,
        visit_item_use: ItemUse,
        visit_item_extern_crate: ItemExternCrate,
    }

    nested! {
        false;
        visit_local: Local,
        visit_stmt_macro: StmtMacro,
        visit_arm: Arm,
        visit_field_value: FieldValue,
        visit_field_pat: FieldPat,
        visit_pat_type: PatType,
        visit_pat_ident: PatIdent,
        visit_pat_or: PatOr,
        visit_pat_paren: PatParen,
        visit_pat_reference: PatReference,
        visit_pat_rest: PatRest,
        visit_pat_slice: PatSlice,
        visit_pat_struct: PatStruct,
        visit_pat_tuple: PatTuple,
        visit_pat_tuple_struct: PatTupleStruct,
        visit_pat_wild: PatWild,
        visit_receiver: Receiver,
        visit_lifetime_param: LifetimeParam,
        visit_type_param: TypeParam,
        visit_const_param: ConstParam,
        visit_expr_array: ExprArray,
        visit_expr_assign: ExprAssign,
        visit_expr_async: ExprAsync,
        visit_expr_await: ExprAwait,
        visit_expr_binary: ExprBinary,
        visit_expr_block: ExprBlock,
        visit_expr_break: ExprBreak,
        visit_expr_call: ExprCall,
        visit_expr_cast: ExprCast,
        visit_expr_closure: ExprClosure,
        visit_expr_const: ExprConst,
        visit_expr_continue: ExprContinue,
        visit_expr_field: ExprField,
        visit_expr_for_loop: ExprForLoop,
        visit_expr_group: ExprGroup,
        visit_expr_if: ExprIf,
        visit_expr_index: ExprIndex,
        visit_expr_infer: ExprInfer,
        visit_expr_let: ExprLet,
        visit_expr_lit: ExprLit,
        visit_expr_loop: ExprLoop,
        visit_expr_macro: ExprMacro,
        visit_expr_match: ExprMatch,
        visit_expr_method_call: ExprMethodCall,
        visit_expr_paren: ExprParen,
        visit_expr_path: ExprPath,
        visit_expr_range: ExprRange,
        visit_expr_raw_addr: ExprRawAddr,
        visit_expr_reference: ExprReference,
        visit_expr_repeat: ExprRepeat,
        visit_expr_return: ExprReturn,
        visit_expr_struct: ExprStruct,
        visit_expr_try: ExprTry,
        visit_expr_try_block: ExprTryBlock,
        visit_expr_tuple: ExprTuple,
        visit_expr_unary: ExprUnary,
        visit_expr_unsafe: ExprUnsafe,
        visit_expr_while: ExprWhile,
        visit_expr_yield: ExprYield,
    }

    fn visit_field(&mut self, i: &'ast syn::Field) {
        self.node(
            &i.attrs,
            true,
            i.ident.as_ref().map(syn::Ident::span),
            |this| visit::visit_field(this, i),
        );
    }

    fn visit_item_macro(&mut self, i: &'ast syn::ItemMacro) {
        self.node(
            &i.attrs,
            true,
            i.ident.as_ref().map(syn::Ident::span),
            |this| {
                visit::visit_item_macro(this, i);
            },
        );
    }
}

pub(super) fn positions(file: &syn::File) -> Vec<(u32, u32)> {
    let mut items = Items {
        context: Context {
            gate: Some([true, true]),
            rewritable: false,
        },
        positions: Vec::new(),
    };
    items.visit_file(file);
    items.positions.sort_unstable();
    items.positions.dedup();
    items.positions
}
