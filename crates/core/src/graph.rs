//! Builds entities, containment and relations from decoded documents.
//!
//! [`build`] turns one SCIP index into the facts of a snapshot. Source text reaches it only
//! through the [`SourceReader`] it is given. The rules are in `spec/snapshot.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;

use gosanja_schema::position::{PositionEncoding, Range, Site};
use gosanja_schema::snapshot::{
    Def, EntityRow, ExternalRow, FileRow, GapKind, GapRow, LayerState, Layers, Package, Project,
    RefRole, Rel, RelationRow, Scope, SyntaxRow, Tool,
};
use protobuf::EnumFull;
use scip::types::occurrence::{Typed_enclosing_range, Typed_range};
use scip::types::symbol_information::Kind;
use scip::types::{Document, Occurrence, PositionEncoding as ScipEncoding, SymbolRole};
use sha2::{Digest, Sha256};

use crate::identity::{self, Parsed, Suffix, Symbol};
use crate::scip_stream::{self, DecodeError, Item};
use crate::write::{Location, SourceReader};

/// The facts of one snapshot, before they are sorted and written.
///
/// Rows may be in any order; [`crate::write`] sorts them and computes the counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Display information about the project.
    pub project: Project,
    /// The one indexed scope.
    pub scope: Scope,
    /// Rows of `files.jsonl`.
    pub files: Vec<FileRow>,
    /// Rows of `entities.jsonl`.
    pub entities: Vec<EntityRow>,
    /// Rows of `externals.jsonl`.
    pub externals: Vec<ExternalRow>,
    /// Rows of `relations.jsonl`.
    pub relations: Vec<RelationRow>,
    /// Rows of `syntax.jsonl`.
    pub syntax: Vec<SyntaxRow>,
    /// Rows of `gaps.jsonl`.
    pub gaps: Vec<GapRow>,
    /// References to local symbols, which are not recorded.
    pub dropped_local_references: u64,
}

/// Why no snapshot could be built from an index.
#[derive(Debug)]
pub enum BuildError {
    /// The index is not valid SCIP.
    Decode(DecodeError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(e) => Some(e),
        }
    }
}

const CONTAINMENT: &str = "core:containment";
const IDENTITY: &str = "core:identity";
const REFERENCE_IN_BODY: &str = "core:reference-in-body";

/// Reads a SCIP index and the sources it names and builds the base layer of a snapshot.
///
/// The scope is recorded as `.`, with the indexers named in the index metadata, the base layer
/// present and the other layers absent; an adapter runner amends it.
///
/// # Errors
///
/// Returns [`BuildError::Decode`] when the index is not valid SCIP. Problems with single
/// documents or occurrences are recorded as gaps instead.
pub fn build(
    index: &mut dyn Read,
    sources: &dyn SourceReader,
    project_name: &str,
) -> Result<Snapshot, BuildError> {
    build_with(index, sources, project_name, |_| {})
}

/// [`build`], with a step between building the entities and attributing references.
///
/// `before_references` sees the snapshot with its files, entities, externals and the gaps found
/// so far, and no relations. A body it sets on an entity's `def` takes part in ownership, which
/// is how a syntax layer supplies bodies the index lacks.
///
/// # Errors
///
/// As [`build`].
pub fn build_with(
    index: &mut dyn Read,
    sources: &dyn SourceReader,
    project_name: &str,
    before_references: impl FnOnce(&mut Snapshot),
) -> Result<Snapshot, BuildError> {
    let mut input = Input::default();
    scip_stream::for_each_item(index, |item| match item {
        Item::Metadata(metadata) => {
            let info = metadata.tool_info.get_or_default();
            input.tools.insert(Tool {
                name: info.name.clone(),
                version: info.version.clone(),
            });
            if input.tool_name.is_none() && !info.name.is_empty() {
                input.tool_name = Some(info.name.clone());
            }
        }
        Item::Document(document) => input.document(&document, sources),
        Item::ExternalSymbol(info) => {
            input.named.insert(info.symbol.clone());
        }
    })
    .map_err(BuildError::Decode)?;
    Ok(input.finish(project_name, before_references))
}

