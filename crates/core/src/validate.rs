//! Checks a snapshot on disk against the specification.
//!
//! [`read`] is the snapshot reader: it applies the compatibility rules of `spec/README.md`
//! ("What a reader must do") and returns the rows it understood. [`validate`] reads a snapshot
//! as its own schema version and reports every way it departs from `spec/snapshot.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use gosanja_schema::compat::{self, Acceptance, Refusal, RowVerdict, Version};
use gosanja_schema::position::{Range, Site};
use gosanja_schema::snapshot::{
    Decl, EntityRow, EntityTest, ExternalRow, FORMAT, FileRow, GapKind, GapRow, Layer, LayerState,
    Manifest, RelationRow, SCHEMA_VERSION, SyntaxRow, TargetRow,
};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::graph::LineIndex;
use crate::schemas::{self, Fit, Schema, validator};
use crate::write::{self, Location, SnapshotFiles, SourceReader};

/// One way a snapshot departs from the specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The snapshot file concerned.
    pub file: &'static str,
    /// One-based line in that file, when one row is concerned.
    pub line: Option<usize>,
    /// A short fixed name for the kind of problem.
    pub code: &'static str,
    /// What was found.
    pub detail: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{line}: {}: {}", self.file, self.code, self.detail),
            None => write!(f, "{}: {}: {}", self.file, self.code, self.detail),
        }
    }
}

/// The rows of a snapshot that a reader understood.
#[derive(Debug, Clone, Default)]
pub struct ReadSnapshot {
    /// `manifest.json`, when it parsed.
    pub manifest: Option<Manifest>,
    /// Rows of `files.jsonl`, with their one-based line numbers.
    pub files: Vec<(usize, FileRow)>,
    /// Rows of `entities.jsonl`.
    pub entities: Vec<(usize, EntityRow)>,
    /// Rows of `externals.jsonl`.
    pub externals: Vec<(usize, ExternalRow)>,
    /// Rows of `relations.jsonl`.
    pub relations: Vec<(usize, RelationRow)>,
    /// Rows of `syntax.jsonl`.
    pub syntax: Vec<(usize, SyntaxRow)>,
    /// Rows of `targets.jsonl`.
    pub targets: Vec<(usize, TargetRow)>,
    /// Rows of `gaps.jsonl`.
    pub gaps: Vec<(usize, GapRow)>,
    /// Rows skipped for a value from a newer minor, or for depending on such a row.
    pub skipped: u64,
    /// Problems found while reading.
    pub diagnostics: Vec<Diagnostic>,
    /// Keys of the rows skipped so far.
    skipped_keys: BTreeSet<String>,
}

/// The first way `value` fails `schema`, if it does.
fn first_error(schema: Schema, value: &serde_json::Value) -> Option<String> {
    validator(schema)
        .iter_errors(value)
        .next()
        .map(|e| format!("{e} at {}", e.instance_path()))
}

/// Refuses a snapshot whose schema version this crate may not read, from its manifest alone.
/// A manifest without a readable version passes here and is diagnosed by [`validate`].
///
/// # Errors
///
/// Returns the [`Refusal`] when the manifest states another schema version.
pub fn check_version(files: &SnapshotFiles) -> Result<(), Refusal> {
    let writer = files
        .manifest
        .as_deref()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        .and_then(|value| value["schema"].as_str()?.parse::<Version>().ok());
    match writer {
        Some(writer) => compat::accept(SCHEMA_VERSION, writer).map(|_| ()),
        None => Ok(()),
    }
}

/// Reads a snapshot as a reader implementing schema version `reader`.
///
/// # Errors
///
/// Returns the [`Refusal`] when the snapshot's schema version may not be read at all; nothing
/// else is read then.
pub fn read(files: &SnapshotFiles, reader: Version) -> Result<ReadSnapshot, Refusal> {
    let (rows, acceptance) = read_manifest(files, reader)?;
    Ok(read_tables(rows, files, acceptance))
}

pub(crate) fn read_manifest(
    files: &SnapshotFiles,
    reader: Version,
) -> Result<(ReadSnapshot, Acceptance), Refusal> {
    let mut out = ReadSnapshot::default();
    let mut acceptance = Acceptance::Full;
    match files.manifest.as_deref() {
        None => out.diag("manifest.json", None, "missing-file", "the file is absent"),
        Some(bytes) => match serde_json::from_slice::<serde_json::Value>(bytes) {
            Err(e) => out.diag("manifest.json", None, "malformed-row", e.to_string()),
            Ok(value) => {
                match value["schema"].as_str().map(str::parse::<Version>) {
                    Some(Ok(writer)) => acceptance = compat::accept(reader, writer)?,
                    _ => out.diag("manifest.json", None, "malformed-row", "no valid `schema`"),
                }
                if acceptance == Acceptance::Full
                    && let Some(error) = first_error(Schema::SnapshotManifest, &value)
                {
                    out.diag("manifest.json", None, "schema", error);
                }
                match serde_json::from_value::<Manifest>(value) {
                    Ok(manifest) => out.manifest = Some(manifest),
                    Err(e) => out.diag("manifest.json", None, "malformed-row", e.to_string()),
                }
            }
        },
    }
    Ok((out, acceptance))
}

