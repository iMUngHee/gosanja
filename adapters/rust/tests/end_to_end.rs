//! The engine finds the adapter and runs it, and the syntax facts of `fixtures/rust-flow`
//! reach the snapshot through the join. Every expected value is derived by hand from the
//! source; every process runs in an environment the test builds, so nothing installed on
//! the host decides a result.
#![cfg(unix)]

#[expect(dead_code, reason = "these runs use part of the shared harness")]
mod support;

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use protobuf::Message;
use scip::types::{Document, Index, Occurrence, PositionEncoding, SymbolRole};
use serde_json::{Value, json};
use support::{Case, engine, listing};

#[path = "end_to_end/build_targets.rs"]
mod build_targets;
#[path = "end_to_end/check_reuse.rs"]
mod check_reuse;

const APP: &str = "app/src/main.rs";
const KIT: &str = "kit/src/lib.rs";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-flow")
}

fn table(snapshot: &Path, name: &str) -> Vec<Value> {
    fs::read_to_string(snapshot.join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .lines()
        .map(|l| serde_json::from_str(l).expect("JSON"))
        .collect()
}

#[track_caller]
fn ok(output: &Output) {
    assert!(
        output.status.success(),
        "exit {:?}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A case whose project is a copy of the fixture named `rust-flow`, as the golden was made,
/// with the adapter installed where the engine looks for it.
fn rust_flow() -> (Case, PathBuf) {
    let case = Case::new();
    let root = case.temp.path().join("rust-flow");
    fs::rename(&case.root, &root).expect("rename the copy");
    symlink(
        support::adapter_binary(),
        case.bin.join("gosanja-adapter-rust"),
    )
    .expect("install the adapter");
    (case, root)
}

/// `gosanja index --root <root> --out <out>`, the adapter chosen by its marker.
fn index_command(case: &Case, root: &Path, out: &Path) -> Command {
    let mut command = case.command(engine());
    command
        .arg("index")
        .arg("--root")
        .arg(root)
        .arg("--out")
        .arg(out);
    command
}

fn index(case: &Case, root: &Path, out: &Path) -> Output {
    index_command(case, root, out)
        .output()
        .expect("gosanja runs")
}

/// Every file of a snapshot directory, by name.
fn snapshot_bytes(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .expect("a snapshot")
        .map(|e| {
            let e = e.expect("an entry");
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).expect("readable"),
            )
        })
        .collect();
    files.sort();
    files
}

/// Every item the adapter describes in the fixture except the inactive one: file, name,
/// entity kind and the position of its name token (all columns here are ASCII).
const ITEMS: [(&str, &str, &str, u32, u32); 36] = [
    (APP, "crate", "rust:module", 0, 0),
    (APP, "marker", "rust:macro", 5, 13),
    (APP, "Shell", "rust:struct", 8, 7),
    (APP, "INNER_VALUE", "rust:constant", 13, 6),
    (APP, "outer", "rust:function", 14, 3),
    (APP, "flow", "rust:function", 16, 9),
    (APP, "nested", "rust:function", 17, 7),
    (APP, "item", "rust:macro", 18, 17),
    (APP, "main", "rust:function", 41, 3),
    (KIT, "crate", "rust:module", 0, 0),
    (KIT, "slot", "rust:macro", 2, 13),
    (KIT, "Op", "rust:struct", 5, 11),
    (KIT, "Flag", "rust:enum", 6, 9),
    (KIT, "Bits", "rust:union", 7, 10),
    (KIT, "Count", "rust:type-alias", 8, 9),
    (KIT, "LIMIT", "rust:constant", 9, 10),
    (KIT, "TOTAL", "rust:static-variable", 10, 15),
    (KIT, "Adjust", "rust:trait", 12, 10),
    (KIT, "Output", "rust:associated-type", 13, 9),
    (KIT, "START", "rust:constant", 14, 10),
    (KIT, "required", "rust:trait-method", 15, 7),
    (KIT, "apply", "rust:method", 20, 11),
    (KIT, "Output", "rust:associated-type", 24, 9),
    (KIT, "START", "rust:constant", 25, 10),
    (KIT, "required", "rust:method", 26, 7),
    (KIT, "step", "rust:function", 33, 7),
    (KIT, "make", "rust:function", 34, 7),
    (KIT, "r#type", "rust:function", 35, 20),
    (KIT, "foreign_abi", "rust:function", 36, 18),
    (KIT, "a", "rust:module", 38, 8),
    (KIT, "restricted", "rust:function", 39, 24),
    (KIT, "shared", "rust:function", 40, 18),
    (KIT, "one", "rust:macro", 48, 13),
    (KIT, "int", "rust:macro", 50, 13),
    (KIT, "ping", "rust:function", 52, 7),
    (KIT, "pong", "rust:function", 56, 3),
];

/// The join on a snapshot of the fixture.
fn assert_joined(snapshot: &Path) {
    let entities = table(snapshot, "entities.jsonl");
    let syntax: BTreeSet<String> = table(snapshot, "syntax.jsonl")
        .iter()
        .map(|r| r["key"].as_str().expect("a key").to_owned())
        .collect();
    let mut keys = BTreeSet::new();
    for (file, name, kind, line, col) in ITEMS {
        let entity = entities
            .iter()
            .find(|e| {
                e["def"]["file"] == file
                    && e["def"]["name"][0] == line
                    && e["def"]["name"][1] == col
            })
            .unwrap_or_else(|| panic!("no entity at {file}:{line}:{col}"));
        assert_eq!(
            (entity["name"].as_str(), entity["kind"].as_str()),
            (Some(name), Some(kind)),
            "{file}:{line}:{col}"
        );
        let key = entity["key"].as_str().expect("a key");
        assert!(syntax.contains(key), "no syntax row for {key}");
        assert!(keys.insert(key.to_owned()), "{key} twice");
    }

    let gaps = table(snapshot, "gaps.jsonl");
    let unanchored: Vec<_> = gaps.iter().filter(|g| g["gap"] == "unanchored").collect();
    assert_eq!(unanchored.len(), 1, "{unanchored:?}");
    assert_eq!(
        unanchored[0]["anchor"],
        json!({"file": KIT, "line": 46, "col": 3})
    );
    for kind in ["position-out-of-range", "skipped-file", "failed-layer"] {
        assert!(
            !gaps
                .iter()
                .any(|g| g["gap"] == kind && g["src"] == "adapter:rust/syntax"),
            "a {kind} gap for the syntax layer"
        );
    }

    assert_calls(&table(snapshot, "relations.jsonl"));
    assert_syntax_gaps(&gaps);
    // No row of the syntax layer joins anything but the listed items.
    assert_eq!(syntax, keys);
}

/// Every call relation of the fixture and the references the calls keep.
fn assert_calls(relations: &[Value]) {
    // The calls layer gives one relation, the fixture's control: `ping` calls `pong`.
    let compiled: Vec<_> = relations
        .iter()
        .filter(|r| r["src"] == "adapter:rust/calls")
        .collect();
    assert_eq!(
        compiled,
        [
            &json!({"from": "cargo:flow-kit:ping().", "to": "cargo:flow-kit:pong().", "rel": "call", "mechanism": "static", "resolution": "direct", "evidence": [{"file": KIT, "line": 53, "col": 4, "build": "flow-kit-lib-flow_kit"}], "src": "adapter:rust/calls"})
        ]
    );
    let calls: Vec<_> = relations
        .iter()
        .filter(|r| r["rel"] == "call" && r["src"] == "adapter:rust/syntax")
        .collect();
    assert!(
        calls
            .iter()
            .all(|c| c["resolution"] == "declared" && c["mechanism"] == "unknown")
    );
    let has_ref = |from: &str, to: &str| {
        relations
            .iter()
            .any(|r| r["rel"] == "ref" && r["from"] == from && r["to"] == to)
    };
    let flow = "cargo:flow-app:flow().";
    // Every call the syntax layer gives the fixture, with the byte column of its callee token:
    // by path across the member crates after `/*é*/` (byte 29), by method syntax, `outer`
    // twice (in the closure and in `outer(INNER_VALUE)`, which calls no constant), `make` in
    // `make()(x)`, the constructor `Some`, and `outer` in the nested function.
    let found: BTreeSet<(String, String, String)> = calls
        .iter()
        .map(|c| {
            let text = |v: &Value| v.as_str().expect("a key").to_owned();
            (text(&c["from"]), text(&c["to"]), c["evidence"].to_string())
        })
        .collect();
    let evidence = |sites: &[(u32, u32, u32)]| {
        json!(
            sites
                .iter()
                .map(|(line, col, end)| json!({"file": APP, "line": line, "col": col, "end": [line, end]}))
                .collect::<Vec<_>>()
        )
        .to_string()
    };
    let call = |from: &str, to: &str, sites: &[(u32, u32, u32)]| {
        (from.to_owned(), to.to_owned(), evidence(sites))
    };
    assert_eq!(
        found,
        BTreeSet::from([
            (
                "cargo:flow-kit:ping().".to_owned(),
                "cargo:flow-kit:pong().".to_owned(),
                json!([{"file": KIT, "line": 53, "col": 4, "end": [53, 8]}]).to_string()
            ),
            call(flow, "cargo:core:option/Option#Some#", &[(38, 4, 8)]),
            call(flow, "cargo:flow-app:outer().", &[(28, 16, 21), (33, 4, 9)]),
            call(flow, "cargo:flow-kit:impl#[Op]apply().", &[(32, 7, 12)]),
            call(flow, "cargo:flow-kit:make().", &[(34, 4, 8)]),
            call(flow, "cargo:flow-kit:step().", &[(31, 29, 33)]),
            call(
                "cargo:flow-app:nested().",
                "cargo:flow-app:outer().",
                &[(17, 18, 23)]
            ),
        ])
    );
    // Each call keeps its reference; the constant passed to `outer` is a reference only.
    for to in [
        "cargo:flow-kit:step().",
        "cargo:flow-kit:impl#[Op]apply().",
        "cargo:flow-app:outer().",
        "cargo:flow-kit:make().",
        "cargo:flow-app:INNER_VALUE.",
    ] {
        assert!(has_ref(flow, to), "{to}");
    }
}

/// Every gap of the syntax layer but the unanchored one.
fn assert_syntax_gaps(gaps: &[Value]) {
    let flow = "cargo:flow-app:flow().";
    // Every gap of the syntax layer: the open calls (the outer call of `make()(x)`, `(op.f)(x)`,
    // the local closure `f`, `(self.f)(x)` in `apply`) and the opaque regions.
    let syntax_gaps: BTreeSet<String> = gaps
        .iter()
        .filter(|g| g["src"] == "adapter:rust/syntax" && g["gap"] != "unanchored")
        .map(Value::to_string)
        .collect();
    let gap = |kind: &str, file: &str, range: [u32; 4], key: Option<&str>| {
        let mut g = json!({"gap": kind, "file": file, "range": range});
        if let Some(key) = key {
            g["keys"] = json!([key]);
        }
        g["src"] = json!("adapter:rust/syntax");
        g.to_string()
    };
    let app_module = Some("cargo:flow-app:crate/");
    let kit_module = Some("cargo:flow-kit:crate/");
    assert_eq!(
        syntax_gaps,
        BTreeSet::from([
            gap("open-call", APP, [34, 4, 34, 13], None),
            gap("open-call", APP, [35, 4, 35, 13], None),
            gap("open-call", APP, [38, 9, 38, 10], None),
            gap("open-call", KIT, [20, 41, 20, 52], None),
            gap("opaque-region", APP, [6, 0, 6, 10], app_module), // top-level invocation
            gap("opaque-region", APP, [10, 4, 10, 14], app_module), // inside the impl block
            gap("opaque-region", APP, [19, 4, 19, 12], Some(flow)), // item! in the function
            gap("opaque-region", APP, [36, 12, 36, 25], Some(flow)), // stringify!
            gap("opaque-region", APP, [37, 4, 37, 17], Some(flow)), // assert!
            gap("opaque-region", APP, [37, 26, 37, 42], Some(flow)), // type position
            gap("opaque-region", APP, [37, 55, 37, 71], Some(flow)), // pattern position
            gap("opaque-region", KIT, [3, 0, 3, 8], kit_module),
            gap(
                "opaque-region",
                KIT,
                [16, 4, 16, 12],
                Some("cargo:flow-kit:Adjust#")
            ),
        ])
    );
}

// The engine chooses the adapter by its marker and gives the checked-in snapshot.
#[test]
fn the_engine_finds_and_runs_the_adapter_and_gives_the_golden_snapshot() {
    let (case, root) = rust_flow();
    let out = case.temp.path().join("snapshot");
    ok(&index(&case, &root, &out));
    ok(&case
        .command(engine())
        .arg("validate")
        .arg(&out)
        .arg("--root")
        .arg(&root)
        .output()
        .expect("gosanja runs"));
    assert_eq!(
        snapshot_bytes(&out),
        snapshot_bytes(&fixture().join("expected"))
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).expect("a manifest"))
            .expect("JSON");
    let scope = &manifest["scopes"][0];
    assert_eq!(scope["adapter"]["name"], "rust");
    assert_eq!(scope["adapter"]["contract"], "0.6");
    assert_eq!(
        scope["layers"],
        json!({"base": "present", "syntax": "present", "calls": "present", "build": "present"})
    );
    let list = case
        .command(engine())
        .args(["adapter", "list"])
        .output()
        .expect("gosanja runs");
    ok(&list);
    let text = String::from_utf8_lossy(&list.stdout);
    let bin = case.bin.display();
    assert!(text.starts_with("rust "), "{text}");
    assert!(
        text.contains(&format!(
            "  requires rust-analyzer: found {bin}/rust-analyzer\n"
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("  requires cargo: found {bin}/cargo\n")),
        "{text}"
    );
}

#[test]
fn selecting_only_base_keeps_all_snapshot_files() {
    let (case, root) = rust_flow();
    let out = case.temp.path().join("snapshot");
    ok(&index_command(&case, &root, &out)
        .args(["--layers", "base"])
        .output()
        .expect("gosanja runs"));
    let files = snapshot_bytes(&out);
    assert_eq!(
        files
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        [
            "entities.jsonl",
            "externals.jsonl",
            "files.jsonl",
            "gaps.jsonl",
            "manifest.json",
            "relations.jsonl",
            "syntax.jsonl",
            "targets.jsonl"
        ]
    );
    assert_eq!(
        fs::read(out.join("syntax.jsonl")).expect("syntax file"),
        Vec::<u8>::new()
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).expect("a manifest"))
            .expect("JSON");
    assert_eq!(
        manifest["scopes"][0]["layers"],
        json!({"base":"present","syntax":"not-requested","calls":"not-requested","build":"not-requested"})
    );
}