/// An occurrence whose positions were converted to UTF-8 bytes.
#[derive(Debug)]
struct Occ {
    file: String,
    symbol: String,
    roles: i32,
    range: Range,
    enclosing: Option<Range>,
}

#[derive(Debug, Default)]
struct Input {
    tools: BTreeSet<Tool>,
    tool_name: Option<String>,
    files: BTreeMap<String, FileRow>,
    occurrences: Vec<Occ>,
    /// Symbol to the `<language>:<kind>` strings any document gave it.
    kinds: BTreeMap<String, BTreeSet<String>>,
    /// Every symbol some input mentions.
    named: BTreeSet<String>,
    invalid_paths: u64,
    gaps: BTreeSet<(GapKind, String)>,
    out_of_range: BTreeMap<String, u64>,
}

impl Input {
    /// Checks location, availability and encoding, in that order, then converts positions.
    fn document(&mut self, document: &Document, sources: &dyn SourceReader) {
        let path = document.relative_path.as_str();
        if !is_valid_path(path) {
            self.invalid_paths += 1;
            return;
        }
        let text = match sources.locate(path) {
            Location::Outside => {
                self.gaps
                    .insert((GapKind::DocumentOutsideRoot, path.to_owned()));
                return;
            }
            Location::Inside => match sources.read(path) {
                Ok(text) => text,
                Err(_unavailable) => {
                    self.gaps
                        .insert((GapKind::SourceUnavailable, path.to_owned()));
                    return;
                }
            },
        };
        let unit = match document.position_encoding.enum_value() {
            Ok(ScipEncoding::UTF8CodeUnitOffsetFromLineStart) => PositionEncoding::Utf8,
            Ok(ScipEncoding::UTF16CodeUnitOffsetFromLineStart) => PositionEncoding::Utf16,
            Ok(ScipEncoding::UTF32CodeUnitOffsetFromLineStart) => PositionEncoding::Utf32,
            Ok(ScipEncoding::UnspecifiedPositionEncoding) | Err(_) => {
                self.gaps
                    .insert((GapKind::UndecodableDocument, path.to_owned()));
                return;
            }
        };
        let lines = LineIndex::new(&text);
        let language = document.language.to_lowercase();
        self.files
            .entry(path.to_owned())
            .or_insert_with(|| FileRow {
                path: path.to_owned(),
                language: (!language.is_empty()).then(|| language.clone()),
                bytes: text.len() as u64,
                lines: lines.count(),
                sha256: hex(&Sha256::digest(&text)),
            });
        for info in &document.symbols {
            self.named.insert(info.symbol.clone());
            if let Some(kind) = kind_name(&language, info.kind.enum_value()) {
                self.kinds
                    .entry(info.symbol.clone())
                    .or_default()
                    .insert(kind);
            }
        }
        for occurrence in &document.occurrences {
            if occurrence.symbol.is_empty() {
                continue;
            }
            self.named.insert(occurrence.symbol.clone());
            match convert(occurrence, &lines, unit) {
                Some((range, enclosing)) => self.occurrences.push(Occ {
                    file: path.to_owned(),
                    symbol: occurrence.symbol.clone(),
                    roles: occurrence.symbol_roles,
                    range,
                    enclosing,
                }),
                None => *self.out_of_range.entry(path.to_owned()).or_default() += 1,
            }
        }
    }

