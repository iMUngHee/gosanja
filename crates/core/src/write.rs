//! Serialises a snapshot deterministically, and is the core's boundary to the file system for
//! snapshots and sources.
//!
//! [`serialise`] is pure: it sorts every table as `spec/snapshot.md` states and writes fields
//! in schema order, so equal snapshots give equal bytes; [`serialise_diff`] does the same for a
//! diff as `spec/diff.md` states. [`write_snapshot`], [`write_diff`] and
//! [`SnapshotFiles::read`] move those bytes to and from a directory, and [`FsSource`] is the
//! one reader through which source text reaches the core.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use gosanja_schema::position::Range;
use gosanja_schema::snapshot::{
    Counts, FORMAT, GapRow, Manifest, RelationRow, SCHEMA_VERSION, Tool,
};
use serde::Serialize;

use crate::diff::Comparison;
use crate::graph::Snapshot;
use gosanja_schema::diff::{
    EntityRow as DiffEntityRow, ExternalRow as DiffExternalRow, RelationKind,
    RelationRow as DiffRelationRow,
};

/// Where a source path leads once links are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// Inside the root; the file may still be missing.
    Inside,
    /// Outside the root; the file must not be opened.
    Outside,
}

/// The one way source text reaches the core.
pub trait SourceReader {
    /// Decides where `path`, relative to the root, leads, without reading the file.
    fn locate(&self, path: &str) -> Location;

    /// Reads the bytes of `path`, relative to the root. Called only after [`Self::locate`]
    /// returned [`Location::Inside`].
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the file is missing or unreadable.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
}

/// Reads sources from a directory on disk.
#[derive(Debug, Clone)]
pub struct FsSource {
    root: PathBuf,
}

impl FsSource {
    /// Reads sources below `root`.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when `root` cannot be resolved.
    pub fn new(root: &Path) -> io::Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
        })
    }
}

impl SourceReader for FsSource {
    /// Resolves the longest existing prefix of the path, links included, and checks that it
    /// stays below the root. A missing file below the root is inside; reading it then fails.
    fn locate(&self, path: &str) -> Location {
        let mut candidate = self.root.join(path);
        loop {
            if let Ok(resolved) = candidate.canonicalize() {
                return if resolved.starts_with(&self.root) {
                    Location::Inside
                } else {
                    Location::Outside
                };
            }
            // Some prefix resolves before the loop runs out: the root itself, which `new`
            // resolved, or for a path that leaves it, the file-system root.
            if !candidate.pop() {
                return Location::Outside;
            }
        }
    }

    /// Opens the resolved path, and only after checking that it is below the root, so the file
    /// read is the one that was checked. The tree is assumed not to change during a run.
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let resolved = self.root.join(path).canonicalize()?;
        if !resolved.starts_with(&self.root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the path resolves outside the root",
            ));
        }
        fs::read(resolved)
    }
}

/// The name a closed enumeration value is written with.
#[must_use]
pub fn enum_name<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => String::new(),
    }
}

/// The seven files of a snapshot as bytes; `None` for a file that is not there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotFiles {
    /// `manifest.json`.
    pub manifest: Option<Vec<u8>>,
    /// `files.jsonl`.
    pub files: Option<Vec<u8>>,
    /// `entities.jsonl`.
    pub entities: Option<Vec<u8>>,
    /// `externals.jsonl`.
    pub externals: Option<Vec<u8>>,
    /// `relations.jsonl`.
    pub relations: Option<Vec<u8>>,
    /// `syntax.jsonl`.
    pub syntax: Option<Vec<u8>>,
    /// `gaps.jsonl`.
    pub gaps: Option<Vec<u8>>,
    /// Files that are there and could not be read, with the reason.
    pub unreadable: Vec<(&'static str, String)>,
}