#[test]
fn selecting_base_and_syntax_keeps_declared_calls_without_the_calls_layer() {
    let (case, root) = rust_flow();
    let out = case.temp.path().join("snapshot");
    ok(&index_command(&case, &root, &out)
        .args(["--layers", "base,syntax"])
        .output()
        .expect("gosanja runs"));
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).expect("a manifest"))
            .expect("JSON");
    let scope = &manifest["scopes"][0];
    assert_eq!(
        scope["layers"],
        json!({"base":"present","syntax":"present","calls":"not-requested","build":"not-requested"})
    );
    assert_eq!(scope["builds"], json!([]));

    let relations = table(&out, "relations.jsonl");
    let gaps = table(&out, "gaps.jsonl");
    for rows in [&relations, &gaps] {
        let compiled: Vec<_> = rows
            .iter()
            .filter(|r| r["src"] == "adapter:rust/calls")
            .collect();
        assert_eq!(compiled, Vec::<&Value>::new());
    }
    let syntax_call = json!({
        "from":"cargo:flow-kit:ping().","to":"cargo:flow-kit:pong().","rel":"call",
        "mechanism":"unknown","resolution":"declared","src":"adapter:rust/syntax",
        "evidence":[{"file":KIT,"line":53,"col":4,"end":[53,8]}]
    });
    assert!(relations.contains(&syntax_call), "{relations:?}");
    ok(&case
        .command(engine())
        .arg("validate")
        .arg(&out)
        .output()
        .expect("gosanja runs"));
}

