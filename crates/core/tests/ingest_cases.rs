//! Ingest cases built from `scip` types: positions, decoding, document location, identity,
//! ownership, and the snapshot reader's compatibility rules.

mod support;

use gosanja_core::graph::Snapshot;
use gosanja_core::validate;
use gosanja_core::write::{self, FsSource, SnapshotFiles};
use gosanja_schema::compat::Version;
use gosanja_schema::position::{Range, Site};
use gosanja_schema::snapshot::{GapKind, Rel, SCHEMA_VERSION};
use scip::types::{Document, PositionEncoding};
use support::{
    DEF, MemSource, REF, Recording, build, diagnostics, doc, doc_in, entity, gaps, index, occ,
    relations, text, typed_occ,
};

/// Ten ASCII lines of forty characters, long enough for every synthetic range.
fn blank() -> String {
    format!("{}\n", "x".repeat(40)).repeat(10)
}

/// Builds the documents in the given order and reversed; both snapshots must be identical.
fn both_orders(documents: &[Document], sources: &MemSource) -> Snapshot {
    let forward = build(&index("t", documents.to_vec()), sources);
    let mut reversed = documents.to_vec();
    reversed.reverse();
    let backward = build(&index("t", reversed), sources);
    assert_eq!(
        text(&forward),
        text(&backward),
        "output depends on document order"
    );
    assert!(
        diagnostics(&forward).is_empty(),
        "{:?}",
        diagnostics(&forward)
    );
    forward
}

fn site(file: &str, sl: u32, sc: u32, el: u32, ec: u32) -> Site {
    Site::new(file, Range::new(sl, sc, el, ec))
}

// Positions.

const POSITIONS: &str = "fn f() {}\nfn g() { \"€🦀\"; f() }\n";

/// The reference to `f` on line 1 and the end of `g`'s body, in each unit. Worked out from
/// `POSITIONS`: twenty characters on line 1, where `€` is one UTF-16 unit and three bytes,
/// and `🦀` two UTF-16 units and four bytes, so `f` starts at character 15.
fn positions_document(encoding: PositionEncoding, typed: bool) -> Document {
    let (f_start, line_end) = match encoding {
        PositionEncoding::UTF32CodeUnitOffsetFromLineStart => (15, 20),
        PositionEncoding::UTF16CodeUnitOffsetFromLineStart => (16, 21),
        _ => (20, 25),
    };
    let make = if typed { typed_occ } else { occ };
    doc_in(
        "x.rs",
        encoding,
        vec![
            make("s m p 1 f().", DEF, &[0, 3, 4], &[0, 0, 9]),
            make("s m p 1 g().", DEF, &[1, 3, 4], &[1, 0, line_end]),
            make("s m p 1 f().", REF, &[1, f_start, f_start + 1], &[]),
        ],
    )
}

#[test]
fn every_encoding_and_range_form_gives_the_same_utf8_columns() {
    let sources = MemSource::with(&[("x.rs", POSITIONS)]);
    let mut outputs = Vec::new();
    for encoding in [
        PositionEncoding::UTF8CodeUnitOffsetFromLineStart,
        PositionEncoding::UTF16CodeUnitOffsetFromLineStart,
        PositionEncoding::UTF32CodeUnitOffsetFromLineStart,
    ] {
        for typed in [false, true] {
            let snapshot = build(
                &index("t", vec![positions_document(encoding, typed)]),
                &sources,
            );
            let refs = relations(&snapshot, "m:p:g().", "m:p:f().");
            assert_eq!(refs.len(), 1, "{encoding:?} typed={typed}");
            assert_eq!(
                refs[0].sites.as_deref(),
                Some(&[site("x.rs", 1, 20, 1, 21)][..]),
                "{encoding:?} typed={typed}"
            );
            let body = entity(&snapshot, "m:p:g().")
                .def
                .as_ref()
                .and_then(|d| d.body);
            assert_eq!(body, Some(Range::new(1, 0, 1, 25)));
            outputs.push(text(&snapshot));
        }
    }
    assert!(
        outputs.windows(2).all(|w| w[0] == w[1]),
        "the six snapshots differ"
    );
}

#[test]
fn typed_ranges_win_over_legacy_ones() {
    let sources = MemSource::with(&[("x.rs", POSITIONS)]);
    let mut def = typed_occ("s m p 1 f().", DEF, &[0, 3, 4], &[0, 0, 9]);
    def.range = vec![1, 0, 1];
    def.enclosing_range = vec![1, 0, 1, 5];
    let multi_line = typed_occ("s m p 1 g().", DEF, &[1, 3, 1, 4], &[1, 0, 1, 25]);
    let snapshot = build(
        &index("t", vec![doc("x.rs", vec![def, multi_line])]),
        &sources,
    );
    let g = entity(&snapshot, "m:p:g().").def.clone().expect("def");
    assert_eq!(
        (g.name, g.body),
        (Range::new(1, 3, 1, 4), Some(Range::new(1, 0, 1, 25)))
    );
    let d = entity(&snapshot, "m:p:f().").def.clone().expect("def");
    assert_eq!(
        (d.name, d.body),
        (Range::new(0, 3, 0, 4), Some(Range::new(0, 0, 0, 9)))
    );
}

