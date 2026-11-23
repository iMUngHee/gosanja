//! Adapter side-car conformance against concrete definitions and reference sites.
use gosanja_core::adapter::{read_calls, read_syntax};
use gosanja_schema::compat::{Acceptance, Version, accept};
use std::io::Cursor;

#[test]
fn newer_minor_skips_only_unknown_enum_strings() {
    let policy = accept(Version::new(1, 0), Version::new(1, 1)).unwrap();
    let known = r#"{"anchor":{"file":"x","line":0,"name":"f"},"visibility":"pub"}"#;
    let unknown = r#"{"anchor":{"file":"x","line":0,"name":"f"},"outline":[{"k":"future","range":[0,0,0,1]}]}"#;
    let data = format!("{unknown}\n{known}\n");
    let (rows, skipped) = read_syntax(Cursor::new(&data), policy).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].visibility.as_deref(), Some("pub"));
    assert_eq!(skipped, 1);
    read_syntax(Cursor::new(&data), Acceptance::Full).unwrap_err();
    for invalid in [
        unknown.replace("\"f\"", "false"),
        unknown.replace("\"x\"", "\"../x\""),
        unknown.replace("\"future\"", "3"),
        unknown.replace("[0,0,0,1]", "[0,1]"),
        unknown.replace("[0,0,0,1]", "[0,3,0,1]"),
    ] {
        read_syntax(Cursor::new(format!("{known}\n{invalid}\n")), policy).unwrap_err();
    }
    let row = r#"{"caller":{"file":"x","line":0,"col":0},"callee":{"file":"x","line":1,"col":0},"resolution":"direct","mechanism":"static","build":"d"}"#;
    let (calls, count) = read_calls(
        Cursor::new(format!("{row}\n{}\n", row.replace("static", "future"))),
        policy,
    )
    .unwrap();
    assert_eq!((calls.len(), count), (1, 1));
}

use gosanja_core::{
    adapter::{self, AdapterOutput},
    graph::Snapshot,
    sidecar,
    write::{FsSource, serialise},
};
use gosanja_schema::{
    contract::{CallEvidence, SyntaxFact},
    position::{PositionEncoding, Range},
    snapshot::{GapKind, Layer, Mechanism, Rel, Resolution},
};
use protobuf::Message;
use scip::types::{Document, Index, Occurrence, PositionEncoding as ScipEncoding};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Seek, Write},
    path::Path,
    process::Command,
};

const SOURCE: &str = "owner\n🦀foo(bar)\nempty()\nfoo bar empty\ndup dup\n";
fn fixture(root: &Path, tmp: &Path, mode: &str) -> AdapterOutput {
    let mut output = adapter::run(
        || {
            let mut command = Command::new("sh");
            command
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../fixtures/fake-adapter/adapter.sh"
                ))
                .args(["--mode", mode]);
            command
        },
        root,
        tmp,
    )
    .unwrap();
    let mut doc = Document {
        relative_path: "x".into(),
        language: "test".into(),
        position_encoding: ScipEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
        ..Default::default()
    };
    for (line, col, end, name, def) in [
        (0, 0, 5, "owner().", 1),
        (3, 0, 3, "Interface#foo().", 1),
        (3, 4, 7, "bar.", 1),
        (3, 8, 13, "empty().", 1),
        (4, 0, 3, "A#dup().", 1),
        (4, 4, 7, "B#dup().", 1),
        (1, 4, 7, "Interface#foo().", 0),
        (1, 8, 11, "bar.", 0),
        (2, 0, 5, "empty().", 0),
    ] {
        let occ = Occurrence {
            symbol: format!("scip cargo test 1 {name}"),
            range: vec![line, col, end],
            symbol_roles: def,
            ..Default::default()
        };
        doc.occurrences.push(occ);
    }
    let bytes = Index {
        documents: vec![doc],
        ..Default::default()
    }
    .write_to_bytes()
    .unwrap();
    output.index = tempfile::tempfile().unwrap();
    output.index.write_all(&bytes).unwrap();
    output.index.rewind().unwrap();
    output.manifest.position_encoding = PositionEncoding::Utf16;
    output
}
fn run(facts: &[Value], observations: &[Value]) -> Snapshot {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("x"), SOURCE).unwrap();
    let mut output = fixture(&root, tmp.path(), "good");
    output.syntax = facts
        .iter()
        .cloned()
        .map(serde_json::from_value::<SyntaxFact>)
        .collect::<Result<_, _>>()
        .unwrap();
    output.calls = observations
        .iter()
        .cloned()
        .map(serde_json::from_value::<CallEvidence>)
        .collect::<Result<_, _>>()
        .unwrap();
    sidecar::build(output, &FsSource::new(&root).unwrap(), "fixture").unwrap()
}
fn owner() -> Value {
    json!({"file":"x","line":0,"name":"owner"})
}
fn fact() -> Value {
    json!({"anchor":owner(),"body":[0,0,2,7],"outline":[{"k":"branch","range":[1,0,2,7],"children":[
    {"k":"call","range":[1,2,1,10],"callee":[1,2,1,5]},
    {"k":"call","range":[2,0,2,7],"callee":[2,0,2,5]},
    {"k":"call","range":[1,0,1,10],"callee":[1,2,1,4]},
    {"k":"opaque","range":[1,0,1,2]}]}]})
}