// Every described item joins its entity, and the calls and gaps the syntax layer implies are there.
#[test]
fn the_syntax_facts_reach_the_snapshot() {
    let (case, root) = rust_flow();
    let out = case.temp.path().join("snapshot");
    ok(&index(&case, &root, &out));
    assert_joined(&out);
}

// Runs are deterministic, the adapter's output and the snapshot alike.
#[test]
fn two_runs_give_the_same_bytes() {
    let (case, root) = rust_flow();
    let mut adapter_outputs = Vec::new();
    for name in ["a", "b"] {
        let out = case.temp.path().join(name);
        fs::create_dir(&out).expect("mkdir");
        ok(&case
            .adapter()
            .arg("index")
            .arg("--root")
            .arg(&root)
            .arg("--out")
            .arg(&out)
            .output()
            .expect("the adapter runs"));
        adapter_outputs
            .push(["result.json", "syntax.jsonl"].map(|f| fs::read(out.join(f)).expect(f)));
    }
    assert_eq!(adapter_outputs[0], adapter_outputs[1]);
    let snapshots: Vec<_> = ["snap-a", "snap-b"]
        .iter()
        .map(|name| {
            let out = case.temp.path().join(name);
            ok(&index(&case, &root, &out));
            snapshot_bytes(&out)
        })
        .collect();
    assert_eq!(snapshots[0], snapshots[1]);
}

// The adapter's own extent check, through the engine.

fn definition(symbol: &str, name: [i32; 3], enclosing: [i32; 3]) -> Occurrence {
    Occurrence {
        symbol: symbol.to_owned(),
        symbol_roles: SymbolRole::Definition as i32,
        range: name.to_vec(),
        enclosing_range: enclosing.to_vec(),
        ..Default::default()
    }
}

/// The fixture with `app/src/probe.rs` holding `source`, its index extended by a document for
/// it, indexed through the engine; returns the snapshot directory.
fn probe(source: &str, occurrences: Vec<Occurrence>) -> (Case, PathBuf) {
    probes(vec![("app/src/probe.rs", source.to_owned(), occurrences)])
}

/// The fixture with more source files, each with a document of its own in the index.
fn probes(files: Vec<(&str, String, Vec<Occurrence>)>) -> (Case, PathBuf) {
    let (case, root) = rust_flow();
    let mut index = Index::parse_from_bytes(&fs::read(root.join("index.scip")).expect("an index"))
        .expect("SCIP");
    for (path, source, occurrences) in files {
        fs::write(root.join(path), source).expect("write");
        index.documents.push(Document {
            relative_path: path.into(),
            language: "rust".into(),
            position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            occurrences,
            ..Default::default()
        });
    }
    fs::write(
        root.join("index.scip"),
        index.write_to_bytes().expect("encodable"),
    )
    .expect("write");
    let before = listing(&root);
    let out = case.temp.path().join("snapshot");
    ok(&index_run(&case, &root, &out));
    assert_eq!(listing(&root), before);
    (case, out)
}

fn index_run(case: &Case, root: &Path, out: &Path) -> Output {
    index(case, root, out)
}

fn syntax_state(snapshot: &Path) -> Value {
    let manifest: Value =
        serde_json::from_slice(&fs::read(snapshot.join("manifest.json")).expect("a manifest"))
            .expect("JSON");
    manifest["scopes"][0]["layers"]["syntax"].clone()
}