#[test]
fn unspecified_encoding_and_split_characters_are_gaps() {
    let sources = MemSource::with(&[("x.rs", POSITIONS), ("y.rs", POSITIONS)]);
    let undecodable = doc_in(
        "y.rs",
        PositionEncoding::UnspecifiedPositionEncoding,
        vec![occ("s m p 1 h().", DEF, &[0, 3, 4], &[])],
    );
    // Byte 11 of line 1 is inside `€`, which spans bytes 10 to 12.
    let split = doc(
        "x.rs",
        vec![
            occ("s m p 1 f().", DEF, &[0, 3, 4], &[0, 0, 9]),
            occ("s m p 1 f().", REF, &[1, 11, 12], &[]),
            occ("s m p 1 f().", REF, &[7, 0, 1], &[]),
        ],
    );
    let snapshot = build(&index("t", vec![undecodable, split]), &sources);
    let undecodable = gaps(&snapshot, GapKind::UndecodableDocument);
    assert_eq!(undecodable.len(), 1);
    assert_eq!(undecodable[0].file.as_deref(), Some("y.rs"));
    assert!(snapshot.entities.iter().all(|e| e.key != "m:p:h()."));
    assert!(snapshot.files.iter().all(|f| f.path != "y.rs"));
    let out = gaps(&snapshot, GapKind::PositionOutOfRange);
    assert_eq!(out.len(), 1);
    assert_eq!(
        (out[0].file.as_deref(), out[0].count),
        (Some("x.rs"), Some(2))
    );
    assert_eq!(snapshot.relations.len(), 0);
}

// Multi-message indexes and document location.

#[test]
fn concatenated_index_messages_are_all_read() {
    let sources = MemSource::with(&[("a.rs", &blank()), ("b.rs", &blank())]);
    let mut bytes = index(
        "t",
        vec![doc("a.rs", vec![occ("s m p 1 a().", DEF, &[0, 0, 1], &[])])],
    );
    bytes.extend(index(
        "t",
        vec![doc("b.rs", vec![occ("s m p 1 b().", DEF, &[0, 0, 1], &[])])],
    ));
    let snapshot = build(&bytes, &sources);
    assert_eq!(entity(&snapshot, "m:p:a().").name, "a");
    assert_eq!(entity(&snapshot, "m:p:b().").name, "b");
}

#[cfg(unix)]
#[test]
fn documents_outside_the_root_are_never_opened() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("root");
    std::fs::create_dir(&root).expect("root");
    std::fs::write(root.join("in.rs"), blank()).expect("write");
    let secret = dir.path().join("secret.rs");
    std::fs::write(&secret, blank()).expect("write");
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    std::os::unix::fs::symlink(&secret, root.join("escape.rs")).expect("symlink");

    let defining = |path: &str, symbol: &str| doc(path, vec![occ(symbol, DEF, &[0, 0, 1], &[])]);
    let mut escape_unspecified = defining("escape.rs", "s m p 1 e().");
    escape_unspecified.position_encoding =
        protobuf::EnumOrUnknown::new(PositionEncoding::UnspecifiedPositionEncoding);
    let bytes = index(
        "t",
        vec![
            defining("in.rs", "s m p 1 a()."),
            escape_unspecified,
            defining("missing.rs", "s m p 1 m()."),
            defining("/etc/absolute.rs", "s m p 1 x()."),
            defining("../up.rs", "s m p 1 y()."),
        ],
    );
    let sources = Recording {
        inner: FsSource::new(&root).expect("root"),
        requested: std::cell::RefCell::default(),
    };
    let snapshot = gosanja_core::graph::build(&mut &*bytes, &sources, "t").expect("builds");
    assert_eq!(diagnostics(&snapshot), Vec::<String>::new());

    assert_eq!(*sources.requested.borrow(), ["in.rs", "missing.rs"]);
    assert_eq!(entity(&snapshot, "m:p:a().").name, "a");
    let paths: Vec<_> = snapshot.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["in.rs"]);
    let outside = gaps(&snapshot, GapKind::DocumentOutsideRoot);
    let described: Vec<_> = outside
        .iter()
        .map(|g| (g.file.as_deref(), g.count))
        .collect();
    assert_eq!(described, [(None, Some(2)), (Some("escape.rs"), None)]);
    let unavailable = gaps(&snapshot, GapKind::SourceUnavailable);
    assert_eq!(unavailable.len(), 1);
    assert_eq!(unavailable[0].file.as_deref(), Some("missing.rs"));
    assert_eq!(gaps(&snapshot, GapKind::UndecodableDocument).len(), 0);
    assert_eq!(snapshot.entities.len(), 1);
}

