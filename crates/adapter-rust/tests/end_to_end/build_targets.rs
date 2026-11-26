use super::*;

fn targets() -> Case {
    let case = Case::new();
    fs::remove_dir_all(&case.root).expect("remove the default fixture copy");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-targets");
    support::copy_tree(&fixture, &case.root);
    symlink(
        support::adapter_binary(),
        case.bin.join("gosanja-adapter-rust"),
    )
    .expect("install adapter");
    case
}

fn snapshot(case: &Case, selection: Option<&str>) -> PathBuf {
    let out = case.temp.path().join(selection.unwrap_or("default"));
    let mut command = index_command(case, &case.root, &out);
    if let Some(selection) = selection {
        command.args(["--layers", selection]);
    }
    ok(&command.output().expect("index runs"));
    ok(&case
        .command(engine())
        .arg("validate")
        .arg(&out)
        .arg("--root")
        .arg(&case.root)
        .output()
        .expect("validate runs"));
    out
}

#[test]
fn build_targets_one_row_per_cargo_target() {
    let case = targets();
    let rows = table(&snapshot(&case, Some("base,build")), "targets.jsonl");
    let keys: BTreeSet<_> = rows
        .iter()
        .map(|row| row["key"].as_str().expect("target key"))
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "cargo:indep:lib:indep",
            "cargo:pm:proc-macro:pm",
            "cargo:rust-targets:bench:b",
            "cargo:rust-targets:bench:lib",
            "cargo:rust-targets:bin:feat",
            "cargo:rust-targets:bin:rust-targets",
            "cargo:rust-targets:build:build-script-build",
            "cargo:rust-targets:example:e",
            "cargo:rust-targets:example:plain",
            "cargo:rust-targets:lib:rust_targets",
            "cargo:rust-targets:test:it",
            "cargo:rust-targets:test:lib",
            "cargo:rust-targets:test:noharness",
        ])
    );
    assert_eq!(rows.len(), keys.len());
    for row in &rows {
        let test = matches!(row["kind"].as_str(), Some("test" | "bench"));
        assert_eq!(row.get("test"), test.then_some(&Value::Bool(true)), "{row}");
        if row["name"] == "feat" {
            assert_eq!(row["required_features"], json!(["x"]));
        }
    }
}

#[test]
fn build_roots_resolve_to_crate_members() {
    let case = targets();
    let out = snapshot(&case, Some("base,build"));
    let targets = table(&out, "targets.jsonl");
    let entities = table(&out, "entities.jsonl");
    for target in targets {
        let anchor = &target["root"]["anchor"];
        assert_eq!(anchor["line"], 0, "{target}");
        assert_eq!(anchor["col"], 0, "{target}");
        let entity = entities
            .iter()
            .find(|entity| entity["key"] == target["entity"])
            .expect("linked entity");
        assert_eq!(entity["def"]["file"], anchor["file"], "{entity}");
        assert_eq!(entity["symbol"], target["root"]["symbol"], "{entity}");
        if target["kind"] == "lib" && target["name"] == "rust_targets" {
            assert_eq!(target["entity"], "cargo:rust-targets:crate/~7");
        }
        if target["name"] == "indep" {
            assert_eq!(target["entity"], "cargo:indep:crate/");
        }
    }
    assert!(
        !table(&out, "gaps.jsonl")
            .iter()
            .any(|gap| gap["gap"] == "unresolved-root")
    );
}

#[test]
fn build_root_omitted_by_the_indexer() {
    let case = targets();
    let path = case.root.join("index.scip");
    let mut index = Index::parse_from_bytes(&fs::read(&path).unwrap()).unwrap();
    let file = index
        .documents
        .iter_mut()
        .find(|doc| doc.relative_path == "src/bin/feat.rs")
        .unwrap();
    assert!(
        file.occurrences
            .iter()
            .any(|occ| occ.symbol.ends_with(" crate/"))
    );
    file.occurrences
        .retain(|occ| !occ.symbol.ends_with(" crate/"));
    file.occurrences.push(Occurrence {
        symbol: "rust-analyzer cargo rust-targets 0.1.0 crate/".into(),
        range: vec![0, 0, 1],
        ..Occurrence::default()
    });
    fs::write(&path, index.write_to_bytes().unwrap()).unwrap();
    let out = snapshot(&case, Some("base,build"));
    let target = table(&out, "targets.jsonl")
        .into_iter()
        .find(|row| row["name"] == "feat")
        .unwrap();
    assert!(target.get("root").is_none());
    assert!(target.get("entity").is_none());
    assert!(
        !table(&out, "gaps.jsonl")
            .iter()
            .any(|gap| gap["gap"] == "unresolved-root")
    );
}

