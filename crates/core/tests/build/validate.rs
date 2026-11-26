// The validator's checks of build facts (criterion 11 of the build plan). Included by
// `validate_cases.rs`. Each case starts from a snapshot the engine wrote and changes one fact.

include!("run.rs");

mod build_check {
    use super::build_run::{self, CRATE, Out, file, root, skipped, standard, target};
    use serde_json::{Value, json};

    pub const TSCONFIG: &str = "packages/app/tsconfig.json";

    /// An engine-written snapshot holding every kind of build fact.
    pub fn valid() -> Out {
        let mut spec = standard();
        spec.syntax = vec![
            json!({"anchor": {"file": "src/lib.rs", "line": 1, "col": 7}, "visibility": "pub"})
                .to_string(),
            json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 0, "col": 7}, "test": "cfg"})
                .to_string(),
        ];
        spec.build = vec![
            target("lib", Some(root(CRATE, Some(("src/lib.rs", 0, 0))))),
            target("bin", Some(root(CRATE, Some(("src/main.rs", 0, 0))))),
            target("it", None),
            target(TSCONFIG, Some(root("s m p 1 nobody/", None))),
            file("src/lib.rs", json!({"targets": ["bin", "lib"]})),
            file("tests/it.rs", json!({"targets": ["it"], "test": "target"})),
            file("src/main.rs", json!({"constraint": "x"})),
        ];
        spec.build_entry = Some(skipped(&[
            ("src/main.rs", "target-unknown"),
            ("src/lib.rs", "targets-incomplete"),
        ]));
        let out = build_run::run(&spec).unwrap();
        assert_eq!(out.validation(), Vec::<String>::new());
        out
    }

    /// `out` with the rows of `table` changed by `edit`, the manifest's counts kept true.
    pub fn with(out: &Out, table: &str, edit: impl FnOnce(&mut Vec<Value>)) -> Out {
        let mut rows = out.rows(table);
        edit(&mut rows);
        let mut tables = out.tables.clone();
        tables.insert(
            table.to_owned(),
            rows.iter().fold(String::new(), |mut out, r| {
                out.push_str(&r.to_string());
                out.push('\n');
                out
            }),
        );
        let changed = Out { tables };
        let mut manifest = changed.manifest();
        for (name, count) in [
            ("files.jsonl", "files"),
            ("entities.jsonl", "entities"),
            ("gaps.jsonl", "gaps"),
            ("targets.jsonl", "targets"),
            ("syntax.jsonl", "syntax"),
        ] {
            manifest["counts"][count] = json!(changed.rows(name).len());
        }
        with_manifest(&changed, &manifest)
    }

    pub fn with_manifest(out: &Out, manifest: &Value) -> Out {
        let mut tables = out.tables.clone();
        tables.insert("manifest.json".to_owned(), format!("{manifest}\n"));
        Out { tables }
    }

    pub fn gap_sorted(rows: &mut [Value]) {
        rows.sort_by(|a, b| {
            (
                a["gap"].as_str(),
                a["file"].as_str(),
                a["range"].to_string(),
                a.to_string(),
            )
                .cmp(&(
                    b["gap"].as_str(),
                    b["file"].as_str(),
                    b["range"].to_string(),
                    b.to_string(),
                ))
        });
    }

    pub fn add_gap(out: &Out, gap: Value) -> Out {
        with(out, "gaps.jsonl", |rows| {
            rows.push(gap);
            gap_sorted(rows);
        })
    }

    pub fn edit_row(
        out: &Out,
        table: &str,
        field: &str,
        value: &str,
        edit: impl FnOnce(&mut Value),
    ) -> Out {
        with(out, table, |rows| {
            let row = rows
                .iter_mut()
                .find(|r| r[field] == value)
                .unwrap_or_else(|| panic!("no row {field} = {value}"));
            edit(row);
        })
    }

    /// Asserts that the validator reports `code` for this snapshot.
    pub fn rejects(out: &Out, code: &str, case: &str) {
        let codes = out.validation();
        assert!(
            codes.iter().any(|c| c.ends_with(&format!(":{code}"))),
            "{case}: expected {code}, got {codes:?}"
        );
    }

    pub fn accepts(out: &Out, case: &str) {
        assert_eq!(out.validation(), Vec::<String>::new(), "{case}");
    }

    /// A snapshot made without the build layer, with the given state and its gap.
    pub fn without_build(state: &str) -> Out {
        let mut spec = standard();
        spec.syntax = vec![
            json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 0, "col": 7}, "test": "cfg"})
                .to_string(),
        ];
        match state {
            "not-requested" => spec.request = Some("base,syntax"),
            "absent" => spec.offered = Some(r#"["base","syntax","calls"]"#),
            "failed" => spec.mode = Some("build-failed"),
            other => panic!("{other}"),
        }
        let out = build_run::run(&spec).unwrap();
        assert_eq!(out.layer("build"), state);
        accepts(&out, state);
        out
    }
}