// Collisions and repeated observations.

#[test]
fn distinct_definitions_of_one_symbol_form_a_collision_group() {
    let sources = MemSource::with(&[("a.rs", &blank()), ("b.rs", &blank())]);
    let snapshot = both_orders(
        &[
            doc(
                "b.rs",
                vec![
                    occ("s m p 1 a/T#", DEF, &[0, 0, 1], &[0, 0, 5, 0]),
                    occ("s m p 1 a/T#x.", DEF, &[1, 0, 1], &[]),
                ],
            ),
            {
                let mut d = doc(
                    "a.rs",
                    vec![
                        occ("s m p 1 a/T#", DEF, &[2, 0, 1], &[]),
                        occ("s m p 1 a/g().", DEF, &[5, 0, 1], &[5, 0, 9, 0]),
                        occ("s m p 1 a/T#", REF, &[6, 0, 1], &[]),
                    ],
                );
                d.symbols = vec![support::info(
                    "s m p 1 a/T#",
                    scip::types::symbol_information::Kind::Class,
                )];
                d
            },
        ],
        &sources,
    );
    let group = entity(&snapshot, "m:p:a/T#");
    assert!(group.group && group.def.is_none());
    assert_eq!(
        (group.kind.as_deref(), group.src.as_str()),
        (None, "core:identity")
    );
    assert_eq!(
        entity(&snapshot, "m:p:a/T#~1").kind.as_deref(),
        Some("test:class")
    );
    assert_eq!(group.symbol.as_deref(), Some("s m p 1 a/T#"));
    let first = entity(&snapshot, "m:p:a/T#~1");
    let second = entity(&snapshot, "m:p:a/T#~2");
    assert_eq!(first.def.as_ref().map(|d| d.file.as_str()), Some("a.rs"));
    assert_eq!(second.def.as_ref().map(|d| d.file.as_str()), Some("b.rs"));
    assert_eq!(first.member_of.as_deref(), Some("m:p:a/T#"));
    assert_eq!(second.member_of.as_deref(), Some("m:p:a/T#"));
    assert_eq!(
        entity(&snapshot, "m:p:a/T#x.").parent.as_deref(),
        Some("m:p:a/T#")
    );
    let refs = relations(&snapshot, "m:p:a/g().", "m:p:a/T#");
    assert_eq!(refs.len(), 1);
    let collisions = gaps(&snapshot, GapKind::SymbolCollision);
    assert_eq!(collisions.len(), 1);
    assert_eq!(
        collisions[0].keys.as_deref(),
        Some(
            &[
                "m:p:a/T#".to_owned(),
                "m:p:a/T#~1".to_owned(),
                "m:p:a/T#~2".to_owned()
            ][..]
        )
    );
}

#[test]
fn one_definition_observed_twice_is_one_entity() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let twice = doc(
        "a.rs",
        vec![occ("s m p 1 f().", DEF, &[0, 0, 1], &[0, 0, 2, 0])],
    );
    let snapshot = both_orders(&[twice.clone(), twice], &sources);
    assert_eq!(snapshot.entities.len(), 1);
    assert_eq!(snapshot.gaps.len(), 0);
}

// External and local targets, and key assignment.

#[test]
fn undefined_globals_are_externals_and_locals_are_dropped() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let snapshot = both_orders(
        &[doc(
            "a.rs",
            vec![
                occ("s m p 1 f().", DEF, &[0, 0, 1], &[0, 0, 5, 0]),
                occ("s other q 2 g().", REF, &[1, 0, 1], &[]),
                occ("local 3", REF, &[2, 0, 1], &[]),
                occ("not a symbol", REF, &[4, 0, 1], &[]),
                occ("s m p 1 f().[T]", REF, &[3, 0, 1], &[]),
            ],
        )],
        &sources,
    );
    assert_eq!(snapshot.externals.len(), 1);
    let external = &snapshot.externals[0];
    assert_eq!(external.key, "other:q:g().");
    assert_eq!(
        (
            external.package.manager.as_str(),
            external.package.name.as_str(),
            external.package.version.as_str()
        ),
        ("other", "q", "2")
    );
    assert_eq!(relations(&snapshot, "m:p:f().", "other:q:g().").len(), 1);
    assert_eq!(snapshot.relations.len(), 1);
    assert_eq!(snapshot.dropped_local_references, 2);
}