#[test]
fn a_local_definition_at_an_anchor_does_not_let_an_outline_past_the_extent() {
    // `f()` lies at 0:12-15; the index gives `wide` the extent 0:0-10, and a local symbol a
    // wider one at the same name range, which the engine does not make an entity of.
    let symbol = "rust-analyzer cargo flow-app 0.1.0 probe/wide().";
    let (_case, out) = probe(
        "fn wide() { f(); }\n",
        vec![
            definition(symbol, [0, 3, 7], [0, 0, 10]),
            definition("local 99", [0, 3, 7], [0, 0, 18]),
        ],
    );
    assert_eq!(syntax_state(&out), "present");
    // The engine joined the row, without its outline.
    let row = table(&out, "syntax.jsonl")
        .into_iter()
        .find(|r| r["key"] == "cargo:flow-app:probe/wide().")
        .expect("the row of `wide`");
    assert_eq!(row.get("outline"), None);
    let gaps = table(&out, "gaps.jsonl");
    assert!(gaps.iter().any(|g| g["gap"] == "skipped-file"
        && g["file"] == "app/src/probe.rs"
        && g["code"] == "outline-outside-definition"));
    assert!(!gaps.iter().any(|g| g["gap"] == "failed-layer"));
    // Another function keeps its outline.
    let flow = table(&out, "syntax.jsonl")
        .into_iter()
        .find(|r| r["key"] == "cargo:flow-app:flow().")
        .expect("the row of `flow`");
    assert!(flow["outline"].is_array());
}

// Files the syntax layer cannot read, through the engine.
#[test]
fn files_the_layer_cannot_read_become_skipped_file_gaps() {
    let (case, root) = rust_flow();
    let files: [(&str, &[u8]); 3] = [
        ("app/src/broken.rs", b"fn broken( {\n"),
        ("app/src/latin.rs", b"fn caf\xe9() {}\n"),
        ("app/src/bom.rs", "\u{feff}fn bom() {}\n".as_bytes()),
    ];
    let mut index = Index::parse_from_bytes(&fs::read(root.join("index.scip")).expect("an index"))
        .expect("SCIP");
    for (path, bytes) in files {
        fs::write(root.join(path), bytes).expect("write");
        index.documents.push(Document {
            relative_path: path.into(),
            language: "rust".into(),
            position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            ..Default::default()
        });
    }
    fs::write(
        root.join("index.scip"),
        index.write_to_bytes().expect("encodable"),
    )
    .expect("write");
    let out = case.temp.path().join("snapshot");
    ok(&index_run(&case, &root, &out));
    assert_eq!(syntax_state(&out), "present");
    let skipped: Vec<_> = table(&out, "gaps.jsonl")
        .into_iter()
        .filter(|g| g["gap"] == "skipped-file" && g["layer"] == "syntax")
        .map(|g| {
            (
                g["file"].as_str().map(str::to_owned),
                g["code"].as_str().map(str::to_owned),
                g["layer"].clone(),
            )
        })
        .collect();
    let gap = |f: &str, c: &str| (Some(f.to_owned()), Some(c.to_owned()), json!("syntax"));
    assert_eq!(
        skipped,
        [
            gap("app/src/bom.rs", "byte-order-mark"),
            gap("app/src/broken.rs", "parse-failed"),
            gap("app/src/latin.rs", "unreadable"),
        ]
    );
}

#[test]
fn a_closure_s_types_reach_the_snapshot_as_opaque_regions() {
    let symbol = "rust-analyzer cargo flow-app 0.1.0 probe/f().";
    let source = "fn f() { let c = |v: ty!()| -> ty!() { id(v) }; }\n";
    let (_case, out) = probe(source, vec![definition(symbol, [0, 3, 4], [0, 0, 49])]);
    assert_eq!(syntax_state(&out), "present");
    let opaque: BTreeSet<_> = table(&out, "gaps.jsonl")
        .into_iter()
        .filter(|g| g["gap"] == "opaque-region" && g["file"] == "app/src/probe.rs")
        .map(|g| (g["range"].to_string(), g["keys"].to_string()))
        .collect();
    let key = json!(["cargo:flow-app:probe/f()."]).to_string();
    assert_eq!(
        opaque,
        BTreeSet::from([
            (json!([0, 21, 0, 26]).to_string(), key.clone()),
            (json!([0, 31, 0, 36]).to_string(), key),
        ])
    );
}

/// A source file the adapter cannot read: the syntax layer skips it and the engine, which
/// cannot read it either, records it as unavailable to the base layer; the run succeeds.
#[test]
fn a_file_without_read_permission_is_skipped_by_both_layers() {
    use std::os::unix::fs::PermissionsExt;
    let (case, root) = rust_flow();
    let locked = root.join("app/src/locked.rs");
    fs::write(&locked, "fn locked() {}\n").expect("write");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    if fs::read(&locked).is_ok() {
        println!("skipped: permissions do not bind the super-user, who may read any file");
        return;
    }
    let mut index = Index::parse_from_bytes(&fs::read(root.join("index.scip")).expect("an index"))
        .expect("SCIP");
    index.documents.push(Document {
        relative_path: "app/src/locked.rs".into(),
        language: "rust".into(),
        position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
        ..Default::default()
    });
    fs::write(
        root.join("index.scip"),
        index.write_to_bytes().expect("encodable"),
    )
    .expect("write");
    let out = case.temp.path().join("snapshot");
    let run = index_run(&case, &root, &out);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).expect("chmod");
    ok(&run);
    assert_eq!(syntax_state(&out), "present");
    let about: Vec<Value> = table(&out, "gaps.jsonl")
        .into_iter()
        .filter(|g| g["file"] == "app/src/locked.rs")
        .collect();
    assert_eq!(
        about,
        [
            json!({"gap": "skipped-file", "file": "app/src/locked.rs", "layer": "syntax", "code": "unreadable", "src": "adapter:rust/syntax"}),
            json!({"gap": "source-unavailable", "file": "app/src/locked.rs", "src": "scip:rust-analyzer"}),
        ]
    );
}