#[test]
fn syntax_body_supplies_ownership_and_only_exact_callee_sites_become_declared_calls() {
    let base = run(&[], &[]);
    assert_eq!(base.relations.len(), 0);
    let got = run(&[fact()], &[]);
    let owner = got.entities.iter().find(|row| row.name == "owner").unwrap();
    let def = owner.def.as_ref().unwrap();
    assert_eq!(def.body, Some(Range::new(0, 0, 2, 7)));
    assert_eq!(def.body_src.as_deref(), Some("adapter:fake/syntax"));
    assert_eq!(
        got.gaps
            .iter()
            .filter(|g| g.gap == GapKind::UnknownBuildContext)
            .count(),
        0
    );
    let calls: Vec<_> = got
        .relations
        .iter()
        .filter(|r| r.rel == Rel::Call)
        .collect();
    assert_eq!(calls.len(), 2);
    for (to, line, start, end) in [
        ("cargo:test:Interface#foo().", 1, 4, 7),
        ("cargo:test:empty().", 2, 0, 5),
    ] {
        let call = calls.iter().find(|r| r.to == to).unwrap();
        assert_eq!(call.from, "cargo:test:owner().");
        assert_eq!(call.src, "adapter:fake/syntax");
        assert_eq!(
            (call.mechanism, call.resolution),
            (Some(Mechanism::Unknown), Some(Resolution::Declared))
        );
        assert_eq!(
            serde_json::to_value(&call.evidence).unwrap(),
            json!([{"file":"x","line":line,"col":start,"end":[line,end]}])
        );
        assert!(
            got.relations
                .iter()
                .any(|r| r.rel == Rel::Ref && r.to == to)
        );
    }
    assert!(
        got.relations
            .iter()
            .any(|r| r.rel == Rel::Ref && r.to == "cargo:test:bar.")
    );
    let gap = got
        .gaps
        .iter()
        .find(|g| g.gap == GapKind::OpenCall)
        .unwrap();
    assert_eq!(gap.range, Some(Range::new(1, 4, 1, 6)));
    let opaque = got
        .gaps
        .iter()
        .find(|g| g.gap == GapKind::OpaqueRegion)
        .unwrap();
    assert_eq!(opaque.range, Some(Range::new(1, 0, 1, 4)));
    assert_eq!(opaque.src, "adapter:fake/syntax");
}

fn observation() -> Value {
    json!({"caller":owner(),"callee":{"file":"x","line":3,"col":0},"mechanism":"static","resolution":"direct","build":"dev"})
}