#[test]
fn versions_qualify_keys_without_making_a_collision() {
    let sources = MemSource::with(&[("a.rs", &blank()), ("b.rs", &blank())]);
    let snapshot = both_orders(
        &[
            doc(
                "a.rs",
                vec![
                    occ("s m p 1 a/f().", DEF, &[0, 0, 1], &[0, 0, 9, 0]),
                    occ("s m p 3 a/f().", REF, &[1, 0, 1], &[]),
                    occ("s m p 1 a/f().", REF, &[2, 0, 1], &[]),
                ],
            ),
            doc(
                "b.rs",
                vec![
                    occ("s m p 2 a/f().", DEF, &[0, 0, 1], &[0, 0, 9, 0]),
                    occ("s m p 4 a/f().", REF, &[1, 0, 1], &[]),
                    occ("s m p 2 a/f().", REF, &[2, 0, 1], &[]),
                ],
            ),
        ],
        &sources,
    );
    for v in ["1", "2"] {
        assert_eq!(entity(&snapshot, &format!("m:p:a/f().@{v}")).name, "f");
        assert_eq!(
            entity(&snapshot, &format!("m:p:a/f().@{v}"))
                .parent
                .as_deref(),
            Some(format!("m:p:a/@{v}").as_str())
        );
    }
    let externals: Vec<_> = snapshot.externals.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(externals, ["m:p:a/f().@3", "m:p:a/f().@4"]);
    assert_eq!(
        relations(&snapshot, "m:p:a/f().@1", "m:p:a/f().@3").len(),
        1
    );
    assert_eq!(
        relations(&snapshot, "m:p:a/f().@1", "m:p:a/f().@1").len(),
        1
    );
    assert_eq!(
        relations(&snapshot, "m:p:a/f().@2", "m:p:a/f().@4").len(),
        1
    );
    assert_eq!(
        relations(&snapshot, "m:p:a/f().@2", "m:p:a/f().@2").len(),
        1
    );
    assert_eq!(gaps(&snapshot, GapKind::SymbolCollision).len(), 0);
}

#[test]
fn one_indexed_and_one_external_version_are_both_qualified() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let snapshot = both_orders(
        &[doc(
            "a.rs",
            vec![
                occ("s m p 1 f().", DEF, &[0, 0, 1], &[0, 0, 9, 0]),
                occ("s m p 2 f().", REF, &[1, 0, 1], &[]),
            ],
        )],
        &sources,
    );
    assert_eq!(entity(&snapshot, "m:p:f().@1").name, "f");
    assert_eq!(snapshot.externals[0].key, "m:p:f().@2");
}

#[test]
fn schemes_qualify_keys_only_when_versions_agree() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let snapshot = both_orders(
        &[doc(
            "a.rs",
            vec![
                occ("x m p 1 f().", DEF, &[0, 0, 1], &[]),
                occ("y m p 1 f().", DEF, &[1, 0, 1], &[]),
            ],
        )],
        &sources,
    );
    assert_eq!(
        entity(&snapshot, "m:p:f().@x").symbol.as_deref(),
        Some("x m p 1 f().")
    );
    assert_eq!(
        entity(&snapshot, "m:p:f().@y").symbol.as_deref(),
        Some("y m p 1 f().")
    );
}

#[test]
fn component_boundaries_are_kept_in_keys() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let snapshot = both_orders(
        &[doc(
            "a.rs",
            vec![
                occ("s m p 0 g().", DEF, &[0, 0, 1], &[0, 0, 9, 0]),
                occ("y m p 1@x f().", REF, &[1, 0, 1], &[]),
                occ("x@y m p 1 f().", REF, &[2, 0, 1], &[]),
                occ("s a:b c 1 h().", REF, &[3, 0, 1], &[]),
                occ("s a b:c 1 h().", REF, &[4, 0, 1], &[]),
            ],
        )],
        &sources,
    );
    let expected = [
        ("m:p:f().@1%40x@y", 1),
        ("m:p:f().@1@x%40y", 2),
        ("a%3Ab:c:h().", 3),
        ("a:b%3Ac:h().", 4),
    ];
    for (key, line) in expected {
        let refs = relations(&snapshot, "m:p:g().", key);
        assert_eq!(refs.len(), 1, "{key}");
        assert_eq!(
            refs[0].sites.as_deref(),
            Some(&[site("a.rs", line, 0, line, 1)][..])
        );
    }
}

