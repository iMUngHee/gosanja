//! The syntax layer: rows of `syntax.jsonl` read from the source files with syn.

mod items;
mod outline;
mod position;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use gosanja_schema::contract::{Anchor, LayerResult, LayerStatus, SkippedFile, SyntaxFact};
use gosanja_schema::position::Range;
use gosanja_schema::snapshot::OutlineNode;
use scip::types::occurrence::{Typed_enclosing_range, Typed_range};
use scip::types::{Occurrence, SymbolRole};

use position::Lines;

/// The syntax layer of one run.
#[derive(Debug, Default)]
pub struct Layer {
    /// Rows in file, line and column order.
    pub rows: Vec<SyntaxFact>,
    /// Files the layer could not process, or processed in part, sorted by file and code.
    pub skipped: Vec<SkippedFile>,
    /// Lines for standard error.
    pub notes: Vec<String>,
}

/// Why a file gives no rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// It could not be read, or is not UTF-8.
    Unreadable,
    /// syn rejected it.
    ParseFailed,
    /// It starts with a byte-order mark.
    ByteOrderMark,
}

impl Skip {
    /// The code `skipped` gives it.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::ParseFailed => "parse-failed",
            Self::ByteOrderMark => "byte-order-mark",
        }
    }
}

/// The rows of one file's text, without the check against the index.
///
/// # Errors
/// Why the file gives no rows.
pub fn rows(file: &str, text: &str) -> Result<Vec<SyntaxFact>, Skip> {
    Ok(items(text)?.into_iter().map(|i| fact(file, i)).collect())
}

fn items(text: &str) -> Result<Vec<items::Item>, Skip> {
    if text.starts_with('\u{feff}') {
        return Err(Skip::ByteOrderMark);
    }
    let result = syn::parse_file(text)
        .ok()
        .map(|f| items::of_file(&f))
        .ok_or(Skip::ParseFailed);
    // Every position has been taken out of the spans; the per-thread table can go.
    proc_macro2::extra::invalidate_current_thread_spans();
    let mut items = result?;
    items.sort_by_key(|i| (i.line, i.col));
    Ok(items)
}

fn fact(file: &str, item: items::Item) -> SyntaxFact {
    let some = |v: Vec<String>| (!v.is_empty()).then_some(v);
    SyntaxFact {
        anchor: Anchor {
            file: file.to_owned(),
            line: item.line,
            col: Some(item.col),
            name: None,
        },
        body: None,
        visibility: item.visibility,
        qualifiers: some(item.qualifiers),
        attributes: some(item.attributes),
        outline: item.outline,
    }
}

/// The layer for the documents of `index` under `root`, which must be resolved.
#[must_use]
pub fn layer(root: &Path, index: &scip::types::Index) -> Layer {
    let mut layer = Layer::default();
    let mut skipped = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut files: Vec<&str> = Vec::new();
    for document in &index.documents {
        let file = document.relative_path.as_str();
        if seen.insert(file) && admitted(root, file) {
            files.push(file);
        }
    }
    files.sort_unstable();
    for file in files {
        let Ok(Ok(text)) = fs::read(root.join(file)).map(String::from_utf8) else {
            skipped.insert((file.to_owned(), Skip::Unreadable.code()));
            continue;
        };
        let items = match items(&text) {
            Ok(items) => items,
            Err(skip) => {
                skipped.insert((file.to_owned(), skip.code()));
                continue;
            }
        };
        let extents = Extents::of(index, file, &Lines::new(&text));
        for mut item in items {
            if let Some(outline) = &item.outline
                && let Some(code) = check(outline, extents.at(item.line, item.col))
            {
                layer.notes.push(format!(
                    "{file}:{}:{}: {} is sent without its outline ({code})",
                    item.line + 1,
                    item.col + 1,
                    item.name
                ));
                item.outline = None;
                skipped.insert((file.to_owned(), code));
            }
            layer.rows.push(fact(file, item));
        }
    }
    layer.skipped = skipped
        .into_iter()
        .map(|(file, code)| SkippedFile {
            file,
            code: code.to_owned(),
        })
        .collect();
    layer
}

/// Runs the layer for the documents of `index` under `root`, writes `syntax.jsonl` into
/// `out`, prints the notes on standard error and gives the layer's entry of `result.json`.
///
/// # Errors
/// When `root` cannot be resolved or the file cannot be written.
pub fn extract(root: &Path, out: &Path, index: &scip::types::Index) -> io::Result<LayerResult> {
    let layer = layer(&root.canonicalize()?, index);
    write(&layer.rows, &out.join("syntax.jsonl"))?;
    for note in &layer.notes {
        eprintln!("{note}");
    }
    Ok(LayerResult {
        status: LayerStatus::Ok,
        diagnostics: None,
        skipped: (!layer.skipped.is_empty()).then_some(layer.skipped),
    })
}

/// Writes the rows as JSON Lines.
///
/// # Errors
/// When the file cannot be written.
pub fn write(rows: &[SyntaxFact], path: &Path) -> io::Result<()> {
    let mut out = io::BufWriter::new(fs::File::create(path)?);
    for row in rows {
        serde_json::to_writer(&mut out, row)?;
        out.write_all(b"\n")?;
    }
    out.flush()
}