#[test]
fn build_membership_from_dep_info() {
    let case = targets();
    let out = snapshot(&case, Some("base,build"));
    let rows = table(&out, "files.jsonl");
    let lib = "cargo:rust-targets:lib:rust_targets";
    let bin = "cargo:rust-targets:bin:rust-targets";
    let it = "cargo:rust-targets:test:it";
    for (file, membership, test) in [
        ("src/lib.rs", vec![lib], None),
        ("src/shared.rs", vec![bin, lib], None),
        ("src/a.rs", vec![lib, it], None),
        ("src/a/child.rs", vec![lib], None),
        ("src/child.rs", vec![lib], None),
        ("src/tests.rs", vec![lib], Some("cfg")),
        (
            "indep/src/only_test.rs",
            vec!["cargo:indep:lib:indep"],
            Some("cfg"),
        ),
        ("tests/it.rs", vec![it], Some("target")),
        (
            "src/bin/feat.rs",
            vec!["cargo:rust-targets:bin:feat", it],
            None,
        ),
        (
            "build.rs",
            vec!["cargo:rust-targets:build:build-script-build"],
            None,
        ),
        (
            "build_m.rs",
            vec!["cargo:rust-targets:build:build-script-build"],
            None,
        ),
        (
            "examples/support/e_t.rs",
            vec!["cargo:rust-targets:example:e"],
            Some("cfg"),
        ),
    ] {
        let row = rows
            .iter()
            .find(|row| row["path"] == file)
            .unwrap_or_else(|| panic!("missing {file}"));
        assert_eq!(row["targets"], json!(membership), "{row}");
        assert_eq!(row["test"].as_str(), test, "{row}");
    }
}

#[test]
fn build_selection_with_the_rust_adapter() {
    let case = targets();
    for (layers, build, syntax) in [
        (None, true, true),
        (Some("base"), false, false),
        (Some("base,syntax"), false, true),
        (Some("base,build"), true, false),
    ] {
        let out = snapshot(&case, layers);
        let manifest: Value =
            serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["scopes"][0]["layers"]["build"],
            if build { "present" } else { "not-requested" }
        );
        assert_eq!(table(&out, "targets.jsonl").is_empty(), !build);
        let entities = table(&out, "entities.jsonl");
        let gate = entities
            .iter()
            .find(|row| row["name"] == "gated")
            .expect("gate admitted");
        let test = entities
            .iter()
            .find(|row| row["name"] == "it")
            .expect("integration test admitted");
        assert_eq!(gate["test"].as_str(), syntax.then_some("cfg"));
        assert_eq!(test["test"].as_str(), build.then_some("target"));
    }
}

#[test]
fn build_target_outside_the_root_is_not_reported() {
    use std::io::Write as _;
    let case = targets();
    fs::write(case.temp.path().join("outside.rs"), "fn main() {}").unwrap();
    let mut manifest = fs::OpenOptions::new()
        .append(true)
        .open(case.root.join("Cargo.toml"))
        .unwrap();
    writeln!(
        manifest,
        "\n[[bin]]\nname = \"outside\"\npath = \"../outside.rs\""
    )
    .unwrap();
    let out = snapshot(&case, Some("base,build"));
    let targets = table(&out, "targets.jsonl");
    assert_eq!(targets.len(), 13);
    assert!(targets.iter().all(|row| row["name"] != "outside"));
    let files = table(&out, "files.jsonl");
    assert!(files.iter().all(|row| row["path"] != "../outside.rs"));
    let lib = files
        .iter()
        .find(|row| row["path"] == "src/lib.rs")
        .unwrap();
    assert_eq!(
        lib["targets"],
        json!(["cargo:rust-targets:lib:rust_targets"])
    );
}