#[test]
fn an_undefined_container_that_is_referenced_is_a_virtual_entity() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let snapshot = both_orders(
        &[doc(
            "a.rs",
            vec![
                occ("s m p 1 P/f().", DEF, &[0, 0, 1], &[0, 0, 9, 0]),
                occ("s m p 1 P/", REF, &[1, 0, 1], &[]),
            ],
        )],
        &sources,
    );
    let container = entity(&snapshot, "m:p:P/");
    assert!(container.r#virtual);
    assert_eq!(container.symbol.as_deref(), Some("s m p 1 P/"));
    assert_eq!(snapshot.externals.len(), 0);
    assert_eq!(
        entity(&snapshot, "m:p:P/f().").parent.as_deref(),
        Some("m:p:P/")
    );
    assert_eq!(relations(&snapshot, "m:p:P/f().", "m:p:P/").len(), 1);
}

// Ownership from the index.

#[test]
fn the_innermost_body_owns_a_reference() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let snapshot = both_orders(
        &[doc(
            "a.rs",
            vec![
                occ("s m p 1 outer().", DEF, &[0, 0, 1], &[0, 0, 9, 0]),
                occ("s m p 1 outer().inner().", DEF, &[2, 0, 1], &[2, 0, 4, 0]),
                occ("s m p 1 t().", REF, &[3, 0, 1], &[]),
                occ("s m p 1 t().", REF, &[6, 0, 1], &[]),
            ],
        )],
        &sources,
    );
    let inner = relations(&snapshot, "m:p:outer().inner().", "m:p:t().");
    assert_eq!(
        inner[0].sites.as_deref(),
        Some(&[site("a.rs", 3, 0, 3, 1)][..])
    );
    let outer = relations(&snapshot, "m:p:outer().", "m:p:t().");
    assert_eq!(
        outer[0].sites.as_deref(),
        Some(&[site("a.rs", 6, 0, 6, 1)][..])
    );
}

#[test]
fn overlapping_bodies_own_nothing() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let snapshot = both_orders(
        &[doc(
            "a.rs",
            vec![
                occ("s m p 1 a().", DEF, &[0, 0, 1], &[0, 0, 5, 0]),
                occ("s m p 1 b().", DEF, &[3, 0, 1], &[3, 0, 8, 0]),
                occ("s m p 1 c().", DEF, &[9, 0, 1], &[9, 0, 9, 9]),
                occ("s m p 1 d().", DEF, &[9, 2, 3], &[9, 0, 9, 9]),
                occ("s m p 1 t().", REF, &[4, 0, 1], &[]),
                occ("s m p 1 t().", REF, &[9, 4, 5], &[]),
            ],
        )],
        &sources,
    );
    assert!(
        snapshot
            .relations
            .iter()
            .all(|r| r.rel != Rel::Ref || r.to != "m:p:t().")
    );
    let overlaps: Vec<_> = gaps(&snapshot, GapKind::OverlappingBodies)
        .iter()
        .map(|g| g.keys.clone().unwrap_or_default())
        .collect();
    assert_eq!(
        overlaps,
        [
            vec!["m:p:a().".to_owned(), "m:p:b().".to_owned()],
            vec!["m:p:c().".to_owned(), "m:p:d().".to_owned()]
        ]
    );
    let unattributed = gaps(&snapshot, GapKind::UnattributedReferences);
    assert_eq!(unattributed[0].count, Some(2));
}

#[test]
fn without_bodies_references_are_counted_not_guessed() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let snapshot = both_orders(
        &[doc(
            "a.rs",
            vec![
                occ("s m p 1 a().", DEF, &[0, 0, 1], &[]),
                occ("s m p 1 a().", REF, &[1, 0, 1], &[]),
                occ("s m p 1 b().", REF, &[2, 0, 1], &[]),
            ],
        )],
        &sources,
    );
    assert_eq!(snapshot.relations.len(), 0);
    let unattributed = gaps(&snapshot, GapKind::UnattributedReferences);
    assert_eq!(unattributed.len(), 1);
    assert_eq!(
        (unattributed[0].file.as_deref(), unattributed[0].count),
        (Some("a.rs"), Some(2))
    );
}

#[test]
fn a_body_supplied_before_references_takes_part_in_ownership() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let bytes = index(
        "t",
        vec![doc(
            "a.rs",
            vec![
                occ("s m p 1 a().", DEF, &[0, 0, 1], &[]),
                occ("s m p 1 b().", REF, &[1, 0, 1], &[]),
            ],
        )],
    );
    let snapshot = gosanja_core::graph::build_with(&mut &*bytes, &sources, "t", |s| {
        let def = s.entities[0].def.as_mut().expect("def");
        def.body = Some(Range::new(0, 0, 3, 0));
        def.body_src = Some("adapter:fake/syntax".to_owned());
    })
    .expect("builds");
    assert_eq!(diagnostics(&snapshot), Vec::<String>::new());
    assert_eq!(relations(&snapshot, "m:p:a().", "m:p:b().").len(), 1);
    assert_eq!(snapshot.gaps.len(), 0);
}

