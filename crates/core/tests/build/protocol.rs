// Build and test sidecar handling through a fake adapter.

include!("run.rs");

#[test]
fn build_layer_outcome_table() {
    use build_run::{run, standard, target};
    let kept = || {
        let mut spec = standard();
        spec.build = vec![target("k", None)];
        spec
    };
    // Not requested: never opened, no gap.
    let mut spec = kept();
    spec.request = Some("base,syntax");
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "not-requested");
    assert_eq!(out.layer_gaps("build"), [] as [serde_json::Value; 0]);
    assert_eq!(out.rows("targets.jsonl"), [] as [serde_json::Value; 0]);
    // Requested, not offered: absent with a missing-layer gap naming the scope.
    let mut spec = kept();
    spec.offered = Some(r#"["base","syntax","calls"]"#);
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "absent");
    let gaps = out.layer_gaps("build");
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["gap"], "missing-layer");
    assert_eq!(gaps[0]["scope"], ".");
    // An entry that fails the result schema fails the run.
    let mut spec = kept();
    spec.mode = Some("build-malformed");
    assert_eq!(run(&spec).err().map(|e| e.exit_code()), Some(3));
    // Missing entry, reported failure, missing file, discarded rows.
    for (mode, rows, detail, code) in [
        (
            "build-no-status",
            vec![target("k", None)],
            "no-status",
            None,
        ),
        (
            "build-failed",
            vec![target("k", None)],
            "reported-failed",
            Some("probe-build-error"),
        ),
        ("build-no-file", vec![], "file-missing", None),
        (
            "good",
            vec![r#"{"row":"file","file":"src/lib.rs"}"#.to_owned()],
            "row-invalid",
            None,
        ),
    ] {
        let mut spec = standard();
        spec.mode = Some(mode);
        spec.build = rows;
        let out = run(&spec).unwrap();
        assert_eq!(out.layer("build"), "failed", "{mode}");
        let gaps = out.layer_gaps("build");
        assert_eq!(gaps.len(), 1, "{mode}: {gaps:?}");
        assert_eq!(gaps[0]["gap"], "failed-layer", "{mode}");
        assert_eq!(gaps[0]["detail"], detail, "{mode}");
        assert_eq!(gaps[0]["scope"], ".", "{mode}");
        assert_eq!(gaps[0]["code"].as_str(), code, "{mode}");
        assert!(out.rows("targets.jsonl").is_empty(), "{mode}");
        assert_eq!(out.layer("syntax"), "present", "{mode}");
    }
    // Read and kept.
    let out = run(&kept()).unwrap();
    assert_eq!(out.layer("build"), "present");
    assert_eq!(out.rows("targets.jsonl").len(), 1);
    assert_eq!(out.manifest()["counts"]["targets"], 1);
}

#[test]
fn build_layer_unsolicited_malformed_entry_is_removed() {
    use build_run::{run, standard};
    let mut spec = standard();
    spec.offered = Some(r#"["base","syntax","calls"]"#);
    spec.mode = Some("build-malformed");
    spec.build = vec!["not json".to_owned()];
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "absent");
    let gaps = out.layer_gaps("build");
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["gap"], "missing-layer");
}

