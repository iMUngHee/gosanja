//! `#[derive(Describe)]` gives a struct an associated `describe` function.
use proc_macro::{Group, Span, TokenStream, TokenTree};

#[proc_macro_derive(Describe)]
pub fn describe(input: TokenStream) -> TokenStream {
    let name = struct_name(input);
    let code = format!("impl {name} {{ pub fn describe() -> &'static str {{ \"{name}\" }} }}");
    code.parse().unwrap()
}

fn struct_name(input: TokenStream) -> String {
    let mut tokens = input.into_iter();
    while let Some(token) = tokens.next() {
        if let TokenTree::Ident(ident) = &token
            && ident.to_string() == "struct"
            && let Some(TokenTree::Ident(name)) = tokens.next()
        {
            return name.to_string();
        }
    }
    panic!("Describe needs a struct")
}

/// `#[proxy]` on an `impl` keeps it and adds `<Type>Proxy` with a method `run` of its own,
/// every generated token carrying the span of the written method's `fn`.
#[proc_macro_attribute]
pub fn proxy(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let (ty, span) = impl_parts(item.clone());
    let code = format!(
        "pub struct {ty}Proxy; impl {ty}Proxy {{ pub fn run(&self) -> u32 {{ crate::proxy_helper() }} }}"
    );
    let generated = respan(code.parse().unwrap(), span);
    item.into_iter().chain(generated).collect()
}

fn impl_parts(item: TokenStream) -> (String, Span) {
    let mut tokens = item.into_iter();
    let mut ty = None;
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Ident(ident) if ident.to_string() == "impl" => {
                if let Some(TokenTree::Ident(name)) = tokens.next() {
                    ty = Some(name.to_string());
                }
            }
            TokenTree::Group(body) => return (ty.expect("proxy needs a type"), fn_span(body.stream())),
            _ => {}
        }
    }
    panic!("proxy needs an impl block")
}

fn fn_span(body: TokenStream) -> Span {
    for token in body {
        if let TokenTree::Ident(ident) = &token
            && ident.to_string() == "fn"
        {
            return ident.span();
        }
    }
    panic!("proxy needs a method")
}

fn respan(stream: TokenStream, span: Span) -> TokenStream {
    let mut out = Vec::new();
    for mut token in stream {
        if let TokenTree::Group(group) = &token {
            let mut inner = Group::new(group.delimiter(), respan(group.stream(), span));
            inner.set_span(span);
            token = TokenTree::Group(inner);
        } else {
            token.set_span(span);
        }
        out.push(token);
    }
    out.into_iter().collect()
}

/// `#[retype(Arg)]` on `impl Type<..>` keeps it and adds `impl Type<Arg>` with a method `run`
/// of its own, every generated token carrying the span of the written method's `fn`.
#[proc_macro_attribute]
pub fn retype(attr: TokenStream, item: TokenStream) -> TokenStream {
    let (ty, span) = impl_parts(item.clone());
    let code = format!("impl {ty}<{attr}> {{ pub fn run(&self) -> u32 {{ crate::proxy_helper() }} }}");
    let generated = respan(code.parse().unwrap(), span);
    item.into_iter().chain(generated).collect()
}

/// `#[keep]` returns its item unchanged.
#[proc_macro_attribute]
pub fn keep(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// `#[retrait(Path)]` on `impl Trait for Type { fn name .. }` keeps it and adds
/// `impl Path for Type` with a method `name` of its own, every generated token carrying the
/// span of the written method's `fn`.
#[proc_macro_attribute]
pub fn retrait(attr: TokenStream, item: TokenStream) -> TokenStream {
    let (ty, name, span) = trait_impl_parts(item.clone());
    let code = format!("impl {attr} for {ty} {{ fn {name}(&self) -> u32 {{ crate::proxy_helper() }} }}");
    let generated = respan(code.parse().unwrap(), span);
    item.into_iter().chain(generated).collect()
}

fn trait_impl_parts(item: TokenStream) -> (String, String, Span) {
    let mut tokens = item.into_iter();
    let mut ty = None;
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Ident(ident) if ident.to_string() == "for" => {
                if let Some(TokenTree::Ident(name)) = tokens.next() {
                    ty = Some(name.to_string());
                }
            }
            TokenTree::Group(body) => {
                let mut inner = body.stream().into_iter();
                while let Some(token) = inner.next() {
                    if let TokenTree::Ident(ident) = &token
                        && ident.to_string() == "fn"
                        && let Some(TokenTree::Ident(name)) = inner.next()
                    {
                        return (ty.expect("retrait needs `for Type`"), name.to_string(), ident.span());
                    }
                }
            }
            _ => {}
        }
    }
    panic!("retrait needs a trait impl with a method")
}

/// `#[copy_to(Other)]` on `impl Type { .. }` keeps it and adds the same block for `Other`,
/// every copied token keeping its written span.
#[proc_macro_attribute]
pub fn copy_to(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut copy = Vec::new();
    let mut after_impl = false;
    for token in item.clone() {
        match &token {
            TokenTree::Ident(ident) if ident.to_string() == "impl" => after_impl = true,
            TokenTree::Ident(_) if after_impl => {
                copy.extend(attr.clone());
                after_impl = false;
                continue;
            }
            _ => {}
        }
        copy.push(token);
    }
    item.into_iter().chain(copy).collect()
}

/// `#[derive(Label)]` gives a type an associated `label` function, every generated token
/// carrying the span of the item's last token: its `;`, or the closing brace of its body.
#[proc_macro_derive(Label)]
pub fn label(input: TokenStream) -> TokenStream {
    let name = struct_name(input.clone());
    let span = match input.into_iter().last() {
        Some(TokenTree::Group(body)) => body.span_close(),
        Some(token) => token.span(),
        None => Span::call_site(),
    };
    let code = format!("impl {name} {{ pub fn label() -> u32 {{ crate::proxy_helper() }} }}");
    respan(code.parse().unwrap(), span)
}
