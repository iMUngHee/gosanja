// Build layer selection and test evidence through the CLI.

include!("world.rs");

#[test]
fn build_selection_matrix() {
    use build_cli::{World, entity, layer, ok, rows};
    use serde_json::json;
    let world = World::rust_basic();
    let syntax = world.lines(
        "syntax.jsonl",
        &[
            json!({"anchor": {"file": "src/views.rs", "line": 2, "col": 7}, "visibility": "pub"})
                .to_string(),
            json!({"row": "test", "anchor": {"file": "src/views.rs", "line": 10, "col": 7}, "test": "cfg"})
                .to_string(),
        ],
    );
    let good = world.lines(
        "good.jsonl",
        &[
            json!({"row": "target", "key": "cargo:demo:lib:demo", "kind": "lib", "name": "demo",
                   "root": {"symbol": "rust-analyzer cargo demo 0.1.0 crate/",
                            "anchor": {"file": "src/lib.rs", "line": 0, "col": 0}}})
            .to_string(),
            json!({"row": "file", "file": "src/views.rs", "targets": ["cargo:demo:lib:demo"], "test": "target"})
                .to_string(),
            json!({"row": "file", "file": "src/lib.rs", "targets": ["cargo:demo:lib:demo"], "constraint": "x"})
                .to_string(),
        ],
    );
    let bad = world.lines(
        "bad.jsonl",
        &[json!({"row": "file", "file": "src/lib.rs"}).to_string()],
    );
    for (outcome, mode, file, state) in [
        ("ok", "good", &good, "present"),
        ("reported failed", "build-failed", &good, "failed"),
        ("file missing", "build-no-file", &good, "failed"),
        ("rows discarded", "good", &bad, "failed"),
    ] {
        let adapter = world.adapter(
            &format!("adapter-{mode}-{}", file.len()),
            &[
                ("--mode", mode.to_owned()),
                ("--syntax-file", syntax.clone()),
                ("--build-file", file.clone()),
            ],
        );
        for (i, request) in [
            None,
            Some("base"),
            Some("base,syntax"),
            Some("base,build"),
            Some("base,syntax,build"),
        ]
        .into_iter()
        .enumerate()
        {
            let (output, out) = world.index(&adapter, &format!("out-{outcome}-{i}"), request);
            ok(&output);
            let wants_build = request.is_none_or(|r| r.contains("build"));
            let wants_syntax = request.is_none_or(|r| r.contains("syntax"));
            let expected = if wants_build { state } else { "not-requested" };
            let case = format!("{outcome} / {request:?}");
            assert_eq!(layer(&out, "build"), expected, "{case}");
            assert!(out.join("targets.jsonl").is_file(), "{case}");
            let targets = rows(&out, "targets.jsonl");
            assert_eq!(targets.is_empty(), expected != "present", "{case}");
            for row in rows(&out, "files.jsonl") {
                for field in ["targets", "test", "constraint"] {
                    assert_eq!(
                        row.get(field).is_some(),
                        expected == "present"
                            && (field != "constraint" || row["path"] == "src/lib.rs")
                            && (field != "test" || row["path"] == "src/views.rs"),
                        "{case}: {row}"
                    );
                }
            }
            let marked = entity(&out, "cargo:demo:views/empty().");
            if wants_syntax {
                assert_eq!(marked["test"], "cfg", "{case}");
            } else if expected == "present" {
                assert_eq!(marked["test"], "target", "{case}");
            } else {
                assert!(marked.get("test").is_none(), "{case}");
            }
        }
    }
}

#[test]
fn build_old_manifest_refused() {
    use build_cli::World;
    let world = World::rust_basic();
    let record = world.tmp.path().join("record");
    let adapter = world.adapter(
        "old",
        &[
            ("--contract", "0.4".to_owned()),
            ("--record", record.display().to_string()),
        ],
    );
    let (output, out) = world.index(&adapter, "out", None);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!out.exists());
    assert_eq!(std::fs::read_to_string(record).unwrap(), "describe\n");
}

#[test]
fn build_old_result_refused() {
    use build_cli::World;
    let world = World::rust_basic();
    let adapter = world.adapter("old-result", &[("--mode", "old-build-result".to_owned())]);
    let (output, out) = world.index(&adapter, "out", None);
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!out.exists());
}

