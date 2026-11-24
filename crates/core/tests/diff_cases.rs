//! Public diff input validation and fixture conformance.
mod support;
use gosanja_core::diff::compare;
use gosanja_core::write::SnapshotFiles;
use serde_json::to_value;
use support::{fixture, read_dir};

fn jsonl<T: serde::Serialize>(rows: &[T]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for row in rows {
        serde_json::to_writer(&mut bytes, row).unwrap();
        bytes.push(b'\n');
    }
    bytes
}

fn materialize(files: &SnapshotFiles) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in files.named() {
        if let Some(bytes) = bytes {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
    }
    dir
}

#[test]
fn manifest_counts_match_the_emitted_rows_with_and_without_excluded_layers() {
    use gosanja_schema::diff::{Counts, Excluded};
    let root = fixture("diff-basic");
    let new = root.join("new/expected");
    let mut calls_only = read_dir(&fixture("rust-basic").join("expected"));
    let mut manifest: serde_json::Value =
        serde_json::from_slice(calls_only.manifest.as_ref().unwrap()).unwrap();
    manifest["scopes"][0]["layers"]["calls"] = "present".into();
    calls_only.manifest = Some(serde_json::to_vec(&manifest).unwrap());

    let calls_only = materialize(&calls_only);
    for (old, excluded) in [
        (root.join("old/expected"), vec![Excluded::Calls]),
        (
            calls_only.path().to_owned(),
            vec![Excluded::Calls, Excluded::Syntax],
        ),
    ] {
        let result = compare(&old, &new).unwrap();
        assert_eq!(result.manifest.not_compared, excluded);
        let mut expected = to_value(Counts::default()).unwrap();
        for (table, rows) in [
            ("entities", to_value(result.entities).unwrap()),
            ("externals", to_value(result.externals).unwrap()),
            ("relations", to_value(result.relations).unwrap()),
        ] {
            for row in rows.as_array().unwrap() {
                let count = &mut expected[table][row["change"].as_str().unwrap()];
                *count = (count.as_u64().unwrap() + 1).into();
            }
        }
        expected["candidates"] = result.candidates.len().into();
        assert_eq!(to_value(result.manifest.counts).unwrap(), expected);
    }
}

#[test]
fn fixture_change_rows_match_the_hand_derived_rows_byte_for_byte() {
    let root = fixture("diff-basic");
    let load = |side: &str| root.join(side).join("expected");
    let comparison = compare(&load("old"), &load("new")).unwrap();
    for (file, actual) in [
        ("entities", jsonl(&comparison.entities)),
        ("externals", jsonl(&comparison.externals)),
        ("relations", jsonl(&comparison.relations)),
        ("candidates", jsonl(&comparison.candidates)),
    ] {
        let expected = std::fs::read(root.join("expected").join(format!("{file}.jsonl"))).unwrap();
        assert_eq!(
            String::from_utf8(actual).unwrap(),
            String::from_utf8(expected).unwrap(),
            "{file}"
        );
    }
    for row in to_value(&comparison.entities)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .chain(to_value(&comparison.externals).unwrap().as_array().unwrap())
    {
        if row["change"] == "rekeyed" {
            assert_ne!(row["old"], row["new"]);
        }
    }
    let first: Vec<_> = comparison
        .candidates
        .iter()
        .filter(|candidate| candidate.rank == 1)
        .map(|candidate| {
            (
                candidate.old.as_str(),
                candidate.new.as_str(),
                [
                    candidate.name,
                    candidate.file,
                    candidate.parents,
                    candidate.size,
                    candidate.outgoing,
                    candidate.incoming,
                    candidate.score,
                ],
            )
        })
        .collect();
    for expected in [
        (
            "cargo:diff-basic:mover().",
            "cargo:diff-basic:util/mover().",
            [1_000_000, 0, 0, 1_000_000, 1_000_000, 1_000_000, 640_000],
        ),
        (
            "cargo:diff-basic:old_name().",
            "cargo:diff-basic:new_name().",
            [
                625_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 0, 797_500,
            ],
        ),
        (
            "cargo:diff-basic:wander().",
            "cargo:diff-basic:inner/meander().",
            [
                714_285, 1_000_000, 0, 1_000_000, 1_000_000, 1_000_000, 734_285,
            ],
        ),
    ] {
        assert!(first.contains(&expected));
    }
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("expected/manifest.json")).unwrap())
            .unwrap();
    assert_eq!(to_value(comparison.manifest).unwrap(), expected);
}