#[test]
fn build_layer_terminal_phases() {
    use build_run::{file, root, run, standard, target};
    use serde_json::json;
    let long = "é".repeat(600);
    for (case, rows) in [
        (
            "schema",
            vec![r#"{"row":"target","key":"k","kind":"Lib","name":"n"}"#.to_owned()],
        ),
        ("no fact", vec![file("src/lib.rs", json!({}))]),
        (
            "non-global root",
            vec![target("k", Some(root("local 1", None)))],
        ),
        (
            "bare unicode root",
            vec![target("k", Some(root("s m p 1 café#", None)))],
        ),
        ("key grammar", vec![target(&long, None)]),
        ("repeated key", vec![target("k", None), target("k", None)]),
        (
            "unknown target",
            vec![file("src/lib.rs", json!({"targets": ["nobody"]}))],
        ),
        (
            "file in two rows",
            vec![
                target("k", None),
                file("src/lib.rs", json!({"targets": ["k"]})),
                file("src/lib.rs", json!({"targets": ["k"]})),
            ],
        ),
    ] {
        let mut spec = standard();
        spec.build = rows;
        let out = run(&spec).unwrap();
        assert_eq!(out.layer("build"), "failed", "{case}");
        let gaps = out.layer_gaps("build");
        assert_eq!(gaps.len(), 1, "{case}: {gaps:?}");
        assert_eq!(gaps[0]["detail"], "row-invalid", "{case}");
        assert_eq!(out.layer("base"), "present", "{case}");
        assert_eq!(out.layer("syntax"), "present", "{case}");
        assert!(out.rows("targets.jsonl").is_empty(), "{case}");
        assert!(!out.rows("entities.jsonl").is_empty(), "{case}");
    }
}

#[test]
fn build_root_anchor_must_reach_a_kept_declaration() {
    use build_run::{CRATE, root, run, standard, target};
    let spec = |main_enclosing: &[i32]| {
        let mut spec = standard();
        spec.documents[1] = support::doc(
            "src/main.rs",
            vec![support::occ(
                CRATE,
                support::DEF,
                &[0, 0, 0],
                main_enclosing,
            )],
        );
        spec.build = vec![
            target("lib", Some(root(CRATE, Some(("src/lib.rs", 0, 0))))),
            target("bin", Some(root(CRATE, Some(("src/main.rs", 0, 0))))),
        ];
        spec
    };
    // The main definition's enclosing range ends before it starts, so the core drops it.
    let out = run(&spec(&[0, 5, 0, 1])).unwrap();
    let survivors: Vec<_> = out
        .rows("entities.jsonl")
        .into_iter()
        .filter(|e| e["symbol"] == CRATE && e.get("def").is_some())
        .collect();
    assert_eq!(survivors.len(), 1, "{survivors:?}");
    let lib = out.target("lib");
    assert_eq!(lib["entity"], survivors[0]["key"]);
    let bin = out.target("bin");
    assert!(bin.get("entity").is_none(), "{bin}");
    assert_eq!(bin["root"]["anchor"]["file"], "src/main.rs");
    let gaps = out.gaps("unresolved-root");
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["target"], "bin");
    assert_eq!(gaps[0]["anchor"]["file"], "src/main.rs");
    // Paired: with a valid range each target resolves to its own member.
    let out = run(&spec(&[0, 0, 1, 0])).unwrap();
    assert_eq!(
        out.target("lib")["entity"],
        out.member_in(CRATE, "src/lib.rs")
    );
    assert_eq!(
        out.target("bin")["entity"],
        out.member_in(CRATE, "src/main.rs")
    );
    assert_eq!(out.gaps("unresolved-root"), [] as [serde_json::Value; 0]);
}

#[test]
fn build_layer_roots_resolve() {
    use build_run::{ALPHA, CRATE, PACKAGE, root, run, standard, target};
    let mut spec = standard();
    spec.build = vec![
        target("plain", Some(root(ALPHA, None))),
        target("declared", Some(root(PACKAGE, None))),
        target("member", Some(root(CRATE, Some(("src/main.rs", 0, 0))))),
    ];
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "present");
    assert_eq!(out.target("plain")["entity"], "m:p:alpha().");
    assert_eq!(out.target("declared")["entity"], "m:p:pkg/");
    assert!(out.entity("m:p:pkg/").get("decls").is_some());
    assert_eq!(
        out.target("member")["entity"],
        out.member_in(CRATE, "src/main.rs")
    );
    assert_eq!(out.gaps("unresolved-root"), [] as [serde_json::Value; 0]);
    assert_eq!(out.validation(), Vec::<String>::new());
}

#[test]
fn build_layer_unresolved_roots() {
    use support::{DEF, doc, occ};
    use build_run::{ALPHA, CRATE, LIB, root, run, standard, target};
    let mut spec = standard();
    // Two definitions of `dup()` named on one line: a name-only anchor matches both.
    spec.documents.push(doc(
        "src/dup.rs",
        vec![
            occ("s m p 1 dup().", DEF, &[0, 3, 6], &[0, 0, 11]),
            occ("s m p 1 dup().", DEF, &[0, 15, 18], &[0, 12, 23]),
        ],
    ));
    spec.sources
        .push(("src/dup.rs", "fn dup() {} fn dup() {}\n".to_owned()));
    let ambiguous = serde_json::json!({"symbol": "s m p 1 dup().", "anchor": {"file": "src/dup.rs", "line": 0, "name": "dup"}});
    spec.build = vec![
        target("group", Some(root(CRATE, None))),
        target("elsewhere", Some(root(CRATE, Some(("src/lib.rs", 0, 7))))),
        target("ambiguous", Some(ambiguous)),
        target("nobody", Some(root("s m p 1 nobody().", None))),
        target("other", Some(root(ALPHA, Some(("src/lib.rs", 1, 7))))),
    ];
    assert!(LIB.contains("beta"));
    let out = run(&spec).unwrap();
    for key in ["group", "elsewhere", "ambiguous", "nobody", "other"] {
        let row = out.target(key);
        assert!(row.get("entity").is_none(), "{key}: {row}");
        let gaps: Vec<_> = out
            .gaps("unresolved-root")
            .into_iter()
            .filter(|g| g["target"] == key)
            .collect();
        assert_eq!(gaps.len(), 1, "{key}");
        assert_eq!(gaps[0]["anchor"], row["root"]["anchor"].clone(), "{key}");
        assert_eq!(gaps[0]["layer"], "build");
    }
    assert_eq!(out.gaps("unresolved-root")[0]["src"], "adapter:fake/build");
    assert_eq!(out.validation(), Vec::<String>::new());
}

