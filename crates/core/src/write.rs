//! Serialises a snapshot deterministically, and is the core's boundary to the file system for
//! snapshots and sources.
//!
//! [`serialise`] is pure: it sorts every table as `spec/snapshot.md` states and writes fields
//! in schema order, so equal snapshots give equal bytes. [`write_snapshot`] and
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

use crate::graph::Snapshot;

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
            if !candidate.pop() || !candidate.starts_with(&self.root) {
                return Location::Outside;
            }
        }
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        fs::read(self.root.join(path))
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

    /// Reads the seven files of the snapshot at `dir`; a missing file is `None`.
    ///
    /// # Errors
    ///
    /// Returns an I/O error other than a missing file.
    pub fn read(dir: &Path) -> io::Result<Self> {
        let read = |name: &str| match fs::read(dir.join(name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        };
        Ok(Self {
            manifest: read("manifest.json")?,
            files: read("files.jsonl")?,
            entities: read("entities.jsonl")?,
            externals: read("externals.jsonl")?,
            relations: read("relations.jsonl")?,
            syntax: read("syntax.jsonl")?,
            gaps: read("gaps.jsonl")?,
        })
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
        let _missing = source.read("src/missing.rs").unwrap_err();
    }
}