// The snapshot reader.

/// A snapshot whose rows include values that schema 1.0 does not define.
fn newer_minor_snapshot() -> SnapshotFiles {
    let line = |rows: &[&str]| -> Option<Vec<u8>> {
        Some(
            rows.iter()
                .fold(String::new(), |mut out, r| {
                    out.push_str(r);
                    out.push('\n');
                    out
                })
                .into_bytes(),
        )
    };
    SnapshotFiles {
        manifest: Some(
            br#"{"format":"gosanja-snapshot","schema":"1.1","tool":{"name":"gosanja","version":"9"},"project":{"name":"p"},"scopes":[],"counts":{"files":0,"entities":3,"externals":0,"relations":2,"syntax":0,"gaps":0,"dropped_local_references":0},"new":1}"#
                .to_vec(),
        ),
        files: Some(Vec::new()),
        entities: line(&[
            r#"{"key":"m:p:a/","role":"container","name":"a","virtual":true,"src":"core:containment","new":1}"#,
            r#"{"key":"m:p:a/F#","role":"future","name":"F","parent":"m:p:a/","virtual":true,"src":"core:containment"}"#,
            r#"{"key":"m:p:a/F#x.","role":"member","name":"x","parent":"m:p:a/F#","virtual":true,"src":"core:containment"}"#,
        ]),
        externals: Some(Vec::new()),
        relations: line(&[
            r#"{"from":"m:p:a/","to":"m:p:a/F#","rel":"ref","roles":[],"sites":[],"src":"core:reference-in-body"}"#,
            r#"{"from":"m:p:a/","to":"m:p:a/","rel":"teleports","sites":[],"src":"core:reference-in-body"}"#,
        ]),
        syntax: Some(Vec::new()),
        gaps: Some(Vec::new()),
        unreadable: Vec::new(),
    }
}

#[test]
fn a_newer_minor_skips_unknown_values_and_what_depends_on_them() {
    let read = validate::read(&newer_minor_snapshot(), Version::new(1, 0)).expect("same major");
    let keys: Vec<_> = read.entities.iter().map(|(_, e)| e.key.as_str()).collect();
    assert_eq!(keys, ["m:p:a/"]);
    assert_eq!(read.relations.len(), 0);
    assert_eq!(read.skipped, 4);
    assert!(read.diagnostics.is_empty(), "{:?}", read.diagnostics);
}

#[test]
fn unknown_values_are_malformed_under_an_understood_minor() {
    let read = validate::read(&newer_minor_snapshot(), Version::new(1, 1)).expect("same version");
    assert_eq!(read.skipped, 0);
    let codes: Vec<_> = read.diagnostics.iter().map(|d| (d.file, d.code)).collect();
    assert_eq!(
        codes,
        [
            ("entities.jsonl", "unknown-value"),
            ("relations.jsonl", "unknown-value")
        ]
    );
}

#[test]
fn another_minor_or_major_is_refused_before_reading() {
    let mut files = write::serialise(&build(&index("t", vec![]), &MemSource::default()));
    let own = format!("\"schema\":\"{SCHEMA_VERSION}\"");
    let manifest = String::from_utf8(files.manifest.clone().expect("manifest")).expect("utf-8");
    assert!(manifest.contains(&own), "{manifest}");
    let _own = validate::read(&files, SCHEMA_VERSION).expect("its own version");
    for schema in ["0.2", "0.4", "1.0"] {
        files.manifest = Some(
            manifest
                .replace(&own, &format!("\"schema\":\"{schema}\""))
                .into_bytes(),
        );
        assert!(validate::read(&files, SCHEMA_VERSION).is_err(), "{schema}");
    }
}

#[test]
fn an_enclosing_range_on_a_reference_is_never_converted() {
    let sources = MemSource::with(&[("x.rs", POSITIONS)]);
    let snapshot = build(
        &index(
            "t",
            vec![doc(
                "x.rs",
                vec![
                    occ("s m p 1 g().", DEF, &[1, 3, 4], &[1, 0, 25]),
                    occ("s m p 1 f().", REF, &[1, 20, 21], &[7, 0, 7, 99]),
                ],
            )],
        ),
        &sources,
    );
    assert_eq!(relations(&snapshot, "m:p:g().", "m:p:f().").len(), 1);
    assert_eq!(snapshot.gaps, []);
}

