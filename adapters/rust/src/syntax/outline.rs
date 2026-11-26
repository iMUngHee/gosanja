//! The control-flow outline of a body, as the syntax decision of the plan defines it.

use gosanja_schema::position::Range;
use gosanja_schema::snapshot::{OutlineKind, OutlineNode};
use proc_macro2::TokenStream;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, Item, Pat, Stmt, Type};

use super::position::{between, range};

/// The deepest outline the adapter sends: each level costs two levels of JSON nesting, and
/// the engine's reader accepts 128.
pub const MAX_DEPTH: usize = 32;

/// How many levels `nodes` has; 0 for none.
pub fn depth(nodes: &[OutlineNode]) -> usize {
    nodes
        .iter()
        .map(|n| 1 + n.children.as_deref().map_or(0, depth))
        .max()
        .unwrap_or(0)
}

/// A node for a macro invocation: from its path to its closing delimiter.
pub fn macro_node(mac: &syn::Macro) -> OutlineNode {
    let path = mac
        .path
        .span()
        .source_text()
        .unwrap_or_else(|| quote::ToTokens::to_token_stream(&mac.path).to_string());
    leaf(
        OutlineKind::Opaque,
        Some(format!("{path}!")),
        range(mac.span()),
    )
}

fn leaf(k: OutlineKind, detail: Option<String>, range: Range) -> OutlineNode {
    OutlineNode {
        k,
        detail,
        range,
        callee: None,
        children: None,
    }
}

fn verbatim(tokens: &TokenStream) -> Option<OutlineNode> {
    (!tokens.is_empty()).then(|| {
        leaf(
            OutlineKind::Opaque,
            Some("verbatim".into()),
            range(tokens.span()),
        )
    })
}

/// The outline of a block's statements.
pub fn of_block(block: &syn::Block) -> Vec<OutlineNode> {
    collect(|o| o.visit_block(block))
}

fn collect(f: impl FnOnce(&mut Outline)) -> Vec<OutlineNode> {
    let mut outline = Outline::default();
    f(&mut outline);
    outline.nodes
}

#[derive(Default)]
struct Outline {
    nodes: Vec<OutlineNode>,
}

impl Outline {
    fn push(
        &mut self,
        k: OutlineKind,
        detail: Option<&str>,
        range: Range,
        callee: Option<Range>,
        children: Vec<OutlineNode>,
    ) {
        self.nodes.push(OutlineNode {
            k,
            detail: detail.map(str::to_owned),
            range,
            callee,
            children: (!children.is_empty()).then_some(children),
        });
    }

    fn else_arm(&mut self, else_branch: &Expr, children: Vec<OutlineNode>) {
        self.nodes.push(OutlineNode {
            k: OutlineKind::Arm,
            detail: Some("else".into()),
            range: range(else_branch.span()),
            callee: None,
            children: (!children.is_empty()).then_some(children),
        });
    }
}

