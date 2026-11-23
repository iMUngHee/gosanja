//! Shared helpers for the core's integration tests: synthetic SCIP indexes, in-memory and
//! recording source readers, and access to the checked-in fixtures.

#![allow(dead_code, reason = "each test file uses a different subset")]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use gosanja_core::graph::{self, Snapshot};
use gosanja_core::write::{self, Location, SnapshotFiles, SourceReader};
use gosanja_schema::snapshot::{EntityRow, GapKind, GapRow, RelationRow};
use protobuf::{EnumOrUnknown, Message};
use scip::types::occurrence::{Typed_enclosing_range, Typed_range};
use scip::types::{
    Document, Index, Metadata, MultiLineRange, Occurrence, PositionEncoding, SingleLineRange,
    SymbolInformation, SymbolRole, ToolInfo,
};

pub const DEF: i32 = SymbolRole::Definition as i32;
pub const REF: i32 = 0;

/// The directory of a checked-in fixture.
#[must_use]
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// A file of the repository, relative to its root.
#[must_use]
pub fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// Builds the snapshot of a checked-in fixture from its `index.scip`.
#[must_use]
pub fn build_fixture(name: &str) -> Snapshot {
    let root = fixture(name);
    let sources = write::FsSource::new(&root).expect("fixture root");
    let mut index = std::fs::File::open(root.join("index.scip")).expect("index.scip");
    graph::build(&mut index, &sources, name).expect("builds")
}

/// The snapshot files at `dir`.
#[must_use]
pub fn read_dir(dir: &Path) -> SnapshotFiles {
    SnapshotFiles::read(dir)
}

/// One occurrence with legacy integer-array ranges; an empty `enclosing` means none.
#[must_use]
pub fn occ(symbol: &str, roles: i32, range: &[i32], enclosing: &[i32]) -> Occurrence {
    let mut o = Occurrence::new();
    symbol.clone_into(&mut o.symbol);
    o.symbol_roles = roles;
    o.range = range.to_vec();
    o.enclosing_range = enclosing.to_vec();
    o
}

/// One occurrence with typed ranges.
#[must_use]
pub fn typed_occ(symbol: &str, roles: i32, range: &[i32], enclosing: &[i32]) -> Occurrence {
    let mut o = Occurrence::new();
    symbol.clone_into(&mut o.symbol);
    o.symbol_roles = roles;
    o.typed_range = Some(match *range {
        [line, start, end] => Typed_range::SingleLineRange(single(line, start, end)),
        [sl, sc, el, ec] => Typed_range::MultiLineRange(multi(sl, sc, el, ec)),
        _ => panic!("a range has three or four numbers"),
    });
    o.typed_enclosing_range = match *enclosing {
        [] => None,
        [line, start, end] => Some(Typed_enclosing_range::SingleLineEnclosingRange(single(
            line, start, end,
        ))),
        [sl, sc, el, ec] => Some(Typed_enclosing_range::MultiLineEnclosingRange(multi(
            sl, sc, el, ec,
        ))),
        _ => panic!("a range has three or four numbers"),
    };
    o
}

fn single(line: i32, start: i32, end: i32) -> SingleLineRange {
    let mut r = SingleLineRange::new();
    r.line = line;
    r.start_character = start;
    r.end_character = end;
    r
}

fn multi(sl: i32, sc: i32, el: i32, ec: i32) -> MultiLineRange {
    let mut r = MultiLineRange::new();
    r.start_line = sl;
    r.start_character = sc;
    r.end_line = el;
    r.end_character = ec;
    r
}

/// A document in UTF-8 positions with the given occurrences.
#[must_use]
pub fn doc(path: &str, occurrences: Vec<Occurrence>) -> Document {
    doc_in(
        path,
        PositionEncoding::UTF8CodeUnitOffsetFromLineStart,
        occurrences,
    )
}

/// A document in the given position encoding.
#[must_use]
pub fn doc_in(path: &str, encoding: PositionEncoding, occurrences: Vec<Occurrence>) -> Document {
    let mut d = Document::new();
    path.clone_into(&mut d.relative_path);
    "test".clone_into(&mut d.language);
    d.position_encoding = EnumOrUnknown::new(encoding);
    d.occurrences = occurrences;
    d
}

/// Symbol information for `symbol` with the given kind.
#[must_use]
pub fn info(symbol: &str, kind: scip::types::symbol_information::Kind) -> SymbolInformation {
    let mut s = SymbolInformation::new();
    symbol.clone_into(&mut s.symbol);
    s.kind = EnumOrUnknown::new(kind);
    s
}