#[test]
fn implementation_relationships_become_implements_rows() {
    let sources = MemSource::with(&[("a.rs", &blank())]);
    let mut document = doc(
        "a.rs",
        vec![
            occ("s m p 1 T#m().", DEF, &[2, 4, 5], &[2, 0, 4, 0]),
            occ("s m p 1 I#m().", DEF, &[6, 4, 5], &[]),
        ],
    );
    document.symbols = vec![support::implementing(
        "s m p 1 T#m().",
        &["s m p 1 I#m().", "s other q 1 J#m()."],
    )];
    let snapshot = both_orders(&[document], &sources);
    for to in ["m:p:I#m().", "other:q:J#m()."] {
        let rows = relations(&snapshot, "m:p:T#m().", to);
        assert_eq!(rows.len(), 1, "{to}");
        assert_eq!(rows[0].rel, Rel::Implements);
        assert_eq!(
            rows[0].sites.as_deref(),
            Some(&[site("a.rs", 2, 4, 2, 5)][..])
        );
        assert_eq!(rows[0].src, "scip:t");
    }
    let externals: Vec<_> = snapshot
        .externals
        .iter()
        .map(|e| e.symbol.as_str())
        .collect();
    assert_eq!(externals, ["s other q 1 J#m()."]);
    assert!(snapshot.entities.iter().all(|e| e.name != "OnlyReferenced"));
}

#[test]
fn each_document_takes_the_tool_of_the_metadata_before_it() {
    let sources = MemSource::with(&[("a.rs", &blank()), ("b.rs", &blank())]);
    let mut bytes = index(
        "first",
        vec![doc(
            "a.rs",
            vec![occ("s m p 1 a().", DEF, &[0, 0, 1], &[0, 0, 2, 0])],
        )],
    );
    bytes.extend(index(
        "second",
        vec![doc(
            "b.rs",
            vec![
                occ("s m p 1 b().", DEF, &[0, 0, 1], &[0, 0, 2, 0]),
                occ("s other q 1 x().", REF, &[1, 0, 1], &[]),
                occ("s m p 1 b().", REF, &[30, 0, 1], &[]),
            ],
        )],
    ));
    let snapshot = build(&bytes, &sources);
    let a = entity(&snapshot, "m:p:a().");
    let b = entity(&snapshot, "m:p:b().");
    assert_eq!(
        (a.src.as_str(), b.src.as_str()),
        ("scip:first", "scip:second")
    );
    assert_eq!(
        a.def.as_ref().and_then(|d| d.body_src.as_deref()),
        Some("scip:first")
    );
    assert_eq!(
        b.def.as_ref().and_then(|d| d.body_src.as_deref()),
        Some("scip:second")
    );
    assert_eq!(snapshot.externals[0].src, "scip:second");
    assert_eq!(
        gaps(&snapshot, GapKind::PositionOutOfRange)[0].src,
        "scip:second"
    );
    let tools: Vec<_> = snapshot
        .scope
        .indexers
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(tools, ["first", "second"]);
}

#[test]
fn a_colon_in_a_file_name_is_an_ordinary_path() {
    let sources = MemSource::with(&[("1:b", &blank())]);
    let snapshot = build(
        &index(
            "t",
            vec![doc("1:b", vec![occ("s m p 1 a().", DEF, &[0, 0, 1], &[])])],
        ),
        &sources,
    );
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(gaps(&snapshot, GapKind::DocumentOutsideRoot).len(), 0);
}

#[test]
fn rows_left_dangling_by_the_writer_are_kept_for_the_validator() {
    let mut files = newer_minor_snapshot();
    let mut relations = files.relations.take().expect("relations");
    relations.extend_from_slice(
        b"{\"from\":\"m:p:a/\",\"to\":\"m:p:nowhere/\",\"rel\":\"ref\",\"roles\":[],\"sites\":[],\"src\":\"core:reference-in-body\"}\n",
    );
    files.relations = Some(relations);
    let read = validate::read(&files, Version::new(1, 0)).expect("same major");
    let kept: Vec<_> = read.relations.iter().map(|(_, r)| r.to.as_str()).collect();
    assert_eq!(kept, ["m:p:nowhere/"]);
    assert_eq!(read.skipped, 4);
}

#[test]
fn a_skipped_syntax_row_does_not_remove_its_entity_or_relations() {
    let mut files = newer_minor_snapshot();
    files.syntax = Some(
        b"{\"key\":\"m:p:a/\",\"outline\":[{\"k\":\"future\",\"range\":[0,0,0,0]}],\"src\":\"adapter:x/syntax\"}\n"
            .to_vec(),
    );
    let mut relations = files.relations.take().expect("relations");
    relations.extend_from_slice(
        b"{\"from\":\"m:p:a/\",\"to\":\"m:p:a/\",\"rel\":\"ref\",\"roles\":[],\"sites\":[],\"src\":\"core:reference-in-body\"}\n",
    );
    files.relations = Some(relations);
    let read = validate::read(&files, Version::new(1, 0)).expect("same major");
    let entities: Vec<_> = read.entities.iter().map(|(_, e)| e.name.as_str()).collect();
    assert_eq!(entities, ["a"]);
    let kept: Vec<_> = read
        .relations
        .iter()
        .map(|(_, r)| (r.from.as_str(), r.to.as_str()))
        .collect();
    assert_eq!(kept, [("m:p:a/", "m:p:a/")]);
    assert_eq!(read.syntax.len(), 0);
    assert_eq!(read.skipped, 5);
}