#[test]
fn compiler_evidence_stays_with_its_classification_site_and_build() {
    let first = observation();
    let mut second = first.clone();
    second["build"] = json!("release");
    second["site"] = json!({"file":"x","line":1,"col":2});
    let got = run(&[], &[first.clone(), second.clone()]);
    assert_eq!(
        serde_json::to_value(&got.scope.builds).unwrap(),
        json!([{"id":"dev","features":["fast"]},{"id":"release","features":["full"]}])
    );
    assert_eq!(got.relations.len(), 1);
    assert_eq!(
        serde_json::to_value(&got.relations[0].evidence).unwrap(),
        json!([{"build":"dev"},{"file":"x","line":1,"col":4,"build":"release"}])
    );
    second["mechanism"] = json!("dynamic");
    second["resolution"] = json!("coercion-filtered");
    let mut one = first;
    one["site"] = json!({"file":"x","line":2,"col":0});
    let got = run(&[], &[one, second]);
    assert_eq!(got.relations.len(), 2);
    for (m, r, e) in [
        (
            Mechanism::Static,
            Resolution::Direct,
            json!([{"file":"x","line":2,"col":0,"build":"dev"}]),
        ),
        (
            Mechanism::Dynamic,
            Resolution::CoercionFiltered,
            json!([{"file":"x","line":1,"col":4,"build":"release"}]),
        ),
    ] {
        let row = got
            .relations
            .iter()
            .find(|row| row.mechanism == Some(m))
            .unwrap();
        assert_eq!(row.resolution, Some(r));
        assert_eq!(row.rel, Rel::Call);
        assert_eq!(row.from, "cargo:test:owner().");
        assert_eq!(row.to, "cargo:test:Interface#foo().");
        assert_eq!(row.src, "adapter:fake/calls");
        assert_eq!(serde_json::to_value(&row.evidence).unwrap(), e);
    }
}

#[test]
fn bad_rows_are_counted_without_losing_other_rows() {
    let row = observation();
    let mut unknown = row.clone();
    unknown["build"] = json!("absent");
    unknown["callee"]["col"] = json!(99);
    let mut unresolved = row.clone();
    unresolved["callee"] = json!({"file":"x","line":4,"name":"dup"});
    let mut bad_site = row.clone();
    bad_site["site"] = json!({"file":"x","line":1,"col":1});
    let mut bad = fact();
    bad["outline"][0]["children"][2]["callee"] = json!([1, 1, 1, 2]);
    let got = run(
        &[bad, json!({"anchor":{"file":"x","line":0,"name":"absent"}})],
        &[row, unknown, unresolved, bad_site],
    );
    assert_eq!(got.relations.len(), 1);
    assert_eq!(
        got.scope.layers.syntax,
        gosanja_schema::snapshot::LayerState::Present
    );
    assert_eq!(got.syntax.len(), 0);
    assert_eq!(
        got.entities
            .iter()
            .find(|e| e.name == "owner")
            .unwrap()
            .def
            .as_ref()
            .unwrap()
            .body,
        None
    );
    assert_eq!(
        got.gaps
            .iter()
            .filter(|g| g.gap == GapKind::Unanchored)
            .count(),
        2
    );
    for (kind, layer, file) in [
        (GapKind::UnknownBuildContext, Layer::Calls, None),
        (GapKind::PositionOutOfRange, Layer::Calls, Some("x")),
        (GapKind::PositionOutOfRange, Layer::Syntax, Some("x")),
    ] {
        let gap = got
            .gaps
            .iter()
            .find(|g| g.gap == kind && g.layer == Some(layer))
            .unwrap();
        assert_eq!(gap.file.as_deref(), file);
        assert_eq!(gap.count, Some(1));
    }
}

#[test]
fn additional_sidecar_fields_leave_the_snapshot_bytes_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/rust-basic"
    ));
    let make = |mode| {
        let adapter = adapter::run(
            || {
                let mut cmd = Command::new("sh");
                cmd.arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../fixtures/fake-adapter/adapter.sh"
                ))
                .args(["--mode", mode]);
                cmd
            },
            root,
            tmp.path(),
        )
        .unwrap();
        let snapshot = sidecar::build(adapter, &FsSource::new(root).unwrap(), "fixture").unwrap();
        assert_eq!(snapshot.syntax.len(), 1);
        assert_eq!(snapshot.syntax[0].visibility.as_deref(), Some("pub"));
        serialise(&snapshot)
    };
    assert_eq!(make("good").named(), make("extra").named());
}

