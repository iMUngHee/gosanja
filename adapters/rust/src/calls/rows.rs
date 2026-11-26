//! Corroborate direct instructions with written definitions and indexed call references.
use super::{
    debug::{Module, RootFiles, Source, Subprogram},
    ir::Target,
};
use crate::syntax::{call_facts::Facts, convert, is_entity, position::Lines};
use gosanja_schema::{
    contract::{Anchor, CallEvidence, CallSite, SkippedFile},
    position::Range,
    snapshot::{Mechanism, Resolution},
};
use protobuf::EnumFull;
use scip::types::{Index, SymbolRole, symbol_information::Kind};
use std::collections::{BTreeMap, BTreeSet};

/// Published instruction evidence and the omissions attributable to project files.
#[derive(Debug, Default)]
pub struct Rows {
    /// One row per instruction, in serialized byte order.
    pub evidence: Vec<CallEvidence>,
    /// One entry per file and omission code.
    pub skipped: Vec<SkippedFile>,
}

type Point = (u32, u32);
#[derive(Debug)]
struct Definition {
    symbol: String,
    name: String,
    pos: Range,
    extent: Option<Range>,
    function: bool,
}
struct Document {
    facts: Option<Facts>,
    lines: Vec<String>,
    definitions: Vec<Definition>,
    starts: BTreeMap<Point, usize>,
    by_name: BTreeMap<(u32, String), Vec<usize>>,
    refs: BTreeMap<(u32, String), BTreeSet<Range>>,
}
impl Document {
    fn new(src: &Source, index: &Index) -> Self {
        let text = std::str::from_utf8(&src.bytes).ok();
        let facts = text.and_then(Facts::parse);
        let text = text.unwrap_or_default();
        let lines = Lines::new(text);
        let mut document = Self {
            facts,
            lines: text.split('\n').map(str::to_owned).collect(),
            definitions: Vec::new(),
            starts: BTreeMap::new(),
            by_name: BTreeMap::new(),
            refs: BTreeMap::new(),
        };
        let mut places = BTreeMap::new();
        let mut names = BTreeMap::new();
        for doc in index
            .documents
            .iter()
            .filter(|d| d.relative_path == src.path)
        {
            let Ok(encoding) = doc.position_encoding.enum_value() else {
                continue;
            };
            if encoding == scip::types::PositionEncoding::UnspecifiedPositionEncoding {
                continue;
            }
            for s in &doc.symbols {
                if let Ok(kind) = s.kind.enum_value()
                    && kind != Kind::UnspecifiedKind
                {
                    let entry = names.entry(s.symbol.as_str()).or_insert(kind);
                    if kind.descriptor().name() < entry.descriptor().name() {
                        *entry = kind;
                    }
                }
            }
            for o in &doc.occurrences {
                if !is_entity(&o.symbol) {
                    continue;
                }
                let Some((pos, extent)) = convert(o, |r| lines.scip_range(r, encoding)) else {
                    continue;
                };
                if o.symbol_roles & SymbolRole::Definition as i32 != 0 {
                    let place = places.entry((o.symbol.as_str(), pos)).or_insert(None);
                    *place = (*place).max(extent);
                } else {
                    document
                        .refs
                        .entry((pos.0[0], o.symbol.clone()))
                        .or_default()
                        .insert(pos);
                }
            }
        }
        for ((symbol, pos), extent) in places {
            let Some(descriptor) = scip::symbol::parse_symbol(symbol)
                .ok()
                .and_then(|mut sym| sym.descriptors.pop())
            else {
                continue;
            };
            let name = descriptor.name;
            let function = matches!(
                names.get(symbol),
                Some(
                    Kind::Function
                        | Kind::Method
                        | Kind::StaticMethod
                        | Kind::TraitMethod
                        | Kind::AbstractMethod
                )
            );
            *document.starts.entry(pos.start()).or_default() += 1;
            document
                .by_name
                .entry((pos.0[0], name.clone()))
                .or_default()
                .push(document.definitions.len());
            document.definitions.push(Definition {
                symbol: symbol.into(),
                name,
                pos,
                extent,
                function,
            });
        }
        document
    }
    fn line(&self, l: u32) -> Option<&str> {
        self.lines.get(l as usize).map(String::as_str)
    }
    fn token(&self, r: Range) -> Option<&str> {
        let [start_line, start_col, end_line, end_col] = r.0;
        if start_line != end_line {
            return None;
        }
        let line = self.line(start_line)?;
        let byte = |i: u32| {
            line.char_indices()
                .map(|(j, _)| j)
                .chain(std::iter::once(line.len()))
                .nth(i as usize)
        };
        line.get(byte(start_col)?..byte(end_col)?)
    }
    fn call_head(&self, r: Range) -> bool {
        let Some(token) = self.token(r) else {
            return false;
        };
        if matches!(
            token,
            "+" | "-"
                | "*"
                | "/"
                | "%"
                | "^"
                | "!"
                | "&"
                | "|"
                | "<<"
                | ">>"
                | "=="
                | "!="
                | "<"
                | ">"
                | "<="
                | ">="
                | "+="
                | "-="
                | "*="
                | "/="
                | "%="
                | "^="
                | "&="
                | "|="
                | "<<="
                | ">>="
                | "[]"
        ) {
            return true;
        }
        if syn::parse_str::<syn::Ident>(token).is_err() {
            return false;
        }
        let (l, c) = r.end();
        let mut chars = self.lines[l as usize..]
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                s.chars()
                    .skip(if i == 0 { c as usize } else { 0 })
                    .chain(std::iter::once('\n'))
            })
            .filter(|c| !c.is_whitespace());
        match chars.next() {
            Some('(') => true,
            Some(':') if chars.next() == Some(':') && chars.next() == Some('<') => {
                let mut depth = 1;
                for c in chars.by_ref() {
                    match c {
                        '<' => depth += 1,
                        '>' => depth -= 1,
                        _ => {}
                    }
                    if depth == 0 {
                        return chars.next() == Some('(');
                    }
                }
                false
            }
            _ => false,
        }
    }
    fn callee(&self, sp: &Subprogram) -> Option<&Definition> {
        let ids = self
            .by_name
            .get(&(sp.line.checked_sub(1)?, own_name(&sp.name).to_owned()))?;
        let [i] = ids.as_slice() else { return None };
        self.definitions.get(*i)
    }
    fn owner(&self, r: Range) -> Option<usize> {
        let mut covers: Vec<_> = self
            .definitions
            .iter()
            .enumerate()
            .filter_map(|(i, d)| {
                d.extent
                    .filter(|e| d.function && e.contains(r))
                    .map(|e| (i, e))
            })
            .collect();
        // Outer to inner, including ranges that begin at the same column.
        covers.sort_unstable_by_key(|(_, e)| (e.start(), std::cmp::Reverse(e.end())));
        if !covers
            .windows(2)
            .all(|pair| pair[0].1 != pair[1].1 && pair[0].1.contains(pair[1].1))
        {
            return None;
        }
        covers.last().map(|(i, _)| *i)
    }
    fn end(&self, d: &Definition) -> bool {
        self.starts.get(&d.pos.start()) == Some(&1)
            && self
                .facts
                .as_ref()
                .is_some_and(|f| f.end(&d.name, d.pos.start()))
    }
}
fn own_name(name: &str) -> &str {
    if name.ends_with('>')
        && let Some(i) = name.find('<')
    {
        &name[..i]
    } else {
        name
    }
}
fn project_name(symbol: &str, crates: &BTreeSet<String>) -> bool {
    if !symbol.starts_with("_R") {
        return false;
    }
    let Ok(parsed) = rustc_demangle::try_demangle(symbol) else {
        return false;
    };
    let path = format!("{parsed:#}");
    let root = |s: &str| {
        s.trim_start_matches('&')
            .trim_start_matches("mut ")
            .split([':', '<', ' ', '>'])
            .next()
            .is_some_and(|c| crates.contains(c))
    };
    let Some(implementation) = path.strip_prefix('<') else {
        return root(&path);
    };
    if root(implementation) {
        return true;
    }
    let mut depth = 0usize;
    for (i, c) in implementation.char_indices() {
        match c {
            '<' => depth += 1,
            // A function pointer's return arrow can occur in an impl receiver.
            '>' if !implementation[..i].ends_with('-') => {
                let Some(parent) = depth.checked_sub(1) else {
                    break;
                };
                depth = parent;
            }
            ' ' if depth == 0 => {
                if let Some(trait_path) = implementation[i..].strip_prefix(" as ") {
                    return root(trait_path);
                }
            }
            _ => {}
        }
    }
    false
}
struct Candidate<'a> {
    site: String,
    callee: String,
    sp: &'a Subprogram,
    scope: &'a Subprogram,
    scope_file: Option<String>,
    line: u32,
    column: u32,
    build: &'a str,
    physical: (usize, usize),
}
fn corroborate(
    c: &Candidate<'_>,
    doc: &Document,
    to: &Document,
    excess: bool,
) -> Option<CallEvidence> {
    let callee = to.callee(c.sp)?;
    let refs = doc.refs.get(&(c.line, callee.symbol.clone()))?;
    let mut callers = refs
        .iter()
        .filter(|r| doc.call_head(**r))
        .map(|r| doc.owner(*r));
    let caller_id = callers.next()??;
    if excess || !callers.all(|id| id == Some(caller_id)) {
        return None;
    }
    let owner = &doc.definitions[caller_id];
    let facts = doc.facts.as_ref()?;
    let prefix = doc.line(c.line)?.get(..c.column.checked_sub(1)? as usize)?;
    let col = u32::try_from(prefix.chars().count()).ok()?;
    if !doc.end(owner)
        || !to.end(callee)
        || facts.taken(c.line)
        || c.scope_file.as_ref() != Some(&c.site)
        || !facts.owns(
            owner.pos.start(),
            (c.line, col),
            own_name(&c.scope.name),
            c.scope.line.checked_sub(1)?,
        )
    {
        return None;
    }
    let anchor = |file: &str, pos: Range| Anchor {
        file: file.to_owned(),
        line: pos.0[0],
        col: Some(pos.0[1]),
        name: None,
    };
    Some(CallEvidence {
        caller: anchor(&c.site, owner.pos),
        callee: anchor(&c.callee, callee.pos),
        mechanism: Mechanism::Static,
        resolution: Resolution::Direct,
        site: Some(CallSite {
            file: c.site.clone(),
            line: c.line,
            col: Some(col),
        }),
        build: c.build.to_owned(),
    })
}

