//! Keys for symbols: base keys, qualifiers, collision groups and containers.
//!
//! The rules are in `spec/snapshot.md`, "Keys".

use std::collections::{BTreeMap, BTreeSet};

use gosanja_schema::snapshot::Role;

/// The kind of a SCIP descriptor, named by its suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suffix {
    /// `name/`
    Namespace,
    /// `name#`
    Type,
    /// `name.`
    Term,
    /// `name(disambiguator).`
    Method,
    /// `[name]`
    TypeParameter,
    /// `(name)`
    Parameter,
    /// `name:`
    Meta,
    /// `name!`
    Macro,
}

impl Suffix {
    /// Whether a symbol ending in this descriptor is a row only as a container.
    #[must_use]
    pub const fn is_parameter(self) -> bool {
        matches!(self, Self::TypeParameter | Self::Parameter)
    }
}

/// One descriptor of a global symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// Byte offset in the symbol string just past this descriptor.
    pub end: usize,
    /// What the descriptor names.
    pub suffix: Suffix,
    /// The name, with backtick escaping removed.
    pub name: String,
}

/// A parsed global SCIP symbol. Every slice it hands out borrows the original string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol<'a> {
    raw: &'a str,
    /// Scheme, unescaped.
    pub scheme: String,
    /// Package manager, unescaped.
    pub manager: String,
    /// Package name, unescaped.
    pub package: String,
    /// Package version, unescaped.
    pub version: String,
    descriptors_start: usize,
    /// The descriptors in order; never empty.
    pub descriptors: Vec<Descriptor>,
}

/// What a symbol string is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed<'a> {
    /// `local <id>`.
    Local,
    /// A global symbol.
    Global(Symbol<'a>),
}

/// A symbol string that does not follow the SCIP grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSymbol;

/// Parses a SCIP symbol.
///
/// # Errors
///
/// Returns [`InvalidSymbol`] when the string does not follow the grammar in `scip.proto`.
pub fn parse(raw: &str) -> Result<Parsed<'_>, InvalidSymbol> {
    if raw.starts_with("local ") {
        return Ok(Parsed::Local);
    }
    let mut at = 0;
    let scheme = space_terminated(raw, &mut at)?;
    let manager = space_terminated(raw, &mut at)?;
    let package = space_terminated(raw, &mut at)?;
    let version = space_terminated(raw, &mut at)?;
    if scheme.is_empty() {
        return Err(InvalidSymbol);
    }
    let descriptors_start = at;
    let mut descriptors = Vec::new();
    while at < raw.len() {
        descriptors.push(descriptor(raw, &mut at)?);
    }
    if descriptors.is_empty() {
        return Err(InvalidSymbol);
    }
    Ok(Parsed::Global(Symbol {
        raw,
        scheme,
        manager,
        package,
        version,
        descriptors_start,
        descriptors,
    }))
}

/// Reads a part ended by a single space, where two spaces stand for one literal space.
fn space_terminated(raw: &str, at: &mut usize) -> Result<String, InvalidSymbol> {
    let mut out = String::new();
    let mut chars = raw[*at..].char_indices();
    while let Some((i, c)) = chars.next() {
        if c != ' ' {
            out.push(c);
            continue;
        }
        if raw[*at + i + 1..].starts_with(' ') {
            out.push(' ');
            chars.next();
            continue;
        }
        *at += i + 1;
        return Ok(out);
    }
    Err(InvalidSymbol)
}

fn descriptor(raw: &str, at: &mut usize) -> Result<Descriptor, InvalidSymbol> {
    let rest = &raw[*at..];
    let (name, suffix, used) = if let Some(inner) = rest.strip_prefix('[') {
        let (name, n) = name(inner)?;
        expect(&inner[n..], ']')?;
        (name, Suffix::TypeParameter, 1 + n + 1)
    } else if let Some(inner) = rest.strip_prefix('(') {
        let (name, n) = name(inner)?;
        expect(&inner[n..], ')')?;
        (name, Suffix::Parameter, 1 + n + 1)
    } else {
        let (name, n) = name(rest)?;
        let after = &rest[n..];
        let mut tail = after.chars();
        let suffix = match tail.next().ok_or(InvalidSymbol)? {
            '/' => Suffix::Namespace,
            '#' => Suffix::Type,
            '.' => Suffix::Term,
            ':' => Suffix::Meta,
            '!' => Suffix::Macro,
            '(' => Suffix::Method,
            _ => return Err(InvalidSymbol),
        };
        let used = if suffix == Suffix::Method {
            let close = after.find(").").ok_or(InvalidSymbol)?;
            if !after[1..close].chars().all(is_identifier_char) {
                return Err(InvalidSymbol);
            }
            n + close + 2
        } else {
            n + 1
        };
        (name, suffix, used)
    };
    *at += used;
    Ok(Descriptor {
        end: *at,
        suffix,
        name,
    })
}