fn assert_discarded_syntax(snapshot: &Snapshot) {
    use gosanja_schema::snapshot::{FailedDetail, LayerState};
    assert_eq!(snapshot.scope.layers.syntax, LayerState::Failed);
    assert_eq!(
        gosanja_core::validate::validate(&serialise(snapshot), None).unwrap(),
        Vec::<gosanja_core::validate::Diagnostic>::new()
    );
    assert_eq!(snapshot.syntax.len(), 0);
    assert_eq!(snapshot.relations.len(), 0);
    let owner = snapshot
        .entities
        .iter()
        .find(|e| e.name == "owner")
        .unwrap();
    let def = owner.def.as_ref().unwrap();
    assert_eq!((def.body, &def.body_src), (None, &None));
    let gaps: Vec<_> = snapshot
        .gaps
        .iter()
        .filter(|g| g.src == "adapter:fake/syntax")
        .collect();
    assert_eq!(gaps.len(), 1);
    assert_eq!(
        (gaps[0].gap, gaps[0].layer, gaps[0].detail),
        (
            GapKind::FailedLayer,
            Some(Layer::Syntax),
            Some(FailedDetail::RowInvalid)
        )
    );
}

#[test]
fn duplicate_resolved_definition_discards_syntax_in_either_order() {
    let first = fact();
    let other = json!({"anchor":{"file":"x","line":0,"col":0},"body":[0,0,0,90]});
    let missing = json!({"anchor":{"file":"x","line":2,"name":"no-match"}});
    let a = run(&[missing.clone(), first.clone(), other.clone()], &[]);
    let b = run(&[other, first, missing], &[]);
    assert_discarded_syntax(&a);
    assert_eq!(serialise(&a).named(), serialise(&b).named());
}

#[test]
fn malformed_outline_geometry_discards_the_entire_syntax_layer() {
    let wrong_callee = json!({"k":"call","range":[1,0,1,2],"callee":[1,2,1,5]});
    let escaped_child =
        json!({"k":"branch","range":[1,0,1,2],"children":[{"k":"opaque","range":[2,0,2,2]}]});
    let backwards = json!({"k":"opaque","range":[1,5,1,2]});
    for node in [wrong_callee, escaped_child, backwards] {
        let row = json!({"anchor":{"file":"x","line":3,"name":"unmatched"},"outline":[node]});
        let a = run(&[fact(), row.clone()], &[]);
        let b = run(&[row, fact()], &[]);
        assert_discarded_syntax(&a);
        assert_eq!(serialise(&a).named(), serialise(&b).named());
    }
    let backwards = json!({"anchor":{"file":"x","line":3,"name":"empty"},"body":[3,5,3,2]});
    assert_discarded_syntax(&run(&[fact(), backwards], &[]));
}