    fn finish(self, project_name: &str, before_references: impl FnOnce(&mut Snapshot)) -> Snapshot {
        let tool = self.tool_name.as_deref().unwrap_or("unknown");
        let scip_src = format!("scip:{tool}");
        let mut out = Snapshot {
            project: Project {
                name: project_name.to_owned(),
            },
            scope: Scope {
                path: ".".to_owned(),
                adapter: None,
                indexers: self.tools.iter().cloned().collect(),
                layers: Layers {
                    base: LayerState::Present,
                    syntax: LayerState::Absent,
                    calls: LayerState::Absent,
                },
                builds: Vec::new(),
            },
            files: self.files.values().cloned().collect(),
            entities: Vec::new(),
            externals: Vec::new(),
            relations: Vec::new(),
            syntax: Vec::new(),
            gaps: Vec::new(),
            dropped_local_references: 0,
        };
        self.document_gaps(&scip_src, &mut out.gaps);
        let graph = Graph::new(&self.occurrences);
        graph.entities(&self, &scip_src, &mut out);
        before_references(&mut out);
        graph.references(&self, &mut out);
        out
    }

    fn document_gaps(&self, scip_src: &str, gaps: &mut Vec<GapRow>) {
        if self.invalid_paths > 0 {
            let mut gap = GapRow::new(GapKind::DocumentOutsideRoot, scip_src);
            gap.count = Some(self.invalid_paths);
            gaps.push(gap);
        }
        for (kind, file) in &self.gaps {
            let mut gap = GapRow::new(*kind, scip_src);
            gap.file = Some(file.clone());
            gaps.push(gap);
        }
        for (file, count) in &self.out_of_range {
            let mut gap = GapRow::new(GapKind::PositionOutOfRange, scip_src);
            gap.file = Some(file.clone());
            gap.count = Some(*count);
            gaps.push(gap);
        }
    }
}

