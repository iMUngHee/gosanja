//! The validator and the snapshot reader, one fault at a time: each snapshot here differs
//! from a valid one in exactly one way, and the test names the diagnostic that way must
//! produce.

mod support;

use gosanja_core::validate::{self, Diagnostic};
use gosanja_core::write::{self, FsSource, SnapshotFiles};
use gosanja_schema::compat::Version;
use support::{DEF, MemSource, REF, build, doc, fixture, implementing, index, occ, read_dir};

fn golden() -> SnapshotFiles {
    read_dir(&fixture("rust-basic").join("expected"))
}

fn slot<'a>(files: &'a mut SnapshotFiles, name: &str) -> &'a mut Option<Vec<u8>> {
    match name {
        "manifest.json" => &mut files.manifest,
        "files.jsonl" => &mut files.files,
        "entities.jsonl" => &mut files.entities,
        "externals.jsonl" => &mut files.externals,
        "relations.jsonl" => &mut files.relations,
        "syntax.jsonl" => &mut files.syntax,
        "gaps.jsonl" => &mut files.gaps,
        _ => panic!("no snapshot file {name}"),
    }
}

/// `files` with one of its files rewritten by `edit`, which must change it.
fn with(mut files: SnapshotFiles, name: &str, edit: impl Fn(&str) -> String) -> SnapshotFiles {
    let slot = slot(&mut files, name);
    let before = String::from_utf8(slot.take().unwrap_or_default()).expect("utf-8");
    let after = edit(&before);
    assert_ne!(before, after, "the edit changed nothing in {name}");
    *slot = Some(after.into_bytes());
    files
}

/// `files` with `rows` appended to one table.
fn appended(files: SnapshotFiles, name: &str, rows: &[&str]) -> SnapshotFiles {
    with(files, name, |t| {
        rows.iter().fold(t.to_owned(), |mut out, row| {
            out.push_str(row);
            out.push('\n');
            out
        })
    })
}