#[test]
fn generated_call_site_uses_the_source_boundary_and_records_file_metadata() {
    struct Checked(FsSource);
    impl gosanja_core::write::SourceReader for Checked {
        fn locate(&self, path: &str) -> gosanja_core::write::Location {
            self.0.locate(path)
        }
        fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
            assert_eq!(
                self.locate(path),
                gosanja_core::write::Location::Inside,
                "outside file was opened"
            );
            self.0.read(path)
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("x"), SOURCE).unwrap();
    fs::write(root.join("generated"), "🦀x\n").unwrap();
    fs::write(root.join("bad-position"), "x").unwrap();
    let paths = [
        "generated",
        "generated",
        "missing",
        "bad-position",
        #[cfg(unix)]
        "link",
    ];
    let outside = tmp.path().join("outside");
    fs::write(&outside, "🦀x\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside, root.join("link")).unwrap();
    let mut output = fixture(&root, tmp.path(), "good");
    output.syntax.clear();
    output.calls = paths
        .iter()
        .map(|path| {
            let mut obs = observation();
            obs["site"] = json!({"file":path,"line":0,"col":2});
            serde_json::from_value(obs).unwrap()
        })
        .collect();
    let got = sidecar::build(output, &Checked(FsSource::new(&root).unwrap()), "demo").unwrap();
    assert_eq!(
        gosanja_core::validate::validate(&serialise(&got), None).unwrap(),
        Vec::<gosanja_core::validate::Diagnostic>::new()
    );
    let files: Vec<_> = got.files.iter().filter(|f| f.path != "x").collect();
    assert_eq!(files.len(), 1);
    let file = files[0];
    assert_eq!(file.path, "generated");
    assert_eq!(file.language, None);
    assert_eq!((file.bytes, file.lines), (6, 1));
    assert_eq!(
        file.sha256,
        "54a195bd5f68cdb94a24b902774a69804fbce0c7684c4e90ba9631a022297c1b"
    );
    let evidence = got
        .relations
        .iter()
        .find(|r| r.rel == Rel::Call)
        .unwrap()
        .evidence
        .as_ref()
        .unwrap();
    assert_eq!(evidence.len(), 2);
    for e in evidence {
        assert_eq!(e.file.as_deref(), Some("generated"));
        assert_eq!(e.col, Some(4));
    }
    for path in &paths[2..] {
        let gap = got
            .gaps
            .iter()
            .find(|g| g.file.as_deref() == Some(*path) && g.gap == GapKind::PositionOutOfRange)
            .unwrap();
        assert_eq!(gap.count, Some(1));
    }
}

#[test]
fn discarding_duplicate_syntax_rows_removes_only_that_layers_skipped_count() {
    use gosanja_schema::snapshot::GapRow;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("x"), SOURCE).unwrap();
    let mut output = fixture(&root, tmp.path(), "good");
    output.syntax = vec![serde_json::from_value(fact()).unwrap(); 2];
    for (layer, src) in [
        (Layer::Syntax, "adapter:fake/syntax"),
        (Layer::Calls, "adapter:fake/calls"),
    ] {
        let mut gap = GapRow::new(GapKind::UnknownValue, src);
        gap.count = Some(2);
        gap.layer = Some(layer);
        output.gaps.push(gap);
    }
    let mut got = sidecar::build(output, &FsSource::new(&root).unwrap(), "demo").unwrap();
    assert_discarded_syntax(&got);
    assert_eq!(
        got.gaps
            .iter()
            .filter(|g| g.gap == GapKind::UnknownValue)
            .map(|g| g.layer)
            .collect::<Vec<_>>(),
        [Some(Layer::Calls)]
    );
    // In-memory consumers can supply a position converter directly.
    sidecar::derive_calls(
        &mut got,
        &[serde_json::from_value(observation()).unwrap()],
        "fake",
        &|_, _, col| Some(col),
    );
    assert_eq!(got.relations.len(), 1);
    assert_eq!(got.relations[0].rel, Rel::Call);
}

#[test]
fn outline_extent_uses_the_definition_body_after_position_conversion() {
    use gosanja_schema::snapshot::{FailedDetail, LayerState};
    let root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/go-basic"
    ));
    let temp = tempfile::tempdir().unwrap();
    let build = |rows: Vec<Value>| {
        let mut output = fixture(root, temp.path(), "good");
        output.index = fs::File::open(root.join("index.scip")).unwrap();
        output.syntax = rows
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        sidecar::build(output, &FsSource::new(root).unwrap(), "go-basic").unwrap()
    };
    let main = json!({"anchor":{"file":"main.go","line":6,"name":"main"},"body":[6,0,8,1],"outline":[{"k":"opaque","range":[2,0,2,3]}]});
    let greet = json!({"anchor":{"file":"main.go","line":2,"name":"greet"},"body":[2,0,4,1]});
    let missing = json!({"anchor":{"file":"main.go","line":0,"name":"missing"}});
    let mut inside = main.clone();
    inside["outline"][0]["range"] = json!([7, 1, 7, 2]);
    let valid = build(vec![inside]);
    assert_eq!(valid.scope.layers.syntax, LayerState::Present);
    assert_eq!(valid.syntax.len(), 1);
    assert!(
        valid
            .gaps
            .iter()
            .any(|g| g.gap == GapKind::OpaqueRegion && g.range == Some(Range::new(7, 1, 7, 2)))
    );

    let first = build(vec![missing.clone(), greet.clone(), main.clone()]);
    let last = build(vec![main, greet, missing]);
    assert_eq!(first.scope.layers.syntax, LayerState::Failed);
    assert_eq!(serialise(&first), serialise(&last));
    assert_eq!(first.syntax.len(), 0);
    let base = build(vec![]);
    assert_eq!(first.entities, base.entities);
    assert_eq!(first.relations, base.relations);
    let gaps: Vec<_> = first
        .gaps
        .iter()
        .filter(|g| g.src == "adapter:fake/syntax")
        .collect();
    assert_eq!(gaps.len(), 1);
    assert_eq!(
        (gaps[0].gap, gaps[0].detail),
        (GapKind::FailedLayer, Some(FailedDetail::RowInvalid))
    );
}