fn expect(rest: &str, c: char) -> Result<(), InvalidSymbol> {
    if rest.starts_with(c) {
        Ok(())
    } else {
        Err(InvalidSymbol)
    }
}

const fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '$')
}

/// Reads a simple or backtick-escaped name; returns it unescaped and the bytes consumed.
fn name(rest: &str) -> Result<(String, usize), InvalidSymbol> {
    if let Some(inner) = rest.strip_prefix('`') {
        let mut out = String::new();
        let mut chars = inner.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            if c != '`' {
                out.push(c);
            } else if chars.peek().is_some_and(|&(_, next)| next == '`') {
                out.push('`');
                chars.next();
            } else if out.is_empty() {
                return Err(InvalidSymbol);
            } else {
                return Ok((out, 1 + i + 1));
            }
        }
        return Err(InvalidSymbol);
    }
    let n = rest
        .char_indices()
        .find(|&(_, c)| !(is_identifier_char(c) || (!c.is_ascii() && c.is_alphanumeric())))
        .map_or(rest.len(), |(i, _)| i);
    if n == 0 {
        return Err(InvalidSymbol);
    }
    Ok((rest[..n].to_owned(), n))
}

impl<'a> Symbol<'a> {
    /// The full symbol string.
    #[must_use]
    pub const fn raw(&self) -> &'a str {
        self.raw
    }

    /// The last descriptor.
    #[must_use]
    pub fn last(&self) -> &Descriptor {
        // `parse` refuses a symbol without descriptors.
        &self.descriptors[self.descriptors.len() - 1]
    }

    /// The full symbol of each proper container, outermost first.
    pub fn containers(&self) -> impl Iterator<Item = &'a str> + '_ {
        let raw = self.raw;
        self.descriptors[..self.descriptors.len() - 1]
            .iter()
            .map(move |d| &raw[..d.end])
    }

    /// Each proper container as a symbol of its own, outermost first.
    pub fn container_symbols(&self) -> impl Iterator<Item = Self> + '_ {
        (1..self.descriptors.len()).map(|n| Self {
            raw: &self.raw[..self.descriptors[n - 1].end],
            descriptors: self.descriptors[..n].to_vec(),
            ..self.clone()
        })
    }

    /// The full symbol of the longest proper container, if there is one.
    #[must_use]
    pub fn parent(&self) -> Option<&'a str> {
        self.containers().last()
    }

    /// `<manager>:<package>:<descriptors>`, escaped.
    #[must_use]
    pub fn base_key(&self) -> String {
        let mut key = escape_component(&self.manager);
        key.push(':');
        key.push_str(&escape_component(&self.package));
        key.push(':');
        for c in self.raw[self.descriptors_start..].chars() {
            match c {
                '%' => key.push_str("%25"),
                '@' => key.push_str("%40"),
                '~' => key.push_str("%7E"),
                c => key.push(c),
            }
        }
        key
    }
}

/// Escapes a manager, package, version or scheme so that it cannot read as a key delimiter.
#[must_use]
pub fn escape_component(part: &str) -> String {
    let mut out = String::with_capacity(part.len());
    for c in part.chars() {
        match c {
            '%' => out.push_str("%25"),
            ':' => out.push_str("%3A"),
            '@' => out.push_str("%40"),
            '~' => out.push_str("%7E"),
            c => out.push(c),
        }
    }
    out
}

/// The role a symbol's last descriptor gives it, from the table in `spec/snapshot.md`.
#[must_use]
pub const fn role(last: Suffix, parent_is_type: bool) -> Role {
    match last {
        Suffix::Namespace | Suffix::TypeParameter | Suffix::Parameter => Role::Container,
        Suffix::Type => Role::Type,
        Suffix::Method | Suffix::Macro => Role::Callable,
        Suffix::Term | Suffix::Meta if parent_is_type => Role::Member,
        Suffix::Term | Suffix::Meta => Role::Value,
    }
}