impl<'a> Visit<'a> for Outline {
    // A nested item has its own row; nothing inside a macro invocation is read.
    fn visit_item(&mut self, _: &'a Item) {}
    fn visit_macro(&mut self, _: &'a syn::Macro) {}

    fn visit_stmt(&mut self, i: &'a Stmt) {
        match i {
            Stmt::Macro(m) => self.nodes.push(macro_node(&m.mac)),
            Stmt::Item(Item::Verbatim(tokens)) => self.nodes.extend(verbatim(tokens)),
            Stmt::Local(local) => {
                let Some(init) = &local.init else {
                    return visit::visit_local(self, local);
                };
                let Some((_, diverge)) = &init.diverge else {
                    return visit::visit_local(self, local);
                };
                let mut children = collect(|o| {
                    o.visit_pat(&local.pat);
                    o.visit_expr(&init.expr);
                });
                let mut arm = Outline::default();
                arm.else_arm(diverge, collect(|o| o.visit_expr(diverge)));
                children.extend(arm.nodes);
                self.push(
                    OutlineKind::Branch,
                    Some("let-else"),
                    between(local.let_token.span, local.semi_token.span),
                    None,
                    children,
                );
            }
            _ => visit::visit_stmt(self, i),
        }
    }

    fn visit_pat(&mut self, i: &'a Pat) {
        match i {
            Pat::Macro(m) => self.nodes.push(macro_node(&m.mac)),
            Pat::Verbatim(tokens) => self.nodes.extend(verbatim(tokens)),
            _ => visit::visit_pat(self, i),
        }
    }

    fn visit_type(&mut self, i: &'a Type) {
        match i {
            Type::Macro(m) => self.nodes.push(macro_node(&m.mac)),
            Type::Verbatim(tokens) => self.nodes.extend(verbatim(tokens)),
            _ => visit::visit_type(self, i),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per row of the outline table reads better than helpers"
    )]
    fn visit_expr(&mut self, i: &'a Expr) {
        let whole = range(i.span());
        match i {
            Expr::If(e) => {
                let mut children = collect(|o| {
                    o.visit_expr(&e.cond);
                    o.visit_block(&e.then_branch);
                });
                if let Some((_, else_branch)) = &e.else_branch {
                    let inner = collect(|o| match &**else_branch {
                        Expr::Block(b) => o.visit_block(&b.block),
                        other => o.visit_expr(other),
                    });
                    let mut arm = Outline::default();
                    arm.else_arm(else_branch, inner);
                    children.extend(arm.nodes);
                }
                self.push(OutlineKind::Branch, Some("if"), whole, None, children);
            }
            Expr::Match(e) => {
                let mut children = collect(|o| o.visit_expr(&e.expr));
                for arm in &e.arms {
                    let end = arm
                        .comma
                        .as_ref()
                        .map_or_else(|| arm.body.span(), |c| c.span);
                    let inner = collect(|o| {
                        o.visit_pat(&arm.pat);
                        if let Some((_, guard)) = &arm.guard {
                            o.visit_expr(guard);
                        }
                        o.visit_expr(&arm.body);
                    });
                    children.push(OutlineNode {
                        k: OutlineKind::Arm,
                        detail: None,
                        range: between(arm.pat.span(), end),
                        callee: None,
                        children: (!inner.is_empty()).then_some(inner),
                    });
                }
                self.push(OutlineKind::Branch, Some("match"), whole, None, children);
            }
            Expr::Loop(e) => {
                let children = collect(|o| o.visit_block(&e.body));
                self.push(OutlineKind::Loop, Some("loop"), whole, None, children);
            }
            Expr::While(e) => {
                let children = collect(|o| {
                    o.visit_expr(&e.cond);
                    o.visit_block(&e.body);
                });
                self.push(OutlineKind::Loop, Some("while"), whole, None, children);
            }
            Expr::ForLoop(e) => {
                let children = collect(|o| {
                    o.visit_pat(&e.pat);
                    o.visit_expr(&e.expr);
                    o.visit_block(&e.body);
                });
                self.push(OutlineKind::Loop, Some("for"), whole, None, children);
            }
            Expr::Return(e) => {
                let children = collect(|o| e.expr.iter().for_each(|v| o.visit_expr(v)));
                self.push(OutlineKind::Exit, Some("return"), whole, None, children);
            }
            Expr::Break(e) => {
                let children = collect(|o| e.expr.iter().for_each(|v| o.visit_expr(v)));
                self.push(OutlineKind::Exit, Some("break"), whole, None, children);
            }
            Expr::Continue(_) => {
                self.push(OutlineKind::Exit, Some("continue"), whole, None, vec![]);
            }
            Expr::Try(e) => {
                let children = collect(|o| o.visit_expr(&e.expr));
                self.push(OutlineKind::Exit, Some("?"), whole, None, children);
            }
            Expr::Await(e) => {
                let children = collect(|o| o.visit_expr(&e.base));
                self.push(OutlineKind::Await, None, whole, None, children);
            }
            Expr::Closure(e) => {
                let children = collect(|o| {
                    e.inputs.iter().for_each(|p| o.visit_pat(p));
                    o.visit_return_type(&e.output);
                    o.visit_expr(&e.body);
                });
                self.push(OutlineKind::Closure, Some("closure"), whole, None, children);
            }
            Expr::Async(e) => {
                let children = collect(|o| o.visit_block(&e.block));
                self.push(OutlineKind::Closure, Some("async"), whole, None, children);
            }
            Expr::Call(e) => {
                let callee = match &*e.func {
                    Expr::Path(p) => p.path.segments.last().map(|s| range(s.ident.span())),
                    _ => None,
                };
                let children = collect(|o| {
                    o.visit_expr(&e.func);
                    e.args.iter().for_each(|a| o.visit_expr(a));
                });
                self.push(OutlineKind::Call, None, whole, callee, children);
            }
            Expr::MethodCall(e) => {
                let children = collect(|o| {
                    o.visit_expr(&e.receiver);
                    if let Some(turbofish) = &e.turbofish {
                        o.visit_angle_bracketed_generic_arguments(turbofish);
                    }
                    e.args.iter().for_each(|a| o.visit_expr(a));
                });
                self.push(
                    OutlineKind::Call,
                    None,
                    whole,
                    Some(range(e.method.span())),
                    children,
                );
            }
            Expr::Macro(m) => self.nodes.push(macro_node(&m.mac)),
            Expr::Verbatim(tokens) => self.nodes.extend(verbatim(tokens)),
            _ => visit::visit_expr(self, i),
        }
    }
}