#[test]
fn build_check_failure() {
    use std::io::Write as _;
    for (cfg, expected) in [("", "failed"), ("#[cfg(feature = \"x\")]\n", "present")] {
        let case = targets();
        let mut source = fs::OpenOptions::new()
            .append(true)
            .open(case.root.join("src/lib.rs"))
            .unwrap();
        writeln!(
            source,
            "\n{cfg}compile_error!(\"the controlled check failure\");"
        )
        .unwrap();
        let out = snapshot(&case, Some("base,syntax,build"));
        let manifest: Value =
            serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
        let layers = &manifest["scopes"][0]["layers"];
        assert_eq!(layers["base"], "present");
        assert_eq!(layers["syntax"], "present");
        assert_eq!(layers["build"], expected);
        let files = table(&out, "files.jsonl");
        let marked = files
            .iter()
            .find(|file| file["path"] == "tests/it.rs")
            .unwrap();
        assert!(marked.get("test").is_none());
        let entities = table(&out, "entities.jsonl");
        let gated = entities
            .iter()
            .find(|entity| entity["name"] == "gated")
            .unwrap();
        assert!(gated.get("test").is_none());
        let gaps = table(&out, "gaps.jsonl");
        for code in [
            "test-unknown",
            if cfg.is_empty() {
                "check-failed"
            } else {
                "targets-incomplete"
            },
        ] {
            assert!(
                gaps.iter().any(|gap| gap["code"] == code),
                "missing {code}: {gaps:?}"
            );
        }
    }
}

#[test]
fn build_configured_compiler_is_not_used() {
    use std::os::unix::fs::PermissionsExt;
    let case = targets();
    let record = case.temp.path().join("compiler-trace");
    let config = case.root.join(".cargo");
    let wrapper = case.temp.path().join("compiler");
    fs::create_dir(&config).unwrap();
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$CARGO_BUILD_BUILD_DIR\" >> '{}'\nexec \"$@\"\n",
            record.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        config.join("config.toml"),
        format!(
            "[build]\nrustc-wrapper = {:?}\nrustc-workspace-wrapper = {:?}\n",
            wrapper.display().to_string(),
            wrapper.display().to_string()
        ),
    )
    .unwrap();
    let control = case.temp.path().join("control-build");
    ok(&case
        .command(&case.bin.join("cargo"))
        .current_dir(&case.root)
        .env("CARGO_TARGET_DIR", case.temp.path().join("control-target"))
        .env("CARGO_BUILD_BUILD_DIR", &control)
        .args(["check", "--locked", "--offline", "--lib"])
        .output()
        .unwrap());
    let trace = fs::read_to_string(&record).unwrap();
    assert!(trace.lines().any(|line| Path::new(line) == control));
    let out = snapshot(&case, Some("base,syntax,build"));
    let trace = fs::read_to_string(record).unwrap();
    assert!(
        !trace.lines().any(|line| line.contains("/work/check-")),
        "{trace}"
    );
    assert!(
        table(&out, "entities.jsonl")
            .iter()
            .any(|entity| entity["name"] == "gated" && entity["test"] == "cfg")
    );
}

#[test]
fn build_root_with_unspecified_encoding_is_not_admitted() {
    use std::io::Write as _;
    let case = targets();
    let mut manifest = fs::OpenOptions::new()
        .append(true)
        .open(case.root.join("Cargo.toml"))
        .unwrap();
    writeln!(
        manifest,
        "\n[[test]]\nname = \"it\"\npath = \"tests/../tests/it.rs\""
    )
    .unwrap();
    let path = case.root.join("index.scip");
    let mut index = Index::parse_from_bytes(&fs::read(&path).unwrap()).unwrap();
    index.documents.push(Document {
        language: "rust".into(),
        relative_path: "data/text_only.rs".into(),
        position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
        ..Document::default()
    });
    fs::write(&path, index.write_to_bytes().unwrap()).unwrap();
    let out = snapshot(&case, Some("base,build"));
    let files = table(&out, "files.jsonl");
    let text = files
        .iter()
        .find(|row| row["path"] == "data/text_only.rs")
        .unwrap();
    assert_eq!(text["targets"], json!(["cargo:rust-targets:test:it"]));
    assert_eq!(text["test"], "target", "{text}");

    index
        .documents
        .iter_mut()
        .find(|doc| doc.relative_path == "tests/it.rs")
        .unwrap()
        .position_encoding = PositionEncoding::UnspecifiedPositionEncoding.into();
    fs::write(path, index.write_to_bytes().unwrap()).unwrap();
    let out = snapshot(&case, Some("base,syntax,build"));
    let files = table(&out, "files.jsonl");
    let text = files
        .iter()
        .find(|row| row["path"] == "data/text_only.rs")
        .unwrap();
    assert_eq!(text["targets"], json!(["cargo:rust-targets:test:it"]));
    assert!(text.get("test").is_none(), "{text}");
    assert!(files.iter().all(|row| row["path"] != "tests/it.rs"));
    let control = files
        .iter()
        .find(|row| row["path"] == "tests/same_name.rs")
        .unwrap();
    assert_eq!(control["test"], "target");
}