#[test]
fn diff_types_read_and_write_every_example_row_in_field_order() {
    use gosanja_schema::diff::{
        CandidateRow, EntityRow as EntityChange, ExternalRow, Manifest,
        RelationRow as RelationChange,
    };
    fn roundtrip<T: serde::de::DeserializeOwned + serde::Serialize>(text: &str) {
        for line in text.lines() {
            let row: T = serde_json::from_str(line).unwrap();
            assert_eq!(serde_json::to_string(&row).unwrap(), line);
        }
    }
    let root = fixture("diff-basic").join("expected");
    let file = |name| std::fs::read_to_string(root.join(name)).unwrap();
    roundtrip::<Manifest>(&file("manifest.json"));
    roundtrip::<EntityChange>(&file("entities.jsonl"));
    roundtrip::<ExternalRow>(&file("externals.jsonl"));
    roundtrip::<RelationChange>(&file("relations.jsonl"));
    roundtrip::<CandidateRow>(&file("candidates.jsonl"));
}

#[test]
fn public_base64_list_reproduces_the_measured_ranks() {
    use sha2::{Digest, Sha256};
    let root = fixture("diff-public");
    let labels = std::fs::read_to_string(root.join("truth.tsv")).unwrap();
    assert_eq!(
        Sha256::digest(labels.as_bytes()).as_slice(),
        &[
            0xbe, 0x21, 0xeb, 0x37, 0x29, 0xbe, 0x71, 0xbc, 0xb2, 0x24, 0x8a, 0xe5, 0xe3, 0x2b,
            0xed, 0x5e, 0xd0, 0x6c, 0xf5, 0xb8, 0xe0, 0x7e, 0x94, 0xff, 0xb7, 0x27, 0x51, 0xaf,
            0xeb, 0x46, 0x0d, 0x25
        ]
    );
    let old = root.join("v0.20.0");
    let new = root.join("v0.21.0");
    for (left, right) in [(&old, &new), (&new, &old)] {
        assert!(matches!(
            compare(left, right),
            Err(gosanja_core::diff::CompareError::SchemaVersion(_))
        ));
    }
    let current = |dir: &std::path::Path| {
        let mut files = read_dir(dir);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(files.manifest.as_ref().unwrap()).unwrap();
        assert_eq!(manifest["schema"], "0.2");
        manifest["schema"] = gosanja_schema::snapshot::SCHEMA_VERSION.to_string().into();
        files.manifest = Some(serde_json::to_vec(&manifest).unwrap());
        materialize(&files)
    };
    let old = current(&old);
    let new = current(&new);
    let comparison = compare(old.path(), new.path()).unwrap();
    let mut correct = 0;
    let mut deletions = 0;
    let mut missed = vec![];
    for line in labels.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] != "base64" {
            continue;
        }
        let first = comparison
            .candidates
            .iter()
            .find(|row| row.old == fields[1]);
        match (fields[3], first) {
            ("yes", Some(candidate)) => {
                assert_eq!((candidate.new.as_str(), candidate.rank), (fields[2], 1));
                correct += 1;
            }
            ("yes", None) => missed.push((fields[1], fields[2])),
            ("no", None) => deletions += 1,
            _ => panic!("unexpected prediction for {line}"),
        }
    }
    assert_eq!((correct, deletions), (17, 3));
    assert_eq!(
        missed,
        [
            (
                "cargo:base64:engine/fast_portable/FastPortable#",
                "cargo:base64:engine/general_purpose/GeneralPurpose#"
            ),
            (
                "cargo:base64:engine/fast_portable/impl#[FastPortable]from().",
                "cargo:base64:engine/general_purpose/impl#[GeneralPurpose]new()."
            ),
            (
                "cargo:base64:engine/fast_portable/decode/impl#[FastPortableEstimate]from().",
                "cargo:base64:engine/general_purpose/decode/impl#[GeneralPurposeEstimate]new()."
            )
        ]
    );
}