#[test]
fn wider_adapter_body_cannot_attach_greets_opaque_region_to_main() {
    use gosanja_schema::snapshot::{FailedDetail, LayerState};
    let root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/go-basic"
    ));
    let temp = tempfile::tempdir().unwrap();
    let mut output = fixture(root, temp.path(), "good");
    output.index = fs::File::open(root.join("index.scip")).unwrap();
    output.syntax = vec![
        serde_json::from_value(json!({
            "anchor":{"file":"main.go","line":6,"name":"main"},
            "body":[0,0,8,1],
            "outline":[{"k":"opaque","range":[2,0,2,3]}]
        }))
        .unwrap(),
    ];
    let got = sidecar::build(output, &FsSource::new(root).unwrap(), "go-basic").unwrap();
    assert!(got.entities.iter().any(|e| {
        e.name == "main"
            && e.def
                .as_ref()
                .is_some_and(|d| d.body == Some(Range::new(6, 0, 8, 1)))
    }));
    assert_eq!(got.scope.layers.syntax, LayerState::Failed);
    assert_eq!(got.syntax.len(), 0);
    let gaps: Vec<_> = got
        .gaps
        .iter()
        .filter(|g| g.src == "adapter:fake/syntax")
        .collect();
    assert_eq!(gaps.len(), 1);
    assert_eq!(
        (gaps[0].gap, gaps[0].detail),
        (GapKind::FailedLayer, Some(FailedDetail::RowInvalid))
    );
}

#[test]
fn outline_extent_uses_the_row_body_as_fallback_in_utf8_bytes() {
    let mut snapshot = run(&[], &[]);
    let row = json!({"anchor":owner(),"outline":[{"k":"opaque","range":[1,2,1,5]}]});
    let lines = gosanja_core::graph::LineIndex::new(SOURCE.as_bytes());
    let column = |_: &str, line, col| lines.byte_column(line, col, PositionEncoding::Utf16);
    let attach = |entities: &mut [gosanja_schema::snapshot::EntityRow], row: Value| {
        let mut gaps = Vec::new();
        let rows = sidecar::apply_syntax(
            entities,
            &[serde_json::from_value(row).unwrap()],
            "fake",
            &column,
            &mut gaps,
        );
        (rows, gaps)
    };
    // With no extent from either producer, the converted node is kept.
    let (rows, gaps) = attach(&mut snapshot.entities, row.clone());
    assert_eq!(gaps.len(), 0);
    assert_eq!(
        rows[0].outline.as_ref().unwrap()[0].range,
        Range::new(1, 4, 1, 7)
    );
    let def = snapshot
        .entities
        .iter_mut()
        .find(|e| e.name == "owner")
        .unwrap()
        .def
        .as_mut()
        .unwrap();
    def.body = Some(Range::new(0, 0, 1, 6));
    let (rows, gaps) = attach(&mut snapshot.entities, row.clone());
    assert_eq!(rows.len(), 0);
    assert_eq!(gaps[0].gap, GapKind::FailedLayer);
    let mut own = row;
    own["body"] = json!([0, 0, 1, 5]);
    let mut row_only = run(&[], &[]);
    let (rows, gaps) = attach(&mut row_only.entities, own.clone());
    assert_eq!(rows.len(), 1);
    assert_eq!(gaps.len(), 0);
    let (rows, gaps) = attach(&mut snapshot.entities, own);
    assert_eq!(rows.len(), 0);
    assert_eq!(gaps[0].gap, GapKind::FailedLayer);
    assert_eq!(
        snapshot
            .entities
            .iter()
            .find(|e| e.name == "owner")
            .unwrap()
            .def
            .as_ref()
            .unwrap()
            .body,
        Some(Range::new(0, 0, 1, 6))
    );
}