#[test]
fn build_marks_regenerated_source_withholds() {
    let case = targets();
    let source = fs::read_to_string(case.root.join("src/tests.rs")).unwrap();
    let path = case.root.join("build.rs");
    fs::write(
        path,
        format!(
            r#"#[path = "build_m.rs"] mod build_m;
fn main() {{
    build_m::f();
    let contents = if std::env::var_os("CARGO_FEATURE_X").is_some() {{ {:?} }} else {{ {:?} }};
    std::fs::write("src/tests.rs", contents).unwrap();
}}
"#,
            format!("{source}\n// regenerated under all features\n"),
            source,
        ),
    )
    .unwrap();
    let out = snapshot(&case, Some("base,syntax,build"));
    let files = table(&out, "files.jsonl");
    let rewritten = files
        .iter()
        .find(|row| row["path"] == "src/tests.rs")
        .unwrap();
    assert_eq!(
        rewritten["targets"],
        json!(["cargo:rust-targets:lib:rust_targets"])
    );
    assert!(rewritten.get("test").is_none(), "{rewritten}");
    let unchanged = files
        .iter()
        .find(|row| row["path"] == "indep/src/only_test.rs")
        .unwrap();
    assert_eq!(unchanged["test"], "cfg");
    assert!(
        table(&out, "gaps.jsonl")
            .iter()
            .any(|row| row["file"] == "src/tests.rs" && row["code"] == "test-unknown")
    );
    assert_eq!(
        fs::read_to_string(case.root.join("src/tests.rs")).unwrap(),
        source
    );
}

#[test]
fn build_root_requires_one_symbol_at_its_start() {
    let case = targets();
    let path = case.root.join("index.scip");
    let mut index = Index::parse_from_bytes(&fs::read(&path).unwrap()).unwrap();
    let file = index
        .documents
        .iter_mut()
        .find(|doc| doc.relative_path == "src/lib.rs")
        .unwrap();
    let root = file
        .occurrences
        .iter()
        .find(|occ| occ.symbol.ends_with(" crate/"))
        .unwrap()
        .clone();
    file.occurrences.push(root.clone());
    fs::write(&path, index.write_to_bytes().unwrap()).unwrap();
    let out = snapshot(&case, Some("base,build"));
    let rows = table(&out, "targets.jsonl");
    let lib = rows
        .iter()
        .find(|row| row["name"] == "rust_targets")
        .unwrap();
    assert_eq!(lib["entity"], "cargo:rust-targets:crate/~7");

    let file = index
        .documents
        .iter_mut()
        .find(|doc| doc.relative_path == "src/lib.rs")
        .unwrap();
    file.occurrences.push(Occurrence {
        symbol: "rust-analyzer cargo ambiguous 0.1.0 crate/".into(),
        ..root
    });
    fs::write(path, index.write_to_bytes().unwrap()).unwrap();
    let out = snapshot(&case, Some("base,syntax,build"));
    let rows = table(&out, "targets.jsonl");
    let lib = rows
        .iter()
        .find(|row| row["name"] == "rust_targets")
        .unwrap();
    assert!(lib.get("root").is_none());
}

#[test]
fn build_alias_only_document_keeps_membership() {
    let case = targets();
    symlink("tests.rs", case.root.join("src/tests_alias.rs")).unwrap();
    let path = case.root.join("index.scip");
    let mut index = Index::parse_from_bytes(&fs::read(&path).unwrap()).unwrap();
    index
        .documents
        .iter_mut()
        .find(|doc| doc.relative_path == "src/tests.rs")
        .unwrap()
        .relative_path = "src/tests_alias.rs".into();
    fs::write(path, index.write_to_bytes().unwrap()).unwrap();
    let out = snapshot(&case, Some("base,build"));
    let files = table(&out, "files.jsonl");
    let alias = files
        .iter()
        .find(|row| row["path"] == "src/tests_alias.rs")
        .unwrap();
    assert_eq!(
        alias["targets"],
        json!(["cargo:rust-targets:lib:rust_targets"])
    );
    assert!(alias.get("test").is_none());
    assert!(files.iter().all(|row| row["path"] != "src/tests.rs"));
    let unchanged = files
        .iter()
        .find(|row| row["path"] == "indep/src/only_test.rs")
        .unwrap();
    assert_eq!(unchanged["test"], "cfg");
    assert!(
        !table(&out, "gaps.jsonl")
            .iter()
            .any(|gap| gap["gap"] == "unknown-file")
    );
}