#[test]
fn build_layer_root_anchor_utf16() {
    use support::{DEF, doc, occ};
    use build_run::{root, run, standard, target};
    const GAMMA: &str = "s m p 1 gamma().";
    let mut spec = standard();
    // `gamma` follows `é` (one UTF-16 unit, two bytes) and the crab (two units, four bytes).
    spec.sources.push((
        "src/uni.rs",
        "é🦀 fn gamma() {}\nfn gamma() {}\n".to_owned(),
    ));
    spec.documents.push(doc(
        "src/uni.rs",
        vec![
            occ(GAMMA, DEF, &[0, 10, 15], &[0, 7, 20]),
            occ(GAMMA, DEF, &[1, 3, 8], &[1, 0, 13]),
        ],
    ));
    spec.encoding = Some("utf-16");
    spec.build = vec![
        target("crab", Some(root(GAMMA, Some(("src/uni.rs", 0, 7))))),
        target("split", Some(root(GAMMA, Some(("src/uni.rs", 0, 2))))),
    ];
    let out = run(&spec).unwrap();
    let crab = out.target("crab");
    assert_eq!(crab["entity"], out.member_in(GAMMA, "src/uni.rs"));
    assert_eq!(crab["root"]["anchor"]["col"], 10);
    let split = out.target("split");
    assert!(split.get("entity").is_none());
    assert!(split["root"].get("anchor").is_none(), "{split}");
    let gaps = out.gaps("unresolved-root");
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["target"], "split");
    assert!(gaps[0].get("anchor").is_none());
}

#[test]
fn build_layer_rootless_target() {
    use build_run::{run, standard, target};
    let mut spec = standard();
    spec.build = vec![target("k", None)];
    let out = run(&spec).unwrap();
    let row = out.target("k");
    assert_eq!(row["src"], "adapter:fake/build");
    assert_eq!(row["scope"], ".");
    assert!(row.get("entity").is_none() && row.get("root").is_none());
    assert_eq!(out.gaps("unresolved-root"), [] as [serde_json::Value; 0]);
}

#[test]
fn build_layer_unknown_file() {
    use build_run::{file, run, skipped, standard, target};
    use serde_json::json;
    let mut spec = standard();
    spec.build = vec![
        target("k", None),
        file("gen/x.rs", json!({"targets": ["k"]})),
    ];
    spec.build_entry = Some(skipped(&[
        ("gen/x.rs", "targets-incomplete"),
        ("gen/y.rs", "target-unknown"),
    ]));
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "present");
    let unknown: Vec<_> = out
        .gaps("unknown-file")
        .iter()
        .map(|g| g["file"].clone())
        .collect();
    assert_eq!(unknown, [json!("gen/x.rs"), json!("gen/y.rs")]);
    assert!(
        out.layer_gaps("build")
            .iter()
            .all(|g| g["gap"] == "unknown-file")
    );
}