/// An outline too deep for the engine's reader is withheld, and only that row loses it.
#[test]
fn an_outline_too_deep_is_withheld_and_the_layer_kept() {
    // Each call of the chain holds the one before it: 70 levels.
    let chain = format!("fn chain() {{ x{}; }}\n", ".m()".repeat(70));
    let width = i32::try_from(chain.len() - 1).expect("short");
    let plain = "fn plain() { f(); }\n".to_owned();
    let (_case, out) = probes(vec![
        (
            "app/src/chain.rs",
            chain,
            vec![definition(
                "rust-analyzer cargo flow-app 0.1.0 chain/chain().",
                [0, 3, 8],
                [0, 0, width],
            )],
        ),
        (
            "app/src/plain.rs",
            plain,
            vec![definition(
                "rust-analyzer cargo flow-app 0.1.0 plain/plain().",
                [0, 3, 8],
                [0, 0, 19],
            )],
        ),
    ]);
    assert_eq!(syntax_state(&out), "present");
    let rows = table(&out, "syntax.jsonl");
    let row = |key: &str| {
        rows.iter()
            .find(|r| r["key"] == key)
            .unwrap_or_else(|| panic!("no row {key}"))
            .clone()
    };
    assert_eq!(row("cargo:flow-app:chain/chain().").get("outline"), None);
    assert!(row("cargo:flow-app:plain/plain().")["outline"].is_array());
    let skipped: Vec<_> = table(&out, "gaps.jsonl")
        .into_iter()
        .filter(|g| g["gap"] == "skipped-file" && g["layer"] == "syntax")
        .collect();
    assert_eq!(
        skipped,
        [
            json!({"gap": "skipped-file", "file": "app/src/chain.rs", "layer": "syntax", "code": "outline-too-deep", "src": "adapter:rust/syntax"})
        ]
    );
}
/// A case whose project is a copy of `fixtures/rust-calls`, as its golden was made.
fn rust_calls() -> (Case, PathBuf) {
    let case = Case::new();
    let root = case.temp.path().join("rust-calls");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    support::copy_tree(&fixtures.join("rust-calls"), &root);
    fs::remove_dir_all(root.join("expected")).expect("drop the golden from the copy");
    symlink(
        support::adapter_binary(),
        case.bin.join("gosanja-adapter-rust"),
    )
    .expect("install the adapter");
    (case, root)
}

struct TokenBinaries {
    engine: PathBuf,
    adapter: PathBuf,
    baseline: bool,
}

impl TokenBinaries {
    fn selected() -> Self {
        match (
            std::env::var_os("GOSANJA_TOKEN_BASELINE_BIN"),
            std::env::var_os("GOSANJA_TOKEN_BASELINE_RUST_ADAPTER"),
        ) {
            (None, None) => Self {
                engine: engine().into(),
                adapter: support::adapter_binary().into(),
                baseline: false,
            },
            (Some(engine), Some(adapter)) => {
                let (engine, adapter) = (PathBuf::from(engine), PathBuf::from(adapter));
                assert!(engine.is_absolute() && adapter.is_absolute());
                Self {
                    engine,
                    adapter,
                    baseline: true,
                }
            }
            _ => panic!("select the matching baseline engine and adapter together"),
        }
    }

    fn snapshot(&self, case: &Case, root: &Path, name: &str, layers: &str) -> PathBuf {
        let out = case.temp.path().join(name);
        ok(&case
            .command(&self.engine)
            .arg("index")
            .arg("--adapter")
            .arg(&self.adapter)
            .arg("--root")
            .arg(root)
            .arg("--layers")
            .arg(layers)
            .arg("--out")
            .arg(&out)
            .output()
            .unwrap());
        ok(&case
            .command(&self.engine)
            .arg("validate")
            .arg(&out)
            .arg("--root")
            .arg(root)
            .output()
            .unwrap());
        let manifest: Value =
            serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["schema"],
            if self.baseline { "0.5" } else { "0.6" }
        );
        out
    }
}

fn has_site(row: &Value, field: &str, site: &Value) -> bool {
    row[field]
        .as_array()
        .is_some_and(|sites| sites.contains(site))
}

#[test]
fn token_sites_join_rust_basic_index() {
    let case = Case::new();
    let root = case.temp.path().join("rust-basic");
    support::copy_tree(&fixture().with_file_name("rust-basic"), &root);
    ok(&case
        .command(&case.bin.join("cargo"))
        .args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .unwrap());
    let binaries = TokenBinaries::selected();
    let out = binaries.snapshot(&case, &root, "syntax", "base,syntax");
    let base = binaries.snapshot(&case, &root, "base", "base");
    let base = table(&base, "relations.jsonl");
    let rows = table(&out, "relations.jsonl");
    assert_eq!(syntax_state(&out), "present");
    for site in [
        json!(["src/lib.rs", 4, 8, 4, 13]),
        json!(["src/lib.rs", 4, 15, 4, 21]),
    ] {
        let reference = |rows: &[Value], marked| {
            rows.iter().any(|row| {
                row["rel"] == "ref"
                    && has_site(row, "sites", &site)
                    && has_site(row, "syntax_import_sites", &site) == marked
                    && row["roles"].as_array().unwrap().contains(&json!("import")) == marked
            })
        };
        assert!(reference(&base, false));
        assert!(reference(&rows, !binaries.baseline));
    }
    let entities = table(&out, "entities.jsonl");
    for (method, target, kind, site) in [
        (
            "new",
            "views/Joined#",
            "impl-self-type",
            json!(["src/views.rs", 22, 9, 22, 15]),
        ),
        (
            "fmt",
            "views/Joined#",
            "impl-self-type",
            json!(["src/views.rs", 28, 27, 28, 33]),
        ),
        (
            "fmt",
            "fmt/Display#",
            "impl-trait",
            json!(["src/views.rs", 28, 15, 28, 22]),
        ),
    ] {
        let def = entities
            .iter()
            .find(|row| row["name"] == method && row["def"].is_object())
            .unwrap();
        let parent = &def["parent"];
        assert!(
            entities
                .iter()
                .any(|row| &row["key"] == parent && row["virtual"] == true)
        );
        assert_eq!(
            rows.iter().any(|row| row["rel"] == kind
                && &row["from"] == parent
                && row["to"].as_str().unwrap().ends_with(target)
                && has_site(row, "sites", &site)),
            !binaries.baseline,
            "{kind} {site}"
        );
        assert!(
            base.iter()
                .any(|row| row["rel"] == "ref" && has_site(row, "sites", &site))
        );
    }
}