#[test]
fn build_contradiction_in_both_cfg_forms() {
    for flags in ["[\"--cfg\", \"test\"]", "[\"--cfg=test\"]"] {
        let case = targets();
        let config = case.root.join(".cargo");
        fs::create_dir(&config).unwrap();
        fs::write(
            config.join("config.toml"),
            format!("[build]\nrustflags = {flags}\n"),
        )
        .unwrap();
        let out = snapshot(&case, Some("base,syntax,build"));
        let files = table(&out, "files.jsonl");
        let tested = files
            .iter()
            .find(|row| row["path"] == "src/tests.rs")
            .unwrap();
        assert_eq!(
            tested["targets"],
            json!(["cargo:rust-targets:lib:rust_targets"])
        );
        assert!(tested.get("test").is_none());
        let entities = table(&out, "entities.jsonl");
        let gated = entities.iter().find(|row| row["name"] == "gated").unwrap();
        assert!(gated.get("test").is_none());
        assert!(
            table(&out, "gaps.jsonl")
                .iter()
                .any(|gap| gap["file"] == "src/lib.rs" && gap["code"] == "test-unknown")
        );
    }
}

#[test]
fn build_unknown_kind_reports_a_diagnostic_without_discarding_the_layer() {
    use std::os::unix::fs::PermissionsExt;
    let case = targets();
    let cargo = case.bin.join("cargo");
    let real = case.bin.join("real-cargo");
    let sed = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("sed"))
        .find(|path| path.is_file())
        .unwrap();
    fs::rename(&cargo, &real).unwrap();
    fs::write(
        &cargo,
        format!(
            r#"#!/bin/sh
if [ "$1" = metadata ] && [ "$2" = --locked ]; then
    '{}' "$@" | '{}' 's/"kind":\["example"\]/"kind":["future"]/g'
else
    exec '{}' "$@"
fi
"#,
            real.display(),
            sed.display(),
            real.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    let result = case.run(case.index().args(["--layers", "base,build"]));
    let build = &result["layers"]["build"];
    assert_eq!(build["status"], "ok", "{result}");
    assert_eq!(
        build["diagnostics"],
        json!([{"code": "target-kind-unknown"}])
    );
    let rows = table(&case.out, "build.jsonl");
    assert!(
        rows.iter()
            .any(|row| row["row"] == "target" && row["name"] == "rust_targets")
    );
    assert!(
        rows.iter()
            .all(|row| row["kind"] != "example" && row["kind"] != "future")
    );
}

#[test]
fn build_unkeyed_test_units_only_veto_from_evidence_runs() {
    use std::io::Write as _;
    for (feature_required, expected) in [(true, Some("cfg")), (false, None)] {
        let case = targets();
        fs::write(
            case.temp.path().join("outside.rs"),
            "const TEXT: &str = include_str!(\"project/src/tests.rs\");\n#[test] fn read() { assert!(!TEXT.is_empty()); }\n",
        )
        .unwrap();
        let mut manifest = fs::OpenOptions::new()
            .append(true)
            .open(case.root.join("Cargo.toml"))
            .unwrap();
        writeln!(
            manifest,
            "\n[[test]]\nname = \"outside\"\npath = \"../outside.rs\""
        )
        .unwrap();
        if feature_required {
            writeln!(manifest, "required-features = [\"x\"]").unwrap();
        }
        let out = snapshot(&case, Some("base,syntax,build"));
        let rows = table(&out, "files.jsonl");
        let t = rows
            .iter()
            .find(|row| row["path"] == "src/tests.rs")
            .unwrap();
        assert_eq!(t["targets"], json!(["cargo:rust-targets:lib:rust_targets"]));
        assert_eq!(t["test"].as_str(), expected, "{t}");
        let control = rows
            .iter()
            .find(|row| row["path"] == "indep/src/only_test.rs")
            .unwrap();
        assert_eq!(control["test"], "cfg");
        assert!(
            table(&out, "targets.jsonl")
                .iter()
                .all(|row| row["name"] != "outside")
        );
    }
}

#[test]
fn build_marks_rewritten_gate_withheld() {
    use std::io::Write as _;
    let case = targets();
    let mut macros = fs::OpenOptions::new()
        .append(true)
        .open(case.root.join("pm/src/lib.rs"))
        .unwrap();
    macros.write_all(br#"
#[proc_macro_attribute]
pub fn retype(_: TokenStream, item: TokenStream) -> TokenStream {
    use proc_macro::{Delimiter, Group, Ident, TokenTree};
    let copy = item.clone().into_iter().map(|token| match token {
        TokenTree::Ident(id) if id.to_string() == "u16" =>
            TokenTree::Ident(Ident::new("u8", id.span())),
        TokenTree::Group(body) if body.delimiter() == Delimiter::Brace => {
            // The fixture's first two tokens are the nested #[cfg(test)].
            let mut ungated = Group::new(Delimiter::Brace, body.stream().into_iter().skip(2).collect());
            ungated.set_span(body.span());
            TokenTree::Group(ungated)
        }
        other => other,
    });
    item.clone().into_iter().chain(copy).collect()
}
"#).unwrap();
    let lib = case.root.join("src/lib.rs");
    let mut source = fs::read_to_string(&lib).unwrap();
    source.push_str(
        r"
pub struct Host<T>(pub T);
#[pm::retype]
impl Host<u16> {
    #[cfg(test)]
    pub fn run(&self) {}
}
pub fn production_can_call_the_copy() {
    Host(0_u8).run();
}
",
    );
    fs::write(lib, &source).unwrap();
    let path = case.root.join("index.scip");
    let mut index = Index::parse_from_bytes(&fs::read(&path).unwrap()).unwrap();
    let file = index
        .documents
        .iter_mut()
        .find(|doc| doc.relative_path == "src/lib.rs")
        .unwrap();
    let (line, text) = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("pub fn run"))
        .unwrap();
    let col = i32::try_from(text.find("run").unwrap()).unwrap();
    let line = i32::try_from(line).unwrap();
    file.occurrences.push(definition(
        "rust-analyzer cargo rust-targets 0.1.0 Host#run().",
        [line, col, col + 3],
        [line, 4, i32::try_from(text.len()).unwrap()],
    ));
    fs::write(path, index.write_to_bytes().unwrap()).unwrap();

    let out = snapshot(&case, Some("base,syntax,build"));
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["scopes"][0]["layers"]["build"], "present");
    let rows = table(&out, "entities.jsonl");
    let rewritten = rows
        .iter()
        .find(|row| row["name"] == "run" && row["def"]["file"] == "src/lib.rs")
        .unwrap();
    assert!(rewritten.get("test").is_none(), "{rewritten}");
    let unchanged = rows.iter().find(|row| row["name"] == "gated").unwrap();
    assert_eq!(unchanged["test"], "cfg");
}

