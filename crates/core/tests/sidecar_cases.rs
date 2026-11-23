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
    bad["outline"][0]["children"][1]["callee"] = json!([1, 1, 1, 2]);
    let got = run(
        &[bad, json!({"anchor":{"file":"x","line":0,"name":"absent"}})],
        &[row, unknown, unresolved, bad_site],
    );
    assert_eq!(got.relations.len(), 1);
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