#[test]
fn build_rejects_target_entity_unknown() {
    use build_check::{edit_row, rejects, valid};
    let out = edit_row(&valid(), "targets.jsonl", "key", "lib", |t| {
        t["entity"] = "m:p:nobody().".into();
    });
    rejects(&out, "dangling-key", "entity");
}

#[test]
fn build_rejects_file_target_unknown() {
    use build_check::{edit_row, rejects, valid};
    let out = edit_row(&valid(), "files.jsonl", "path", "tests/it.rs", |f| {
        f["targets"] = serde_json::json!(["zzz"]);
    });
    rejects(&out, "dangling-target", "file target");
}

#[test]
fn build_rejects_targets_empty_or_unsorted() {
    use build_check::{edit_row, rejects, valid};
    let base = valid();
    let empty = edit_row(&base, "files.jsonl", "path", "tests/it.rs", |f| {
        f["targets"] = serde_json::json!([]);
    });
    rejects(&empty, "schema", "empty");
    let unsorted = edit_row(&base, "files.jsonl", "path", "src/lib.rs", |f| {
        f["targets"] = serde_json::json!(["lib", "bin"]);
    });
    rejects(&unsorted, "targets-unsorted", "unsorted");
}

#[test]
fn build_rejects_unknown_test_value() {
    use build_check::{edit_row, rejects, valid};
    let base = valid();
    let file = edit_row(&base, "files.jsonl", "path", "tests/it.rs", |f| {
        f["test"] = "sometimes".into();
    });
    rejects(&file, "unknown-value", "file");
    let entity = edit_row(&base, "entities.jsonl", "key", "m:p:alpha().", |e| {
        e["test"] = "sometimes".into();
    });
    rejects(&entity, "unknown-value", "entity");
}

#[test]
fn build_rejects_key_outside_grammar() {
    use build_check::{edit_row, rejects, valid};
    let long = "é".repeat(600);
    let out = edit_row(&valid(), "targets.jsonl", "key", "it", |t| {
        t["key"] = long.clone().into();
    });
    let out = edit_row(&out, "files.jsonl", "path", "tests/it.rs", |f| {
        f["targets"] = serde_json::json!([long]);
    });
    rejects(&out, "target-key-grammar", "grammar");
}

#[test]
fn build_rejects_counts_targets() {
    use build_check::{rejects, valid, with_manifest};
    let out = valid();
    let mut manifest = out.manifest();
    manifest["counts"]["targets"] = 9.into();
    rejects(&with_manifest(&out, &manifest), "count-mismatch", "counts");
}

#[test]
fn build_rejects_root_gap_naming_no_target() {
    use build_check::{TSCONFIG, rejects, valid, with};
    let out = with(&valid(), "gaps.jsonl", |rows| {
        let gap = rows.iter_mut().find(|g| g["target"] == TSCONFIG).unwrap();
        gap["target"] = "nobody".into();
    });
    rejects(&out, "dangling-target", "gap target");
}

#[test]
fn build_rejects_test_in_syntax() {
    use build_check::{rejects, valid, with};
    let out = with(&valid(), "syntax.jsonl", |rows| {
        rows[0]["test"] = "cfg".into();
    });
    rejects(&out, "test-in-syntax", "syntax");
}

#[test]
fn build_accepts_a_root_gap_with_a_non_entity_key() {
    use build_check::{TSCONFIG, accepts, valid};
    let out = valid();
    assert!(
        out.gaps("unresolved-root")
            .iter()
            .any(|g| g["target"] == TSCONFIG)
    );
    accepts(&out, "tsconfig");
}