pub(crate) fn read_tables(
    mut out: ReadSnapshot,
    files: &SnapshotFiles,
    acceptance: Acceptance,
) -> ReadSnapshot {
    for (name, error) in &files.unreadable {
        out.diag(name, None, "unreadable", error.clone());
    }
    out.files = out.table(
        "files.jsonl",
        Schema::File,
        false,
        files.files.as_deref(),
        acceptance,
    );
    out.entities = out.table(
        "entities.jsonl",
        Schema::Entity,
        true,
        files.entities.as_deref(),
        acceptance,
    );
    out.externals = out.table(
        "externals.jsonl",
        Schema::External,
        true,
        files.externals.as_deref(),
        acceptance,
    );
    out.relations = out.table(
        "relations.jsonl",
        Schema::Relation,
        false,
        files.relations.as_deref(),
        acceptance,
    );
    out.syntax = out.table(
        "syntax.jsonl",
        Schema::Syntax,
        false,
        files.syntax.as_deref(),
        acceptance,
    );
    out.targets = out.table(
        "targets.jsonl",
        Schema::Target,
        false,
        files.targets.as_deref(),
        acceptance,
    );
    out.gaps = out.table(
        "gaps.jsonl",
        Schema::Gap,
        false,
        files.gaps.as_deref(),
        acceptance,
    );
    out.skip_dependents();
    out
}

impl ReadSnapshot {
    fn diag(
        &mut self,
        file: &'static str,
        line: Option<usize>,
        code: &'static str,
        detail: impl Into<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            file,
            line,
            code,
            detail: detail.into(),
        });
    }

    /// Reads one table. `names_rows` says whether a row's `key` is the key of that row, as in
    /// `entities.jsonl` and `externals.jsonl`, rather than a reference to another row.
    fn table<T: DeserializeOwned>(
        &mut self,
        name: &'static str,
        schema: Schema,
        names_rows: bool,
        bytes: Option<&[u8]>,
        acceptance: Acceptance,
    ) -> Vec<(usize, T)> {
        let Some(bytes) = bytes else {
            if !self.diagnostics.iter().any(|d| d.file == name) {
                self.diag(name, None, "missing-file", "the file is absent");
            }
            return Vec::new();
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.diag(name, None, "not-json-lines", "the file is not UTF-8");
            return Vec::new();
        };
        if !text.is_empty() && !text.ends_with('\n') {
            self.diag(name, None, "not-json-lines", "the last line has no newline");
        }
        let mut rows = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let at = Some(i + 1);
            if line.is_empty() {
                self.diag(name, at, "not-json-lines", "blank line");
                continue;
            }
            let value = serde_json::from_str::<serde_json::Value>(line);
            let parsed = value
                .as_ref()
                .map_err(ToString::to_string)
                .and_then(|v| T::deserialize(v).map_err(|e| e.to_string()));
            // The schema alone decides whether a row fails only because of values from a newer
            // minor (spec/README.md, rule 5); every closed field counts.
            let fit = value.as_ref().ok().map(|v| schemas::fit(schema, v));
            let unknown = fit == Some(Fit::UnknownValueOnly);
            let schema_error = match fit {
                Some(Fit::Invalid(error)) => Some(error),
                _ => None,
            };
            let well_formed = parsed.is_ok() && schema_error.is_none();
            match compat::classify_row(acceptance, well_formed, unknown) {
                RowVerdict::Keep => rows.extend(parsed.ok().map(|row| (i + 1, row))),
                RowVerdict::Skip => {
                    self.skipped += 1;
                    // Only entity and external keys are targets others depend on; a syntax
                    // row's `key` names the entity it describes, which stays.
                    if names_rows
                        && let Ok(serde_json::Value::Object(row)) = &value
                        && let Some(serde_json::Value::String(key)) = row.get("key")
                    {
                        self.skipped_keys.insert(key.clone());
                    }
                }
                RowVerdict::Malformed => {
                    let (code, detail) = match (parsed, schema_error) {
                        (Err(e), _) => ("malformed-row", e),
                        (Ok(_), Some(e)) => ("schema", e),
                        (Ok(_), None) => (
                            "unknown-value",
                            "a value this schema version does not define".to_owned(),
                        ),
                    };
                    self.diag(name, at, code, detail);
                }
            }
        }
        rows
    }

    /// Rule 4: a row that depends on a skipped row is skipped too, so no key dangles. Only
    /// keys that were actually skipped count; any other dangling key is left for the
    /// validator to report.
    fn skip_dependents(&mut self) {
        // A pass that skips nothing ends the loop and every other pass skips at least one
        // entity, so one pass more than there are entities always reaches that end.
        for _ in 0..=self.entities.len() {
            let skipped = &mut self.skipped_keys;
            let before = self.entities.len();
            self.entities.retain(|(_, e)| {
                let depends = [&e.parent, &e.member_of]
                    .into_iter()
                    .flatten()
                    .any(|k| skipped.contains(k));
                if depends {
                    skipped.insert(e.key.clone());
                }
                !depends
            });
            self.skipped += (before - self.entities.len()) as u64;
            if before == self.entities.len() {
                break;
            }
        }
        let skipped = &self.skipped_keys;
        let before = self.relations.len() + self.syntax.len() + self.gaps.len();
        self.relations
            .retain(|(_, r)| !skipped.contains(&r.from) && !skipped.contains(&r.to));
        self.syntax.retain(|(_, row)| !skipped.contains(&row.key));
        self.gaps
            .retain(|(_, g)| !g.keys.iter().flatten().any(|k| skipped.contains(k)));
        let after = self.relations.len() + self.syntax.len() + self.gaps.len();
        self.skipped += (before - after) as u64;
    }
}