#[test]
fn build_layer_target_unknown_skip() {
    use build_run::{file, run, skipped, standard, target};
    use serde_json::json;
    let mut spec = standard();
    spec.build = vec![
        target("k", None),
        file("src/main.rs", json!({"constraint": "x"})),
        file("src/lib.rs", json!({"targets": ["k"]})),
    ];
    spec.build_entry = Some(skipped(&[
        ("src/main.rs", "target-unknown"),
        ("src/lib.rs", "targets-incomplete"),
    ]));
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "present");
    let main = out.file("src/main.rs");
    assert!(main.get("targets").is_none());
    assert_eq!(main["constraint"], "x");
    assert_eq!(out.file("src/lib.rs")["targets"], json!(["k"]));
    let skips: Vec<_> = out
        .gaps("skipped-file")
        .iter()
        .map(|g| (g["file"].clone(), g["code"].clone(), g["layer"].clone()))
        .collect();
    assert_eq!(
        skips,
        [
            (
                json!("src/lib.rs"),
                json!("targets-incomplete"),
                json!("build")
            ),
            (
                json!("src/main.rs"),
                json!("target-unknown"),
                json!("build")
            ),
        ]
    );
    assert_eq!(out.validation(), Vec::<String>::new());
    // Beside that accepted pair, a constraint-only row without its skip discards the layer.
    spec.build
        .push(file("tests/it.rs", json!({"constraint": "y"})));
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "failed");
    assert_eq!(out.layer_gaps("build")[0]["detail"], "row-invalid");
    assert_eq!(out.layer("syntax"), "present");
}

#[test]
fn build_layer_unresolved_root_names_a_target_key() {
    use build_run::{root, run, standard, target};
    let mut spec = standard();
    let key = "packages/app/tsconfig.json";
    spec.build = vec![target(key, Some(root("s m p 1 nobody/", None)))];
    let out = run(&spec).unwrap();
    let gaps = out.gaps("unresolved-root");
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["target"], key);
    assert_eq!(out.validation(), Vec::<String>::new());
}

#[test]
fn build_layer_unsorted_targets_discard() {
    use build_run::{file, run, standard, target};
    use serde_json::json;
    for targets in [json!(["b", "a"]), json!(["a", "a"])] {
        let mut spec = standard();
        spec.build = vec![
            target("a", None),
            target("b", None),
            file("src/lib.rs", json!({ "targets": targets })),
        ];
        let out = run(&spec).unwrap();
        assert_eq!(out.layer("build"), "failed", "{targets}");
        assert_eq!(out.layer_gaps("build")[0]["detail"], "row-invalid");
        assert_eq!(out.layer("syntax"), "present");
    }
}

#[test]
fn build_layer_test_rows_alias_one_definition() {
    use build_run::{run, standard};
    for (first, second) in [
        (
            r#"{"row":"test","anchor":{"file":"src/lib.rs","line":0,"col":7},"test":"cfg"}"#,
            r#"{"row":"test","anchor":{"file":"src/lib.rs","line":0,"col":7},"test":"cfg"}"#,
        ),
        (
            r#"{"row":"test","anchor":{"file":"src/lib.rs","line":0,"col":7},"test":"cfg"}"#,
            r#"{"row":"test","anchor":{"file":"src/lib.rs","line":0,"name":"alpha"},"test":"cfg"}"#,
        ),
    ] {
        let mut spec = standard();
        spec.syntax = vec![first.to_owned(), second.to_owned()];
        let out = run(&spec).unwrap();
        assert_eq!(out.layer("syntax"), "failed", "{second}");
        let gaps = out.layer_gaps("syntax");
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0]["detail"], "row-invalid");
        assert!(out.entity("m:p:alpha().").get("test").is_none());
    }
}

#[test]
fn build_layer_syntax_row_and_test_row_for_one_definition() {
    use build_run::{run, standard};
    let mut spec = standard();
    spec.syntax = vec![
        r#"{"anchor":{"file":"src/lib.rs","line":0,"col":7},"visibility":"pub"}"#.to_owned(),
        r#"{"row":"test","anchor":{"file":"src/lib.rs","line":0,"name":"alpha"},"test":"cfg"}"#
            .to_owned(),
    ];
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("syntax"), "present");
    let syntax = out.rows("syntax.jsonl");
    assert_eq!(syntax.len(), 1);
    assert_eq!(syntax[0]["visibility"], "pub");
    assert_eq!(out.entity("m:p:alpha().")["test"], "cfg");
}

#[test]
fn build_layer_file_row_without_targets_needs_its_skip() {
    use build_run::{file, run, standard};
    use serde_json::json;
    let mut spec = standard();
    spec.build = vec![file("src/main.rs", json!({"constraint": "x"}))];
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "failed");
    assert_eq!(out.layer_gaps("build")[0]["detail"], "row-invalid");
    assert_eq!(out.layer("base"), "present");
    assert_eq!(out.layer("syntax"), "present");
}