#[test]
fn build_rejects_targets_beside_target_unknown() {
    use build_check::{add_gap, rejects, valid};
    let out = add_gap(
        &valid(),
        serde_json::json!({"gap": "skipped-file", "file": "tests/it.rs", "layer": "build",
                           "code": "target-unknown", "src": "adapter:fake/build"}),
    );
    rejects(&out, "targets-beside-target-unknown", "pair");
}

#[test]
fn build_rejects_targetless_row_without_target_unknown() {
    use build_check::{accepts, edit_row, rejects, valid, with};
    let base = valid();
    let without = |out: &build_run::Out, file: &str| {
        with(out, "gaps.jsonl", |rows| {
            rows.retain(|g| !(g["file"] == file && g["code"] == "target-unknown"));
        })
    };
    rejects(
        &without(&base, "src/main.rs"),
        "targetless-without-target-unknown",
        "constraint-only",
    );
    let test_only = edit_row(&base, "files.jsonl", "path", "tests/it.rs", |f| {
        f.as_object_mut().unwrap().remove("targets");
    });
    rejects(&test_only, "targetless-without-target-unknown", "test-only");
    let with_skip = build_check::add_gap(
        &test_only,
        serde_json::json!({"gap": "skipped-file", "file": "tests/it.rs", "layer": "build",
                           "code": "target-unknown", "src": "adapter:fake/build"}),
    );
    accepts(&with_skip, "test-only with its skip");
}

#[test]
fn build_rejects_targets_incomplete_without_targets() {
    use build_check::{accepts, add_gap, rejects, valid, with};
    let base = valid();
    let stray = add_gap(
        &base,
        serde_json::json!({"gap": "skipped-file", "file": "src/main.rs", "layer": "build",
                           "code": "targets-incomplete", "src": "adapter:fake/build"}),
    );
    rejects(&stray, "targets-incomplete-without-targets", "stray");
    let removed = with(&base, "gaps.jsonl", |rows| {
        rows.retain(|g| g["code"] != "targets-incomplete");
    });
    accepts(&removed, "without the incomplete skip");
}

#[test]
fn build_rejects_build_facts_without_build_layer() {
    use build_check::{edit_row, rejects, with, without_build};
    for state in ["absent", "failed", "not-requested"] {
        let base = without_build(state);
        let target = with(&base, "targets.jsonl", |rows| {
            rows.push(serde_json::json!({"scope": ".", "key": "k", "kind": "lib", "name": "k", "src": "adapter:fake/build"}));
        });
        rejects(
            &target,
            "build-facts-without-build-layer",
            &format!("{state}: target"),
        );
        for (field, value) in [
            ("targets", serde_json::json!(["k"])),
            ("test", serde_json::json!("target")),
            ("constraint", serde_json::json!("x")),
        ] {
            let fact = edit_row(&base, "files.jsonl", "path", "tests/it.rs", |f| {
                f[field] = value.clone();
            });
            rejects(
                &fact,
                "build-facts-without-build-layer",
                &format!("{state}: {field}"),
            );
        }
    }
}

#[test]
fn build_rejects_build_gap_outside_its_state() {
    use build_check::{add_gap, rejects, valid, with, without_build};
    use serde_json::json;
    let skipped = json!({"gap": "skipped-file", "file": "src/lib.rs", "layer": "build",
                         "code": "parse-failed", "src": "adapter:fake/build"});
    let missing = json!({"gap": "missing-layer", "layer": "build", "scope": ".", "src": "adapter:fake/build"});
    let failed = json!({"gap": "failed-layer", "layer": "build", "scope": ".", "detail": "row-invalid", "src": "adapter:fake/build"});
    let not_requested = without_build("not-requested");
    rejects(
        &add_gap(&not_requested, missing.clone()),
        "build-gap-outside-its-state",
        "not-requested",
    );
    let absent = without_build("absent");
    rejects(
        &add_gap(&absent, failed.clone()),
        "build-gap-outside-its-state",
        "absent + failed",
    );
    rejects(
        &add_gap(&absent, skipped.clone()),
        "build-gap-outside-its-state",
        "absent + skip",
    );
    let failed_scope = without_build("failed");
    rejects(
        &add_gap(&failed_scope, skipped),
        "build-gap-outside-its-state",
        "failed + skip",
    );
    let none = with(&failed_scope, "gaps.jsonl", |rows| {
        rows.retain(|g| g["layer"] != "build");
    });
    rejects(
        &none,
        "build-gap-outside-its-state",
        "failed without its gap",
    );
    rejects(
        &add_gap(&valid(), missing),
        "build-gap-outside-its-state",
        "present + missing",
    );
}