#[derive(Debug)]
pub(crate) struct SymbolParts {
    pub scheme: String,
    pub package: (String, String),
    pub version: String,
    pub base_key: String,
}

pub(crate) type SymbolIndex = BTreeMap<String, SymbolParts>;

pub(crate) fn symbol_parts(snap: &ReadSnapshot, diagnostics: &mut Vec<Diagnostic>) -> SymbolIndex {
    use crate::identity::{Parsed, parse};
    let symbols = snap
        .entities
        .iter()
        .filter_map(|(line, e)| Some(("entities.jsonl", *line, e.symbol.as_deref()?)))
        .chain(
            snap.externals
                .iter()
                .map(|(line, e)| ("externals.jsonl", *line, e.symbol.as_str())),
        );
    let mut index = SymbolIndex::new();
    for (file, line, symbol) in symbols {
        if index.contains_key(symbol) {
            continue;
        }
        if let Ok(Parsed::Global(parsed)) = parse(symbol) {
            let base_key = parsed.base_key();
            index.insert(
                symbol.to_owned(),
                SymbolParts {
                    base_key,
                    scheme: parsed.scheme,
                    package: (parsed.manager, parsed.package),
                    version: parsed.version,
                },
            );
        } else {
            diagnostics.push(Diagnostic {
                file,
                line: Some(line),
                code: "invalid-symbol",
                detail: format!("expected a global SCIP symbol: {symbol}"),
            });
        }
    }
    // Per row, not per symbol: two rows may carry one symbol, and each states its own package.
    for (line, external) in &snap.externals {
        let Some(parts) = index.get(&external.symbol) else {
            continue;
        };
        let package = &external.package;
        let (manager, name) = &parts.package;
        if (&package.manager, &package.name, &package.version) != (manager, name, &parts.version) {
            diagnostics.push(Diagnostic {
                file: "externals.jsonl",
                line: Some(*line),
                code: "package-mismatch",
                detail: format!(
                    "`package` {}:{}:{} is not the symbol's {manager}:{name}:{}",
                    package.manager, package.name, package.version, parts.version
                ),
            });
        }
    }
    index
}

/// Validates a snapshot written with this crate's schema version. With `sources`, the files it
/// records are compared with the source tree as well.
///
/// # Errors
///
/// Returns the [`Refusal`] when the snapshot has another schema version.
pub fn validate(
    files: &SnapshotFiles,
    sources: Option<&dyn SourceReader>,
) -> Result<Vec<Diagnostic>, Refusal> {
    let snapshot = read(files, SCHEMA_VERSION)?;
    Ok(checked(snapshot, files, sources).0.diagnostics)
}

pub(crate) fn checked(
    mut snap: ReadSnapshot,
    files: &SnapshotFiles,
    sources: Option<&dyn SourceReader>,
) -> (ReadSnapshot, SymbolIndex) {
    let mut v = Checks {
        diagnostics: std::mem::take(&mut snap.diagnostics),
    };
    v.manifest(&snap);
    v.order(files, &snap);
    let lines = v.files(&snap, sources);
    v.entities(&snap, &lines);
    let symbols = symbol_parts(&snap, &mut v.diagnostics);
    v.identities(&snap);
    v.relations(&snap, &lines);
    v.gaps(&snap, &lines);
    v.build(&snap, files);
    let entity_keys: BTreeSet<&str> = snap.entities.iter().map(|(_, e)| e.key.as_str()).collect();
    let declared: BTreeSet<&str> = snap
        .entities
        .iter()
        .filter(|(_, e)| e.decls.is_some())
        .map(|(_, e)| e.key.as_str())
        .collect();
    for (line, row) in &snap.syntax {
        if !entity_keys.contains(row.key.as_str()) {
            v.diag(
                "syntax.jsonl",
                *line,
                "dangling-key",
                format!("`key` {} names no entity", row.key),
            );
        } else if declared.contains(row.key.as_str()) {
            v.diag(
                "syntax.jsonl",
                *line,
                "syntax-on-decls",
                format!(
                    "`key` {} names an entity declared at several sites",
                    row.key
                ),
            );
        }
    }
    snap.diagnostics = v.diagnostics;
    (snap, symbols)
}