/// Whether a document is read: a `.rs` path under the specification's path rules that
/// resolves, links followed, to a regular file inside `root`.
fn admitted(root: &Path, file: &str) -> bool {
    let drive = file.as_bytes().get(1) == Some(&b':');
    let valid = Path::new(file).extension() == Some("rs".as_ref())
        && !drive
        && !file.contains('\\')
        && file
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..");
    valid
        && root
            .join(file)
            .canonicalize()
            .is_ok_and(|p| p.starts_with(root) && p.is_file())
}

/// Why an outline is withheld, if it is.
fn check(outline: &[OutlineNode], extent: Option<Range>) -> Option<&'static str> {
    if outline::depth(outline) > outline::MAX_DEPTH {
        return Some("outline-too-deep");
    }
    let inside = extent.is_none_or(|e| outline.iter().all(|n| e.contains(n.range)));
    (!inside || !nested(outline, None)).then_some("outline-outside-definition")
}

/// The contract's range rules: well-formed, children inside their parent, a callee inside
/// its node.
fn nested(nodes: &[OutlineNode], parent: Option<Range>) -> bool {
    nodes.iter().all(|n| {
        n.range.start() <= n.range.end()
            && parent.is_none_or(|p| p.contains(n.range))
            && n.callee
                .is_none_or(|c| c.start() <= c.end() && n.range.contains(c))
            && n.children
                .as_deref()
                .is_none_or(|c| nested(c, Some(n.range)))
    })
}

/// The extents the index gives the definitions of one file, in characters, found the way
/// the engine finds them.
struct Extents {
    /// Name start -> each (symbol, name range) defined there -> the greatest enclosing range.
    at: BTreeMap<(u32, u32), Definitions>,
}

/// The definitions that start at one position.
type Definitions = BTreeMap<(String, Range), Option<Range>>;

impl Extents {
    fn of(index: &scip::types::Index, file: &str, lines: &Lines<'_>) -> Self {
        let mut at: BTreeMap<_, Definitions> = BTreeMap::new();
        let occurrences = index
            .documents
            .iter()
            .filter(|d| d.relative_path == file)
            .flat_map(|d| d.occurrences.iter())
            .filter(|o| o.symbol_roles & SymbolRole::Definition as i32 != 0)
            .filter(|o| is_entity(&o.symbol));
        for occurrence in occurrences {
            // An occurrence the engine cannot convert is dropped there too.
            let Some((name, enclosing)) = convert(occurrence, lines) else {
                continue;
            };
            let extent = at
                .entry(name.start())
                .or_default()
                .entry((occurrence.symbol.clone(), name))
                .or_default();
            *extent = (*extent).max(enclosing);
        }
        Self { at }
    }

    /// The extent of the one definition at an anchor; `None` when there is none, several, or
    /// one without an enclosing range.
    fn at(&self, line: u32, col: u32) -> Option<Range> {
        let mut definitions = self.at.get(&(line, col))?.values();
        let first = *definitions.next()?;
        if definitions.next().is_some() {
            return None;
        }
        first
    }
}

/// Whether the engine makes an entity of a symbol's definition: not of a local symbol, and
/// not of a parameter, whose descriptor ends in `)`.
fn is_entity(symbol: &str) -> bool {
    !symbol.starts_with("local ") && !symbol.ends_with(')')
}

/// An occurrence's name and enclosing ranges in characters, typed fields first as the engine
/// reads them; `None` when one cannot be converted against the file.
fn convert(o: &Occurrence, lines: &Lines<'_>) -> Option<(Range, Option<Range>)> {
    let name = match &o.typed_range {
        Some(Typed_range::SingleLineRange(r)) => {
            [r.line, r.start_character, r.line, r.end_character]
        }
        Some(Typed_range::MultiLineRange(r)) => {
            [r.start_line, r.start_character, r.end_line, r.end_character]
        }
        _ => legacy(&o.range)?,
    };
    let enclosing = match &o.typed_enclosing_range {
        Some(Typed_enclosing_range::SingleLineEnclosingRange(r)) => {
            Some([r.line, r.start_character, r.line, r.end_character])
        }
        Some(Typed_enclosing_range::MultiLineEnclosingRange(r)) => {
            Some([r.start_line, r.start_character, r.end_line, r.end_character])
        }
        _ if o.enclosing_range.is_empty() => None,
        _ => Some(legacy(&o.enclosing_range)?),
    };
    let name = lines.char_range(unsigned(name)?)?;
    let enclosing = match enclosing {
        Some(raw) => Some(lines.char_range(unsigned(raw)?)?),
        None => None,
    };
    Some((name, enclosing))
}

fn legacy(raw: &[i32]) -> Option<[i32; 4]> {
    match *raw {
        [line, start, end] => Some([line, start, line, end]),
        [sl, sc, el, ec] => Some([sl, sc, el, ec]),
        _ => None,
    }
}

fn unsigned(raw: [i32; 4]) -> Option<Range> {
    let [sl, sc, el, ec] = raw.map(|v| u32::try_from(v).ok());
    Some(Range::new(sl?, sc?, el?, ec?))
}
