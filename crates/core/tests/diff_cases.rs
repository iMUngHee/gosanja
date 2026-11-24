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
        (root.join("old/expected"), vec![]),
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
    let load = |tag| root.join(tag);
    let comparison = compare(&load("v0.20.0"), &load("v0.21.0")).unwrap();
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