/// Lines as a JSON Lines file: each followed by a newline.
fn joined(lines: &[&str]) -> String {
    lines.iter().fold(String::new(), |mut out, line| {
        out.push_str(line);
        out.push('\n');
        out
    })
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<(&'static str, &'static str)> {
    diagnostics.iter().map(|d| (d.file, d.code)).collect()
}

fn check(files: &SnapshotFiles) -> Vec<(&'static str, &'static str)> {
    codes(&validate::validate(files, None).expect("own version"))
}

fn check_with_root(files: &SnapshotFiles) -> Vec<(&'static str, &'static str)> {
    let root = FsSource::new(&fixture("rust-basic")).expect("root");
    codes(&validate::validate(files, Some(&root)).expect("own version"))
}

#[test]
fn the_golden_is_valid_with_and_without_its_sources() {
    assert_eq!(check(&golden()), []);
    assert_eq!(check_with_root(&golden()), []);
}

// manifest.json

#[test]
fn a_manifest_that_fails_its_schema_is_reported() {
    let files = with(golden(), "manifest.json", |t| {
        t.replace(r#""path":".""#, r#""path":"/abs""#)
    });
    assert_eq!(check(&files), [("manifest.json", "schema")]);
}

#[test]
fn counts_and_format_are_checked() {
    let files = with(golden(), "manifest.json", |t| {
        t.replace(r#""entities":13"#, r#""entities":12"#)
    });
    assert_eq!(check(&files), [("manifest.json", "count-mismatch")]);
}

// Missing and unreadable files.

#[test]
fn a_missing_table_is_reported_once_and_an_unreadable_one_only_as_unreadable() {
    let mut files = golden();
    files.syntax = None;
    files.gaps = None;
    files
        .unreadable
        .push(("gaps.jsonl", "a directory".to_owned()));
    assert_eq!(
        check(&files),
        [
            ("gaps.jsonl", "unreadable"),
            ("syntax.jsonl", "missing-file")
        ]
    );
}

#[cfg(unix)]
#[test]
fn reading_a_directory_tells_a_missing_table_from_an_unreadable_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, bytes) in golden().named() {
        std::fs::write(dir.path().join(name), bytes.expect("golden file")).expect("write");
    }
    std::fs::remove_file(dir.path().join("syntax.jsonl")).expect("remove");
    std::fs::remove_file(dir.path().join("gaps.jsonl")).expect("remove");
    std::fs::create_dir(dir.path().join("gaps.jsonl")).expect("a directory in its place");
    let mut files = SnapshotFiles::read(dir.path());
    let unreadable: Vec<_> = files.unreadable.iter().map(|(name, _)| *name).collect();
    assert_eq!(unreadable, ["gaps.jsonl"]);
    assert_eq!(
        check(&files),
        [
            ("gaps.jsonl", "unreadable"),
            ("syntax.jsonl", "missing-file")
        ]
    );

    std::fs::remove_dir(dir.path().join("gaps.jsonl")).expect("remove dir");
    std::fs::write(dir.path().join("gaps.jsonl"), "").expect("write");
    std::fs::write(dir.path().join("syntax.jsonl"), "").expect("write");
    files.read_tables(dir.path());
    assert_eq!(
        files.unreadable,
        [],
        "a reread replaces the table diagnostics"
    );
    assert_eq!(check(&files), []);
}

// Order and uniqueness.

#[test]
fn rows_out_of_order_and_repeated_keys_are_reported() {
    let swapped = with(golden(), "entities.jsonl", |t| {
        let mut lines: Vec<&str> = t.lines().collect();
        lines.swap(0, 1);
        joined(&lines)
    });
    assert_eq!(check(&swapped), [("entities.jsonl", "unsorted")]);

    let repeated = with(golden(), "externals.jsonl", |t| {
        let first = t.lines().next().expect("a row");
        format!("{first}\n{t}")
    });
    assert!(check(&repeated).contains(&("externals.jsonl", "duplicate-key")));
}

#[test]
fn an_external_that_is_also_an_entity_is_reported() {
    let files = with(golden(), "externals.jsonl", |t| {
        t.replace(
            r#""key":"cargo:std:crate/""#,
            r#""key":"cargo:demo:views/render().""#,
        )
    });
    assert!(check(&files).contains(&("externals.jsonl", "duplicate-key")));
}

/// Two `ref` rows that differ only in `roles` share their sort key, so their bytes decide.
#[test]
fn ties_in_the_sort_key_are_broken_by_the_bytes_of_the_row() {
    let read = r#"{"from":"cargo:demo:crate/","to":"cargo:demo:views/","rel":"ref","roles":["read"],"sites":[["src/lib.rs",3,0,3,1]],"src":"core:reference-in-body"}"#;
    let plain = r#"{"from":"cargo:demo:crate/","to":"cargo:demo:views/","rel":"ref","roles":[],"sites":[["src/lib.rs",2,8,2,13],["src/lib.rs",4,8,4,13]],"src":"core:reference-in-body"}"#;
    let ordered = with(golden(), "relations.jsonl", |t| {
        t.replacen(plain, &format!("{read}\n{plain}"), 1)
    });
    let ordered = with(ordered, "manifest.json", |t| {
        t.replace(r#""relations":27"#, r#""relations":28"#)
    });
    assert_eq!(check(&ordered), []);
    let reversed = with(ordered.clone(), "relations.jsonl", |t| {
        t.replacen(&format!("{read}\n{plain}"), &format!("{plain}\n{read}"), 1)
    });
    assert_eq!(check(&reversed), [("relations.jsonl", "unsorted")]);
}

#[test]
fn a_relation_written_twice_is_reported_and_a_repeated_gap_is_not() {
    let twice = with(golden(), "relations.jsonl", |t| {
        let first = t.lines().next().expect("a row");
        format!("{first}\n{t}")
    });
    let twice = with(twice, "manifest.json", |t| {
        t.replace(r#""relations":27"#, r#""relations":28"#)
    });
    assert_eq!(check(&twice), [("relations.jsonl", "duplicate-row")]);

    let gap = r#"{"gap":"unattributed-references","file":"src/lib.rs","count":1,"src":"core:reference-in-body"}"#;
    let gaps = appended(golden(), "gaps.jsonl", &[gap, gap]);
    let gaps = with(gaps, "manifest.json", |t| {
        t.replace(r#""gaps":0"#, r#""gaps":2"#)
    });
    assert_eq!(check(&gaps), []);
}

/// The sort key of `relations.jsonl` is (`from`, `rel`, `to`, …), which is not the byte order
/// of a row that starts `from`, `to`, `rel`.
#[test]
fn relations_sort_by_rel_before_to() {
    let sources = MemSource::with(&[("a.rs", &format!("{}\n", "x".repeat(40)).repeat(10))]);
    let mut document = doc(
        "a.rs",
        vec![
            occ("s m p 1 T#m().", DEF, &[0, 0, 1], &[0, 0, 5, 0]),
            occ("s m p 1 A#", REF, &[1, 0, 1], &[]),
        ],
    );
    document.symbols = vec![implementing("s m p 1 T#m().", &["s m p 1 Z#"])];
    let files = write::serialise(&build(&index("t", vec![document]), &sources));
    let text = String::from_utf8(files.relations.clone().expect("relations")).expect("utf-8");
    let rels: Vec<_> = text
        .lines()
        .map(|l| (l.contains("\"implements\""), l.contains("m:p:Z#")))
        .collect();
    assert_eq!(rels, [(true, true), (false, false)]);
    let byte_order = with(files, "relations.jsonl", |t| {
        let mut lines: Vec<&str> = t.lines().collect();
        lines.reverse();
        joined(&lines)
    });
    assert_eq!(check(&byte_order), [("relations.jsonl", "unsorted")]);
}

// Sources.

#[test]
fn each_recorded_property_of_a_file_is_compared_with_the_source() {
    let views =
        r#""path":"src/views.rs","language":"rust","bytes":630,"lines":33,"sha256":"d6a8125b"#;
    for wrong in [
        r#""path":"src/views.rs","language":"rust","bytes":631,"lines":33,"sha256":"d6a8125b"#,
        r#""path":"src/views.rs","language":"rust","bytes":630,"lines":34,"sha256":"d6a8125b"#,
        r#""path":"src/views.rs","language":"rust","bytes":630,"lines":33,"sha256":"e6a8125b"#,
    ] {
        let files = with(golden(), "files.jsonl", |t| t.replace(views, wrong));
        assert_eq!(
            check_with_root(&files),
            [("files.jsonl", "source-mismatch")],
            "{wrong}"
        );
    }
    let upper = with(golden(), "files.jsonl", |t| {
        t.replace(
            "d6a8125b599ff2ac0dfbfaa4297a9e23",
            "D6A8125B599FF2AC0DFBFAA4297A9E23",
        )
    });
    assert_eq!(check_with_root(&upper), []);
}

// Positions.

#[test]
fn a_position_must_lie_in_its_file() {
    let site = r#"["src/views.rs",4,8,4,13]"#;
    let at = |range: &str| {
        with(golden(), "relations.jsonl", |t| {
            t.replace(site, &format!(r#"["src/views.rs",{range}]"#))
        })
    };
    // views.rs has 33 lines, so line 33 exists only as the position after the last newline.
    assert_eq!(check(&at("32,0,33,0")), []);
    for outside in [
        "33,1,33,1",
        "34,0,34,0",
        "4,13,4,8",
        "4,8,40,0",
        "40,0,40,1",
    ] {
        assert_eq!(
            check(&at(outside)),
            [("relations.jsonl", "position-outside-file")],
            "{outside}"
        );
    }
    let inside_a_character = at("15,6,15,7");
    assert_eq!(check(&inside_a_character), []);
    assert_eq!(
        check_with_root(&inside_a_character),
        [("relations.jsonl", "position-outside-file")]
    );
}

#[test]
fn a_gap_must_name_listed_files_and_existing_rows() {
    let gap = |row: &str| {
        let files = appended(golden(), "gaps.jsonl", &[row]);
        with(files, "manifest.json", |t| {
            t.replace(r#""gaps":0"#, r#""gaps":1"#)
        })
    };
    // A document gap may name a file that was never indexed; a gap with a range may not.
    let unindexed =
        gap(r#"{"gap":"source-unavailable","file":"src/other.rs","src":"scip:rust-analyzer"}"#);
    assert_eq!(check(&unindexed), []);
    let unlisted = gap(
        r#"{"gap":"open-call","file":"src/other.rs","range":[0,0,0,1],"layer":"syntax","src":"adapter:x/syntax"}"#,
    );
    assert_eq!(check(&unlisted), [("gaps.jsonl", "unknown-file")]);
    let dangling = gap(
        r#"{"gap":"symbol-collision","keys":["cargo:demo:x/","cargo:demo:x/~1","cargo:demo:x/~2"],"src":"core:identity"}"#,
    );
    assert_eq!(
        check(&dangling),
        [("gaps.jsonl", "dangling-key"); 3],
        "one per key"
    );
    let outside = gap(
        r#"{"gap":"open-call","file":"src/lib.rs","range":[9,0,9,1],"layer":"syntax","src":"adapter:x/syntax"}"#,
    );
    assert_eq!(check(&outside), [("gaps.jsonl", "position-outside-file")]);
}

#[test]
fn a_syntax_row_must_describe_an_entity() {
    let row = |key: &str| {
        let files = appended(
            golden(),
            "syntax.jsonl",
            &[&format!(r#"{{"key":"{key}","src":"adapter:x/syntax"}}"#)],
        );
        with(files, "manifest.json", |t| {
            t.replace(r#""syntax":0"#, r#""syntax":1"#)
        })
    };
    assert_eq!(check(&row("cargo:demo:views/render().")), []);
    assert_eq!(
        check(&row("cargo:demo:nowhere/")),
        [("syntax.jsonl", "dangling-key")]
    );
}

// Entity variants.

/// A snapshot with a collision group, its members and a virtual container, from the engine.
fn variants() -> SnapshotFiles {
    let text = format!("{}\n", "x".repeat(40)).repeat(10);
    let sources = MemSource::with(&[("a.rs", &text), ("b.rs", &text)]);
    write::serialise(&build(
        &index(
            "t",
            vec![
                doc("a.rs", vec![occ("s m p 1 P/f().", DEF, &[0, 0, 1], &[])]),
                doc("b.rs", vec![occ("s m p 1 P/f().", DEF, &[0, 0, 1], &[])]),
            ],
        ),
        &sources,
    ))
}

/// The entity schema states the four variants, so a row that is none of them fails it.
#[test]
fn each_entity_variant_needs_its_own_fields() {
    assert_eq!(check(&variants()), []);
    let group = r#"{"key":"m:p:P/f().","symbol":"s m p 1 P/f().","role":"callable","name":"f","parent":"m:p:P/","group":true,"src":"core:identity"}"#;
    let virtual_row =
        r#"{"key":"m:p:P/","role":"container","name":"P","virtual":true,"src":"core:containment"}"#;
    let member = r#"{"key":"m:p:P/f().~1","symbol":"s m p 1 P/f().","role":"callable","name":"f","parent":"m:p:P/","member_of":"m:p:P/f().","def":{"file":"a.rs","name":[0,0,0,1]},"src":"scip:t"}"#;
    let def = r#","def":{"file":"a.rs","name":[0,0,0,1]}"#;
    let cases = [
        (group, group.replace(r#""symbol":"s m p 1 P/f().","#, "")),
        (
            group,
            group.replace(r#","group":true"#, &format!(r#","group":true{def}"#)),
        ),
        (
            virtual_row,
            virtual_row.replace(r#""role":"container","#, ""),
        ),
        (
            virtual_row,
            virtual_row.replace(r#","virtual":true"#, &format!(r#","virtual":true{def}"#)),
        ),
        (member, member.replace(r#""symbol":"s m p 1 P/f().","#, "")),
        (member, member.replace(r#""role":"callable","#, "")),
        (member, member.replace(def, "")),
    ];
    for (row, broken) in cases {
        let files = with(variants(), "entities.jsonl", |t| t.replace(row, &broken));
        assert!(
            check(&files).contains(&("entities.jsonl", "schema")),
            "{broken}"
        );
    }
}

#[test]
fn parents_and_groups_must_exist() {
    let files = with(golden(), "entities.jsonl", |t| {
        t.replacen(
            r#""parent":"cargo:demo:views/""#,
            r#""parent":"cargo:demo:gone/""#,
            1,
        )
    });
    assert_eq!(check(&files), [("entities.jsonl", "dangling-key")]);
}

// The reader under a newer minor.

/// A schema-1.1 snapshot whose entity `m:p:a/F#` uses a role 1.0 does not define.
fn newer(extra: &[(&str, &str)]) -> SnapshotFiles {
    let mut files = SnapshotFiles {
        manifest: Some(
            br#"{"format":"gosanja-snapshot","schema":"1.1","tool":{"name":"gosanja","version":"9"},"project":{"name":"p"},"scopes":[{"path":".","indexers":[],"layers":{"base":"present","syntax":"partial","calls":"absent"},"builds":[]}],"counts":{"files":0,"entities":0,"externals":0,"relations":0,"syntax":0,"gaps":0,"dropped_local_references":0}}"#
                .to_vec(),
        ),
        files: Some(Vec::new()),
        entities: Some(
            concat!(
                r#"{"key":"m:p:a/","role":"container","name":"a","virtual":true,"src":"core:containment"}"#,
                "\n",
                r#"{"key":"m:p:a/F#","role":"future","name":"F","parent":"m:p:a/","virtual":true,"src":"core:containment"}"#,
                "\n",
            )
            .as_bytes()
            .to_vec(),
        ),
        externals: Some(Vec::new()),
        relations: Some(Vec::new()),
        syntax: Some(Vec::new()),
        gaps: Some(Vec::new()),
        unreadable: Vec::new(),
    };
    for (name, row) in extra {
        files = appended(files, name, &[row]);
    }
    files
}

fn read(files: &SnapshotFiles, minor: u32) -> validate::ReadSnapshot {
    validate::read(files, Version::new(1, minor)).expect("same major")
}

#[test]
fn a_newer_manifest_is_not_held_to_this_schema() {
    assert_eq!(codes(&read(&newer(&[]), 0).diagnostics), []);
}

#[test]
fn every_closed_field_of_a_relation_is_checked_for_unknown_values() {
    let rows = [
        r#"{"from":"m:p:a/","to":"m:p:a/","rel":"ref","roles":["future"],"sites":[],"src":"core:reference-in-body"}"#,
        r#"{"from":"m:p:a/","to":"m:p:a/","rel":"call","mechanism":"future","resolution":"direct","evidence":[{}],"src":"core:reference-in-body"}"#,
        r#"{"from":"m:p:a/","to":"m:p:a/","rel":"call","mechanism":"static","resolution":"future","evidence":[{}],"src":"core:reference-in-body"}"#,
    ];
    for row in rows {
        let files = newer(&[("relations.jsonl", row)]);
        let newer_reader = read(&files, 0);
        assert_eq!(
            (newer_reader.relations.len(), newer_reader.skipped),
            (0, 2),
            "{row}"
        );
        let same = read(&files, 1);
        assert!(
            codes(&same.diagnostics).contains(&("relations.jsonl", "unknown-value")),
            "{row}"
        );
    }
}

#[test]
fn unknown_gap_layers_and_nested_outline_kinds_are_unknown_values() {
    let gap = r#"{"gap":"symbol-collision","keys":["m:p:a/","m:p:a/~1","m:p:a/~2"],"layer":"future","src":"core:identity"}"#;
    let syntax = r#"{"key":"m:p:a/","outline":[{"k":"branch","range":[0,0,0,1],"children":[{"k":"future","range":[0,0,0,1]}]}],"src":"adapter:x/syntax"}"#;
    for (name, row) in [("gaps.jsonl", gap), ("syntax.jsonl", syntax)] {
        let read = read(&newer(&[(name, row)]), 0);
        assert_eq!(read.skipped, 2, "{name}");
        assert_eq!((read.gaps.len(), read.syntax.len()), (0, 0), "{name}");
        assert_eq!(read.entities.len(), 1, "{name}");
    }
}

#[test]
fn everything_that_depends_on_a_skipped_row_is_skipped_and_counted() {
    let files = newer(&[
        (
            "relations.jsonl",
            r#"{"from":"m:p:a/","to":"m:p:a/F#","rel":"ref","roles":[],"sites":[],"src":"core:reference-in-body"}"#,
        ),
        (
            "relations.jsonl",
            r#"{"from":"m:p:a/F#","to":"m:p:a/","rel":"ref","roles":[],"sites":[],"src":"core:reference-in-body"}"#,
        ),
        (
            "relations.jsonl",
            r#"{"from":"m:p:a/","to":"m:p:a/","rel":"ref","roles":[],"sites":[],"src":"core:reference-in-body"}"#,
        ),
        (
            "syntax.jsonl",
            r#"{"key":"m:p:a/F#","src":"adapter:x/syntax"}"#,
        ),
        (
            "syntax.jsonl",
            r#"{"key":"m:p:a/","src":"adapter:x/syntax"}"#,
        ),
        (
            "gaps.jsonl",
            r#"{"gap":"symbol-collision","keys":["m:p:a/F#","m:p:a/F#~1","m:p:a/F#~2"],"src":"core:identity"}"#,
        ),
        (
            "gaps.jsonl",
            r#"{"gap":"symbol-collision","keys":["m:p:a/","m:p:a/~1","m:p:a/~2"],"src":"core:identity"}"#,
        ),
        (
            "gaps.jsonl",
            r#"{"gap":"symbol-collision","keys":["m:p:a/","m:p:a/~3","m:p:a/~4"],"src":"core:identity"}"#,
        ),
    ]);
    let read = read(&files, 0);
    assert_eq!(read.entities.len(), 1);
    assert_eq!(
        (read.relations.len(), read.syntax.len(), read.gaps.len()),
        (1, 1, 2)
    );
    // The entity itself, two relations, one syntax row and one gap.
    assert_eq!(read.skipped, 5);
}