#[test]
fn build_ordinary_production_run_fails() {
    use std::os::unix::fs::PermissionsExt;

    let case = targets();
    let selection = Some("base,syntax,build");
    let control = snapshot(&case, selection);
    let control_files = table(&control, "files.jsonl");
    assert!(
        control_files
            .iter()
            .any(|row| { row["path"] == "tests/it.rs" && row["test"] == "target" })
    );
    fs::rename(&control, case.temp.path().join("control")).unwrap();

    let cargo = case.bin.join("cargo");
    let real_dir = case.temp.path().join("real-tool");
    fs::create_dir(&real_dir).unwrap();
    let real_cargo = real_dir.join("cargo");
    fs::rename(&cargo, &real_cargo).unwrap();
    let record = case.temp.path().join("ordinary-failure");
    fs::write(
        &cargo,
        format!(
            r#"#!/bin/sh
if [ "$1" = check ]; then
    lib=0 bins=0 all=0
    for arg in "$@"; do
        case "$arg" in
            --lib) lib=1 ;;
            --bins) bins=1 ;;
            --all-targets) all=1 ;;
        esac
    done
    if [ "$lib$bins$all" = 110 ]; then
        printf '%s\n' intercepted > '{}'
        exit 29
    fi
fi
exec '{}' "$@"
"#,
            record.display(),
            real_cargo.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();

    let out = snapshot(&case, selection);
    assert_eq!(fs::read_to_string(&record).unwrap(), "intercepted\n");
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["scopes"][0]["layers"]["build"], "present");
    for table_name in ["files.jsonl", "entities.jsonl"] {
        for row in table(&out, table_name) {
            assert!(row.get("test").is_none(), "{table_name}: {row}");
        }
    }
    let with_membership = table(&out, "files.jsonl")
        .into_iter()
        .filter(|row| row.get("targets").is_some())
        .map(|row| row["path"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(with_membership.contains("tests/it.rs"));
    let incomplete = table(&out, "gaps.jsonl")
        .into_iter()
        .filter(|row| row["layer"] == "build" && row["code"] == "targets-incomplete")
        .map(|row| row["file"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(incomplete, with_membership);
}

fn lib_depfiles(dir: &Path, matches: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            lib_depfiles(&path, matches);
        } else if path.extension().is_some_and(|ext| ext == "d")
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with("rust_targets")
        {
            let text = fs::read_to_string(path).unwrap();
            if text.contains("src/lib.rs") {
                matches.push(text);
            }
        }
    }
}

#[test]
fn build_dev_dependency_feature_unification() {
    use std::io::Write as _;

    let case = targets();
    let manifest = case.root.join("Cargo.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    assert!(original.contains("[features]\nx = []\n"));
    fs::write(
        &manifest,
        original
            .replace("[features]\nx = []\n", "[features]\nx = []\nextra = []\n")
            .replace(
                "edition = \"2021\"\n",
                "edition = \"2021\"\nautobins = false\n",
            )
            + "\n[dev-dependencies]\nprobe-helper = { path = \"../probe-helper\" }\n",
    )
    .unwrap();
    let helper = case.temp.path().join("probe-helper");
    fs::create_dir_all(helper.join("src")).unwrap();
    fs::write(
        helper.join("Cargo.toml"),
        r#"[package]
name = "probe-helper"
version = "0.1.0"
edition = "2021"

[dependencies]
rust-targets = { path = "../project", features = ["extra"] }
"#,
    )
    .unwrap();
    fs::write(helper.join("src/lib.rs"), "pub fn helper() {}\n").unwrap();

    let lib = case.root.join("src/lib.rs");
    let source = fs::read_to_string(&lib).unwrap();
    let path = r#"#[path = "shared.rs"]"#;
    assert!(source.contains(path));
    fs::write(
        lib,
        source.replace(
            path,
            r#"#[cfg(not(feature = "extra"))] #[path = "shared.rs"]"#,
        ),
    )
    .unwrap();
    let mut integration = fs::OpenOptions::new()
        .append(true)
        .open(case.root.join("tests/it.rs"))
        .unwrap();
    writeln!(integration, "\n#[path = \"../src/shared.rs\"]\nmod shared;").unwrap();

    ok(&case
        .command(&case.bin.join("cargo"))
        .current_dir(&case.root)
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap());

    let result = case.run(case.index().args(["--layers", "base,build"]));
    assert_eq!(result["layers"]["build"]["status"], "ok", "{result}");

    for run in 0..4 {
        let dir = case.out.join(format!("work/check-{run}"));
        let mut listed = Vec::new();
        lib_depfiles(&dir, &mut listed);
        assert!(!listed.is_empty(), "no library depfile in {dir:?}");
        assert_eq!(
            listed
                .iter()
                .any(|depfile| depfile.contains("src/shared.rs")),
            run == 3,
            "run {run}: {listed:?}"
        );
    }

    let snapshot = snapshot(&case, Some("base,build"));
    let files = table(&snapshot, "files.jsonl");
    let shared = files
        .iter()
        .find(|file| file["path"] == "src/shared.rs")
        .unwrap();
    assert!(shared.get("test").is_none(), "{shared}");
    assert_eq!(
        shared["targets"],
        json!([
            "cargo:rust-targets:lib:rust_targets",
            "cargo:rust-targets:test:it"
        ])
    );
    assert!(
        files
            .iter()
            .any(|file| { file["path"] == "tests/it.rs" && file["test"] == "target" })
    );
}