#[test]
fn build_layer_file_added_by_calls_gets_no_build_facts() {
    use build_run::{file, run, skipped, standard, target};
    use serde_json::json;
    let mut spec = standard();
    spec.sources
        .push(("gen/out.rs", "// generated\n".to_owned()));
    spec.calls = vec![
        json!({
            "caller": {"file": "src/lib.rs", "line": 0, "col": 7},
            "callee": {"file": "src/lib.rs", "line": 1, "col": 7},
            "mechanism": "static", "resolution": "direct",
            "site": {"file": "gen/out.rs", "line": 0, "col": 0}, "build": "dev"
        })
        .to_string(),
    ];
    spec.build = vec![
        target("k", None),
        file("gen/out.rs", json!({"targets": ["k"]})),
    ];
    spec.build_entry = Some(skipped(&[("gen/out.rs", "parse-failed")]));
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("calls"), "present");
    let added = out.file("gen/out.rs");
    for field in ["targets", "test", "constraint"] {
        assert!(added.get(field).is_none(), "{field}: {added}");
    }
    let unknown = out.gaps("unknown-file");
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0]["file"], "gen/out.rs");
    assert!(
        out.gaps("skipped-file")
            .iter()
            .all(|g| g["layer"] != "build"),
        "{:?}",
        out.gaps("skipped-file")
    );
}

#[test]
fn build_layer_discard_before_row_level() {
    use build_run::{root, run, standard, target};
    let mut spec = standard();
    spec.build = vec![
        target("unresolved", Some(root("s m p 1 nobody().", None))),
        r#"{"row":"target","key":"bad key","kind":"lib","name":"n"}"#.to_owned(),
    ];
    let out = run(&spec).unwrap();
    assert_eq!(out.layer("build"), "failed");
    let gaps = out.layer_gaps("build");
    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert_eq!(gaps[0]["gap"], "failed-layer");
    assert_eq!(out.gaps("unresolved-root"), [] as [serde_json::Value; 0]);
}

#[test]
fn build_layer_test_rows_are_not_syntax_rows() {
    use build_run::{run, standard};
    let plain = {
        let mut spec = standard();
        spec.syntax = vec![r#"{"anchor":{"file":"src/lib.rs","line":1,"col":7}}"#.to_owned()];
        spec
    };
    let mut tested = plain.clone();
    tested.syntax.extend([
        r#"{"row":"test","anchor":{"file":"src/lib.rs","line":0,"col":7},"test":"cfg"}"#.to_owned(),
        r#"{"row":"test","anchor":{"file":"src/lib.rs","line":1,"col":7},"test":"attribute"}"#
            .to_owned(),
        r#"{"row":"test","anchor":{"file":"src/lib.rs","line":9,"col":0},"test":"cfg"}"#.to_owned(),
    ]);
    let before = run(&plain).unwrap();
    let after = run(&tested).unwrap();
    assert_eq!(after.layer("syntax"), "present");
    assert_eq!(after.tables["syntax.jsonl"], before.tables["syntax.jsonl"]);
    assert_eq!(
        after.manifest()["counts"]["syntax"],
        before.manifest()["counts"]["syntax"]
    );
    assert_eq!(after.rows("syntax.jsonl").len(), 1);
    assert_eq!(after.entity("m:p:alpha().")["test"], "cfg");
    assert_eq!(after.entity("m:p:beta().")["test"], "attribute");
    let unanchored = after.gaps("unanchored");
    assert_eq!(unanchored.len(), 1);
    assert_eq!(unanchored[0]["anchor"]["line"], 9);
}

#[test]
fn build_layer_conflicting_membership_discards() {
    use build_run::{file, run, skipped, standard, target};
    use serde_json::json;
    for (case, rows, skips) in [
        (
            "targets beside target-unknown",
            vec![
                target("k", None),
                file("src/lib.rs", json!({"targets": ["k"]})),
            ],
            vec![("src/lib.rs", "target-unknown")],
        ),
        (
            "targets-incomplete without targets",
            vec![
                target("k", None),
                file("src/lib.rs", json!({"constraint": "x"})),
            ],
            vec![
                ("src/lib.rs", "target-unknown"),
                ("src/lib.rs", "targets-incomplete"),
            ],
        ),
    ] {
        let mut spec = standard();
        spec.build = rows;
        spec.build_entry = Some(skipped(&skips));
        let out = run(&spec).unwrap();
        assert_eq!(out.layer("build"), "failed", "{case}");
        assert_eq!(out.layer_gaps("build").len(), 1, "{case}");
        assert_eq!(
            out.layer_gaps("build")[0]["detail"],
            "row-invalid",
            "{case}"
        );
    }
}