#[test]
fn build_test_evidence_keeps_fold_and_identity() {
    use build_cli::{World, entity, ok, rows, scip};
    use serde_json::json;
    const PACKAGE: &str = "s m p 1 pkg/";
    const USER: &str = "s m q 1 use/f().";
    let world = World::with_files(&[
        ("a.go", "package pkg\n"),
        ("b.go", "package pkg\n"),
        ("c.go", "func f() {\n\tpkg.X()\n}\n"),
    ]);
    let index = world.tmp.path().join("index.scip");
    std::fs::write(
        &index,
        scip(&[
            ("a.go", vec![(PACKAGE, 1, vec![0, 8, 11], vec![])]),
            ("b.go", vec![(PACKAGE, 1, vec![0, 8, 11], vec![])]),
            (
                "c.go",
                vec![
                    (USER, 1, vec![0, 5, 6], vec![0, 0, 2, 1]),
                    (PACKAGE, 0, vec![1, 1, 4], vec![]),
                ],
            ),
        ]),
    )
    .unwrap();
    let run = |name: &str, syntax: &[String]| {
        let syntax = world.lines(&format!("{name}.jsonl"), syntax);
        let adapter = world.adapter(
            name,
            &[
                ("--index-file", index.display().to_string()),
                ("--syntax-file", syntax),
                ("--merged", format!(r#"["{PACKAGE}"]"#)),
            ],
        );
        let (output, out) = world.index(&adapter, name, Some("base,syntax"));
        ok(&output);
        out
    };
    let plain = run("plain", &[]);
    let tested = run(
        "tested",
        &[
            json!({"row": "test", "anchor": {"file": "a.go", "line": 0, "col": 8}, "test": "cfg"})
                .to_string(),
        ],
    );
    for out in [&plain, &tested] {
        let package = entity(out, "m:p:pkg/");
        assert!(
            package.get("decls").is_some() && package.get("group").is_none(),
            "{package}"
        );
        assert!(
            rows(out, "syntax.jsonl")
                .iter()
                .all(|r| r["key"] != "m:p:pkg/")
        );
    }
    let keys = |out| {
        rows(out, "entities.jsonl")
            .iter()
            .map(|e| e["key"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&plain), keys(&tested));
    assert_eq!(
        rows(&plain, "relations.jsonl"),
        rows(&tested, "relations.jsonl")
    );
    let (output, diff) = world.diff(&plain, &tested);
    ok(&output);
    assert_eq!(rows(&diff, "entities.jsonl"), [] as [serde_json::Value; 0]);
    assert_eq!(rows(&diff, "relations.jsonl"), [] as [serde_json::Value; 0]);
}

#[test]
fn build_test_evidence_keeps_anchor_only_rows() {
    use build_cli::{World, entity, layer, manifest, ok, rows};
    use serde_json::json;
    let world = World::rust_basic();
    let anchor = |line: u32| json!({"file": "src/views.rs", "line": line, "col": 7});
    let syntax = vec![
        json!({"anchor": anchor(2)}).to_string(),
        json!({"anchor": anchor(10)}).to_string(),
        json!({"anchor": anchor(14), "visibility": "pub"}).to_string(),
    ];
    let mut tested = syntax.clone();
    for line in [2, 10, 14] {
        tested.push(json!({"row": "test", "anchor": anchor(line), "test": "cfg"}).to_string());
    }
    let build = world.lines(
        "build.jsonl",
        &[
            json!({"row": "target", "key": "cargo:demo:lib:demo", "kind": "lib", "name": "demo"})
                .to_string(),
            json!({"row": "file", "file": "src/views.rs", "targets": ["cargo:demo:lib:demo"], "test": "file"})
                .to_string(),
        ],
    );
    let run = |name: &str, lines: &[String]| {
        let syntax = world.lines(&format!("{name}.jsonl"), lines);
        let adapter = world.adapter(
            name,
            &[("--syntax-file", syntax), ("--build-file", build.clone())],
        );
        let (output, out) = world.index(&adapter, name, None);
        ok(&output);
        out
    };
    let plain = run("plain", &syntax);
    let marked = run("marked", &tested);
    assert_eq!(layer(&plain, "build"), "present");
    assert_eq!(layer(&marked, "build"), "present");
    assert_eq!(
        std::fs::read(plain.join("syntax.jsonl")).unwrap(),
        std::fs::read(marked.join("syntax.jsonl")).unwrap()
    );
    assert_eq!(
        manifest(&plain)["counts"]["syntax"],
        manifest(&marked)["counts"]["syntax"]
    );
    assert_eq!(rows(&plain, "syntax.jsonl").len(), 3);
    for key in [
        "cargo:demo:views/render().",
        "cargo:demo:views/empty().",
        "cargo:demo:views/badge().",
    ] {
        assert_eq!(entity(&marked, key)["test"], "cfg", "{key}");
        assert_eq!(entity(&plain, key)["test"], "file", "{key}");
    }
    let (output, diff) = world.diff(&plain, &marked);
    ok(&output);
    for table in [
        "entities.jsonl",
        "externals.jsonl",
        "relations.jsonl",
        "candidates.jsonl",
    ] {
        assert!(rows(&diff, table).is_empty(), "{table}");
    }
    let not_compared = &manifest(&diff)["not_compared"];
    assert_eq!(not_compared, &json!(["build", "test"]));
}
