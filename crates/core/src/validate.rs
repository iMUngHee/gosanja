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
    EntityRow, ExternalRow, FORMAT, FileRow, GapKind, GapRow, Layer, Manifest, Mechanism,
    OutlineKind, OutlineNode, RefRole, Rel, RelationRow, Resolution, Role, SCHEMA_VERSION,
    SyntaxRow,
};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::graph::LineIndex;
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
    /// Rows of `gaps.jsonl`.
    pub gaps: Vec<(usize, GapRow)>,
    /// Rows skipped for a value from a newer minor, or for depending on such a row.
    pub skipped: u64,
    /// Problems found while reading.
    pub diagnostics: Vec<Diagnostic>,
}

/// Reads a snapshot as a reader implementing schema version `reader`.
///
/// # Errors
///
/// Returns the [`Refusal`] when the snapshot's schema version may not be read at all; nothing
/// else is read then.
pub fn read(files: &SnapshotFiles, reader: Version) -> Result<ReadSnapshot, Refusal> {
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
                match serde_json::from_value::<Manifest>(value) {
                    Ok(manifest) => out.manifest = Some(manifest),
                    Err(e) => out.diag("manifest.json", None, "malformed-row", e.to_string()),
                }
            }
        },
    }
    out.files = out.table(
        "files.jsonl",
        files.files.as_deref(),
        acceptance,
        |_: &FileRow| false,
    );
    out.entities = out.table(
        "entities.jsonl",
        files.entities.as_deref(),
        acceptance,
        |r: &EntityRow| r.role == Some(Role::Unknown),
    );
    out.externals = out.table(
        "externals.jsonl",
        files.externals.as_deref(),
        acceptance,
        |_: &ExternalRow| false,
    );
    out.relations = out.table(
        "relations.jsonl",
        files.relations.as_deref(),
        acceptance,
        |r: &RelationRow| {
            r.rel == Rel::Unknown
                || r.roles.iter().flatten().any(|x| *x == RefRole::Unknown)
                || r.mechanism == Some(Mechanism::Unrecognised)
                || r.resolution == Some(Resolution::Unknown)
        },
    );
    out.syntax = out.table(
        "syntax.jsonl",
        files.syntax.as_deref(),
        acceptance,
        |r: &SyntaxRow| r.outline.as_deref().is_some_and(has_unknown_node),
    );
    out.gaps = out.table(
        "gaps.jsonl",
        files.gaps.as_deref(),
        acceptance,
        |r: &GapRow| r.gap == GapKind::Unrecognised || r.layer == Some(Layer::Unknown),
    );
    out.skip_dependents();
    Ok(out)
}