#[test]
fn token_sites_join_rust_calls_index() {
    let (case, root) = rust_calls();
    let binaries = TokenBinaries::selected();
    let out = binaries.snapshot(&case, &root, "syntax", "base,syntax");
    let base = binaries.snapshot(&case, &root, "base", "base");
    let base = table(&base, "relations.jsonl");
    let rows = table(&out, "relations.jsonl");
    let entities = table(&out, "entities.jsonl");
    for (kind, end, site) in [
        (
            "impl-self-type",
            "drops/Token#",
            json!(["calls-lib/src/drops.rs", 62, 18, 62, 23]),
        ),
        (
            "impl-trait",
            "ops/drop/Drop#",
            json!(["calls-lib/src/drops.rs", 62, 9, 62, 13]),
        ),
        (
            "impl-self-type",
            "Wrapper#",
            json!(["calls-lib/src/lib.rs", 125, 8, 125, 15]),
        ),
        (
            "impl-trait",
            "a/Task#",
            json!(["calls-lib/src/lib.rs", 387, 8, 387, 12]),
        ),
        (
            "impl-trait",
            "Tagged#",
            json!(["calls-lib/src/lib.rs", 253, 8, 253, 14]),
        ),
        (
            "impl-self-type",
            "Kept#",
            json!(["calls-lib/src/lib.rs", 430, 5, 430, 9]),
        ),
    ] {
        let link = rows
            .iter()
            .find(|row| row["rel"] == kind && has_site(row, "sites", &site));
        assert_eq!(link.is_some(), !binaries.baseline, "{kind} {site}");
        if let Some(link) = link {
            assert!(link["to"].as_str().unwrap().ends_with(end), "{link}");
            assert!(
                entities
                    .iter()
                    .any(|row| row["parent"] == link["from"] && row["def"].is_object()),
                "{link}"
            );
        }
    }
    let file = "calls-lib/src/lib.rs";
    for (line, start, end) in [(462, 12, 24), (462, 26, 30), (462, 34, 38), (288, 8, 18)] {
        let site = json!([file, line, start, line, end]);
        assert!(base.iter().any(|row| has_site(row, "sites", &site)));
        assert_eq!(
            rows.iter()
                .any(|row| has_site(row, "syntax_import_sites", &site)),
            !binaries.baseline,
            "{site}"
        );
    }
    let gaps = table(&out, "gaps.jsonl");
    assert!(gaps.iter().any(|row| row["gap"] == "opaque-region"
        && row["file"] == file
        && row["range"] == json!([109, 80, 109, 94])));
    assert!(
        !rows
            .iter()
            .flat_map(|row| row["syntax_import_sites"].as_array().into_iter().flatten())
            .any(|site| site[0] == file && (site[1] == 291 || site[1] == 109))
    );
    assert!(!rows.iter().any(|row| {
        (row["rel"] == "impl-self-type" && row["from"] == "cargo:calls-lib:impl#[T][Tagged]")
            || (row["rel"] == "impl-trait"
                && row["from"] == "cargo:calls-lib:impl#[Local][Task]"
                && row["to"] == "cargo:calls-lib:b/Task#")
    }));
    let implementations = |relations: &[Value]| {
        relations
            .iter()
            .filter(|row| row["rel"] == "implements")
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(implementations(&rows), implementations(&base));
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one fixture and both runs of the Cargo evidence check"
)]
fn token_sites_survive_unavailable_test_evidence() {
    use std::os::unix::fs::PermissionsExt as _;

    let case = Case::new();
    let root = case.temp.path().join("probe");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'probe'\nversion = '0.1.0'\nedition = '2021'\n",
    )
    .unwrap();
    let text = concat!(
        "pub mod model { pub struct Token; }\n",
        "pub use model::Token;\n",
        "impl Token { pub fn method() {} }\n",
        "#[cfg(test)] #[test]\n",
        "fn probe_harness_test() {}\n",
    );
    let file = "src/lib.rs";
    fs::write(root.join(file), text).unwrap();
    let name = |line: usize, token: &str| {
        let col = text.lines().nth(line).unwrap().find(token).unwrap();
        [
            i32::try_from(line).unwrap(),
            i32::try_from(col).unwrap(),
            i32::try_from(col + token.len()).unwrap(),
        ]
    };
    let body = |line: usize| {
        [
            i32::try_from(line).unwrap(),
            0,
            i32::try_from(text.lines().nth(line).unwrap().len()).unwrap(),
        ]
    };
    let model = "rust-analyzer cargo probe 0.1.0 model/";
    let token = "rust-analyzer cargo probe 0.1.0 model/Token#";
    let reference = |symbol: &str, range: [i32; 3]| Occurrence {
        symbol: symbol.into(),
        range: range.to_vec(),
        ..Occurrence::default()
    };
    let mut module = definition(
        "rust-analyzer cargo probe 0.1.0 crate/",
        [0, 0, 0],
        [0, 0, 0],
    );
    module.enclosing_range = vec![0, 0, 5, 0];
    let mut index =
        Index::parse_from_bytes(&fs::read(fixture().join("index.scip")).unwrap()).unwrap();
    index.documents = vec![Document {
        relative_path: file.into(),
        language: "rust".into(),
        position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
        occurrences: vec![
            module,
            definition(model, name(0, "model"), body(0)),
            definition(token, name(0, "Token"), body(0)),
            reference(model, name(1, "model")),
            reference(token, name(1, "Token")),
            reference(token, name(2, "Token")),
            definition(
                "rust-analyzer cargo probe 0.1.0 impl#[Token]method().",
                name(2, "method"),
                body(2),
            ),
            definition(
                "rust-analyzer cargo probe 0.1.0 probe_harness_test().",
                name(4, "probe_harness_test"),
                body(4),
            ),
        ],
        ..Document::default()
    }];
    fs::write(root.join("index.scip"), index.write_to_bytes().unwrap()).unwrap();
    let cargo = case.bin.join("cargo");
    ok(&case
        .command(&cargo)
        .args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .unwrap());
    assert!(root.join("Cargo.lock").exists());
    let binaries = TokenBinaries::selected();
    let successful = binaries.snapshot(&case, &root, "success", "base,syntax");

    let real = case.bin.join("real-cargo");
    fs::rename(&cargo, &real).unwrap();
    let intercepted = case.temp.path().join("intercepted");
    fs::write(
        &cargo,
        format!(
            r#"#!/bin/sh
if [ "$1" = check ]; then
    lib=
    bins=
    for arg do
        case "$arg" in
            --lib) lib=1 ;;
            --bins) bins=1 ;;
        esac
    done
    if [ "$lib$bins" = 11 ]; then
        echo intercepted >> '{}'
        exit 1
    fi
fi
exec '{}' "$@"
"#,
            intercepted.display(),
            real.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    let failed = binaries.snapshot(&case, &root, "failed", "base,syntax");
    // The check fails in the directories it shares, then again in directories of its own.
    assert_eq!(
        fs::read_to_string(intercepted).unwrap(),
        "intercepted\nintercepted\n"
    );
    let entities = |out: &Path, expected_mark| {
        let mut rows = table(out, "entities.jsonl");
        let marked_entity = rows
            .iter_mut()
            .find(|row| row["name"] == "probe_harness_test")
            .unwrap();
        assert_eq!(
            marked_entity.as_object_mut().unwrap().remove("test"),
            expected_mark
        );
        rows
    };
    assert_eq!(
        entities(&successful, Some(json!("cfg"))),
        entities(&failed, None)
    );
    assert_eq!(
        table(&successful, "syntax.jsonl"),
        table(&failed, "syntax.jsonl")
    );
    for (out, unknown) in [(&successful, false), (&failed, true)] {
        assert_eq!(
            table(out, "gaps.jsonl")
                .iter()
                .any(|row| row["gap"] == "skipped-file"
                    && row["file"] == file
                    && row["layer"] == "syntax"
                    && row["code"] == "test-unknown"),
            unknown
        );
    }
    let rows = table(&successful, "relations.jsonl");
    assert_eq!(rows, table(&failed, "relations.jsonl"));
    let site = |[line, start, end]: [i32; 3]| json!([file, line, start, line, end]);
    for mark in [site(name(1, "model")), site(name(1, "Token"))] {
        assert!(
            rows.iter()
                .any(|row| row["rel"] == "ref" && has_site(row, "sites", &mark))
        );
        assert_eq!(
            rows.iter()
                .any(|row| has_site(row, "syntax_import_sites", &mark)),
            !binaries.baseline
        );
    }
    let mark = site(name(2, "Token"));
    assert_eq!(
        rows.iter().any(|row| row["rel"] == "impl-self-type"
            && row["to"] == "cargo:probe:model/Token#"
            && has_site(row, "sites", &mark)),
        !binaries.baseline
    );
}