/// Assigns a key to every symbol that becomes a row.
///
/// Symbols are grouped by base key; in a group of more than one, each key is qualified by the
/// version and then the scheme where those differ within the group. The result does not depend
/// on the order of `symbols`.
#[must_use]
pub fn assign_keys<'a>(symbols: &BTreeMap<&'a str, Symbol<'a>>) -> BTreeMap<&'a str, String> {
    let mut groups: BTreeMap<String, Vec<&Symbol<'a>>> = BTreeMap::new();
    for symbol in symbols.values() {
        groups.entry(symbol.base_key()).or_default().push(symbol);
    }
    let mut keys = BTreeMap::new();
    for (base, members) in groups {
        let versions: BTreeSet<&str> = members.iter().map(|s| s.version.as_str()).collect();
        let schemes: BTreeSet<&str> = members.iter().map(|s| s.scheme.as_str()).collect();
        for symbol in members {
            let mut key = base.clone();
            if versions.len() > 1 {
                key.push('@');
                key.push_str(&escape_component(&symbol.version));
            }
            if schemes.len() > 1 {
                key.push('@');
                key.push_str(&escape_component(&symbol.scheme));
            }
            keys.insert(symbol.raw, key);
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn global(raw: &str) -> Symbol<'_> {
        match parse(raw).expect("parses") {
            Parsed::Global(symbol) => symbol,
            Parsed::Local => panic!("not global"),
        }
    }

    #[test]
    fn parses_rust_analyzer_impl_symbols() {
        let s = global("rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'_>`][Display]fmt().");
        let names: Vec<_> = s
            .descriptors
            .iter()
            .map(|d| (d.suffix, d.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                (Suffix::Namespace, "views"),
                (Suffix::Type, "impl"),
                (Suffix::TypeParameter, "Joined<'_>"),
                (Suffix::TypeParameter, "Display"),
                (Suffix::Method, "fmt"),
            ]
        );
        assert_eq!(
            s.parent(),
            Some("rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'_>`][Display]")
        );
        assert_eq!(
            s.base_key(),
            "cargo:demo:views/impl#[`Joined<'_>`][Display]fmt()."
        );
    }

    #[test]
    fn package_parts_unescape_double_spaces_and_keys_escape_delimiters() {
        let s = global("a  b m%:x p@q 1~2 `x@y`/f(+1).");
        assert_eq!(s.scheme, "a b");
        assert_eq!(s.manager, "m%:x");
        assert_eq!(s.package, "p@q");
        assert_eq!(s.version, "1~2");
        assert_eq!(s.descriptors[1].name, "f");
        assert_eq!(s.base_key(), "m%25%3Ax:p%40q:`x%40y`/f(+1).");
    }

    #[test]
    fn escaped_backticks_and_go_paths() {
        let s = global("scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/greet().");
        assert_eq!(s.descriptors[0].name, "example.com/go-basic");
        assert_eq!(global("s m p v `a``b`#").last().name, "a`b");
        assert_eq!(parse("local 4"), Ok(Parsed::Local));
    }

    #[test]
    fn rejects_malformed_symbols() {
        for raw in [
            "",
            "s m p",
            "s m p v ",
            "s m p v x",
            "s m p v ``#",
            "s m p v f(.",
            "s m p v [a",
        ] {
            assert_eq!(parse(raw), Err(InvalidSymbol), "{raw:?}");
        }
    }

    #[test]
    fn roles_follow_the_last_descriptor() {
        assert_eq!(role(Suffix::Term, true), Role::Member);
        assert_eq!(role(Suffix::Term, false), Role::Value);
        assert_eq!(role(Suffix::Meta, true), Role::Member);
        assert_eq!(role(Suffix::Macro, false), Role::Callable);
        assert_eq!(role(Suffix::TypeParameter, false), Role::Container);
        assert!(Suffix::Parameter.is_parameter());
        assert!(!Suffix::Type.is_parameter());
    }

    fn keys_for(raws: &[&'static str]) -> Vec<String> {
        let symbols: BTreeMap<_, _> = raws.iter().map(|&r| (r, global(r))).collect();
        let keys = assign_keys(&symbols);
        raws.iter().map(|r| keys[r].clone()).collect()
    }

    #[test]
    fn qualifiers_follow_the_specification_table() {
        assert_eq!(keys_for(&["s m p 1 f."]), ["m:p:f."]);
        assert_eq!(
            keys_for(&["s m p 1 f.", "s m p 2 f."]),
            ["m:p:f.@1", "m:p:f.@2"]
        );
        assert_eq!(
            keys_for(&["a m p 1 f.", "b m p 1 f."]),
            ["m:p:f.@a", "m:p:f.@b"]
        );
        assert_eq!(
            keys_for(&["a m p 1 f.", "b m p 2 f."]),
            ["m:p:f.@1@a", "m:p:f.@2@b"]
        );
        assert_eq!(
            keys_for(&["y m p 1@x f.", "x@y m p 1 f."]),
            ["m:p:f.@1%40x@y", "m:p:f.@1@x%40y"]
        );
    }

    #[test]
    fn component_boundaries_survive_serialisation() {
        let keys = keys_for(&["s a:b c 1 f.", "s a b:c 1 f."]);
        assert_ne!(keys[0], keys[1]);
    }
}