fn has_unknown_node(nodes: &[OutlineNode]) -> bool {
    nodes
        .iter()
        .any(|n| n.k == OutlineKind::Unknown || n.children.as_deref().is_some_and(has_unknown_node))
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

    fn table<T: DeserializeOwned>(
        &mut self,
        name: &'static str,
        bytes: Option<&[u8]>,
        acceptance: Acceptance,
        unknown_value: impl Fn(&T) -> bool,
    ) -> Vec<(usize, T)> {
        let Some(bytes) = bytes else {
            self.diag(name, None, "missing-file", "the file is absent");
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
            let parsed = serde_json::from_str::<T>(line);
            let unknown = parsed.as_ref().is_ok_and(&unknown_value);
            match compat::classify_row(acceptance, parsed.is_ok(), unknown) {
                RowVerdict::Keep => rows.extend(parsed.ok().map(|row| (i + 1, row))),
                RowVerdict::Skip => self.skipped += 1,
                RowVerdict::Malformed => {
                    let detail = match parsed {
                        Err(e) => e.to_string(),
                        Ok(_) => "a value this schema version does not define".to_owned(),
                    };
                    let code = if unknown {
                        "unknown-value"
                    } else {
                        "malformed-row"
                    };
                    self.diag(name, at, code, detail);
                }
            }
        }
        rows
    }

    /// Rule 4: a row that depends on a skipped row is skipped too, so no key dangles.
    fn skip_dependents(&mut self) {
        if self.skipped == 0 {
            return;
        }
        loop {
            let keys: BTreeSet<String> = self.entities.iter().map(|(_, e)| e.key.clone()).collect();
            let before = self.entities.len();
            self.entities.retain(|(_, e)| {
                e.parent.as_ref().is_none_or(|p| keys.contains(p))
                    && e.member_of.as_ref().is_none_or(|g| keys.contains(g))
            });
            self.skipped += (before - self.entities.len()) as u64;
            if before == self.entities.len() {
                break;
            }
        }
        let entities: BTreeSet<String> = self.entities.iter().map(|(_, e)| e.key.clone()).collect();
        let targets: BTreeSet<&String> = entities
            .iter()
            .chain(self.externals.iter().map(|(_, e)| &e.key))
            .collect();
        let before = self.relations.len() + self.syntax.len() + self.gaps.len();
        self.relations
            .retain(|(_, r)| entities.contains(&r.from) && targets.contains(&r.to));
        self.syntax.retain(|(_, s)| entities.contains(&s.key));
        self.gaps
            .retain(|(_, g)| g.keys.iter().flatten().all(|k| targets.contains(k)));
        let after = self.relations.len() + self.syntax.len() + self.gaps.len();
        self.skipped += (before - after) as u64;
    }
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
    let mut snap = read(files, SCHEMA_VERSION)?;
    let mut v = Checks {
        diagnostics: std::mem::take(&mut snap.diagnostics),
    };
    v.manifest(&snap);
    v.order(files, &snap);
    let lines = v.files(&snap, sources);
    v.entities(&snap, &lines);
    v.relations(&snap, &lines);
    v.gaps(&snap, &lines);
    let entity_keys: BTreeSet<&str> = snap.entities.iter().map(|(_, e)| e.key.as_str()).collect();
    for (line, row) in &snap.syntax {
        if !entity_keys.contains(row.key.as_str()) {
            v.diag(
                "syntax.jsonl",
                *line,
                "dangling-key",
                format!("`key` {} names no entity", row.key),
            );
        }
    }
    Ok(v.diagnostics)
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
            true,
        );
        let text = lines(files.entities.as_ref());
        self.sorted(
            "entities.jsonl",
            &snap.entities,
            &text,
            |r| vec![Some(r.key.clone())],
            true,
        );
        let text = lines(files.externals.as_ref());
        self.sorted(
            "externals.jsonl",
            &snap.externals,
            &text,
            |r| vec![Some(r.key.clone())],
            true,
        );
        let text = lines(files.relations.as_ref());
        self.sorted(
            "relations.jsonl",
            &snap.relations,
            &text,
            write::relation_order,
            false,
        );
        let text = lines(files.syntax.as_ref());
        self.sorted(
            "syntax.jsonl",
            &snap.syntax,
            &text,
            |r| vec![Some(r.key.clone())],
            true,
        );
        let text = lines(files.gaps.as_ref());
        self.sorted("gaps.jsonl", &snap.gaps, &text, write::gap_order, false);
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
        unique: bool,
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
            if unique && ka == kb {
                self.diag(
                    name,
                    *line,
                    "duplicate-key",
                    "the key of the previous row again".to_owned(),
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
            let variant = match (e.group, e.r#virtual, e.member_of.is_some()) {
                (true, false, false) => e.symbol.is_some() && e.def.is_none(),
                (false, true, false) => e.role.is_some() && e.def.is_none(),
                (false, false, _) => e.symbol.is_some() && e.role.is_some() && e.def.is_some(),
                _ => false,
            };
            if !variant {
                self.diag(
                    "entities.jsonl",
                    line,
                    "variant",
                    format!("{} is none of the four row variants", e.key),
                );
            }
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
                if def.body.is_some() != def.body_src.is_some() {
                    self.diag(
                        "entities.jsonl",
                        line,
                        "variant",
                        "`body` and `body_src` go together".to_owned(),
                    );
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
        for (line, g) in &snap.gaps {
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
}