#[test]
fn an_unknown_value_with_another_schema_error_is_malformed() {
    let mut files = newer_minor_snapshot();
    let mut entities = files.entities.take().expect("entities");
    entities.extend_from_slice(
        b"{\"key\":\"m:p:b/\",\"role\":\"future\",\"name\":\"b\",\"virtual\":true,\"src\":\"not a provenance\"}\n",
    );
    files.entities = Some(entities);
    let read = validate::read(&files, Version::new(1, 0)).expect("same major");
    assert_eq!(
        read.skipped, 4,
        "only the rows whose sole fault is the unknown value"
    );
    let codes: Vec<_> = read
        .diagnostics
        .iter()
        .map(|d| (d.file, d.line, d.code))
        .collect();
    assert_eq!(codes, [("entities.jsonl", Some(4), "schema")]);
}

#[test]
fn implements_sites_come_from_the_document_that_states_it() {
    let sources = MemSource::with(&[("a.rs", &blank()), ("b.rs", &blank())]);
    let silent = doc("a.rs", vec![occ("s m p 1 T#m().", DEF, &[2, 4, 5], &[])]);
    let mut stating = doc(
        "b.rs",
        vec![
            occ("s m p 1 T#m().", DEF, &[3, 4, 5], &[]),
            occ("s m p 1 I#m().", DEF, &[6, 4, 5], &[]),
        ],
    );
    stating.symbols = vec![support::implementing("s m p 1 T#m().", &["s m p 1 I#m()."])];
    let snapshot = both_orders(&[silent, stating], &sources);
    let rows: Vec<_> = snapshot
        .relations
        .iter()
        .filter(|r| r.rel == Rel::Implements)
        .map(|r| (r.from.as_str(), r.to.as_str(), r.sites.clone()))
        .collect();
    assert_eq!(
        rows,
        [(
            "m:p:T#m().~2",
            "m:p:I#m().",
            Some(vec![site("b.rs", 3, 4, 3, 5)])
        )]
    );
}

#[test]
fn a_statement_in_a_document_defining_the_symbol_twice_comes_from_the_group() {
    let sources = MemSource::with(&[("a.rs", &blank()), ("b.rs", &blank())]);
    let mut stating = doc(
        "a.rs",
        vec![
            occ("s m p 1 T#", DEF, &[1, 0, 1], &[]),
            occ("s m p 1 T#", DEF, &[5, 0, 1], &[]),
            occ("s m p 1 I#", DEF, &[8, 0, 1], &[]),
        ],
    );
    stating.symbols = vec![support::implementing("s m p 1 T#", &["s m p 1 I#"])];
    let silent = doc("b.rs", vec![occ("s m p 1 T#", DEF, &[2, 0, 1], &[])]);
    let snapshot = both_orders(&[stating, silent], &sources);
    let rows: Vec<_> = snapshot
        .relations
        .iter()
        .filter(|r| r.rel == Rel::Implements)
        .map(|r| (r.from.as_str(), r.to.as_str(), r.sites.clone()))
        .collect();
    assert_eq!(
        rows,
        [(
            "m:p:T#",
            "m:p:I#",
            Some(vec![site("a.rs", 1, 0, 1, 1), site("a.rs", 5, 0, 5, 1)])
        )]
    );
}

#[test]
fn a_statement_in_a_document_without_the_definition_makes_no_row() {
    let sources = MemSource::with(&[("a.rs", &blank()), ("b.rs", &blank())]);
    let defining = doc(
        "a.rs",
        vec![
            occ("s m p 1 T#", DEF, &[1, 0, 1], &[]),
            occ("s m p 1 I#", DEF, &[2, 0, 1], &[]),
        ],
    );
    let mut stating = doc("b.rs", vec![occ("s m p 1 T#", REF, &[1, 0, 1], &[])]);
    stating.symbols = vec![support::implementing("s m p 1 T#", &["s m p 1 I#"])];
    let snapshot = both_orders(&[defining, stating], &sources);
    assert_eq!(
        snapshot
            .relations
            .iter()
            .filter(|r| r.rel == Rel::Implements)
            .count(),
        0
    );
}