/// A path as `spec/README.md` allows it: relative, `/`-separated, no empty, `.` or `..`
/// segment, and no drive letter.
fn is_valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path.as_bytes().get(1) != Some(&b':')
        && path.split('/').all(|seg| !matches!(seg, "" | "." | ".."))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(2 * bytes.len()), |mut out, b| {
            // Writing to a String cannot fail.
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// `<language>:<kind>` in lower case, as `kind` in `entities.jsonl` wants it.
fn kind_name(language: &str, kind: Result<Kind, i32>) -> Option<String> {
    let kind = kind.ok().filter(|k| *k != Kind::UnspecifiedKind)?;
    if language.is_empty() || language.contains(':') {
        return None;
    }
    let mut out = format!("{language}:");
    for (i, c) in kind.descriptor().name().chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// The lines of a source file, split at `\n`, for converting columns to UTF-8 bytes.
#[derive(Debug)]
pub struct LineIndex<'a> {
    text: &'a [u8],
    lines: Vec<&'a [u8]>,
}

impl<'a> LineIndex<'a> {
    /// Indexes the lines of `text`.
    #[must_use]
    pub fn new(text: &'a [u8]) -> Self {
        Self {
            text,
            lines: text.split(|&b| b == b'\n').collect(),
        }
    }

    /// Lines as `files.jsonl` counts them: a final line without `\n` counts, an empty tail
    /// does not.
    #[must_use]
    pub fn count(&self) -> u64 {
        // `lines` has one more piece than the text has newlines.
        let newlines = self.lines.len() as u64 - 1;
        newlines + u64::from(!self.text.is_empty() && !self.text.ends_with(b"\n"))
    }

    /// The UTF-8 byte column of `col`, counted in `unit`, on `line`; `None` when the position
    /// is past the end of the line or inside a character.
    #[must_use]
    pub fn byte_column(&self, line: u32, col: u32, unit: PositionEncoding) -> Option<u32> {
        let text = *self.lines.get(line as usize)?;
        let col = col as usize;
        if unit == PositionEncoding::Utf8 {
            let on_boundary = text.get(col).is_none_or(|&b| (b & 0xC0) != 0x80);
            return if col <= text.len() && on_boundary {
                u32::try_from(col).ok()
            } else {
                None
            };
        }
        let text = std::str::from_utf8(text).ok()?;
        let mut units = 0;
        for (byte, c) in text.char_indices() {
            if units == col {
                return u32::try_from(byte).ok();
            }
            units += if unit == PositionEncoding::Utf16 {
                c.len_utf16()
            } else {
                1
            };
            if units > col {
                return None;
            }
        }
        if units == col {
            u32::try_from(text.len()).ok()
        } else {
            None
        }
    }

    /// Converts a range counted in `unit` to UTF-8 bytes; `None` when either end is not a
    /// position in the text or the range ends before it starts.
    #[must_use]
    pub fn range(&self, range: Range, unit: PositionEncoding) -> Option<Range> {
        let [sl, sc, el, ec] = range.0;
        let range = Range::new(
            sl,
            self.byte_column(sl, sc, unit)?,
            el,
            self.byte_column(el, ec, unit)?,
        );
        (range.start() <= range.end()).then_some(range)
    }
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

/// Typed ranges first, as `scip.proto` requires, then the deprecated integer arrays.
fn convert(
    o: &Occurrence,
    lines: &LineIndex<'_>,
    unit: PositionEncoding,
) -> Option<(Range, Option<Range>)> {
    let range = match &o.typed_range {
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
    let range = lines.range(unsigned(range)?, unit)?;
    let enclosing = match enclosing {
        Some(raw) => Some(lines.range(unsigned(raw)?, unit)?),
        None => None,
    };
    Some((range, enclosing))
}

const fn is_definition(roles: i32) -> bool {
    roles & SymbolRole::Definition as i32 != 0
}

/// The role bits of a reference, sorted by their names.
fn ref_roles(bits: i32) -> Vec<RefRole> {
    const BITS: [(SymbolRole, RefRole); 6] = [
        (SymbolRole::Import, RefRole::Import),
        (SymbolRole::WriteAccess, RefRole::Write),
        (SymbolRole::ReadAccess, RefRole::Read),
        (SymbolRole::Generated, RefRole::Generated),
        (SymbolRole::Test, RefRole::Test),
        (SymbolRole::ForwardDefinition, RefRole::ForwardDefinition),
    ];
    let mut roles: Vec<RefRole> = BITS
        .iter()
        .filter(|(bit, _)| bits & *bit as i32 != 0)
        .map(|&(_, role)| role)
        .collect();
    roles.sort_by_cached_key(crate::write::enum_name);
    roles
}

/// The definition sites of one symbol, by file and name range, with the body if any.
type Observations = BTreeMap<(String, Range), Option<Range>>;

/// Symbols of the index, sorted into rows.
#[derive(Debug)]
struct Graph<'a> {
    parsed: BTreeMap<&'a str, Symbol<'a>>,
    /// Definitions of symbols that are rows.
    defs: BTreeMap<&'a str, Observations>,
    /// Symbols that are entity rows: definitions and their containers.
    rows: BTreeMap<&'a str, Symbol<'a>>,
    externals: BTreeMap<&'a str, Symbol<'a>>,
    keys: BTreeMap<&'a str, String>,
}

impl<'a> Graph<'a> {
    fn new(occurrences: &'a [Occ]) -> Self {
        let mut parsed = BTreeMap::new();
        for occ in occurrences {
            if let Ok(Parsed::Global(symbol)) = identity::parse(&occ.symbol) {
                parsed.entry(occ.symbol.as_str()).or_insert(symbol);
            }
        }
        let mut all_defs: BTreeMap<&str, Observations> = BTreeMap::new();
        for occ in occurrences.iter().filter(|o| is_definition(o.roles)) {
            if parsed.contains_key(occ.symbol.as_str()) {
                let body = all_defs
                    .entry(occ.symbol.as_str())
                    .or_default()
                    .entry((occ.file.clone(), occ.range))
                    .or_default();
                // Repeated observations merge; the larger keeps a body over none.
                *body = (*body).max(occ.enclosing);
            }
        }
        // A parameter is a row only as a container, so its own definition makes none.
        let mut rows = BTreeMap::new();
        for &raw in all_defs.keys() {
            let symbol = &parsed[raw];
            if symbol.last().suffix.is_parameter() {
                continue;
            }
            for container in symbol.container_symbols() {
                rows.entry(container.raw()).or_insert(container);
            }
            rows.insert(raw, symbol.clone());
        }
        let defs: BTreeMap<_, _> = all_defs
            .into_iter()
            .filter(|(raw, _)| rows.contains_key(raw))
            .collect();
        let mut externals = BTreeMap::new();
        for occ in occurrences.iter().filter(|o| !is_definition(o.roles)) {
            if let Some(symbol) = parsed.get(occ.symbol.as_str())
                && !rows.contains_key(occ.symbol.as_str())
                && !symbol.last().suffix.is_parameter()
            {
                externals.insert(symbol.raw(), symbol.clone());
            }
        }
        let mut keyed = rows.clone();
        keyed.extend(externals.iter().map(|(raw, s)| (*raw, s.clone())));
        let keys = identity::assign_keys(&keyed);
        Self {
            parsed,
            defs,
            rows,
            externals,
            keys,
        }
    }

    fn entities(&self, input: &Input, scip_src: &str, out: &mut Snapshot) {
        for (&raw, symbol) in &self.rows {
            let key = &self.keys[raw];
            let parent_symbol = symbol.parent();
            let parent_is_type =
                parent_symbol.is_some_and(|p| self.rows[p].last().suffix == Suffix::Type);
            let row = EntityRow {
                key: key.clone(),
                symbol: Some(raw.to_owned()),
                kind: input.kinds.get(raw).and_then(|k| k.first().cloned()),
                role: Some(identity::role(symbol.last().suffix, parent_is_type)),
                name: symbol.last().name.clone(),
                parent: parent_symbol.map(|p| self.keys[p].clone()),
                r#virtual: false,
                group: false,
                member_of: None,
                def: None,
                src: scip_src.to_owned(),
            };
            let Some(observations) = self.defs.get(raw) else {
                out.entities.push(EntityRow {
                    symbol: input.named.contains(raw).then(|| raw.to_owned()),
                    r#virtual: true,
                    src: CONTAINMENT.to_owned(),
                    ..row
                });
                continue;
            };
            let definition = |key: String,
                              member_of: Option<String>,
                              (file, name): &(String, Range),
                              body: Option<Range>| EntityRow {
                key,
                member_of,
                def: Some(Def {
                    file: file.clone(),
                    name: *name,
                    body,
                    body_src: body.map(|_| scip_src.to_owned()),
                }),
                ..row.clone()
            };
            if let [(site, body)] = observations.iter().collect::<Vec<_>>()[..] {
                out.entities
                    .push(definition(key.clone(), None, site, *body));
                continue;
            }
            let mut gap_keys = vec![key.clone()];
            for (n, (site, body)) in observations.iter().enumerate() {
                let member = format!("{key}~{}", n + 1);
                out.entities
                    .push(definition(member.clone(), Some(key.clone()), site, *body));
                gap_keys.push(member);
            }
            out.entities.push(EntityRow {
                kind: None,
                group: true,
                src: IDENTITY.to_owned(),
                ..row
            });
            let mut gap = GapRow::new(GapKind::SymbolCollision, IDENTITY);
            gap.keys = Some(gap_keys);
            out.gaps.push(gap);
        }
        for (&raw, symbol) in &self.externals {
            out.externals.push(ExternalRow {
                key: self.keys[raw].clone(),
                symbol: raw.to_owned(),
                package: Package {
                    manager: symbol.manager.clone(),
                    name: symbol.package.clone(),
                    version: symbol.version.clone(),
                },
                src: scip_src.to_owned(),
            });
        }
    }

    /// Attributes every reference to the innermost body containing it, using the bodies the
    /// entities hold now.
    fn references(&self, input: &Input, out: &mut Snapshot) {
        let mut bodies: BTreeMap<String, Vec<(Range, String)>> = BTreeMap::new();
        for entity in &out.entities {
            if let Some(Def {
                file,
                body: Some(body),
                ..
            }) = &entity.def
            {
                bodies
                    .entry(file.clone())
                    .or_default()
                    .push((*body, entity.key.clone()));
            }
        }
        let owners: BTreeMap<String, Vec<(Range, String)>> = bodies
            .into_iter()
            .map(|(file, list)| {
                let remaining = remaining_bodies(&file, &list, &mut out.gaps);
                (file, remaining)
            })
            .collect();
        let mut refs: BTreeMap<(String, String, Vec<RefRole>), BTreeSet<Site>> = BTreeMap::new();
        let mut unattributed: BTreeMap<&str, u64> = BTreeMap::new();
        for occ in input.occurrences.iter().filter(|o| !is_definition(o.roles)) {
            let Some(target) = self.target(&occ.symbol) else {
                if self.is_local_like(&occ.symbol) {
                    out.dropped_local_references += 1;
                }
                continue;
            };
            // The remaining containing bodies nest strictly, so the innermost starts last
            // and, among equal starts, ends first.
            let owner = owners.get(&occ.file).and_then(|list| {
                list.iter()
                    .filter(|(body, _)| body.contains(occ.range))
                    .max_by_key(|(body, _)| (body.start(), std::cmp::Reverse(body.end())))
            });
            let Some((_, from)) = owner else {
                *unattributed.entry(occ.file.as_str()).or_default() += 1;
                continue;
            };
            refs.entry((from.clone(), target.clone(), ref_roles(occ.roles)))
                .or_default()
                .insert(Site::new(occ.file.clone(), occ.range));
        }
        for ((from, to, roles), sites) in refs {
            out.relations.push(RelationRow {
                from,
                to,
                rel: Rel::Ref,
                roles: Some(roles),
                mechanism: None,
                resolution: None,
                sites: Some(sites.into_iter().collect()),
                evidence: None,
                src: REFERENCE_IN_BODY.to_owned(),
            });
        }
        for (file, count) in unattributed {
            let mut gap = GapRow::new(GapKind::UnattributedReferences, REFERENCE_IN_BODY);
            gap.file = Some(file.to_owned());
            gap.count = Some(count);
            out.gaps.push(gap);
        }
    }

    /// The key a reference to `symbol` points at, if it is a row or an external.
    fn target(&self, symbol: &str) -> Option<&String> {
        if self.rows.contains_key(symbol) || self.externals.contains_key(symbol) {
            self.keys.get(symbol)
        } else {
            None
        }
    }

    /// Locals, and parameters that are not containers, are not rows; references to them are
    /// counted instead.
    fn is_local_like(&self, symbol: &str) -> bool {
        match identity::parse(symbol) {
            Ok(Parsed::Local) => true,
            Ok(Parsed::Global(_)) => self
                .parsed
                .get(symbol)
                .is_some_and(|s| s.last().suffix.is_parameter()),
            Err(_) => false,
        }
    }
}

/// Bodies of one file that take part in ownership. Two bodies of different entities that
/// overlap without one strictly containing the other, identical extents included, are left
/// out and reported once per pair as `overlapping-bodies`.
fn remaining_bodies(
    file: &str,
    bodies: &[(Range, String)],
    gaps: &mut Vec<GapRow>,
) -> Vec<(Range, String)> {
    let mut excluded = BTreeSet::new();
    for (i, (a, ka)) in bodies.iter().enumerate() {
        for (b, kb) in &bodies[i + 1..] {
            if ka != kb && (a == b || a.overlaps_without_nesting(*b)) {
                excluded.insert(ka.clone());
                excluded.insert(kb.clone());
                let mut keys = vec![ka.clone(), kb.clone()];
                keys.sort();
                let mut gap = GapRow::new(GapKind::OverlappingBodies, REFERENCE_IN_BODY);
                gap.file = Some(file.to_owned());
                gap.keys = Some(keys);
                gaps.push(gap);
            }
        }
    }
    bodies
        .iter()
        .filter(|(_, key)| !excluded.contains(key))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_convert_from_each_unit_to_utf8_bytes() {
        let text = "    [\"🦀\", &empty()]\n".as_bytes();
        let lines = LineIndex::new(text);
        assert_eq!(lines.byte_column(0, 14, PositionEncoding::Utf8), Some(14));
        assert_eq!(lines.byte_column(0, 12, PositionEncoding::Utf16), Some(14));
        assert_eq!(lines.byte_column(0, 11, PositionEncoding::Utf32), Some(14));
        assert_eq!(lines.byte_column(0, 7, PositionEncoding::Utf8), None);
        assert_eq!(lines.byte_column(0, 7, PositionEncoding::Utf16), None);
        assert_eq!(lines.byte_column(0, 99, PositionEncoding::Utf32), None);
        assert_eq!(lines.byte_column(0, 99, PositionEncoding::Utf8), None);
        assert_eq!(lines.byte_column(1, 0, PositionEncoding::Utf8), Some(0));
        assert_eq!(lines.byte_column(2, 0, PositionEncoding::Utf8), None);
        assert_eq!(lines.count(), 1);
        assert_eq!(LineIndex::new(b"a\nb").count(), 2);
        assert_eq!(LineIndex::new(b"").count(), 0);
    }

    #[test]
    fn line_end_is_a_position_in_every_unit() {
        let lines = LineIndex::new("aé\n".as_bytes());
        assert_eq!(lines.byte_column(0, 3, PositionEncoding::Utf8), Some(3));
        assert_eq!(lines.byte_column(0, 2, PositionEncoding::Utf16), Some(3));
        assert_eq!(lines.byte_column(0, 2, PositionEncoding::Utf32), Some(3));
        assert_eq!(lines.byte_column(0, 3, PositionEncoding::Utf16), None);
        assert_eq!(
            lines.range(Range::new(0, 2, 0, 1), PositionEncoding::Utf8),
            None
        );
    }

    #[test]
    fn paths_follow_the_convention() {
        for ok in ["src/a.rs", "a", "a.b/c", "..a"] {
            assert!(is_valid_path(ok), "{ok}");
        }
        for bad in [
            "", "/a", "a//b", "./a", "a/../b", "..", "a\\b", "C:/a", "a/",
        ] {
            assert!(!is_valid_path(bad), "{bad}");
        }
    }

    #[test]
    fn kinds_are_namespaced_kebab_case() {
        assert_eq!(
            kind_name("rust", Ok(Kind::StaticMethod)).as_deref(),
            Some("rust:static-method")
        );
        assert_eq!(
            kind_name("go", Ok(Kind::Function)).as_deref(),
            Some("go:function")
        );
        assert_eq!(kind_name("rust", Ok(Kind::UnspecifiedKind)), None);
        assert_eq!(kind_name("", Ok(Kind::Function)), None);
        assert_eq!(kind_name("go", Err(9999)), None);
    }

    #[test]
    fn roles_are_sorted_by_name() {
        let bits = SymbolRole::WriteAccess as i32
            | SymbolRole::Import as i32
            | SymbolRole::Definition as i32;
        assert_eq!(ref_roles(bits), [RefRole::Import, RefRole::Write]);
        let all = 0x7e;
        assert_eq!(
            ref_roles(all),
            [
                RefRole::ForwardDefinition,
                RefRole::Generated,
                RefRole::Import,
                RefRole::Read,
                RefRole::Test,
                RefRole::Write
            ]
        );
    }
}