impl SnapshotFiles {
    /// File names in the order the specification lists them, with their contents.
    #[must_use]
    pub fn named(&self) -> [(&'static str, Option<&[u8]>); 7] {
        [
            ("manifest.json", self.manifest.as_deref()),
            ("files.jsonl", self.files.as_deref()),
            ("entities.jsonl", self.entities.as_deref()),
            ("externals.jsonl", self.externals.as_deref()),
            ("relations.jsonl", self.relations.as_deref()),
            ("syntax.jsonl", self.syntax.as_deref()),
            ("gaps.jsonl", self.gaps.as_deref()),
        ]
    }

    /// Reads the seven files of the snapshot at `dir`: [`Self::read_manifest`], then
    /// [`Self::read_tables`].
    #[must_use]
    pub fn read(dir: &Path) -> Self {
        let mut files = Self::read_manifest(dir);
        files.read_tables(dir);
        files
    }

    /// Reads only `manifest.json` of the snapshot at `dir`, so that a reader can judge the
    /// version before it opens any table. A missing file is `None`; one that is there and
    /// cannot be read is `None` too and is listed in `unreadable`.
    #[must_use]
    pub fn read_manifest(dir: &Path) -> Self {
        let mut files = Self::default();
        files.manifest = read_file(dir, "manifest.json", &mut files.unreadable);
        files
    }

    /// Reads the six tables of the snapshot at `dir`, as [`Self::read_manifest`] reads the
    /// manifest. Reading again replaces what an earlier read said about the tables and keeps
    /// what was said about the manifest.
    pub fn read_tables(&mut self, dir: &Path) {
        self.unreadable.retain(|(name, _)| *name == "manifest.json");
        let unreadable = &mut self.unreadable;
        self.files = read_file(dir, "files.jsonl", unreadable);
        self.entities = read_file(dir, "entities.jsonl", unreadable);
        self.externals = read_file(dir, "externals.jsonl", unreadable);
        self.relations = read_file(dir, "relations.jsonl", unreadable);
        self.syntax = read_file(dir, "syntax.jsonl", unreadable);
        self.gaps = read_file(dir, "gaps.jsonl", unreadable);
    }
}

fn read_file(
    dir: &Path,
    name: &'static str,
    unreadable: &mut Vec<(&'static str, String)>,
) -> Option<Vec<u8>> {
    match fs::read(dir.join(name)) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => {
            unreadable.push((name, e.to_string()));
            None
        }
    }
}

/// Writes the seven files of `snapshot` into the existing directory `dir`.
///
/// # Errors
///
/// Returns the first I/O error.
pub fn write_snapshot(dir: &Path, snapshot: &Snapshot) -> io::Result<()> {
    for (name, bytes) in serialise(snapshot).named() {
        fs::write(dir.join(name), bytes.unwrap_or_default())?;
    }
    Ok(())
}

/// The five files of a diff, in the order `spec/diff.md` lists them.
pub const DIFF_FILES: [&str; 5] = [
    "manifest.json",
    "entities.jsonl",
    "externals.jsonl",
    "relations.jsonl",
    "candidates.jsonl",
];

/// Writes the five files of `comparison` into `dir`.
///
/// # Errors
///
/// Returns the I/O error of the first file that cannot be written.
pub fn write_diff(dir: &Path, comparison: &Comparison) -> io::Result<()> {
    for (name, bytes) in serialise_diff(comparison) {
        fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}

/// The bytes of the five files of a diff, named as in [`DIFF_FILES`].
///
/// Every table is sorted as `spec/diff.md` states, an absent key after any key and ties broken
/// by the serialised row. The manifest is written as `comparison` holds it: its counts and its
/// sorted lists are the comparison's.
#[must_use]
pub fn serialise_diff(comparison: &Comparison) -> Vec<(&'static str, Vec<u8>)> {
    let entities = table(&comparison.entities, |r| {
        let (old, new) = diff_entity_keys(r);
        (absent_last(old), absent_last(new))
    });
    let externals = table(&comparison.externals, |r| {
        let (old, new) = diff_external_keys(r);
        (absent_last(old), absent_last(new))
    });
    let relations = table(&comparison.relations, diff_relation_order);
    let candidates = table(&comparison.candidates, |r| (r.old.clone(), r.rank));
    let mut manifest_bytes = to_line(&comparison.manifest);
    manifest_bytes.push(b'\n');
    vec![
        (DIFF_FILES[0], manifest_bytes),
        (DIFF_FILES[1], entities.0),
        (DIFF_FILES[2], externals.0),
        (DIFF_FILES[3], relations.0),
        (DIFF_FILES[4], candidates.0),
    ]
}

/// A sort key part under which an absent key comes after any key.
fn absent_last(key: Option<&str>) -> (bool, String) {
    (key.is_none(), key.unwrap_or_default().to_owned())
}

fn diff_entity_keys(row: &DiffEntityRow) -> (Option<&str>, Option<&str>) {
    match row {
        DiffEntityRow::Added { new, .. } => (None, Some(new)),
        DiffEntityRow::Removed { old, .. } => (Some(old), None),
        DiffEntityRow::Changed { old, new, .. } | DiffEntityRow::Rekeyed { old, new } => {
            (Some(old), Some(new))
        }
    }
}

fn diff_external_keys(row: &DiffExternalRow) -> (Option<&str>, Option<&str>) {
    match row {
        DiffExternalRow::Added { new, .. } => (None, Some(new)),
        DiffExternalRow::Removed { old, .. } => (Some(old), None),
        DiffExternalRow::Rekeyed { old, new } => (Some(old), Some(new)),
    }
}

/// Sort key of a diff's `relations.jsonl`: `from.old`, `from.new`, `to.old`, `to.new`, `rel`.
fn diff_relation_order(row: &DiffRelationRow) -> [(bool, String); 5] {
    let rel = match row.kind {
        RelationKind::Ref { .. } => "ref",
        RelationKind::Call { .. } => "call",
        RelationKind::Implements => "implements",
    };
    [
        absent_last(row.from.old.as_deref()),
        absent_last(row.from.new.as_deref()),
        absent_last(row.to.old.as_deref()),
        absent_last(row.to.new.as_deref()),
        (false, rel.to_owned()),
    ]
}

/// The bytes of every file of `snapshot`, sorted and counted.
#[must_use]
pub fn serialise(snapshot: &Snapshot) -> SnapshotFiles {
    let files = table(&snapshot.files, |r| vec![Some(r.path.clone())]);
    let entities = table(&snapshot.entities, |r| vec![Some(r.key.clone())]);
    let externals = table(&snapshot.externals, |r| vec![Some(r.key.clone())]);
    let relations = table(&snapshot.relations, relation_order);
    let syntax = table(&snapshot.syntax, |r| vec![Some(r.key.clone())]);
    let gaps = table(&snapshot.gaps, gap_order);
    let mut scope = snapshot.scope.clone();
    scope.indexers.sort();
    scope.indexers.dedup();
    scope.builds.sort_by(|a, b| a.id.cmp(&b.id));
    let manifest = Manifest {
        format: FORMAT.to_owned(),
        schema: SCHEMA_VERSION.to_string(),
        tool: Tool {
            name: "gosanja".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        project: snapshot.project.clone(),
        scopes: vec![scope],
        counts: Counts {
            files: files.1,
            entities: entities.1,
            externals: externals.1,
            relations: relations.1,
            syntax: syntax.1,
            gaps: gaps.1,
            dropped_local_references: snapshot.dropped_local_references,
        },
    };
    let mut manifest_bytes = to_line(&manifest);
    manifest_bytes.push(b'\n');
    SnapshotFiles {
        manifest: Some(manifest_bytes),
        files: Some(files.0),
        entities: Some(entities.0),
        externals: Some(externals.0),
        relations: Some(relations.0),
        syntax: Some(syntax.0),
        gaps: Some(gaps.0),
        unreadable: Vec::new(),
    }
}

/// Sort key of `relations.jsonl`: (`from`, `rel`, `to`, `mechanism`, `resolution`, `src`),
/// an absent field first.
pub(crate) fn relation_order(r: &RelationRow) -> Vec<Option<String>> {
    vec![
        Some(r.from.clone()),
        Some(enum_name(&r.rel)),
        Some(r.to.clone()),
        r.mechanism.as_ref().map(enum_name),
        r.resolution.as_ref().map(enum_name),
        Some(r.src.clone()),
    ]
}

/// Sort key of `gaps.jsonl`: (`gap`, `file`, `range`), an absent field first.
pub(crate) fn gap_order(r: &GapRow) -> (String, Option<String>, Option<Range>) {
    (enum_name(&r.gap), r.file.clone(), r.range)
}

fn to_line<T: Serialize>(row: &T) -> Vec<u8> {
    // The row types hold only strings, integers, booleans and their compositions, which always
    // serialise.
    serde_json::to_vec(row).unwrap_or_default()
}

/// One JSON Lines table: rows sorted by `sort_key`, an absent field first, ties broken by the
/// serialised bytes. Returns the bytes and the row count.
fn table<T: Serialize, K: Ord>(rows: &[T], sort_key: impl Fn(&T) -> K) -> (Vec<u8>, u64) {
    let mut lines: Vec<(K, Vec<u8>)> = rows.iter().map(|r| (sort_key(r), to_line(r))).collect();
    lines.sort();
    let mut out = Vec::new();
    for (_, line) in &lines {
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    (out, lines.len() as u64)
}

#[cfg(test)]
mod tests {
    use gosanja_schema::snapshot::GapKind;

    use super::*;

    #[test]
    fn gaps_sort_by_kind_file_and_numeric_range_with_absent_fields_first() {
        let gap = |kind, file: Option<&str>, range: Option<Range>| {
            let mut g = GapRow::new(kind, "core:identity");
            g.file = file.map(str::to_owned);
            g.range = range;
            g
        };
        let rows = [
            gap(GapKind::OpenCall, Some("a"), Some(Range::new(10, 0, 10, 1))),
            gap(GapKind::OpenCall, Some("a"), Some(Range::new(9, 0, 9, 1))),
            gap(GapKind::OpenCall, None, None),
            gap(GapKind::DocumentOutsideRoot, Some("b"), None),
        ];
        let (bytes, count) = table(&rows, gap_order);
        assert_eq!(count, 4);
        let text = String::from_utf8(bytes).expect("utf-8");
        let order: Vec<_> = text
            .lines()
            .map(|l| {
                (
                    l.contains("document"),
                    l.contains("[9,"),
                    l.contains("[10,"),
                )
            })
            .collect();
        assert_eq!(
            order,
            [
                (true, false, false),
                (false, false, false),
                (false, true, false),
                (false, false, true)
            ]
        );
    }

    #[test]
    fn missing_files_inside_the_root_are_inside_and_links_out_are_outside() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("root");
        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::write(root.join("src/a.rs"), "x").expect("write");
        let outside = dir.path().join("secret.rs");
        fs::write(&outside, "y").expect("write");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link.rs")).expect("symlink");
        let source = FsSource::new(&root).expect("root");
        assert_eq!(source.locate("src/a.rs"), Location::Inside);
        assert_eq!(source.locate("src/missing.rs"), Location::Inside);
        assert_eq!(source.locate("missing/dir/x.rs"), Location::Inside);
        #[cfg(unix)]
        assert_eq!(source.locate("link.rs"), Location::Outside);
        assert_eq!(source.read("src/a.rs").expect("reads"), b"x");
        #[cfg(unix)]
        assert_eq!(
            source.read("link.rs").map_err(|e| e.kind()),
            Err(io::ErrorKind::PermissionDenied),
            "read opens only what resolves inside the root"
        );
        let _missing = source.read("src/missing.rs").unwrap_err();
    }

    /// The hand-derived example read back into rows with every table reversed, so that only the
    /// writer can put them back in order.
    fn shuffled_example() -> (crate::diff::Comparison, Vec<(&'static str, Vec<u8>)>) {
        fn rows<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Vec<T> {
            let mut rows: Vec<T> = std::str::from_utf8(bytes)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            rows.reverse();
            rows
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/diff-basic");
        let files: Vec<(&'static str, Vec<u8>)> = DIFF_FILES
            .iter()
            .map(|name| (*name, fs::read(dir.join(name)).unwrap()))
            .collect();
        let manifest: gosanja_schema::diff::Manifest = serde_json::from_slice(&files[0].1).unwrap();
        let comparison = crate::diff::Comparison {
            manifest,
            entities: rows(&files[1].1),
            externals: rows(&files[2].1),
            relations: rows(&files[3].1),
            candidates: rows(&files[4].1),
            pairs: std::collections::BTreeMap::new(),
        };
        (comparison, files)
    }

    #[test]
    fn a_diff_is_written_in_the_orders_of_the_specification() {
        let (comparison, example) = shuffled_example();
        let written = serialise_diff(&comparison);
        assert_eq!(written.len(), 5);
        for ((name, bytes), (expected_name, expected)) in written.iter().zip(&example) {
            assert_eq!(name, expected_name);
            assert!(
                bytes == expected,
                "{name}: got\n{}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn writing_a_diff_puts_exactly_its_five_files_in_the_directory() {
        let (comparison, example) = shuffled_example();
        let dir = tempfile::tempdir().unwrap();
        write_diff(dir.path(), &comparison).unwrap();
        let mut names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut expected: Vec<String> = DIFF_FILES.iter().map(|n| (*n).to_owned()).collect();
        expected.sort();
        assert_eq!(names, expected);
        for (name, bytes) in example {
            assert_eq!(fs::read(dir.path().join(name)).unwrap(), bytes, "{name}");
        }
    }
}
