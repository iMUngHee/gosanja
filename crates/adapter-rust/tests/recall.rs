//! Reference recall of `rust-analyzer scip` on `fixtures/rust-recall`, whose
//! `expected-refs.tsv` lists by hand every identifier token that names a definition, other
//! than the definition's own name token: file, line, start byte column, then the file, line
//! and column of the name token it names.
//!
//! Raw recall asks the index: a reference occurrence starts at the site, and its symbol is
//! the symbol defined at the target. Snapshot recall asks what the engine builds from the
//! index: the site is among the sites of a `ref` relation to the entity defined at the target.

use std::fs;
use std::path::{Path, PathBuf};

use gosanja_core::write::FsSource;
use gosanja_schema::snapshot::Rel;
use protobuf::Message;
use scip::types::{Document, Index, SymbolRole};

type Row = (String, u32, u32, String, u32, u32);

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-recall")
}

fn expected() -> Vec<Row> {
    fs::read_to_string(fixture().join("expected-refs.tsv"))
        .expect("the hand-made list")
        .lines()
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let n = |i: usize| f[i].parse::<u32>().expect("a number");
            (f[0].to_owned(), n(1), n(2), f[3].to_owned(), n(4), n(5))
        })
        .collect()
}

fn index() -> Index {
    Index::parse_from_bytes(&fs::read(fixture().join("index.scip")).expect("an index"))
        .expect("SCIP")
}

/// The symbols of the occurrences starting at a position, definitions or not.
fn symbols_at(document: &Document, line: u32, col: u32, definition: bool) -> Vec<String> {
    document
        .occurrences
        .iter()
        .filter(|o| (o.symbol_roles & SymbolRole::Definition as i32 != 0) == definition)
        .filter(|o| {
            let start = (o.range.first().copied(), o.range.get(1).copied());
            start == (i32::try_from(line).ok(), i32::try_from(col).ok())
        })
        .map(|o| o.symbol.clone())
        .collect()
}

fn document<'a>(index: &'a Index, file: &str) -> &'a Document {
    index
        .documents
        .iter()
        .find(|d| d.relative_path == file)
        .unwrap_or_else(|| panic!("no document {file}"))
}

/// The hand-made rows the index finds and those it misses.
fn raw(index: &Index, rows: &[Row]) -> (usize, Vec<Row>) {
    let mut missing = Vec::new();
    for row in rows {
        let (file, line, col, target, tline, tcol) = row;
        let defined = symbols_at(document(index, target), *tline, *tcol, true);
        let referenced = symbols_at(document(index, file), *line, *col, false);
        if !referenced.iter().any(|s| defined.contains(s)) {
            missing.push(row.clone());
        }
    }
    (rows.len() - missing.len(), missing)
}

/// The hand-made rows the snapshot finds and those it misses.
fn snapshot(rows: &[Row]) -> (usize, Vec<Row>, u64) {
    let root = fixture();
    let mut file = fs::File::open(root.join("index.scip")).expect("an index");
    let snap = gosanja_core::graph::build(
        &mut file,
        &FsSource::new(&root).expect("root"),
        "rust-recall",
    )
    .expect("a snapshot");
    let mut missing = Vec::new();
    for row in rows {
        let (file, line, col, target, tline, tcol) = row;
        let entity = snap.entities.iter().find(|e| {
            e.def
                .as_ref()
                .is_some_and(|d| d.file == *target && d.name.start() == (*tline, *tcol))
        });
        let found = entity.is_some_and(|e| {
            snap.relations.iter().any(|r| {
                r.rel == Rel::Ref
                    && r.to == e.key
                    && r.sites
                        .iter()
                        .flatten()
                        .any(|s| s.file() == file && (s.1, s.2) == (*line, *col))
            })
        });
        if !found {
            missing.push(row.clone());
        }
    }
    (
        rows.len() - missing.len(),
        missing,
        snap.dropped_local_references,
    )
}

fn row(line: u32, col: u32, tline: u32, tcol: u32) -> Row {
    (
        "src/lib.rs".into(),
        line,
        col,
        "src/lib.rs".into(),
        tline,
        tcol,
    )
}

#[test]
fn recall_of_the_index_and_of_the_snapshot_is_pinned() {
    let rows = expected();
    assert_eq!(rows.len(), 34);
    let index = index();

    let (found, missing) = raw(&index, &rows);
    assert_eq!((found, missing), (34, vec![]));

    // The eight the snapshot misses name parameters, a type parameter and local variables:
    // `c`, `T`, `t`, `d` in the signatures of lines 10 to 12 and the `let` bindings `c`,
    // `w`, `v`, `n` of `run`.
    let (found, missing, dropped) = snapshot(&rows);
    assert_eq!(found, 26);
    assert_eq!(
        missing,
        [
            row(9, 26, 9, 9),    // c.read(), the parameter c of plain
            row(10, 24, 10, 11), // &T, the type parameter T of generic
            row(10, 35, 10, 20), // t.read(), the parameter t
            row(11, 33, 11, 11), // d.read(), the parameter d of dynamic
            row(14, 25, 13, 8),  // item: c, the local c
            row(15, 24, 14, 8),  // w.item, the local w
            row(16, 33, 15, 18), // Cell { n: v }, the local v bound by `let Cell { n: v }`
            row(17, 18, 16, 8),  // format!("{}", n), the local n
        ]
    );
    // Each of them is dropped by the engine's rule for local symbols: the index gives the
    // reference a `local` symbol, which has no key and makes no row.
    let lib = document(&index, "src/lib.rs");
    for (_, line, col, ..) in &missing {
        let symbols = symbols_at(lib, *line, *col, false);
        assert!(
            !symbols.is_empty() && symbols.iter().all(|s| s.starts_with("local ")),
            "{line}:{col} {symbols:?}"
        );
    }
    // The snapshot counts exactly those drops.
    assert_eq!(dropped, 8);
}