#[test]
fn build_accepts_missing_layer_outcome() {
    use build_check::{accepts, without_build};
    let out = without_build("absent");
    assert_eq!(out.gaps("missing-layer").len(), 1);
    accepts(&out, "missing-layer");
}

#[test]
fn build_accepts_failed_layer_outcome() {
    use build_check::accepts;
    let mut spec = build_run::standard();
    spec.build = vec![r#"{"row":"file","file":"src/lib.rs"}"#.to_owned()];
    let out = build_run::run(&spec).unwrap();
    assert_eq!(out.layer("build"), "failed");
    assert_eq!(out.gaps("failed-layer").len(), 1);
    accepts(&out, "failed-layer");
}

#[test]
fn build_accepts_syntax_marks_without_build_layer() {
    use build_check::{accepts, without_build};
    let out = without_build("not-requested");
    assert_eq!(out.entity("m:p:alpha().")["test"], "cfg");
    accepts(&out, "syntax-only mark");
}

#[test]
fn build_validates_state_per_scope() {
    use build_check::{accepts, edit_row, rejects, with, with_manifest};
    use build_run::{file, run, target};
    use serde_json::json;
    use support::{DEF, doc, occ};
    // Two copies of a small tree, under `a/` and `b/`; the engine writes them as one scope,
    // which this case then splits into two.
    let mut spec = build_run::Spec::default();
    for dir in ["a", "b"] {
        spec.documents.push(doc(
            &format!("{dir}/lib.rs"),
            vec![occ(
                &format!("s m {dir} 1 f()."),
                DEF,
                &[0, 3, 4],
                &[0, 0, 9],
            )],
        ));
    }
    spec.sources = vec![
        ("a/lib.rs", "fn f() {}\n".to_owned()),
        ("b/lib.rs", "fn f() {}\n".to_owned()),
    ];
    spec.build = vec![
        target("a-lib", None),
        target("a-rootless", None),
        file("a/lib.rs", json!({"targets": ["a-lib"]})),
    ];
    let one = run(&spec).unwrap();
    let mut manifest = one.manifest();
    let mut a = manifest["scopes"][0].clone();
    a["path"] = "a".into();
    let mut b = a.clone();
    b["path"] = "b".into();
    b["layers"]["build"] = "failed".into();
    manifest["scopes"] = json!([a, b]);
    let split = with_manifest(&one, &manifest);
    let split = with(&split, "targets.jsonl", |rows| {
        for row in rows {
            row["scope"] = "a".into();
        }
    });
    let failed = json!({"gap": "failed-layer", "layer": "build", "scope": "b", "detail": "row-invalid", "src": "adapter:fake/build"});
    let two = build_check::add_gap(&split, failed);
    accepts(&two, "two scopes");
    let moved = edit_row(&two, "targets.jsonl", "key", "a-rootless", |t| {
        t["scope"] = "b".into();
    });
    rejects(
        &moved,
        "build-facts-without-build-layer",
        "rootless target in b",
    );
    let wrong = with(&two, "gaps.jsonl", |rows| {
        for g in rows.iter_mut().filter(|g| g["gap"] == "failed-layer") {
            g["scope"] = "a".into();
        }
    });
    rejects(&wrong, "build-gap-outside-its-state", "gap in a");
    let unscoped = with(&two, "gaps.jsonl", |rows| {
        for g in rows.iter_mut().filter(|g| g["gap"] == "failed-layer") {
            g.as_object_mut().unwrap().remove("scope");
        }
    });
    rejects(&unscoped, "schema", "gap without scope");
    let leaked = edit_row(&two, "files.jsonl", "path", "b/lib.rs", |f| {
        f["targets"] = json!(["a-lib"]);
    });
    rejects(&leaked, "build-facts-without-build-layer", "file under b");
}

#[test]
fn build_rejects_target_entity_mismatch() {
    use build_check::{rejects, valid, with};
    let base = valid();
    let swapped = with(&base, "targets.jsonl", |rows| {
        let lib = rows.iter().find(|t| t["key"] == "lib").unwrap()["entity"].clone();
        let bin = rows.iter().find(|t| t["key"] == "bin").unwrap()["entity"].clone();
        for row in rows.iter_mut() {
            if row["key"] == "lib" {
                row["entity"] = bin.clone();
            } else if row["key"] == "bin" {
                row["entity"] = lib.clone();
            }
        }
    });
    rejects(&swapped, "target-entity-mismatch", "swapped");
    let other = build_check::edit_row(&base, "targets.jsonl", "key", "lib", |t| {
        t["entity"] = "m:p:alpha().".into();
    });
    rejects(&other, "target-entity-mismatch", "another symbol");
}

#[test]
fn build_rejects_link_beside_root_gap() {
    use build_check::{add_gap, rejects, valid};
    let base = valid();
    let anchor = base.target("lib")["root"]["anchor"].clone();
    let out = add_gap(
        &base,
        serde_json::json!({"gap": "unresolved-root", "layer": "build", "target": "lib",
                           "anchor": anchor, "src": "adapter:fake/build"}),
    );
    rejects(&out, "link-beside-root-gap", "link and gap");
}

#[test]
fn build_rejects_root_gap_for_a_resolving_anchor() {
    use build_check::{add_gap, edit_row, rejects, valid};
    let base = valid();
    let anchor = base.target("lib")["root"]["anchor"].clone();
    let unlinked = edit_row(&base, "targets.jsonl", "key", "lib", |t| {
        t.as_object_mut().unwrap().remove("entity");
    });
    let out = add_gap(
        &unlinked,
        serde_json::json!({"gap": "unresolved-root", "layer": "build", "target": "lib",
                           "anchor": anchor, "src": "adapter:fake/build"}),
    );
    rejects(&out, "root-gap-for-resolving-anchor", "resolving anchor");
}

#[test]
fn build_accepts_stripped_anchor_with_root_gap() {
    use build_check::{accepts, rejects, with};
    use build_run::{CRATE, root, run, standard, target};
    let mut spec = standard();
    spec.build = vec![target(
        "gone",
        Some(root(CRATE, Some(("gen/none.rs", 0, 0)))),
    )];
    let out = run(&spec).unwrap();
    let row = out.target("gone");
    assert!(
        row["root"].get("anchor").is_none() && row.get("entity").is_none(),
        "{row}"
    );
    accepts(&out, "stripped anchor");
    let without = with(&out, "gaps.jsonl", |rows| {
        rows.retain(|g| g["gap"] != "unresolved-root");
    });
    rejects(&without, "unresolved-root-without-gap", "no gap");
}

#[test]
fn build_rejects_root_gap_anchor_mismatch() {
    use build_check::{TSCONFIG, rejects, valid, with};
    use build_run::{root, run, standard, target};
    // The target and its gap keep one anchor; a gap with another one is written by no engine.
    let mut spec = standard();
    spec.build = vec![target(
        "t",
        Some(root("s m p 1 nobody/", Some(("src/lib.rs", 1, 7)))),
    )];
    let out = run(&spec).unwrap();
    build_check::accepts(&out, "anchored unresolved root");
    let moved = with(&out, "gaps.jsonl", |rows| {
        let gap = rows
            .iter_mut()
            .find(|g| g["gap"] == "unresolved-root")
            .unwrap();
        gap["anchor"]["line"] = 0.into();
    });
    rejects(&moved, "root-gap-anchor-mismatch", "gap anchor only");
    assert!(
        valid()
            .gaps("unresolved-root")
            .iter()
            .any(|g| g["target"] == TSCONFIG)
    );
}

#[test]
fn build_rejects_absent_build_gap_mismatch() {
    use build_check::{accepts, add_gap, rejects, with, with_manifest, without_build};
    let absent = without_build("absent");
    let without = with(&absent, "gaps.jsonl", |rows| {
        rows.retain(|g| g["gap"] != "missing-layer");
    });
    rejects(
        &without,
        "missing-layer-gap-mismatch",
        "adapter scope without gap",
    );
    // A scope made from an index given directly.
    let snapshot = support::build_fixture("rust-basic");
    let tables: std::collections::BTreeMap<String, String> =
        gosanja_core::write::serialise(&snapshot)
            .named()
            .into_iter()
            .map(|(n, b)| {
                (
                    n.to_owned(),
                    String::from_utf8(b.unwrap_or_default().to_vec()).unwrap(),
                )
            })
            .collect();
    let direct = build_run::Out { tables };
    assert!(direct.manifest()["scopes"][0].get("adapter").is_none());
    accepts(&direct, "direct scip");
    let gap = serde_json::json!({"gap": "missing-layer", "layer": "build", "scope": ".", "src": "adapter:fake/build"});
    rejects(
        &add_gap(&direct, gap),
        "missing-layer-gap-mismatch",
        "direct scip with gap",
    );
    let mut manifest = absent.manifest();
    manifest["scopes"][0]
        .as_object_mut()
        .unwrap()
        .remove("adapter");
    rejects(
        &with_manifest(&absent, &manifest),
        "missing-layer-gap-mismatch",
        "adapterless scope with gap",
    );
}

#[test]
fn build_rejects_entity_mark_without_its_source() {
    use build_check::{accepts, edit_row, rejects, valid, without_build};
    use build_run::{CRATE, file, run, skipped, standard};
    use serde_json::json;
    let base = valid();
    let target_without_file = edit_row(&base, "entities.jsonl", "key", "m:p:beta().", |e| {
        e["test"] = "target".into();
    });
    rejects(
        &target_without_file,
        "entity-mark-without-source",
        "target without file mark",
    );
    let helper = base
        .rows("entities.jsonl")
        .into_iter()
        .find(|e| e["def"]["file"] == "tests/it.rs")
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(base.entity(&helper)["test"], "target");
    accepts(&base, "plain target over a target file");
    let file_over_target = edit_row(&base, "entities.jsonl", "key", &helper, |e| {
        e["test"] = "file".into();
    });
    rejects(
        &file_over_target,
        "entity-mark-without-source",
        "file over target",
    );
    let group = base
        .rows("entities.jsonl")
        .into_iter()
        .find(|e| e["symbol"] == CRATE && e["group"] == true)
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_owned();
    let marked_group = edit_row(&base, "entities.jsonl", "key", &group, |e| {
        e["test"] = "cfg".into();
    });
    rejects(&marked_group, "entity-mark-without-source", "group");
    let container = base
        .rows("entities.jsonl")
        .into_iter()
        .find(|e| e["virtual"] == true)
        .map(|e| e["key"].as_str().unwrap().to_owned());
    if let Some(container) = container {
        let marked = edit_row(&base, "entities.jsonl", "key", &container, |e| {
            e["test"] = "cfg".into();
        });
        rejects(&marked, "entity-mark-without-source", "virtual");
    }
    let failed = without_build("failed");
    let in_failed = edit_row(&failed, "entities.jsonl", "key", "m:p:beta().", |e| {
        e["test"] = "target".into();
    });
    rejects(
        &in_failed,
        "entity-mark-without-source",
        "target in a failed scope",
    );
    // A decls entity: its files all `cfg` is a source; one without a mark is none.
    let mut spec = standard();
    spec.build = vec![
        file("a.go", json!({"test": "cfg"})),
        file("b.go", json!({"test": "cfg"})),
    ];
    spec.build_entry = Some(skipped(&[
        ("a.go", "target-unknown"),
        ("b.go", "target-unknown"),
    ]));
    let decls = run(&spec).unwrap();
    assert_eq!(decls.entity("m:p:pkg/")["test"], "cfg");
    accepts(&decls, "decls over cfg files");
    let half = edit_row(&decls, "files.jsonl", "path", "b.go", |f| {
        f.as_object_mut().unwrap().remove("test");
    });
    let half = build_check::edit_row(&half, "files.jsonl", "path", "b.go", |f| {
        f["constraint"] = "c".into();
    });
    rejects(
        &half,
        "entity-mark-without-source",
        "decls over one cfg file",
    );
    accepts(&without_build("not-requested"), "syntax-only cfg");
}