// The fixture of the calls layer: the snapshot is the golden, and every case of the
// fixture's note has the relations it says, and no others.
/// A site of an evidence entry: file, one-based line and column.
type Site = (String, u64, u64);

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one list of the note's cases, each beside the reason it holds"
)]
fn the_calls_of_the_fixture_reach_the_snapshot() {
    let (case, root) = rust_calls();
    let out = case.temp.path().join("snapshot");
    ok(&index(&case, &root, &out));
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-calls/expected");
    assert_eq!(snapshot_bytes(&out), snapshot_bytes(&golden));

    let relations = table(&out, "relations.jsonl");
    let compiled: Vec<&Value> = relations
        .iter()
        .filter(|r| r["src"] == "adapter:rust/calls")
        .collect();
    for r in &compiled {
        assert_eq!(r["rel"], "call");
        assert_eq!(r["mechanism"], "static", "{r}");
        assert_eq!(r["resolution"], "direct", "{r}");
    }
    let evidence = |from: &str, to: &str| -> Vec<Site> {
        compiled
            .iter()
            .filter(|r| r["from"] == format!("cargo:{from}") && r["to"] == format!("cargo:{to}"))
            .flat_map(|r| r["evidence"].as_array().expect("evidence").clone())
            .map(|e| {
                (
                    e["file"].as_str().expect("a file").to_owned(),
                    e["line"].as_u64().expect("a line") + 1,
                    e["col"].as_u64().expect("a column") + 1,
                )
            })
            .collect()
    };
    let lib = "calls-lib/src/lib.rs";
    let at = |file: &str, sites: &[(u64, u64)]| -> Vec<Site> {
        sites
            .iter()
            .map(|&(l, c)| (file.to_owned(), l, c))
            .collect()
    };
    // Each relation the note expects, with its sites (one-based, compiler columns).
    let expected: [(&str, &str, Vec<Site>); 24] = [
        (
            "calls-lib:direct().",
            "calls-lib:leaf().",
            at(lib, &[(17, 5)]),
        ),
        // The binary emits its own instance of the library's generic function.
        (
            "calls-app:main().~3",
            "calls-lib:generic().",
            at("calls-app/src/main.rs", &[(14, 13)]),
        ),
        (
            "calls-lib:two_types().",
            "calls-lib:generic().",
            at(lib, &[(25, 6), (25, 21)]),
        ),
        (
            "calls-lib:get_two().",
            "calls-lib:impl#[`Wrapper<T>`]get().",
            at(lib, &[(299, 19), (299, 40)]),
        ),
        // A closure's and an async body's calls belong to the function they are written in.
        (
            "calls-lib:make_adder().",
            "calls-lib:leaf().",
            at(lib, &[(39, 9)]),
        ),
        (
            "calls-lib:after_await().",
            "calls-lib:leaf().",
            at(lib, &[(54, 5)]),
        ),
        (
            "calls-lib:two_on_one_line().",
            "calls-lib:leaf().",
            at(lib, &[(312, 6), (312, 14)]),
        ),
        (
            "calls-lib:use_operator().",
            "calls-lib:impl#[Meters][`Add<Self>`]add().",
            at(lib, &[(326, 5)]),
        ),
        (
            "calls-lib:use_greet().",
            "calls-lib:Greeter#greet().",
            at(lib, &[(174, 11)]),
        ),
        // One generic caller emitted twice: two instructions at one site.
        (
            "calls-lib:twice().",
            "calls-lib:leaf().",
            at(lib, &[(29, 5), (29, 5)]),
        ),
        (
            "calls-lib:use_shared_entry().",
            "calls-lib:shared_entry().",
            at(lib, &[(330, 5)]),
        ),
        // Each binary's `main` calls its own `helper`.
        (
            "calls-app:main().~2",
            "calls-app:helper().~1",
            at("calls-app/src/bin/twin.rs", &[(6, 13)]),
        ),
        (
            "calls-app:main().~3",
            "calls-app:helper().~2",
            at("calls-app/src/main.rs", &[(17, 13)]),
        ),
        // A function that exists only under the configured `--cfg`.
        (
            "calls-lib:use_flag().",
            "calls-lib:only_with_flag().",
            at(lib, &[(308, 5)]),
        ),
        (
            "calls-lib:only_with_flag().",
            "calls-lib:leaf().",
            at(lib, &[(304, 5)]),
        ),
        // A file mounted under two module names, and one the integration test also mounts.
        (
            "calls-lib:use_mounted().",
            "calls-lib:under_test/mounted().",
            at(lib, &[(295, 6)]),
        ),
        (
            "calls-lib:under_test/mounted().",
            "calls-lib:leaf().",
            at("calls-lib/src/mounted.rs", &[(4, 5)]),
        ),
        (
            "calls-lib:use_shared().",
            "calls-lib:shared/shared_call().",
            at(lib, &[(420, 5)]),
        ),
        (
            "calls-lib:shared/shared_call().",
            "calls-lib:shared/shared_leaf().",
            at("calls-lib/src/shared.rs", &[(8, 5)]),
        ),
        (
            "calls-lib:use_blanket().",
            "calls-lib:impl#[T][Tagged]tag().",
            at(lib, &[(261, 9)]),
        ),
        // Of two methods whose closures are on lines of their own, one row each.
        (
            "calls-lib:impl#[Apart][First]go().",
            "calls-lib:leaf().",
            at(lib, &[(512, 13)]),
        ),
        (
            "calls-lib:impl#[Apart][Second]go().",
            "calls-lib:leaf().",
            at(lib, &[(518, 13)]),
        ),
        // Of the two whose bodies share a line, the second's own call only.
        (
            "calls-lib:impl#[Joined][Second]go().",
            "calls-lib:leaf().",
            at(lib, &[(526, 69)]),
        ),
        (
            "calls-lib:drops/drop_lookalike().",
            "fake-core:mem/drop().",
            at("calls-lib/src/drops.rs", &[(77, 5)]),
        ),
    ];
    for (from, to, sites) in &expected {
        assert_eq!(&evidence(from, to), sites, "{from} -> {to}");
    }
    // The base reference of each `helper` call points to the two helpers' group.
    for (from, file, line) in [
        ("cargo:calls-app:main().~2", "calls-app/src/bin/twin.rs", 5),
        ("cargo:calls-app:main().~3", "calls-app/src/main.rs", 16),
    ] {
        assert!(
            relations.iter().any(|r| r["rel"] == "ref"
                && r["from"] == from
                && r["to"] == "cargo:calls-app:helper()."
                && r["sites"][0][0] == file
                && r["sites"][0][1] == line),
            "the reference from {from} to the group"
        );
    }

    // Only after those are found: what the layer withholds.
    let ends = |key: &str| {
        compiled
            .iter()
            .any(|r| r["from"] == format!("cargo:{key}") || r["to"] == format!("cargo:{key}"))
    };
    let from = |key: &str| compiled.iter().any(|r| r["from"] == format!("cargo:{key}"));
    // A call through a trait object: no compiled relation, the syntax layer's stays.
    assert!(!from("calls-lib:dyn_call()."));
    assert!(
        relations
            .iter()
            .any(|r| r["from"] == "cargo:calls-lib:dyn_call()."
                && r["to"] == "cargo:calls-lib:Shape#area()."
                && r["src"] == "adapter:rust/syntax")
    );
    // The returned closure, the function passed by name, and the generated method's helper.
    assert_eq!(
        evidence("calls-lib:use_adder().", "calls-lib:make_adder()."),
        at(lib, &[(45, 5)])
    );
    assert_eq!(
        compiled
            .iter()
            .filter(|r| r["from"] == "cargo:calls-lib:use_adder().")
            .count(),
        1
    );
    assert_eq!(
        evidence("calls-lib:pass_by_name().", "calls-lib:leaf()."),
        []
    );
    for key in [
        "calls-lib:proxy_helper().",
        "calls-lib:impl#[Engine]run().",
        "calls-lib:use_proxy().",
        // Both calls on the `Slot` line, and the written method of `a::Task`.
        "calls-lib:use_slots().",
        "calls-lib:impl#[`Slot<u8>`]run().",
        "calls-lib:impl#[Local][Task]work().",
        "calls-lib:use_chosen().",
        // Lines a macro template or a derived local type takes.
        "calls-lib:const_run/run().",
        "calls-lib:const_run/nested().",
        // A file with a byte order mark.
        "calls-lib:bom/bom_call().",
        "calls-lib:bom/bom_leaf().",
        // Written methods that share a line with a derive's type.
        "calls-lib:impl#[Panel]label().",
        "calls-lib:impl#[Sheet]label().",
        // Anything under an attribute macro, however named, or a `cfg_attr` holding one.
        "calls-lib:kept().",
        "calls-lib:impl#[Kept]kept_method().",
        "calls-lib:spelled/impl#[ByTest]by_test().",
        "calls-lib:impl#[ByClippy]by_clippy().",
        "calls-lib:maybe_kept().",
        // Two closure-bodied methods sharing a line, and the first of the joined pair.
        "calls-lib:impl#[Shared][First]go().",
        "calls-lib:impl#[Shared][Second]go().",
        "calls-lib:impl#[Joined][First]go().",
        // A written method and its generated copy, and the function written twice on a line.
        "calls-lib:impl#[Original]work().",
        "calls-lib:use_copies().",
        "calls-lib:alt().",
        // Functions that exist only under `cfg(test)`, and the binary that is not built.
        "calls-lib:only_in_test().",
        "calls-lib:call_only_in_test().",
        "calls-app:main().~1",
    ] {
        // These are called, with rows, from `use_go` and the like; only their own calls are
        // withheld.
        let withheld = key.ends_with("use_chosen().")
            || key.ends_with("use_copies().")
            || key.contains("[Shared]")
            || key.contains("[Joined][First]");
        if withheld {
            assert!(!from(key), "a compiled relation from {key}");
        } else {
            assert!(!ends(key), "a compiled relation at {key}");
        }
    }
    // `nested_run::run` keeps exactly its own call, not the nested function's closure.
    assert_eq!(
        evidence("calls-lib:nested_run/run().", "calls-lib:leaf()."),
        at(lib, &[(594, 48)])
    );
    // `templated::run`: the line of the template gives nothing, the next line its call.
    assert_eq!(
        evidence("calls-lib:templated/run().", "calls-lib:leaf()."),
        at(lib, &[(615, 22)])
    );
    // `local_derive`: the line of the derived type gives nothing.
    assert_eq!(
        evidence("calls-lib:local_derive().", "calls-lib:leaf()."),
        at(lib, &[(628, 18)])
    );
    // Nothing from a binary into the library other than through an instance it emits, and
    // nothing to the function the binary only declares under the name `_Read`.
    let into_lib: Vec<&Value> = compiled
        .iter()
        .filter(|r| {
            r["from"]
                .as_str()
                .is_some_and(|f| f.starts_with("cargo:calls-app:"))
                && r["to"]
                    .as_str()
                    .is_some_and(|t| t.starts_with("cargo:calls-lib:"))
        })
        .copied()
        .collect();
    let targets: BTreeSet<&str> = into_lib
        .iter()
        .map(|r| r["to"].as_str().expect("a key"))
        .collect();
    assert_eq!(
        targets,
        BTreeSet::from(["cargo:calls-lib:generic().", "cargo:calls-lib:hinted()."])
    );
    assert!(!compiled.iter().any(|r| {
        r["to"] == "cargo:calls-lib:read_value()."
            && r["from"]
                .as_str()
                .is_some_and(|f| f.starts_with("cargo:calls-app:"))
    }));
    assert!(!compiled.iter().any(|r| {
        r["to"] == "cargo:calls-lib:shared_entry()."
            && r["from"]
                .as_str()
                .is_some_and(|f| f.starts_with("cargo:calls-app:"))
    }));
    // No relation says `mono-instantiated` or `drop`.
    assert!(
        !relations
            .iter()
            .any(|r| r["resolution"] == "mono-instantiated" || r["mechanism"] == "drop")
    );
    // Every entry of the note: 77 relations, 83 entries.
    assert_eq!(compiled.len(), 77);
    assert_eq!(
        compiled
            .iter()
            .map(|r| r["evidence"].as_array().expect("evidence").len())
            .sum::<usize>(),
        83
    );
}