/// Whether a key ends in a collision member's `~n` suffix.
fn identity_suffix(key: &str) -> bool {
    key.rsplit_once('~')
        .is_some_and(|(_, n)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// What a table's order requires besides sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    /// Every row has its own key.
    UniqueKey,
    /// No row appears twice, as no relation does.
    DistinctRows,
    /// Equal rows may repeat, as gaps may.
    Repeatable,
}

struct Checks {
    diagnostics: Vec<Diagnostic>,
}

/// Per file: number of lines, and the text when the source tree was given.
type FileLines = BTreeMap<String, (u64, Option<Vec<u8>>)>;

impl Checks {
    fn diag(&mut self, file: &'static str, line: usize, code: &'static str, detail: String) {
        self.diagnostics.push(Diagnostic {
            file,
            line: Some(line),
            code,
            detail,
        });
    }

    fn manifest(&mut self, snap: &ReadSnapshot) {
        let Some(manifest) = &snap.manifest else {
            return;
        };
        let mut wrong = |code, detail: String| {
            self.diagnostics.push(Diagnostic {
                file: "manifest.json",
                line: None,
                code,
                detail,
            });
        };
        if manifest.format != FORMAT {
            wrong("wrong-format", format!("`format` is {:?}", manifest.format));
        }
        let c = manifest.counts;
        let counted = [
            ("files", c.files, snap.files.len()),
            ("entities", c.entities, snap.entities.len()),
            ("externals", c.externals, snap.externals.len()),
            ("relations", c.relations, snap.relations.len()),
            ("syntax", c.syntax, snap.syntax.len()),
            ("gaps", c.gaps, snap.gaps.len()),
            ("targets", c.targets, snap.targets.len()),
        ];
        for (name, stated, actual) in counted {
            if stated != actual as u64 {
                wrong(
                    "count-mismatch",
                    format!("`counts.{name}` is {stated}, the file has {actual} rows"),
                );
            }
        }
    }

    /// Rows are in the documented order and keys are unique.
    fn order(&mut self, files: &SnapshotFiles, snap: &ReadSnapshot) {
        let lines = |bytes: Option<&Vec<u8>>| -> Vec<String> {
            bytes
                .and_then(|b| std::str::from_utf8(b).ok())
                .map(|t| t.lines().map(str::to_owned).collect())
                .unwrap_or_default()
        };
        let file_text = lines(files.files.as_ref());
        self.sorted(
            "files.jsonl",
            &snap.files,
            &file_text,
            |r| vec![Some(r.path.clone())],
            Order::UniqueKey,
        );
        let text = lines(files.entities.as_ref());
        self.sorted(
            "entities.jsonl",
            &snap.entities,
            &text,
            |r| vec![Some(r.key.clone())],
            Order::UniqueKey,
        );
        let text = lines(files.externals.as_ref());
        self.sorted(
            "externals.jsonl",
            &snap.externals,
            &text,
            |r| vec![Some(r.key.clone())],
            Order::UniqueKey,
        );
        let text = lines(files.relations.as_ref());
        self.sorted(
            "relations.jsonl",
            &snap.relations,
            &text,
            write::relation_order,
            Order::DistinctRows,
        );
        let text = lines(files.syntax.as_ref());
        self.sorted(
            "syntax.jsonl",
            &snap.syntax,
            &text,
            |r| vec![Some(r.key.clone())],
            Order::UniqueKey,
        );
        let text = lines(files.targets.as_ref());
        self.sorted(
            "targets.jsonl",
            &snap.targets,
            &text,
            |r| vec![Some(r.key.clone())],
            Order::UniqueKey,
        );
        let text = lines(files.gaps.as_ref());
        self.sorted(
            "gaps.jsonl",
            &snap.gaps,
            &text,
            write::gap_order,
            Order::Repeatable,
        );
        let entities: BTreeSet<&str> = snap.entities.iter().map(|(_, e)| e.key.as_str()).collect();
        for (line, external) in &snap.externals {
            if entities.contains(external.key.as_str()) {
                self.diag(
                    "externals.jsonl",
                    *line,
                    "duplicate-key",
                    format!("{} is also an entity", external.key),
                );
            }
        }
    }

    fn sorted<T, K: Ord>(
        &mut self,
        name: &'static str,
        rows: &[(usize, T)],
        text: &[String],
        key: impl Fn(&T) -> K,
        order: Order,
    ) {
        let keyed: Vec<(K, &str, usize)> = rows
            .iter()
            .map(|(line, row)| {
                (
                    key(row),
                    text.get(line - 1).map_or("", String::as_str),
                    *line,
                )
            })
            .collect();
        for pair in keyed.windows(2) {
            let ((ka, la, _), (kb, lb, line)) = (&pair[0], &pair[1]);
            if order == Order::UniqueKey && ka == kb {
                self.diag(
                    name,
                    *line,
                    "duplicate-key",
                    "the key of the previous row again".to_owned(),
                );
            } else if order == Order::DistinctRows && (ka, la) == (kb, lb) {
                self.diag(
                    name,
                    *line,
                    "duplicate-row",
                    "the previous row again".to_owned(),
                );
            } else if (ka, la) > (kb, lb) {
                self.diag(
                    name,
                    *line,
                    "unsorted",
                    "the row sorts before the previous one".to_owned(),
                );
            }
        }
    }

    fn files(&mut self, snap: &ReadSnapshot, sources: Option<&dyn SourceReader>) -> FileLines {
        let mut out = FileLines::new();
        for (line, row) in &snap.files {
            let text = sources.and_then(|s| self.source(*line, row, s));
            out.insert(row.path.clone(), (row.lines, text));
        }
        out
    }

    fn source(
        &mut self,
        line: usize,
        row: &FileRow,
        sources: &dyn SourceReader,
    ) -> Option<Vec<u8>> {
        let text = match sources.locate(&row.path) {
            Location::Outside => {
                self.diag(
                    "files.jsonl",
                    line,
                    "source-mismatch",
                    format!("{} leads outside the root", row.path),
                );
                return None;
            }
            Location::Inside => match sources.read(&row.path) {
                Ok(text) => text,
                Err(e) => {
                    self.diag(
                        "files.jsonl",
                        line,
                        "source-mismatch",
                        format!("{}: {e}", row.path),
                    );
                    return None;
                }
            },
        };
        let digest = crate::graph::hex(&Sha256::digest(&text));
        if digest != row.sha256.to_ascii_lowercase()
            || text.len() as u64 != row.bytes
            || LineIndex::new(&text).count() != row.lines
        {
            self.diag(
                "files.jsonl",
                line,
                "source-mismatch",
                format!("{} differs from the source tree", row.path),
            );
        }
        Some(text)
    }

    /// Whether `range` lies in `file`: within its line count, and with the source, on
    /// character boundaries within each line.
    fn position(
        &mut self,
        name: &'static str,
        line: usize,
        lines: &FileLines,
        file: &str,
        range: Range,
    ) {
        let Some((count, text)) = lines.get(file) else {
            self.diag(
                name,
                line,
                "unknown-file",
                format!("{file} is not in files.jsonl"),
            );
            return;
        };
        let ends_ok =
            |(l, c): (u32, u32)| u64::from(l) < *count || (u64::from(l) == *count && c == 0);
        let mut ok = ends_ok(range.start()) && ends_ok(range.end()) && range.start() <= range.end();
        if let (true, Some(text)) = (ok, text) {
            let index = LineIndex::new(text);
            ok = index
                .range(range, gosanja_schema::position::PositionEncoding::Utf8)
                .is_some();
        }
        if !ok {
            self.diag(
                name,
                line,
                "position-outside-file",
                format!("{:?} is not a range of {file}", range.0),
            );
        }
    }

    fn entities(&mut self, snap: &ReadSnapshot, lines: &FileLines) {
        let groups: BTreeSet<&str> = snap
            .entities
            .iter()
            .filter(|(_, e)| e.group)
            .map(|(_, e)| e.key.as_str())
            .collect();
        let keys: BTreeSet<&str> = snap.entities.iter().map(|(_, e)| e.key.as_str()).collect();
        for (line, e) in &snap.entities {
            let line = *line;
            if let Some(parent) = &e.parent
                && !keys.contains(parent.as_str())
            {
                self.diag(
                    "entities.jsonl",
                    line,
                    "dangling-key",
                    format!("`parent` {parent} names no entity"),
                );
            }
            if let Some(group) = &e.member_of
                && !groups.contains(group.as_str())
            {
                self.diag(
                    "entities.jsonl",
                    line,
                    "member-of-no-group",
                    format!("`member_of` {group} names no group"),
                );
            }
            if let Some(def) = &e.def {
                self.position("entities.jsonl", line, lines, &def.file, def.name);
                if let Some(body) = def.body {
                    self.position("entities.jsonl", line, lines, &def.file, body);
                }
            }
            if let Some(decls) = &e.decls {
                self.decls(line, &e.key, decls, lines);
            }
        }
    }

    /// An entity declared at several sites: two or more sites, sorted by file and name range,
    /// each once, under the symbol's own key, each a range of its file. The schema already
    /// refuses `decls` beside `def`, `group`, `member_of` and `virtual`.
    fn decls(&mut self, line: usize, key: &str, decls: &[Decl], lines: &FileLines) {
        if decls.len() < 2 {
            self.diag(
                "entities.jsonl",
                line,
                "decls-too-few",
                format!("`decls` of {key} has {} site, not two or more", decls.len()),
            );
        }
        for pair in decls.windows(2) {
            if pair[0] == pair[1] {
                self.diag(
                    "entities.jsonl",
                    line,
                    "decls-repeated",
                    format!(
                        "`decls` of {key} lists {} {:?} twice",
                        pair[0].file, pair[0].name.0
                    ),
                );
            } else if pair[0] > pair[1] {
                self.diag(
                    "entities.jsonl",
                    line,
                    "decls-unsorted",
                    format!("`decls` of {key} is not sorted by file and name"),
                );
            }
        }
        if identity_suffix(key) {
            self.diag(
                "entities.jsonl",
                line,
                "decls-suffixed-key",
                format!("{key} has `decls` and a `~n` suffix"),
            );
        }
        for decl in decls {
            self.position("entities.jsonl", line, lines, &decl.file, decl.name);
        }
    }

    fn identities(&mut self, snap: &ReadSnapshot) {
        let entities: BTreeMap<_, _> = snap
            .entities
            .iter()
            .map(|(line, e)| (e.key.as_str(), (*line, e)))
            .collect();
        let mut finished = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        for &root in entities.keys() {
            let mut stack = vec![(root, false)];
            while let Some((key, leaving)) = stack.pop() {
                if leaving {
                    visiting.remove(key);
                    finished.insert(key);
                } else if !finished.contains(key)
                    && let Some(&(line, e)) = entities.get(key)
                {
                    if visiting.insert(key) {
                        stack.push((key, true));
                        stack.extend(
                            e.parent
                                .iter()
                                .chain(&e.member_of)
                                .map(|next| (next.as_str(), false)),
                        );
                    } else {
                        self.diag(
                            "entities.jsonl",
                            line,
                            "parent-cycle",
                            format!("containment returns to {key}"),
                        );
                    }
                }
            }
        }
    }

    fn relations(&mut self, snap: &ReadSnapshot, lines: &FileLines) {
        let entities: BTreeSet<&str> = snap.entities.iter().map(|(_, e)| e.key.as_str()).collect();
        let externals: BTreeSet<&str> =
            snap.externals.iter().map(|(_, e)| e.key.as_str()).collect();
        for (line, r) in &snap.relations {
            let line = *line;
            if !entities.contains(r.from.as_str()) {
                self.diag(
                    "relations.jsonl",
                    line,
                    "dangling-key",
                    format!("`from` {} names no entity", r.from),
                );
            }
            if !entities.contains(r.to.as_str()) && !externals.contains(r.to.as_str()) {
                self.diag(
                    "relations.jsonl",
                    line,
                    "dangling-key",
                    format!("`to` {} names no entity or external", r.to),
                );
            }
            for Site(file, sl, sc, el, ec) in r.sites.iter().flatten() {
                self.position(
                    "relations.jsonl",
                    line,
                    lines,
                    file,
                    Range::new(*sl, *sc, *el, *ec),
                );
            }
        }
    }

    fn gaps(&mut self, snap: &ReadSnapshot, lines: &FileLines) {
        let keys: BTreeSet<&str> = snap
            .entities
            .iter()
            .map(|(_, e)| e.key.as_str())
            .chain(snap.externals.iter().map(|(_, e)| e.key.as_str()))
            .collect();
        // One `skipped-file` row per distinct file and code of a layer; its copies need not be
        // neighbours, since unknown fields take part in the byte order.
        let mut skipped = BTreeMap::new();
        for (line, g) in &snap.gaps {
            if g.gap == GapKind::SkippedFile {
                let first = *skipped.entry((g.layer, &g.file, &g.code)).or_insert(*line);
                if first != *line {
                    self.diag(
                        "gaps.jsonl",
                        *line,
                        "duplicate-row",
                        format!("the skipped file of line {first} again"),
                    );
                }
            }
            for key in g.keys.iter().flatten() {
                if !keys.contains(key.as_str()) {
                    self.diag(
                        "gaps.jsonl",
                        *line,
                        "dangling-key",
                        format!("`keys` names {key}, which is no row"),
                    );
                }
            }
            if let (Some(file), Some(range)) = (&g.file, g.range) {
                self.position("gaps.jsonl", *line, lines, file, range);
            }
        }
    }

    /// The facts of the build layer, per scope, as `spec/snapshot.md` ("Build facts in a valid
    /// snapshot") requires: every state a writer cannot emit is rejected.
    fn build(&mut self, snap: &ReadSnapshot, files: &SnapshotFiles) {
        let Some(manifest) = &snap.manifest else {
            return;
        };
        let view = BuildView {
            scopes: manifest
                .scopes
                .iter()
                .map(|s| (s.path.as_str(), (s.layers.build, s.adapter.is_some())))
                .collect(),
            targets: snap
                .targets
                .iter()
                .map(|(line, t)| (t.key.as_str(), (*line, t)))
                .collect(),
        };
        self.build_targets(snap, &view);
        self.build_files(snap, &view);
        self.build_gaps(snap, &view);
        self.entity_marks(snap);
        self.test_in_syntax(files);
    }

    /// Targets: scope, key grammar, layer state, entity and root link.
    fn build_targets(&mut self, snap: &ReadSnapshot, view: &BuildView<'_>) {
        let entities: Vec<EntityRow> = snap.entities.iter().map(|(_, e)| e.clone()).collect();
        let by_key: BTreeMap<&str, &EntityRow> =
            entities.iter().map(|e| (e.key.as_str(), e)).collect();
        let mut root_gaps: BTreeMap<&str, Vec<(usize, &GapRow)>> = BTreeMap::new();
        for (line, g) in &snap.gaps {
            if g.gap == GapKind::UnresolvedRoot {
                match g.target.as_deref() {
                    Some(key) if view.targets.contains_key(key) => {
                        root_gaps.entry(key).or_default().push((*line, g));
                    }
                    Some(key) => self.diag(
                        "gaps.jsonl",
                        *line,
                        "dangling-target",
                        format!("`target` {key} names no row of view.targets.jsonl"),
                    ),
                    None => {}
                }
            }
        }
        for (line, t) in &snap.targets {
            let line = *line;
            if !crate::build::is_target_key(&t.key) {
                self.diag(
                    "view.targets.jsonl",
                    line,
                    "target-key-grammar",
                    format!("{:?} is not a target key", t.key),
                );
            }
            match view.scopes.get(t.scope.as_str()) {
                None => self.diag(
                    "view.targets.jsonl",
                    line,
                    "unknown-scope",
                    format!("`scope` {} names no scope of the manifest", t.scope),
                ),
                Some((state, _)) if *state != LayerState::Present => self.diag(
                    "view.targets.jsonl",
                    line,
                    "build-facts-without-build-layer",
                    format!(
                        "a target of scope {} whose build layer is not present",
                        t.scope
                    ),
                ),
                Some(_) => {}
            }
            let gaps = root_gaps.get(t.key.as_str()).map_or(&[][..], Vec::as_slice);
            self.root_link(line, t, gaps, &entities, &by_key);
        }
    }

    /// File rows: known targets, sorted, layer state, membership against the skips.
    fn build_files(&mut self, snap: &ReadSnapshot, view: &BuildView<'_>) {
        let mut skips: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for (line, g) in &snap.gaps {
            if g.gap == GapKind::SkippedFile
                && g.layer == Some(Layer::Build)
                && let (Some(file), Some(code)) = (&g.file, &g.code)
            {
                skips.insert((file.as_str(), code.as_str()), *line);
            }
        }
        let mut with_targets = BTreeSet::new();
        for (line, f) in &snap.files {
            let line = *line;
            let has_facts = f.targets.is_some() || f.test.is_some() || f.constraint.is_some();
            if has_facts && view.state(view.scope_of(&f.path)) != Some(LayerState::Present) {
                self.diag(
                    "files.jsonl",
                    line,
                    "build-facts-without-build-layer",
                    format!(
                        "{} carries build facts in a scope whose build layer is not present",
                        f.path
                    ),
                );
            }
            if let Some(keys) = &f.targets {
                with_targets.insert(f.path.as_str());
                if !keys.windows(2).all(|p| p[0] < p[1]) {
                    self.diag(
                        "files.jsonl",
                        line,
                        "targets-unsorted",
                        format!("`targets` of {} is not sorted or repeats a key", f.path),
                    );
                }
                for key in keys {
                    if !view.targets.contains_key(key.as_str()) {
                        self.diag(
                            "files.jsonl",
                            line,
                            "dangling-target",
                            format!("`targets` names {key}, which is no row of view.targets.jsonl"),
                        );
                    }
                }
                if skips.contains_key(&(f.path.as_str(), "target-unknown")) {
                    self.diag(
                        "files.jsonl",
                        line,
                        "targets-beside-target-unknown",
                        format!("{} has `targets` and a `target-unknown` skip", f.path),
                    );
                }
            } else if (f.test.is_some() || f.constraint.is_some())
                && !skips.contains_key(&(f.path.as_str(), "target-unknown"))
            {
                self.diag(
                    "files.jsonl",
                    line,
                    "targetless-without-target-unknown",
                    format!(
                        "{} has build facts without `targets` and no `target-unknown` skip",
                        f.path
                    ),
                );
            }
        }
        for (&(file, code), &line) in &skips {
            if code == "targets-incomplete" && !with_targets.contains(file) {
                self.diag(
                    "gaps.jsonl",
                    line,
                    "targets-incomplete-without-targets",
                    format!("{file} has a `targets-incomplete` skip and no `targets`"),
                );
            }
        }
    }

    /// Gaps of layer `build`, per state of their scope.
    fn build_gaps(&mut self, snap: &ReadSnapshot, view: &BuildView<'_>) {
        let mut outcome: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for (line, g) in &snap.gaps {
            if g.layer != Some(Layer::Build) {
                continue;
            }
            let scope = match g.gap {
                GapKind::MissingLayer | GapKind::FailedLayer => g.scope.as_deref(),
                GapKind::UnresolvedRoot => g
                    .target
                    .as_deref()
                    .and_then(|k| view.targets.get(k))
                    .map(|(_, t)| t.scope.as_str()),
                _ => g.file.as_deref().and_then(|f| view.scope_of(f)),
            };
            let Some(scope) = scope.filter(|s| view.scopes.contains_key(*s)) else {
                self.diag(
                    "gaps.jsonl",
                    *line,
                    "unknown-scope",
                    "a gap of layer `build` belongs to no scope of the manifest".to_owned(),
                );
                continue;
            };
            let entry = outcome.entry(scope).or_default();
            match g.gap {
                GapKind::MissingLayer => entry.0 += 1,
                GapKind::FailedLayer => entry.1 += 1,
                _ => {}
            }
            let (state, _) = view.scopes[scope];
            let allowed = match g.gap {
                GapKind::MissingLayer => state == LayerState::Absent,
                GapKind::FailedLayer => state == LayerState::Failed,
                _ => state == LayerState::Present,
            };
            if !allowed {
                self.diag(
                    "gaps.jsonl",
                    *line,
                    "build-gap-outside-its-state",
                    format!(
                        "a `{}` gap of layer `build` in a scope whose build layer is {}",
                        write::enum_name(&g.gap),
                        write::enum_name(&state)
                    ),
                );
            }
        }
        for (&scope, &(state, adapter)) in &view.scopes {
            let (missing, failed) = outcome.get(scope).copied().unwrap_or_default();
            let wrong = match state {
                LayerState::Absent if adapter => missing != 1,
                LayerState::Absent => missing != 0,
                LayerState::Failed => failed != 1,
                _ => false,
            };
            if wrong {
                self.diagnostics.push(Diagnostic {
                    file: "gaps.jsonl",
                    line: None,
                    code: if state == LayerState::Absent {
                        "missing-layer-gap-mismatch"
                    } else {
                        "build-gap-outside-its-state"
                    },
                    detail: format!(
                        "scope {scope}: build layer {} with {missing} `missing-layer` and {failed} `failed-layer` gaps",
                        write::enum_name(&state)
                    ),
                });
            }
        }
    }

    /// A target's `entity` against its `root`, and the `unresolved-root` gaps naming it.
    fn root_link(
        &mut self,
        line: usize,
        t: &TargetRow,
        gaps: &[(usize, &GapRow)],
        entities: &[EntityRow],
        by_key: &BTreeMap<&str, &EntityRow>,
    ) {
        let mut wrong = |code: &'static str, detail: String| {
            self.diagnostics.push(Diagnostic {
                file: "targets.jsonl",
                line: Some(line),
                code,
                detail,
            });
        };
        let Some(root) = &t.root else {
            if t.entity.is_some() || !gaps.is_empty() {
                wrong(
                    "rootless-target-linked",
                    format!("{} has no `root` and an `entity` or a root gap", t.key),
                );
            }
            return;
        };
        let resolved = crate::build::resolve(entities, &root.symbol, root.anchor.as_ref());
        if let Some(entity) = &t.entity {
            if !by_key.contains_key(entity.as_str()) {
                wrong("dangling-key", format!("`entity` {entity} names no entity"));
            } else if resolved.as_deref() != Some(entity.as_str()) {
                wrong(
                    "target-entity-mismatch",
                    format!(
                        "`entity` {entity} is not where the root of {} resolves",
                        t.key
                    ),
                );
            }
            if !gaps.is_empty() {
                wrong(
                    "link-beside-root-gap",
                    format!("{} has an `entity` and an `unresolved-root` gap", t.key),
                );
            }
        } else {
            if resolved.is_some() && root.anchor.is_some() {
                wrong(
                    "root-gap-for-resolving-anchor",
                    format!(
                        "the root anchor of {} resolves, so it needs its `entity`",
                        t.key
                    ),
                );
            }
            match gaps {
                [] => wrong(
                    "unresolved-root-without-gap",
                    format!(
                        "{} has a `root`, no `entity` and no `unresolved-root` gap",
                        t.key
                    ),
                ),
                [(_, gap)] => {
                    if gap.anchor != root.anchor {
                        wrong(
                            "root-gap-anchor-mismatch",
                            format!("the gap of {} has another anchor than its root", t.key),
                        );
                    }
                }
                _ => wrong(
                    "duplicate-root-gap",
                    format!("{} has several `unresolved-root` gaps", t.key),
                ),
            }
        }
    }

    /// Each entity `test` whose source the snapshot determines.
    fn entity_marks(&mut self, snap: &ReadSnapshot) {
        let marks: BTreeMap<&str, Option<EntityTest>> = snap
            .files
            .iter()
            .map(|(_, f)| (f.path.as_str(), f.test.map(EntityTest::from)))
            .collect();
        let mark = |file: &str| marks.get(file).copied().flatten();
        for (line, e) in &snap.entities {
            let Some(test) = e.test else {
                continue;
            };
            let sourced = if e.group || e.r#virtual {
                false
            } else if let Some(decls) = &e.decls {
                decls.iter().all(|d| mark(&d.file) == Some(test))
            } else {
                match test {
                    EntityTest::Target | EntityTest::File => {
                        e.def.as_ref().is_some_and(|d| mark(&d.file) == Some(test))
                    }
                    EntityTest::Attribute | EntityTest::Cfg | EntityTest::Unknown => true,
                }
            };
            if !sourced {
                self.diag(
                    "entities.jsonl",
                    *line,
                    "entity-mark-without-source",
                    format!("the `test` of {} has no source the snapshot holds", e.key),
                );
            }
        }
    }

    /// A row of the snapshot's `syntax.jsonl` never carries a test row's `row` or `test`.
    fn test_in_syntax(&mut self, files: &SnapshotFiles) {
        let text = files
            .syntax
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok())
            .unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            if let Ok(serde_json::Value::Object(row)) = serde_json::from_str(line)
                && (row.contains_key("test") || row.contains_key("row"))
            {
                self.diag(
                    "syntax.jsonl",
                    i + 1,
                    "test-in-syntax",
                    "a syntax row carries test evidence".to_owned(),
                );
            }
        }
    }
}

/// The scopes and targets of a snapshot, for the checks of its build facts.
struct BuildView<'a> {
    /// Per scope `path`: the build layer's state and whether an adapter made the scope.
    scopes: BTreeMap<&'a str, (LayerState, bool)>,
    /// Per target key: its line and row.
    targets: BTreeMap<&'a str, (usize, &'a TargetRow)>,
}

impl BuildView<'_> {
    /// The deepest scope whose `path` contains `file`, `.` containing everything.
    fn scope_of(&self, file: &str) -> Option<&str> {
        self.scopes
            .keys()
            .copied()
            .filter(|scope| {
                *scope == "."
                    || file
                        .strip_prefix(*scope)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|scope| if *scope == "." { 0 } else { scope.len() })
    }

    /// The build layer's state in `scope`.
    fn state(&self, scope: Option<&str>) -> Option<LayerState> {
        scope
            .and_then(|s| self.scopes.get(s))
            .map(|(state, _)| *state)
    }
}