/// One encoded `Index` message with metadata naming `tool` and the given documents.
#[must_use]
pub fn index(tool: &str, documents: Vec<Document>) -> Vec<u8> {
    let mut index = Index::new();
    let mut metadata = Metadata::new();
    let mut tool_info = ToolInfo::new();
    tool.clone_into(&mut tool_info.name);
    "1".clone_into(&mut tool_info.version);
    metadata.tool_info = Some(tool_info).into();
    index.metadata = Some(metadata).into();
    index.documents = documents;
    index.write_to_bytes().expect("encodes")
}

/// Sources held in memory; every path not listed is missing, and paths in `outside` lead out
/// of the root.
#[derive(Debug, Default)]
pub struct MemSource {
    pub files: BTreeMap<String, Vec<u8>>,
    pub outside: Vec<String>,
}

impl MemSource {
    #[must_use]
    pub fn with(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
                .collect(),
            outside: Vec::new(),
        }
    }
}

impl SourceReader for MemSource {
    fn locate(&self, path: &str) -> Location {
        if self.outside.iter().any(|p| p == path) {
            Location::Outside
        } else {
            Location::Inside
        }
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

/// Wraps a reader and records every path whose text was requested.
#[derive(Debug)]
pub struct Recording<R> {
    pub inner: R,
    pub requested: RefCell<Vec<String>>,
}

impl<R: SourceReader> SourceReader for Recording<R> {
    fn locate(&self, path: &str) -> Location {
        self.inner.locate(path)
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.requested.borrow_mut().push(path.to_owned());
        self.inner.read(path)
    }
}

/// Builds a snapshot from encoded index bytes and checks that it passes the validator,
/// normative schemas included.
#[must_use]
pub fn build(bytes: &[u8], sources: &dyn SourceReader) -> Snapshot {
    let snapshot = graph::build(&mut &*bytes, sources, "test").expect("builds");
    assert_eq!(diagnostics(&snapshot), Vec::<String>::new());
    snapshot
}

/// The serialised files of a snapshot, as text.
#[must_use]
pub fn text(snapshot: &Snapshot) -> BTreeMap<&'static str, String> {
    write::serialise(snapshot)
        .named()
        .into_iter()
        .map(|(name, bytes)| {
            let bytes = bytes.expect("every file is written");
            (name, String::from_utf8(bytes.to_vec()).expect("utf-8"))
        })
        .collect()
}

/// The entity with `key`.
#[must_use]
pub fn entity<'a>(snapshot: &'a Snapshot, key: &str) -> &'a EntityRow {
    snapshot
        .entities
        .iter()
        .find(|e| e.key == key)
        .unwrap_or_else(|| panic!("no entity {key}"))
}

/// The relations from `from` to `to`.
#[must_use]
pub fn relations<'a>(snapshot: &'a Snapshot, from: &str, to: &str) -> Vec<&'a RelationRow> {
    snapshot
        .relations
        .iter()
        .filter(|r| r.from == from && r.to == to)
        .collect()
}

/// The gaps of one kind.
#[must_use]
pub fn gaps(snapshot: &Snapshot, kind: GapKind) -> Vec<&GapRow> {
    snapshot.gaps.iter().filter(|g| g.gap == kind).collect()
}

/// Validates a snapshot in memory and returns the diagnostics as text.
#[must_use]
pub fn diagnostics(snapshot: &Snapshot) -> Vec<String> {
    gosanja_core::validate::validate(&write::serialise(snapshot), None)
        .expect("own schema version")
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Symbol information for `symbol` that implements each of `implemented`, plus one
/// relationship that is a reference only and must not become a relation.
#[must_use]
pub fn implementing(symbol: &str, implemented: &[&str]) -> SymbolInformation {
    let mut s = SymbolInformation::new();
    symbol.clone_into(&mut s.symbol);
    for target in implemented {
        let mut r = scip::types::Relationship::new();
        (*target).clone_into(&mut r.symbol);
        r.is_implementation = true;
        s.relationships.push(r);
    }
    let mut reference_only = scip::types::Relationship::new();
    "s m p 1 OnlyReferenced#".clone_into(&mut reference_only.symbol);
    reference_only.is_reference = true;
    s.relationships.push(reference_only);
    s
}

pub fn shell() -> PathBuf {
    #[cfg(unix)]
    {
        PathBuf::from("/bin/sh")
    }
    #[cfg(not(unix))]
    {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|p| p.is_absolute())
            .map(|p| p.join("sh.exe"))
            .find(|p| p.is_file())
            .expect("absolute sh.exe on PATH")
    }
}

pub fn fake(
    mode: &str,
) -> Result<gosanja_core::adapter::Described, gosanja_core::adapter::AdapterError> {
    use gosanja_core::adapter::{Invocation, describe};
    describe(Invocation::new(
        shell(),
        vec![
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/fake-adapter/adapter.sh"
            )
            .into(),
            "--mode".into(),
            mode.into(),
        ],
    )?)
}