type Counts = BTreeMap<(String, u32, String), BTreeMap<(usize, usize), usize>>;
fn instruction_counts(candidates: &[Candidate<'_>], docs: &BTreeMap<String, Document>) -> Counts {
    let mut counts = BTreeMap::new();
    for c in candidates {
        if let Some(d) = docs[&c.callee].callee(c.sp) {
            *counts
                .entry((c.site.clone(), c.line, d.symbol.clone()))
                .or_insert_with(BTreeMap::new)
                .entry(c.physical)
                .or_insert(0usize) += 1;
        }
    }
    counts
}

/// Reads indexed occurrences only in documents needed by candidate calls.
#[must_use]
pub fn rows(
    modules: &[(&Module, &str)],
    index: &Index,
    files: &mut RootFiles,
    crates: &BTreeSet<String>,
) -> Rows {
    let mut docs = BTreeMap::new();
    let mut skipped = BTreeSet::new();
    let mut candidates = Vec::new();
    for (mi, &(module, build)) in modules.iter().enumerate() {
        let mut source = |id: Option<u32>| files.get(module.file(id?));
        for (fi, function) in module.functions.iter().enumerate() {
            let Some(holder) = function.subprogram else {
                continue;
            };
            for ins in &function.instructions {
                if matches!(&ins.target, Target::Direct(name) if name.starts_with("llvm.")) {
                    continue;
                }
                let Some(loc) = ins.location.filter(|l| l.line != 0) else {
                    if matches!(ins.target, Target::Direct(_))
                        && let Some(src) = source(module.subprogram(holder).file)
                    {
                        skipped.insert((src.path, "no-debug-location"));
                    }
                    continue;
                };
                let Some(site) = source(module.scope_file(loc.scope)) else {
                    continue;
                };
                let Target::Direct(symbol) = &ins.target else {
                    skipped.insert((site.path, "indirect-call"));
                    continue;
                };
                let Some(Some(sp)) = module.definition(symbol) else {
                    if module.declared.contains(symbol) && project_name(symbol, crates) {
                        skipped.insert((site.path, "callee-unresolved"));
                    }
                    continue;
                };
                let Some(callee) = source(sp.file) else {
                    continue;
                };
                for src in [&site, &callee] {
                    docs.entry(src.path.clone())
                        .or_insert_with(|| Document::new(src, index));
                }
                let scope = module.subprogram(module.scope_subprogram(loc.scope));
                candidates.push(Candidate {
                    site: site.path,
                    callee: callee.path,
                    sp,
                    scope,
                    scope_file: source(scope.file).map(|s| s.path),
                    line: loc.line - 1,
                    column: loc.column,
                    build,
                    physical: (mi, fi),
                });
            }
        }
    }
    let counts = instruction_counts(&candidates, &docs);
    let mut evidence = Vec::new();
    for c in &candidates {
        let doc = &docs[&c.site];
        let to = &docs[&c.callee];
        if let Some(p) = [&c.site, &c.callee]
            .into_iter()
            .find(|p| docs[*p].facts.is_none())
        {
            skipped.insert((p.clone(), "parse-failed"));
            continue;
        }
        let excess = to.callee(c.sp).is_some_and(|d| {
            let refs = doc
                .refs
                .get(&(c.line, d.symbol.clone()))
                .map_or(0, |rs| rs.iter().filter(|r| doc.call_head(**r)).count());
            counts[&(c.site.clone(), c.line, d.symbol.clone())]
                .values()
                .any(|n| *n > refs)
        });
        if let Some(row) = corroborate(c, doc, to, excess) {
            evidence.push(row);
        } else {
            skipped.insert((c.site.clone(), "uncorroborated"));
        }
    }
    evidence.sort_by_cached_key(serialized_row);
    Rows {
        evidence,
        skipped: skipped
            .into_iter()
            .map(|(file, code)| SkippedFile {
                file,
                code: code.into(),
            })
            .collect(),
    }
}

fn serialized_row(row: &CallEvidence) -> Vec<u8> {
    serde_json::to_vec(row).expect("serializing a typed evidence record to a byte vector")
}