#[test]
fn public_compare_returns_the_validator_diagnostic_on_either_side() {
    use gosanja_core::diff::CompareError;
    use gosanja_core::validate::validate;
    use serde_json::{Value, json};
    let valid = fixture("rust-basic").join("expected");
    for (field, value, code) in [
        ("key", json!("cargo:demo:views/"), "duplicate-key"),
        ("parent", json!("cargo:demo:missing/"), "dangling-key"),
        ("parent", json!("cargo:demo:crate/"), "parent-cycle"),
        ("symbol", json!("malformed"), "invalid-symbol"),
        ("body", json!([5, 0, 2, 0]), "position-outside-file"),
    ] {
        let mut invalid = read_dir(&valid);
        let mut rows: Vec<Value> = std::str::from_utf8(invalid.entities.as_ref().unwrap())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        if field == "body" {
            rows[0]["def"]["body"] = value;
        } else {
            rows[0][field] = value;
        }
        invalid.entities = Some(jsonl(&rows));
        let diagnostics = validate(&invalid, None).unwrap();
        assert_eq!(diagnostics[0].code, code);
        let expected = CompareError::Invalid(diagnostics[0].clone());
        let invalid = materialize(&invalid);
        assert_eq!(compare(invalid.path(), &valid).unwrap_err(), expected);
        assert_eq!(compare(&valid, invalid.path()).unwrap_err(), expected);
    }
}

#[test]
fn two_scopes_are_refused_and_missing_roles_keep_the_schema_diagnostic() {
    use gosanja_core::diff::CompareError;
    use gosanja_core::validate::validate;
    let good = fixture("rust-basic").join("expected");
    let mut bad = read_dir(&good);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(bad.manifest.as_ref().unwrap()).unwrap();
    let mut other = manifest["scopes"][0].clone();
    other["path"] = "other".into();
    manifest["scopes"].as_array_mut().unwrap().push(other);
    bad.manifest = Some(serde_json::to_vec(&manifest).unwrap());
    assert_eq!(
        compare(materialize(&bad).path(), &good).unwrap_err(),
        CompareError::ScopeCount(2)
    );
    bad = read_dir(&good);
    let mut rows: Vec<serde_json::Value> = std::str::from_utf8(bad.relations.as_ref().unwrap())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    rows.iter_mut()
        .find(|row| row["rel"] == "ref")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("roles");
    bad.relations = Some(jsonl(&rows));
    let diagnoses = validate(&bad, None).unwrap();
    assert_eq!(diagnoses[0].code, "schema");
    assert_eq!(
        compare(&good, materialize(&bad).path()).unwrap_err(),
        CompareError::Invalid(diagnoses[0].clone())
    );
}

#[test]
fn unsupported_version_precedes_the_other_inputs_row_diagnostic() {
    use gosanja_core::diff::CompareError;
    let mut old = read_dir(&fixture("rust-basic").join("expected"));
    let mut new = old.clone();
    old.entities = Some(b"malformed\n".to_vec());
    let mut manifest: serde_json::Value =
        serde_json::from_slice(new.manifest.as_ref().unwrap()).unwrap();
    manifest["schema"] = "1.1".into();
    new.manifest = Some(serde_json::to_vec(&manifest).unwrap());
    let error = compare(materialize(&old).path(), materialize(&new).path()).unwrap_err();
    assert_eq!(error, CompareError::SchemaVersion("1.1".into()));
    assert_eq!(error.to_string(), "unsupported snapshot schema 1.1");
}

#[test]
fn diff_packages_use_the_global_symbol_even_when_row_metadata_disagrees() {
    use serde_json::{Value, json};
    let root = fixture("diff-basic");
    let old = root.join("old/expected");
    for (manager, name, expected) in [
        ("cargo", "std", "cargo:std"),
        (".", ".", ".:."),
        ("m:@", "~%", "m%3A%40:%7E%25"),
        ("", "std", ""),
    ] {
        let mut new = read_dir(&root.join("new/expected"));
        let mut rows: Vec<Value> = new
            .externals
            .as_ref()
            .unwrap()
            .split(|&byte| byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        let row = rows
            .iter_mut()
            .find(|row| row["package"]["name"] == "std")
            .unwrap();
        row["symbol"] = format!("ra {manager} {name} 1 crate/").into();
        row["package"]["manager"] = "".into();
        row["package"]["name"] = "wrong".into();
        new.externals = Some(jsonl(&rows));
        let new = materialize(&new);
        let result = compare(&old, new.path());
        if manager.is_empty() {
            assert!(
                matches!(result.unwrap_err(), gosanja_core::diff::CompareError::Invalid(d)
                if d.code == "invalid-symbol")
            );
            continue;
        }
        let added = result.unwrap();
        assert_eq!(added.manifest.packages_added, [expected]);
        assert!(
            to_value(added.externals)
                .unwrap()
                .as_array()
                .unwrap()
                .contains(&json!({"change":"added","new":"cargo:std:crate/","package":expected}))
        );
        let removed = compare(new.path(), &old).unwrap();
        assert_eq!(removed.manifest.packages_removed, [expected]);
        assert!(
            to_value(removed.externals)
                .unwrap()
                .as_array()
                .unwrap()
                .contains(&json!({"change":"removed","old":"cargo:std:crate/","package":expected}))
        );
    }
}

fn compare_snapshots(
    old: &gosanja_core::graph::Snapshot,
    new: &gosanja_core::graph::Snapshot,
) -> gosanja_core::diff::Comparison {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    gosanja_core::write::write_snapshot(left.path(), old).unwrap();
    gosanja_core::write::write_snapshot(right.path(), new).unwrap();
    compare(left.path(), right.path()).unwrap()
}

#[test]
fn optional_layer_comparison_has_the_same_table_for_calls_and_syntax() {
    use gosanja_schema::{diff::Excluded, snapshot::LayerState::*};
    let states = [Present, Absent, Failed, NotRequested];
    let choices = [
        [true, false, false, false],
        [false, true, true, false],
        [false, true, true, false],
        [false, false, false, false],
    ];
    let sample = support::build_fixture("rust-basic");
    for (i, old_state) in states.into_iter().enumerate() {
        for (j, new_state) in states.into_iter().enumerate() {
            let mut old = sample.clone();
            let mut new = sample.clone();
            (old.scope.layers.syntax, old.scope.layers.calls) = (old_state, old_state);
            (new.scope.layers.syntax, new.scope.layers.calls) = (new_state, new_state);
            let compared = compare_snapshots(&old, &new);
            let excluded = if choices[i][j] {
                vec![]
            } else {
                vec![Excluded::Calls, Excluded::Syntax]
            };
            assert_eq!(
                compared.manifest.not_compared, excluded,
                "{old_state:?} -> {new_state:?}"
            );
        }
    }
}

fn with_fake_syntax(
    temp: &std::path::Path,
    source: &str,
    occurrences: Vec<scip::types::Occurrence>,
    syntax: &str,
    list: &str,
) -> gosanja_core::graph::Snapshot {
    use gosanja_core::{adapter, sidecar, write::FsSource};
    use scip::types::symbol_information::Kind;
    use support::{DEF, doc, index, info, repo, shell};
    let root = tempfile::tempdir_in(temp).unwrap();
    let index_file = tempfile::NamedTempFile::new_in(temp).unwrap();
    let syntax_file = tempfile::NamedTempFile::new_in(temp).unwrap();
    std::fs::write(root.path().join("lib.rs"), source).unwrap();
    let mut doc = doc("lib.rs", occurrences);
    doc.language = "rust".into();
    doc.symbols = doc
        .occurrences
        .iter()
        .filter(|o| o.symbol_roles == DEF && o.symbol.ends_with("()."))
        .map(|o| info(&o.symbol, Kind::Function))
        .collect();
    std::fs::write(index_file.path(), index("test", vec![doc])).unwrap();
    std::fs::write(syntax_file.path(), syntax).unwrap();
    let described = adapter::describe(
        adapter::Invocation::new(
            shell(),
            vec![
                repo("fixtures/fake-adapter/adapter.sh").into_os_string(),
                "--index-file".into(),
                index_file.path().into(),
                "--syntax-file".into(),
                syntax_file.path().into(),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let output = described
        .run_with_layers(root.path(), temp, list.parse().unwrap())
        .unwrap();
    sidecar::build(output, &FsSource::new(root.path()).unwrap(), "sample").unwrap()
}

#[test]
fn syntax_selection_changes_base_reference_ownership() {
    use gosanja_schema::{
        diff::Excluded,
        snapshot::{GapKind, Rel},
    };
    use support::{DEF, REF, occ};
    let temp = tempfile::tempdir().unwrap();
    let source = "fn outer() {\n  target();\n}\nfn target() {}\n";
    let syntax = "{\"anchor\":{\"file\":\"lib.rs\",\"line\":0,\"col\":3},\"body\":[0,0,2,1]}\n";
    for module in [false, true] {
        let mut occurrences = vec![
            occ("s cargo sample 1 outer().", DEF, &[0, 3, 8], &[]),
            occ("s cargo sample 1 target().", REF, &[1, 2, 8], &[]),
            occ(
                "s cargo sample 1 target().",
                DEF,
                &[3, 3, 9],
                &[3, 0, 3, 14],
            ),
        ];
        if module {
            occurrences.insert(
                0,
                occ("s cargo sample 1 crate/", DEF, &[0, 0, 0], &[0, 0, 4, 0]),
            );
        }
        let snapshot =
            |list| with_fake_syntax(temp.path(), source, occurrences.clone(), syntax, list);
        let old = snapshot("base,syntax");
        let new = snapshot("base");
        let refs = |s: &gosanja_core::graph::Snapshot| {
            s.relations
                .iter()
                .filter(|r| r.rel == Rel::Ref && r.src == "core:reference-in-body")
                .map(|r| (r.from.clone(), r.to.clone(), r.roles.clone()))
                .collect::<Vec<_>>()
        };
        let reference = |from: &str| {
            (
                from.to_owned(),
                "cargo:sample:target().".to_owned(),
                Some(vec![]),
            )
        };
        assert_eq!(refs(&old), [reference("cargo:sample:outer().")]);
        let diff = compare_snapshots(&old, &new);
        assert_eq!(
            diff.manifest.not_compared,
            [Excluded::Calls, Excluded::Syntax]
        );
        let diffs = diff
            .relations
            .iter()
            .map(|r| to_value(r).unwrap())
            .filter(|r| r["rel"] == "ref")
            .collect::<Vec<_>>();
        let mut expected = vec![serde_json::json!({
            "change":"removed", "from":{"old":"cargo:sample:outer().","new":"cargo:sample:outer()."},
            "to":{"old":"cargo:sample:target().","new":"cargo:sample:target()."}, "rel":"ref", "roles":[]
        })];
        if module {
            assert_eq!(refs(&new), [reference("cargo:sample:crate/")]);
            expected.insert(0, serde_json::json!({
                "change":"added", "from":{"old":"cargo:sample:crate/","new":"cargo:sample:crate/"},
                "to":{"old":"cargo:sample:target().","new":"cargo:sample:target()."}, "rel":"ref", "roles":[]
            }));
        } else {
            assert_eq!(refs(&new), []);
            let gap = new
                .gaps
                .iter()
                .find(|g| g.gap == GapKind::UnattributedReferences)
                .unwrap();
            assert_eq!((gap.file.as_deref(), gap.count), (Some("lib.rs"), Some(1)));
        }
        assert_eq!(diffs, expected);
    }
}

#[test]
fn syntax_bodies_affect_candidate_size_separately_from_references() {
    use support::{DEF, occ};
    let temp = tempfile::tempdir().unwrap();
    let snapshot = |suffix, list| {
        with_fake_syntax(
            temp.path(),
            &format!("fn rename_{suffix}() {{\n  let _x = 0;\n}}\n"),
            vec![occ(
                &format!("s cargo sample 1 rename_{suffix}()."),
                DEF,
                &[0, 3, 11],
                &[],
            )],
            "{\"anchor\":{\"file\":\"lib.rs\",\"line\":0,\"col\":3},\"body\":[0,0,2,1]}\n",
            list,
        )
    };
    let old = snapshot('a', "base,syntax");
    let yes = compare_snapshots(&old, &snapshot('b', "base,syntax"));
    let no = compare_snapshots(&old, &snapshot('b', "base"));
    assert_eq!((yes.candidates.len(), no.candidates.len()), (1, 1));
    let yes = &yes.candidates[0];
    let no = &no.candidates[0];
    assert_eq!((yes.size, no.size), (1_000_000, 0));
    assert_eq!(yes.score - no.score, 120_000);
    let mut restored = no.clone();
    restored.size = yes.size;
    restored.score += 120_000;
    assert_eq!(&restored, yes);
}

#[test]
fn unattributed_references_affect_candidate_incoming_and_package_reachability() {
    use gosanja_schema::snapshot::{GapKind, Rel};
    use support::{DEF, REF, occ};
    let temp = tempfile::tempdir().unwrap();
    let snapshot = |suffix, list, external| {
        let name = format!("rename_{suffix}");
        let local = format!("s cargo sample 1 {name}().");
        let reference = if external {
            "s cargo widget 1 remote().".into()
        } else {
            local.clone()
        };
        let code = if external { "remote" } else { name.as_str() };
        let width = i32::try_from(code.len()).unwrap();
        with_fake_syntax(
            temp.path(),
            &format!("fn holder() {{\n  {code}();\n}}\nfn {name}() {{}}\n"),
            vec![
                occ("s cargo sample 1 holder().", DEF, &[0, 3, 9], &[]),
                occ(&reference, REF, &[1, 2, 2 + width], &[]),
                occ(&local, DEF, &[3, 3, 11], &[3, 0, 3, 16]),
            ],
            "{\"anchor\":{\"file\":\"lib.rs\",\"line\":0,\"col\":3},\"body\":[0,0,2,1]}\n",
            list,
        )
    };
    for external in [false, true] {
        let selected = snapshot('b', "base,syntax", external);
        let omitted = snapshot('b', "base", external);
        let refs = |s: &gosanja_core::graph::Snapshot| {
            s.relations
                .iter()
                .filter(|r| r.rel == Rel::Ref && r.src == "core:reference-in-body")
                .map(|r| (r.from.clone(), r.to.clone(), r.roles.clone()))
                .collect::<Vec<_>>()
        };
        let to = if external {
            "cargo:widget:remote()."
        } else {
            "cargo:sample:rename_b()."
        };
        assert_eq!(
            refs(&selected),
            [("cargo:sample:holder().".into(), to.into(), Some(vec![]))]
        );
        assert_eq!(refs(&omitted), []);
        let gap = omitted
            .gaps
            .iter()
            .find(|g| g.gap == GapKind::UnattributedReferences)
            .unwrap();
        assert_eq!((gap.file.as_deref(), gap.count), (Some("lib.rs"), Some(1)));

        let old = snapshot('a', "base,syntax", false);
        let yes = compare_snapshots(&old, &selected);
        let no = compare_snapshots(&old, &omitted);
        if external {
            assert_eq!(yes.manifest.packages_added, ["cargo:widget"]);
            assert_eq!(no.manifest.packages_added, Vec::<String>::new());
        } else {
            assert_eq!((yes.candidates.len(), no.candidates.len()), (1, 1));
            assert_eq!(
                (yes.candidates[0].incoming, no.candidates[0].incoming),
                (1_000_000, 0)
            );
            assert_eq!(yes.candidates[0].score - no.candidates[0].score, 90_000);
        }
    }
}

fn snapshots_with_layer_contributions()
-> (gosanja_core::graph::Snapshot, gosanja_core::graph::Snapshot) {
    use gosanja_schema::snapshot::RelationRow;
    use support::{DEF, occ};
    let temp = tempfile::tempdir().unwrap();
    let snapshot = |suffix| {
        with_fake_syntax(
            temp.path(),
            &format!("pub fn target() {{}}\nfn rename_{suffix}() {{}}\n"),
            vec![
                occ(
                    "s cargo sample 1 target().",
                    DEF,
                    &[0, 7, 13],
                    &[0, 0, 0, 18],
                ),
                occ(
                    &format!("s cargo sample 1 rename_{suffix}()."),
                    DEF,
                    &[1, 3, 11],
                    &[1, 0, 1, 16],
                ),
            ],
            "{\"anchor\":{\"file\":\"lib.rs\",\"line\":0,\"col\":7},\"visibility\":\"pub\"}\n",
            "base,syntax,calls",
        )
    };

    let mut old = snapshot('a');
    let mut new = snapshot('b');
    new.syntax[0].visibility = Some("pub(crate)".into());
    let old_name = "cargo:sample:rename_a().";
    let new_name = "cargo:sample:rename_b().";
    let target = "cargo:sample:target().";
    let build = |name| serde_json::from_value(serde_json::json!({"id":name})).unwrap();
    old.scope.builds = vec![build("build")];
    new.scope.builds = vec![build("build")];
    let call = |from, to, src: &str| {
        let mut row = serde_json::json!({
            "from":from, "to":to, "rel":"call", "mechanism":"static", "resolution":"direct",
            "evidence":[{"build":"build"}], "src":src,
        });
        if src.ends_with("/syntax") {
            row["resolution"] = "declared".into();
            row["mechanism"] = "unknown".into();
            row["evidence"] = serde_json::json!([{"file":"lib.rs","line":1,"col":3}]);
        }
        serde_json::from_value::<RelationRow>(row).unwrap()
    };
    let base = |from| {
        serde_json::from_value(serde_json::json!({
            "from":from, "to":target, "rel":"ref", "roles":[], "sites":[["lib.rs",1,3,1,4]],
            "src":"core:reference-in-body"
        }))
        .unwrap()
    };
    old.relations.push(base(old_name));
    new.relations.push(base(new_name));
    old.relations
        .push(call(old_name, target, "adapter:rust/calls"));
    new.relations
        .push(call(new_name, target, "adapter:rust/calls"));
    new.relations
        .push(call(new_name, target, "adapter:rust/syntax"));
    new.externals.push(
        serde_json::from_value(serde_json::json!({
            "key":"cargo:widget:remote().", "symbol":"s cargo widget 1 remote().",
            "package":{"manager":"cargo","name":"widget","version":"1"}, "src":"scip:test",
        }))
        .unwrap(),
    );
    new.relations.push(call(
        new_name,
        "cargo:widget:remote().",
        "adapter:rust/calls",
    ));

    (old, new)
}

#[test]
fn layer_states_alone_remove_matching_properties_relations_and_features() {
    use gosanja_schema::{
        diff::{Excluded, RelationKind},
        snapshot::LayerState::{Absent, NotRequested, Present},
    };
    let (mut old, mut new) = snapshots_with_layer_contributions();
    let yes = compare_snapshots(&old, &new);
    assert_eq!(yes.manifest.not_compared, []);
    let relation_deltas = |d: &gosanja_core::diff::Comparison| {
        d.relations
            .iter()
            .filter(|r| to_value(r).unwrap()["rel"] == "call")
            .count()
    };
    assert_eq!(relation_deltas(&yes), 4);
    assert_eq!(yes.manifest.packages_added, ["cargo:widget"]);
    new.scope.layers.calls = NotRequested;
    let no_calls = compare_snapshots(&old, &new);
    assert_eq!(no_calls.manifest.not_compared, [Excluded::Calls]);
    let call_sources: Vec<_> = no_calls
        .relations
        .iter()
        .filter_map(|row| match &row.kind {
            RelationKind::Call { src, .. } => Some(src.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(call_sources, ["adapter:rust/syntax"]);
    let base_deltas = |comparison: &gosanja_core::diff::Comparison| {
        comparison
            .relations
            .iter()
            .map(|r| to_value(r).unwrap())
            .filter(|r| r["rel"] == "ref")
            .collect::<Vec<_>>()
    };
    let base = base_deltas(&yes);
    assert_eq!(base.len(), 2);
    assert_eq!(base_deltas(&no_calls), base);

    assert_eq!(no_calls.manifest.packages_added, Vec::<String>::new());
    assert_eq!((yes.candidates.len(), no_calls.candidates.len()), (1, 1));
    assert_eq!(
        (yes.candidates[0].outgoing, no_calls.candidates[0].outgoing),
        (666_666, 500_000)
    );
    assert_eq!(
        yes.candidates[0].score - no_calls.candidates[0].score,
        21_666
    );

    new.scope.layers.calls = Present;
    for old_syntax in [Present, Absent] {
        old.scope.layers.syntax = old_syntax;
        new.scope.layers.syntax = old_syntax;
        let compared = compare_snapshots(&old, &new);
        assert_eq!(compared.manifest.not_compared, []);
        assert!(
            compared.entities.iter().any(
                |e| to_value(e).unwrap()["properties"]["visibility"] != serde_json::Value::Null
            )
        );
        new.scope.layers.syntax = NotRequested;
        let excluded = compare_snapshots(&old, &new);
        assert_eq!(excluded.manifest.not_compared, [Excluded::Syntax]);
        assert!(
            !excluded.entities.iter().any(
                |e| to_value(e).unwrap()["properties"]["visibility"] != serde_json::Value::Null
            )
        );
        assert_eq!(
            (relation_deltas(&compared), relation_deltas(&excluded)),
            (4, 3)
        );
        assert_eq!(
            (
                compared.candidates[0].outgoing,
                excluded.candidates[0].outgoing
            ),
            (666_666, 1_000_000)
        );
    }
}
