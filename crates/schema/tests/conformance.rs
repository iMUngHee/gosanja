//! The Rust types and the JSON Schemas under `spec/schema` must describe the same shapes.
//!
//! Every row a type can serialise has to pass its schema, the snapshots kept in the repository
//! have to pass too, and a few rows the specification forbids have to fail.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gosanja_schema::compat::{self, Version};
use gosanja_schema::contract::{
    AdapterManifest, AdapterResult, Anchor, CONTRACT_VERSION, CallEvidence, CallSite, Diagnostic,
    LayerName, LayerResult, LayerStatus, Requirement, SkippedFile, SyntaxFact,
};
use gosanja_schema::position::{PositionEncoding, Range, Site};
use gosanja_schema::snapshot::{
    AdapterInfo, BuildContext, Counts, Def, EntityRow, Evidence, ExternalRow, FORMAT, FailedDetail,
    FileRow, GapKind, GapRow, Layer, LayerState, Layers, Manifest, Mechanism, OutlineKind,
    OutlineNode, Package, Project, RefRole, Rel, RelationRow, Resolution, Role, SCHEMA_VERSION,
    Scope, SyntaxRow, Tool,
};
use jsonschema::{Registry, Validator};
use serde_json::{Value, json};

const BASE: &str = "https://gosanja.invalid/schema/";

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Schemas {
    registry: Registry<'static>,
}

impl Schemas {
    fn load() -> Self {
        let dir = repository().join("spec/schema");
        let mut registry = Registry::new();
        let mut count = 0;
        for entry in fs::read_dir(&dir).expect("spec/schema exists") {
            let path = entry.expect("readable directory entry").path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("schema file names are UTF-8")
                .to_owned();
            let text = fs::read_to_string(&path).expect("schema file is readable");
            let schema: Value = serde_json::from_str(&text).expect("schema file is JSON");
            registry = registry
                .add(format!("{BASE}{name}"), schema)
                .expect("schema registers");
            count += 1;
        }
        assert_eq!(count, 18, "the specification names eighteen schemas");
        Self {
            registry: registry
                .prepare()
                .expect("every reference between schemas resolves"),
        }
    }

    fn validator(&self, name: &str) -> Validator {
        jsonschema::options()
            .with_registry(&self.registry)
            .build(&json!({"$ref": format!("{BASE}{name}.schema.json")}))
            .expect("schema compiles")
    }

    fn assert_valid(&self, name: &str, value: &Value) {
        let validator = self.validator(name);
        let errors: Vec<String> = validator
            .iter_errors(value)
            .map(|e| format!("{e} at {}", e.instance_path()))
            .collect();
        assert!(
            errors.is_empty(),
            "{name}: {value} should be valid, got {errors:?}"
        );
    }

    fn assert_invalid(&self, name: &str, value: &Value) {
        assert!(
            !self.validator(name).is_valid(value),
            "{name}: {value} should be invalid"
        );
    }
}

fn to_value<T: serde::Serialize>(row: &T) -> Value {
    serde_json::to_value(row).expect("row serialises")
}

fn anchor(col: Option<u32>, name: Option<&str>) -> Anchor {
    Anchor {
        file: "src/views.rs".into(),
        line: 2,
        col,
        name: name.map(str::to_owned),
    }
}

fn outline() -> Vec<OutlineNode> {
    vec![OutlineNode {
        k: OutlineKind::Branch,
        detail: Some("if".into()),
        range: Range::new(3, 4, 7, 5),
        callee: None,
        children: Some(vec![
            OutlineNode {
                k: OutlineKind::Call,
                detail: None,
                range: Range::new(4, 8, 4, 15),
                callee: Some(Range::new(4, 8, 4, 13)),
                children: None,
            },
            OutlineNode {
                k: OutlineKind::Opaque,
                detail: Some("macro".into()),
                range: Range::new(6, 8, 6, 30),
                callee: None,
                children: None,
            },
        ]),
    }]
}

fn build(id: &str) -> BuildContext {
    BuildContext {
        id: id.into(),
        toolchain: Some("rustc 1.99.1".into()),
        target: Some("x86_64-unknown-linux-gnu".into()),
        profile: Some("debug".into()),
        features: Some(vec!["default".into()]),
    }
}

fn entity(key: &str) -> EntityRow {
    EntityRow {
        key: key.into(),
        symbol: Some("rust-analyzer cargo demo 0.1.0 views/render().".into()),
        kind: Some("rust:function".into()),
        role: Some(Role::Callable),
        name: "render".into(),
        parent: Some("cargo:demo:views/".into()),
        r#virtual: false,
        group: false,
        member_of: None,
        def: Some(Def {
            file: "src/views.rs".into(),
            name: Range::new(2, 7, 2, 13),
            body: Some(Range::new(2, 0, 8, 1)),
            body_src: Some("scip:rust-analyzer".into()),
        }),
        decls: None,
        src: "scip:rust-analyzer".into(),
    }
}

fn relation(rel: Rel) -> RelationRow {
    RelationRow {
        from: "cargo:demo:views/render().".into(),
        to: "cargo:demo:views/empty().".into(),
        rel,
        roles: None,
        mechanism: None,
        resolution: None,
        sites: None,
        evidence: None,
        src: "core:reference-in-body".into(),
    }
}

#[test]
fn manifest_file_and_entity_rows_pass_their_schemas() {
    let schemas = Schemas::load();

    let manifest = Manifest {
        format: FORMAT.into(),
        schema: "0.3".into(),
        tool: Tool {
            name: "gosanja".into(),
            version: "0.1.0".into(),
        },
        project: Project {
            name: "demo".into(),
        },
        scopes: vec![Scope {
            path: ".".into(),
            adapter: Some(AdapterInfo {
                name: "fake".into(),
                version: "0.0.1".into(),
                contract: "0.3".into(),
            }),
            indexers: vec![Tool {
                name: "rust-analyzer".into(),
                version: "1.99.1".into(),
            }],
            layers: Layers {
                base: LayerState::Present,
                syntax: LayerState::Failed,
                calls: LayerState::Absent,
            },
            builds: vec![build("debug-default")],
        }],
        counts: Counts {
            files: 2,
            entities: 9,
            externals: 3,
            relations: 7,
            syntax: 1,
            gaps: 2,
            dropped_local_references: 4,
        },
    };
    schemas.assert_valid("snapshot-manifest", &to_value(&manifest));

    let file = FileRow {
        path: "src/views.rs".into(),
        language: Some("rust".into()),
        bytes: 612,
        lines: 33,
        sha256: "0".repeat(64),
    };
    schemas.assert_valid("file", &to_value(&file));

    schemas.assert_valid("entity", &to_value(&entity("cargo:demo:views/render().")));
    let container = EntityRow {
        key: "cargo:demo:views/".into(),
        symbol: None,
        kind: None,
        role: Some(Role::Container),
        name: "views".into(),
        parent: None,
        r#virtual: true,
        group: false,
        member_of: None,
        def: None,
        decls: None,
        src: "core:containment".into(),
    };
    schemas.assert_valid("entity", &to_value(&container));
    let group = EntityRow {
        r#virtual: false,
        group: true,
        role: None,
        key: "cargo:demo:a/f().".into(),
        symbol: Some("rust-analyzer cargo demo 0.1.0 a/f().".into()),
        src: "core:identity".into(),
        ..container.clone()
    };
    schemas.assert_valid("entity", &to_value(&group));
    let member = EntityRow {
        key: "cargo:demo:a/f().~1".into(),
        member_of: Some("cargo:demo:a/f().".into()),
        ..entity("unused")
    };
    schemas.assert_valid("entity", &to_value(&member));
}

#[test]
fn external_rows_pass_their_schema() {
    let schemas = Schemas::load();

    let external = ExternalRow {
        key: "cargo:alloc:string/String#".into(),
        symbol: "rust-analyzer cargo alloc 1.99.1 string/String#".into(),
        package: Package {
            manager: "cargo".into(),
            name: "alloc".into(),
            version: "1.99.1".into(),
        },
        src: "scip:rust-analyzer".into(),
    };
    schemas.assert_valid("external", &to_value(&external));
}

#[test]
fn relation_rows_pass_their_schema() {
    let schemas = Schemas::load();

    let reference = RelationRow {
        roles: Some(vec![RefRole::Read]),
        sites: Some(vec![Site::new("src/views.rs", Range::new(4, 8, 4, 13))]),
        ..relation(Rel::Ref)
    };
    schemas.assert_valid("relation", &to_value(&reference));
    let declared = RelationRow {
        mechanism: Some(Mechanism::Unknown),
        resolution: Some(Resolution::Declared),
        evidence: Some(vec![Evidence {
            file: Some("src/views.rs".into()),
            line: Some(4),
            col: Some(8),
            end: Some([4, 13]),
            build: None,
        }]),
        src: "adapter:fake/syntax".into(),
        ..relation(Rel::Call)
    };
    schemas.assert_valid("relation", &to_value(&declared));
    let dropped = RelationRow {
        mechanism: Some(Mechanism::Drop),
        resolution: Some(Resolution::Direct),
        evidence: Some(vec![Evidence {
            file: None,
            line: None,
            col: None,
            end: None,
            build: Some("debug-default".into()),
        }]),
        src: "adapter:fake/calls".into(),
        ..relation(Rel::Call)
    };
    schemas.assert_valid("relation", &to_value(&dropped));
    let implements = RelationRow {
        sites: Some(vec![Site::new("src/views.rs", Range::new(29, 7, 29, 10))]),
        src: "scip:scip-go".into(),
        ..relation(Rel::Implements)
    };
    schemas.assert_valid("relation", &to_value(&implements));
}

#[test]
fn syntax_rows_pass_their_schema() {
    let schemas = Schemas::load();

    let syntax = SyntaxRow {
        key: "cargo:demo:views/render().".into(),
        visibility: Some("pub".into()),
        qualifiers: Some(vec!["async".into()]),
        attributes: Some(vec!["inline".into()]),
        outline: Some(outline()),
        src: "adapter:fake/syntax".into(),
    };
    schemas.assert_valid("syntax", &to_value(&syntax));
}

#[test]
fn build_rows_pass_their_schema() {
    let schemas = Schemas::load();
    let target = json!({"row": "target", "key": "cargo:demo:lib:demo", "kind": "lib", "name": "demo",
        "root": {"symbol": "rust-analyzer cargo demo 0.1.0 crate/", "anchor": {"file": "src/lib.rs", "line": 0, "col": 0}},
        "required_features": ["x"], "test": true});
    schemas.assert_valid("build", &target);
    schemas.assert_valid("build", &json!({"row": "target", "key": "gomod:m:m/p:package", "kind": "package", "name": "p", "root": {"symbol": "scip-go gomod m v1 `m/p`/"}}));
    schemas.assert_valid("build", &json!({"row": "file", "file": "src/lib.rs", "targets": ["cargo:demo:lib:demo"], "test": "target", "constraint": "//go:build linux"}));
    for only in [json!({"targets": ["k"]}), json!({"test": "cfg"}), json!({"constraint": ""})] {
        let mut row = json!({"row": "file", "file": "a.go"});
        row.as_object_mut().unwrap().extend(only.as_object().unwrap().clone());
        schemas.assert_valid("build", &row);
    }
    for bad in [
        json!({"row": "file", "file": "a.go"}),
        json!({"row": "file", "file": "a.go", "targets": []}),
        json!({"row": "file", "file": "a.go", "targets": ["k", "k"]}),
        json!({"row": "file", "file": "a.go", "test": "attribute"}),
        json!({"row": "file", "file": "/abs/a.go", "test": "file"}),
        json!({"row": "target", "kind": "lib", "name": "demo"}),
        json!({"row": "target", "key": "has space", "kind": "lib", "name": "demo"}),
        json!({"row": "target", "key": "bad%zz", "kind": "lib", "name": "demo"}),
        json!({"row": "target", "key": "k", "kind": "Lib", "name": "demo"}),
        json!({"row": "target", "key": "k", "kind": "lib", "name": "demo", "test": false}),
        json!({"row": "target", "key": "k", "kind": "lib", "name": "demo", "root": {"anchor": {"file": "a", "line": 0, "col": 0}}}),
        json!({"row": "other", "key": "k"}),
        json!({"key": "k", "kind": "lib", "name": "demo"}),
    ] {
        schemas.assert_invalid("build", &bad);
    }
    let long = "k".repeat(1025);
    schemas.assert_invalid("build", &json!({"row": "target", "key": long, "kind": "lib", "name": "demo"}));
    schemas.assert_valid("build", &json!({"row": "target", "key": "k".repeat(1024), "kind": "lib", "name": "demo"}));
}

#[test]
fn test_rows_pass_their_schema() {
    let schemas = Schemas::load();
    for test in ["attribute", "cfg"] {
        schemas.assert_valid("syntax-fact", &json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 3, "col": 7}, "test": test}));
    }
    schemas.assert_valid("syntax-fact", &json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 3, "name": "t"}, "test": "cfg"}));
    for bad in [
        json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 3, "col": 7}}),
        json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 3, "col": 7}, "test": "target"}),
        json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 3, "col": 7}, "test": "cfg", "attributes": ["test"]}),
        json!({"row": "syntax", "anchor": {"file": "src/lib.rs", "line": 3, "col": 7}, "test": "cfg"}),
        json!({"row": "test", "test": "cfg"}),
        json!({"anchor": {"file": "src/lib.rs", "line": 3, "col": 7}, "test": "cfg"}),
    ] {
        schemas.assert_invalid("syntax-fact", &bad);
    }
    schemas.assert_valid("syntax-fact", &json!({"anchor": {"file": "src/lib.rs", "line": 3, "col": 7}, "attributes": ["test"]}));
}

#[test]
fn target_rows_pass_their_schema() {
    let schemas = Schemas::load();
    let full = json!({"scope": ".", "key": "cargo:demo:lib:demo", "kind": "lib", "name": "demo",
        "root": {"symbol": "rust-analyzer cargo demo 0.1.0 crate/", "anchor": {"file": "src/lib.rs", "line": 0, "col": 0}},
        "entity": "cargo:demo:crate/~1", "required_features": ["x"], "test": true, "src": "adapter:rust/build"});
    schemas.assert_valid("target", &full);
    schemas.assert_valid("target", &json!({"scope": "pkg", "key": "gomod:m:m/p:test", "kind": "test", "name": "p", "src": "adapter:go/build"}));
    for (field, value) in [
        ("scope", json!("/abs")),
        ("key", json!("a b")),
        ("entity", json!("not a key")),
        ("src", json!("adapter:rust/syntax")),
        ("test", json!(false)),
    ] {
        let mut row = full.clone();
        row[field] = value;
        schemas.assert_invalid("target", &row);
    }
    for missing in ["scope", "key", "kind", "name", "src"] {
        let mut row = full.clone();
        row.as_object_mut().unwrap().remove(missing);
        schemas.assert_invalid("target", &row);
    }
}

#[test]
fn file_build_fields_pass_their_schema() {
    let schemas = Schemas::load();
    let plain = json!({"path": "src/lib.rs", "language": "rust", "bytes": 3, "lines": 1, "sha256": "0".repeat(64)});
    schemas.assert_valid("file", &plain);
    for (field, value) in [
        ("targets", json!(["cargo:demo:lib:demo", "cargo:demo:test:it"])),
        ("test", json!("cfg")),
        ("test", json!("target")),
        ("test", json!("file")),
        ("constraint", json!("//go:build linux && amd64")),
    ] {
        let mut row = plain.clone();
        row[field] = value;
        schemas.assert_valid("file", &row);
    }
    for (field, value) in [
        ("targets", json!([])),
        ("targets", json!(["k", "k"])),
        ("targets", json!(["a b"])),
        ("test", json!("attribute")),
        ("test", json!(true)),
        ("constraint", json!(1)),
    ] {
        let mut row = plain.clone();
        row[field] = value;
        schemas.assert_invalid("file", &row);
    }
    schemas.assert_valid("entity", &json!({"key": "cargo:demo:t().", "symbol": "s", "role": "callable", "name": "t",
        "def": {"file": "src/lib.rs", "name": [0, 3, 0, 4]}, "test": "attribute", "src": "scip:rust-analyzer"}));
    schemas.assert_invalid("entity", &json!({"key": "cargo:demo:t().", "symbol": "s", "role": "callable", "name": "t",
        "def": {"file": "src/lib.rs", "name": [0, 3, 0, 4]}, "test": "yes", "src": "scip:rust-analyzer"}));
}

#[test]
fn unresolved_root_gap_passes_its_schema() {
    let schemas = Schemas::load();
    let gap = json!({"gap": "unresolved-root", "layer": "build", "target": "cargo:demo:bin:demo",
        "anchor": {"file": "src/main.rs", "line": 0, "col": 0}, "src": "adapter:rust/build"});
    schemas.assert_valid("gap", &gap);
    schemas.assert_valid("gap", &json!({"gap": "unresolved-root", "layer": "build", "target": "k", "src": "adapter:rust/build"}));
    schemas.assert_valid("gap", &json!({"gap": "unknown-file", "layer": "build", "file": "gen/x.rs", "src": "adapter:rust/build"}));
    schemas.assert_valid("gap", &json!({"gap": "skipped-file", "layer": "build", "file": "src/a.rs", "code": "target-unknown", "src": "adapter:rust/build"}));
    schemas.assert_valid("gap", &json!({"gap": "missing-layer", "layer": "build", "scope": ".", "src": "adapter:rust/build"}));
    schemas.assert_valid("gap", &json!({"gap": "failed-layer", "layer": "build", "scope": "pkg", "detail": "row-invalid", "src": "adapter:rust/build"}));
    for bad in [
        json!({"gap": "unresolved-root", "layer": "build", "src": "adapter:rust/build"}),
        json!({"gap": "unresolved-root", "target": "k", "src": "adapter:rust/build"}),
        json!({"gap": "unresolved-root", "layer": "syntax", "target": "k", "src": "adapter:rust/syntax"}),
        json!({"gap": "unresolved-root", "layer": "build", "target": "a b", "src": "adapter:rust/build"}),
        json!({"gap": "unresolved-root", "layer": "build", "target": "k", "file": "src/a.rs", "src": "adapter:rust/build"}),
        json!({"gap": "unresolved-root", "layer": "build", "target": "k", "src": "scip:rust-analyzer"}),
        json!({"gap": "unknown-file", "layer": "build", "src": "adapter:rust/build"}),
        json!({"gap": "unknown-file", "layer": "build", "file": "a", "target": "k", "src": "adapter:rust/build"}),
        json!({"gap": "missing-layer", "layer": "build", "src": "adapter:rust/build"}),
        json!({"gap": "failed-layer", "layer": "build", "detail": "row-invalid", "src": "adapter:rust/build"}),
        json!({"gap": "skipped-file", "layer": "build", "file": "src/a.rs", "code": "target-unknown", "src": "adapter:rust/syntax"}),
    ] {
        schemas.assert_invalid("gap", &bad);
    }
}

/// One valid row of every gap kind.
fn gap_of_every_kind() -> Vec<GapRow> {
    vec![
        GapRow {
            keys: Some(vec![
                "cargo:demo:a/f().".into(),
                "cargo:demo:a/f().~1".into(),
                "cargo:demo:a/f().~2".into(),
            ]),
            ..GapRow::new(GapKind::SymbolCollision, "core:identity")
        },
        GapRow {
            file: Some("src/x.rs".into()),
            ..GapRow::new(GapKind::UndecodableDocument, "scip:scip-go")
        },
        GapRow {
            count: Some(2),
            ..GapRow::new(GapKind::DocumentOutsideRoot, "scip:scip-go")
        },
        GapRow {
            file: Some("link.rs".into()),
            ..GapRow::new(GapKind::DocumentOutsideRoot, "scip:scip-go")
        },
        GapRow {
            file: Some("src/x.rs".into()),
            ..GapRow::new(GapKind::SourceUnavailable, "scip:scip-go")
        },
        GapRow {
            file: Some("src/x.rs".into()),
            count: Some(3),
            ..GapRow::new(GapKind::PositionOutOfRange, "scip:scip-go")
        },
        GapRow {
            file: Some("src/x.rs".into()),
            count: Some(5),
            ..GapRow::new(GapKind::UnattributedReferences, "core:reference-in-body")
        },
        GapRow {
            file: Some("src/x.rs".into()),
            keys: Some(vec!["cargo:demo:a/f().".into(), "cargo:demo:a/g().".into()]),
            ..GapRow::new(GapKind::OverlappingBodies, "core:reference-in-body")
        },
        GapRow {
            layer: Some(Layer::Calls),
            ..GapRow::new(GapKind::MissingLayer, "adapter:fake/calls")
        },
        GapRow {
            layer: Some(Layer::Syntax),
            code: Some("toolchain-missing".into()),
            detail: Some(FailedDetail::ReportedFailed),
            ..GapRow::new(GapKind::FailedLayer, "adapter:fake/syntax")
        },
        GapRow {
            layer: Some(Layer::Syntax),
            detail: Some(FailedDetail::NoStatus),
            ..GapRow::new(GapKind::FailedLayer, "adapter:fake/syntax")
        },
        GapRow {
            layer: Some(Layer::Syntax),
            anchor: Some(anchor(None, Some("g"))),
            ..GapRow::new(GapKind::Unanchored, "adapter:fake/syntax")
        },
        GapRow {
            file: Some("src/x.rs".into()),
            range: Some(Range::new(1, 2, 1, 5)),
            ..GapRow::new(GapKind::OpenCall, "adapter:fake/syntax")
        },
        GapRow {
            file: Some("src/x.rs".into()),
            range: Some(Range::new(1, 2, 3, 5)),
            keys: Some(vec!["cargo:demo:a/f().".into()]),
            ..GapRow::new(GapKind::OpaqueRegion, "adapter:fake/syntax")
        },
        GapRow {
            layer: Some(Layer::Calls),
            count: Some(1),
            ..GapRow::new(GapKind::UnknownBuildContext, "adapter:fake/calls")
        },
        GapRow {
            layer: Some(Layer::Calls),
            count: Some(1),
            ..GapRow::new(GapKind::UnknownValue, "adapter:fake/calls")
        },
        skipped_file("base"),
    ]
}

fn skipped_file(layer: &str) -> GapRow {
    serde_json::from_value(json!({"gap": "skipped-file", "file": "src/generated.rs", "layer": layer, "code": "parse-error", "src": format!("adapter:fake/{layer}")}))
        .expect("a skipped-file gap")
}

#[test]
fn every_gap_kind_passes_its_schema() {
    let schemas = Schemas::load();
    for gap in &gap_of_every_kind() {
        schemas.assert_valid("gap", &to_value(gap));
    }
}

#[test]
fn the_interfaces_are_at_their_current_versions() {
    assert_eq!(CONTRACT_VERSION.to_string(), "0.4");
    assert_eq!(SCHEMA_VERSION.to_string(), "0.4");
    assert_eq!(
        gosanja_schema::diff::SCHEMA_VERSION.to_string(),
        DIFF_SCHEMA
    );
    assert_eq!(to_value(&GapKind::SkippedFile), json!("skipped-file"));
    assert_eq!(to_value(&LayerState::NotRequested), json!("not-requested"));
}

#[test]
fn only_optional_layers_may_be_not_requested() {
    let schemas = Schemas::load();
    let layers = |base: &str, syntax: &str, calls: &str| json!({"base": base, "syntax": syntax, "calls": calls});
    let snapshot = |layers: Value| {
        json!({
            "format": FORMAT,
            "schema": SCHEMA_VERSION.to_string(),
            "tool": {"name": "gosanja", "version": "0.2.0"},
            "project": {"name": "demo"},
            "scopes": [{"path": ".", "indexers": [], "layers": layers, "builds": []}],
            "counts": {"files": 0, "entities": 0, "externals": 0, "relations": 0,
                       "syntax": 0, "gaps": 0, "dropped_local_references": 0}
        })
    };
    let side = |layers: Value| {
        json!({
            "schema": SCHEMA_VERSION.to_string(),
            "project": {"name": "demo"},
            "layers": layers,
            "builds": [],
            "counts": {"files": 0, "entities": 0, "externals": 0, "relations": 0,
                       "syntax": 0, "gaps": 0, "dropped_local_references": 0},
            "gaps": {}
        })
    };
    let diff = |old: Value| {
        let present = layers("present", "present", "present");
        json!({
            "format": DIFF_FORMAT,
            "schema": DIFF_SCHEMA,
            "tool": {"name": "gosanja", "version": "0.2.0"},
            "weights": {"name": 30, "file": 18, "parents": 18, "size": 12,
                        "outgoing": 13, "incoming": 9, "threshold": 500_000},
            "old": side(old),
            "new": side(present),
            "counts": {"entities": {"added": 0, "removed": 0, "changed": 0, "rekeyed": 0},
                       "externals": {"added": 0, "removed": 0, "rekeyed": 0},
                       "relations": {"added": 0, "removed": 0},
                       "candidates": 0},
            "packages_added": [],
            "packages_removed": [],
            "not_compared": []
        })
    };
    let accepted = [
        layers("present", "not-requested", "present"),
        layers("present", "present", "not-requested"),
        layers("present", "not-requested", "not-requested"),
        layers("failed", "not-requested", "absent"),
    ];
    let refused = [
        layers("not-requested", "present", "present"),
        layers("present", "skipped", "present"),
        layers("present", "present", "requested"),
    ];
    for l in accepted {
        schemas.assert_valid("snapshot-manifest", &snapshot(l.clone()));
        schemas.assert_valid("diff-manifest", &diff(l));
    }
    for l in refused {
        schemas.assert_invalid("snapshot-manifest", &snapshot(l.clone()));
        schemas.assert_invalid("diff-manifest", &diff(l));
    }
}

#[test]
fn every_adapter_file_type_passes_its_schema() {
    let schemas = Schemas::load();

    let manifest = AdapterManifest {
        contract: "0.3".into(),
        name: "fake".into(),
        version: "0.0.1".into(),
        languages: vec!["rust".into()],
        position_encoding: PositionEncoding::Utf16,
        layers: vec![LayerName::Base, LayerName::Syntax, LayerName::Calls],
        requires: Some(vec![Requirement {
            tool: "rust-analyzer".into(),
            version: None,
        }]),
        markers: Some(vec!["Cargo.toml".into()]),
    };
    schemas.assert_valid("adapter-manifest", &to_value(&manifest));

    let result = AdapterResult {
        contract: "0.3".into(),
        layers: BTreeMap::from([
            (
                "base".to_owned(),
                LayerResult {
                    status: LayerStatus::Ok,
                    diagnostics: None,
                    skipped: Some(vec![SkippedFile {
                        file: "src/generated.rs".into(),
                        code: "parse-error".into(),
                    }]),
                    merged: None,
                },
            ),
            (
                "syntax".to_owned(),
                LayerResult {
                    status: LayerStatus::Failed,
                    diagnostics: Some(vec![Diagnostic {
                        code: "parse-error".into(),
                        message: Some("src/a.rs: pid 4242 after 31 ms".into()),
                    }]),
                    skipped: None,
                    merged: None,
                },
            ),
        ]),
        builds: Some(vec![build("debug-default")]),
        tools: Some(vec![Tool {
            name: "rust-analyzer".into(),
            version: "1.99.1".into(),
        }]),
    };
    schemas.assert_valid("adapter-result", &to_value(&result));
    schemas.assert_valid("adapter-result", &json!({"contract": "0.3", "layers": {}}));

    let fact = SyntaxFact {
        anchor: anchor(Some(7), None),
        body: Some(Range::new(2, 0, 8, 1)),
        visibility: Some("pub".into()),
        qualifiers: Some(vec![]),
        attributes: Some(vec!["derive:Debug".into()]),
        outline: Some(outline()),
    };
    schemas.assert_valid("syntax-fact", &to_value(&fact));
    schemas.assert_valid(
        "syntax-fact",
        &to_value(&SyntaxFact {
            anchor: anchor(None, Some("render")),
            body: None,
            visibility: None,
            qualifiers: None,
            attributes: None,
            outline: None,
        }),
    );

    let evidence = CallEvidence {
        caller: anchor(Some(7), None),
        callee: anchor(None, Some("drop")),
        mechanism: Mechanism::Drop,
        resolution: Resolution::Direct,
        site: Some(CallSite {
            file: "src/views.rs".into(),
            line: 8,
            col: None,
        }),
        build: "debug-default".into(),
    };
    schemas.assert_valid("call-evidence", &to_value(&evidence));
    schemas.assert_valid(
        "call-evidence",
        &to_value(&CallEvidence {
            site: None,
            ..evidence
        }),
    );
}

#[test]
fn unknown_fields_pass_and_forbidden_shapes_fail() {
    let schemas = Schemas::load();

    let mut with_extra = to_value(&entity("cargo:demo:views/render()."));
    with_extra["added_by_a_later_minor"] = json!(1);
    schemas.assert_valid("entity", &with_extra);

    // An anchor needs a column or a name.
    schemas.assert_invalid(
        "syntax-fact",
        &json!({"anchor": {"file": "src/a.rs", "line": 3}}),
    );
    // A call carries mechanism, resolution and evidence, and no sites.
    let call = json!({"from": "cargo:d:a().", "to": "cargo:d:b().", "rel": "call", "resolution": "direct", "evidence": [{"build": "b"}], "src": "adapter:x/calls"});
    schemas.assert_invalid("relation", &call);
    let reference_with_evidence = json!({"from": "cargo:d:a().", "to": "cargo:d:b().", "rel": "ref", "roles": [], "sites": [["a.rs", 0, 0, 0, 1]], "evidence": [{}], "src": "core:reference-in-body"});
    schemas.assert_invalid("relation", &reference_with_evidence);
    // The syntax layer may only say `declared`; the calls layer may never say it.
    let syntax_direct = json!({"from": "cargo:d:a().", "to": "cargo:d:b().", "rel": "call", "mechanism": "unknown", "resolution": "direct", "evidence": [{}], "src": "adapter:x/syntax"});
    schemas.assert_invalid("relation", &syntax_direct);
    schemas.assert_invalid(
        "call-evidence",
        &json!({"caller": {"file": "a.rs", "line": 1, "col": 0}, "callee": {"file": "a.rs", "line": 5, "col": 0}, "mechanism": "static", "resolution": "declared", "build": "b"}),
    );
    // The syntax layer claims neither a target nor a dispatch.
    let syntax_dynamic = json!({"from": "cargo:d:a().", "to": "cargo:d:b().", "rel": "call", "mechanism": "dynamic", "resolution": "declared", "evidence": [{}], "src": "adapter:x/syntax"});
    schemas.assert_invalid("relation", &syntax_dynamic);
    // A row is a definition, a member, a group or a virtual container, and nothing in between.
    schemas.assert_invalid(
        "entity",
        &json!({"key": "cargo:d:c.", "name": "c", "role": "value", "src": "scip:x"}),
    );
    schemas.assert_invalid(
        "entity",
        &json!({"key": "cargo:d:f().", "name": "f", "group": true, "src": "core:identity"}),
    );
    // No path can enter a snapshot through a build context.
    schemas.assert_invalid("adapter-result", &json!({"contract": "0.3", "layers": {}, "builds": [{"id": "release", "toolchain": "/home/dev/rustc"}]}));
    schemas.assert_valid("adapter-result", &json!({"contract": "0.3", "layers": {}, "builds": [{"id": "release", "cwd": "ignored, not copied"}]}));
    // A qualified feature name is not a path; an absolute path is refused in any spelling.
    schemas.assert_valid("adapter-result", &json!({"contract": "0.3", "layers": {}, "builds": [{"id": "release", "features": ["ascii/std", "derive"]}]}));
    for absolute in [
        "/abs/feature",
        "~/feature",
        "C:\\feature",
        "\\\\host\\share",
    ] {
        schemas.assert_invalid("adapter-result", &json!({"contract": "0.3", "layers": {}, "builds": [{"id": "release", "features": [absolute]}]}));
    }
    schemas.assert_invalid(
        "adapter-result",
        &json!({"contract": "0.3", "layers": {}, "builds": [{"id": "a/b"}]}),
    );
    // A group row carries no kind.
    schemas.assert_invalid("entity", &json!({"key": "cargo:d:f().", "symbol": "s cargo d 1 f().", "name": "f", "group": true, "kind": "rust:function", "src": "core:identity"}));
    // A gap's provenance and fields follow its kind.
    schemas.assert_invalid("gap", &json!({"gap": "position-out-of-range", "file": "a.rs", "count": 1, "src": "adapter:x/syntax"}));
    schemas.assert_invalid("gap", &json!({"gap": "position-out-of-range", "file": "a.rs", "count": 1, "layer": "calls", "src": "adapter:x/syntax"}));
    schemas.assert_invalid("gap", &json!({"gap": "position-out-of-range", "file": "a.rs", "count": 1, "layer": "syntax", "src": "scip:x"}));
    schemas.assert_invalid("gap", &json!({"gap": "overlapping-bodies", "file": "a.rs", "keys": ["cargo:d:f()."], "src": "core:reference-in-body"}));
    schemas.assert_invalid("gap", &json!({"gap": "overlapping-bodies", "file": "a.rs", "keys": ["cargo:d:f().", "cargo:d:f()."], "src": "core:reference-in-body"}));
    schemas.assert_invalid("gap", &json!({"gap": "overlapping-bodies", "file": "a.rs", "keys": ["cargo:d:f().", "cargo:d:g()."], "src": "scip:x"}));
    schemas.assert_invalid("gap", &json!({"gap": "symbol-collision", "keys": ["cargo:d:f().", "cargo:d:f().~1", "cargo:d:f().~2"], "src": "scip:x"}));
    // Closed enumerations are closed.
    schemas.assert_invalid(
        "gap",
        &json!({"gap": "something-new", "src": "core:identity"}),
    );
    schemas.assert_invalid(
        "entity",
        &json!({"key": "cargo:d:k.", "role": "widget", "name": "n", "virtual": true, "src": "core:containment"}),
    );
    // Paths are relative and normalised; ranges have four coordinates.
    schemas.assert_invalid(
        "file",
        &json!({"path": "/abs/a.rs", "bytes": 1, "lines": 1, "sha256": "0".repeat(64)}),
    );
    schemas.assert_invalid(
        "file",
        &json!({"path": "a/../b.rs", "bytes": 1, "lines": 1, "sha256": "0".repeat(64)}),
    );
    schemas.assert_invalid(
        "syntax-fact",
        &json!({"anchor": {"file": "a.rs", "line": 1, "col": 0}, "body": [1, 2, 3]}),
    );
    // A diagnostic code is a short identifier, not a sentence.
    schemas.assert_invalid("adapter-result", &json!({"contract": "0.3", "layers": {"base": {"status": "failed", "diagnostics": [{"code": "Could not run"}]}}}));
    // A version is two integers.
    schemas.assert_invalid("adapter-manifest", &json!({"contract": "0.1.0", "name": "x", "version": "1", "languages": ["go"], "position_encoding": "utf-8", "layers": ["base"]}));
}

#[test]
fn markers_are_portable_file_names() {
    let schemas = Schemas::load();
    let manifest = json!({"contract": "0.3", "name": "x", "version": "1", "languages": ["rust"], "position_encoding": "utf-8", "layers": ["base"]});
    schemas.assert_valid("adapter-manifest", &manifest);
    let mut with = manifest.clone();
    with["markers"] = json!(["Cargo.toml", "go.mod", ".tool-versions", "a_b-c.1"]);
    schemas.assert_valid("adapter-manifest", &with);
    with["markers"] = json!([]);
    schemas.assert_valid("adapter-manifest", &with);
    for marker in [
        "", ".", "..", "a/b", "a\\b", "C:x", "a\u{0}b", "a b", "a\nb",
    ] {
        let mut bad = manifest.clone();
        bad["markers"] = json!([marker]);
        schemas.assert_invalid("adapter-manifest", &bad);
    }
}

#[test]
fn every_layer_may_list_skipped_files() {
    let schemas = Schemas::load();
    let entry = |skipped: Value| json!({"status": "ok", "skipped": skipped});
    for layer in ["base", "syntax", "calls"] {
        let result = |skipped: Value| json!({"contract": "0.3", "layers": {layer: entry(skipped)}});
        schemas.assert_valid("adapter-result", &result(json!([])));
        schemas.assert_valid(
            "adapter-result",
            &result(json!([{"file": "src/a.rs", "code": "parse-error"}, {"file": "b.rs", "code": "too-large", "later": 1}])),
        );
        for bad in [
            json!({"file": "/abs/a.rs", "code": "parse-error"}),
            json!({"file": "C:/a.rs", "code": "parse-error"}),
            json!({"file": "src/../a.rs", "code": "parse-error"}),
            json!({"file": "src/a.rs", "code": "Parse error"}),
            json!({"file": "src/a.rs"}),
            json!({"code": "parse-error"}),
        ] {
            schemas.assert_invalid("adapter-result", &result(json!([bad])));
        }
        // A failed layer's entry is checked too.
        schemas.assert_invalid(
            "adapter-result",
            &json!({"contract": "0.3", "layers": {layer: {"status": "failed", "skipped": [{"file": "/a.rs", "code": "x"}]}}}),
        );
    }
}

#[test]
fn a_skipped_file_gap_has_exactly_its_fields() {
    let schemas = Schemas::load();
    for layer in ["base", "syntax", "calls"] {
        let gap = to_value(&skipped_file(layer));
        schemas.assert_valid("gap", &gap);
        let mut extra = gap.clone();
        extra["added_by_a_later_minor"] = json!(true);
        schemas.assert_valid("gap", &extra);
        for (field, value) in [
            ("range", json!([0, 0, 0, 1])),
            ("count", json!(1)),
            ("keys", json!(["cargo:demo:a/f()."])),
            ("anchor", json!({"file": "a.rs", "line": 0, "col": 0})),
            ("detail", json!("reported-failed")),
        ] {
            let mut forbidden = gap.clone();
            forbidden[field] = value;
            schemas.assert_invalid("gap", &forbidden);
        }
        for field in ["file", "layer", "code"] {
            let mut missing = gap.clone();
            missing.as_object_mut().expect("an object").remove(field);
            schemas.assert_invalid("gap", &missing);
        }
        for other in ["base", "syntax", "calls"]
            .into_iter()
            .filter(|o| *o != layer)
        {
            let mut disagreeing = gap.clone();
            disagreeing["layer"] = json!(other);
            schemas.assert_invalid("gap", &disagreeing);
        }
    }
}

#[test]
fn only_a_skipped_file_gap_names_the_base_layer() {
    let schemas = Schemas::load();
    for gap in gap_of_every_kind() {
        let mut row = to_value(&gap);
        let kind = row["gap"].clone();
        row["src"] = json!("adapter:x/base");
        if row.get("layer").is_some() {
            row["layer"] = json!("base");
        }
        if kind == json!("skipped-file") {
            schemas.assert_valid("gap", &row);
        } else {
            schemas.assert_invalid("gap", &row);
        }
    }
    let mut relation = to_value(&relation(Rel::Ref));
    relation["src"] = json!("adapter:x/base");
    schemas.assert_invalid("relation", &relation);
    schemas.assert_invalid(
        "syntax",
        &json!({"key": "cargo:demo:a/f().", "src": "adapter:x/base"}),
    );
    schemas.assert_invalid(
        "entity",
        &json!({"key": "cargo:d:k.", "role": "module", "name": "k", "virtual": true, "src": "adapter:x/base"}),
    );
}

#[test]
fn only_a_call_node_names_a_callee_and_it_may_omit_it() {
    let schemas = Schemas::load();
    let row = |node: Value| json!({"key": "cargo:demo:a/f().", "outline": [node], "src": "adapter:x/syntax"});
    let fact =
        |node: Value| json!({"anchor": {"file": "a.rs", "line": 0, "col": 3}, "outline": [node]});
    let call_without = json!({"k": "call", "range": [1, 4, 1, 12], "children": [{"k": "call", "range": [1, 4, 1, 10], "callee": [1, 4, 1, 8]}]});
    let branch_with = json!({"k": "branch", "range": [1, 4, 1, 12], "callee": [1, 4, 1, 8]});
    schemas.assert_valid("syntax", &row(call_without.clone()));
    schemas.assert_valid("syntax-fact", &fact(call_without));
    schemas.assert_invalid("syntax", &row(branch_with.clone()));
    schemas.assert_invalid("syntax-fact", &fact(branch_with));
}

/// The marker and the schema version of a diff, as `spec/diff.md` states them.
const DIFF_FORMAT: &str = "gosanja-diff";
const DIFF_SCHEMA: &str = "0.3";

#[test]
fn diff_schema_version_accepts_0_3_and_refuses_0_2() {
    use gosanja_schema::diff::SCHEMA_VERSION as DIFF_VERSION;
    let path = repository().join("spec/examples/diff-basic/manifest.json");
    let current: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(current["schema"], "0.3");
    assert_eq!(current["old"]["schema"], "0.4");
    assert_eq!(current["new"]["schema"], "0.4");
    let writer = |manifest: &Value| {
        manifest["schema"]
            .as_str()
            .unwrap()
            .parse::<Version>()
            .unwrap()
    };
    assert_eq!(
        compat::accept(DIFF_VERSION, writer(&current)),
        Ok(compat::Acceptance::Full)
    );
    let mut previous = current;
    previous["schema"] = json!("0.2");
    let writer = writer(&previous);
    assert_eq!(
        compat::accept(DIFF_VERSION, writer),
        Err(compat::Refusal::UnstableMinor {
            reader: DIFF_VERSION,
            writer
        })
    );
}

/// What a directory with a `manifest.json` holds, told by the manifest's `format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Output {
    Snapshot,
    Diff,
}

impl Output {
    fn of(manifest: &Value, directory: &Path) -> Self {
        match manifest["format"].as_str() {
            Some(FORMAT) => Self::Snapshot,
            Some(DIFF_FORMAT) => Self::Diff,
            other => panic!("{}: unknown format {other:?}", directory.display()),
        }
    }

    /// Every file of the directory, with the schema of its rows.
    fn files(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Snapshot => &[
                ("manifest.json", "snapshot-manifest"),
                ("files.jsonl", "file"),
                ("entities.jsonl", "entity"),
                ("externals.jsonl", "external"),
                ("relations.jsonl", "relation"),
                ("syntax.jsonl", "syntax"),
                ("gaps.jsonl", "gap"),
            ],
            Self::Diff => &[
                ("manifest.json", "diff-manifest"),
                ("entities.jsonl", "diff-entity"),
                ("externals.jsonl", "diff-external"),
                ("relations.jsonl", "diff-relation"),
                ("candidates.jsonl", "diff-candidate"),
            ],
        }
    }

    fn schema(self) -> String {
        match self {
            Self::Snapshot => SCHEMA_VERSION.to_string(),
            Self::Diff => DIFF_SCHEMA.to_owned(),
        }
    }
}

/// Every directory under `spec/examples` and `fixtures`, down to three levels, that holds a
/// `manifest.json`: a fixture's `expected`, each side's `old/expected` and `new/expected`, and
/// a frozen pair of snapshots side by side.
fn output_directories() -> Vec<(PathBuf, Output)> {
    fn visit(directory: &Path, depth: u32, found: &mut Vec<(PathBuf, Output)>) {
        let manifest = directory.join("manifest.json");
        if manifest.is_file() {
            let text = fs::read_to_string(&manifest).expect("manifest is readable");
            let value: Value = serde_json::from_str(&text).expect("manifest is JSON");
            found.push((directory.to_path_buf(), Output::of(&value, directory)));
            return;
        }
        if depth == 0 {
            return;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries {
            let path = entry.expect("readable directory entry").path();
            if path.is_dir() {
                visit(&path, depth - 1, found);
            }
        }
    }
    let root = repository();
    let mut found = Vec::new();
    for parent in ["spec/examples", "fixtures"] {
        visit(&root.join(parent), 3, &mut found);
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

#[test]
fn outputs_in_the_repository_pass_the_schemas() {
    let schemas = Schemas::load();
    let directories = output_directories();
    let root = repository();
    for (expected, kind) in [
        ("spec/examples/rust-basic", Output::Snapshot),
        ("spec/examples/diff-basic", Output::Diff),
        ("fixtures/diff-basic/expected", Output::Diff),
        ("fixtures/diff-basic/old/expected", Output::Snapshot),
        ("fixtures/diff-basic/new/expected", Output::Snapshot),
        ("fixtures/diff-public/v0.20.0", Output::Snapshot),
        ("fixtures/diff-public/v0.21.0", Output::Snapshot),
    ] {
        assert!(
            directories
                .iter()
                .any(|(d, k)| d.ends_with(expected) && *k == kind),
            "{expected} must be found as a {kind:?}, found {directories:?}"
        );
    }
    for (directory, kind) in directories {
        for (name, schema) in kind.files() {
            let path = directory.join(name);
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} must exist: {e}", path.display()));
            if *name == "manifest.json" {
                let manifest: Value = serde_json::from_str(&text).expect("manifest is JSON");
                schemas.assert_valid(schema, &manifest);
                if directory.starts_with(root.join("fixtures/diff-public")) {
                    // Frozen inputs keep the schema they were measured at, which the
                    // reader of the current minor refuses.
                    assert_eq!(manifest["schema"], json!("0.2"), "{}", path.display());
                    let stamped: Version = "0.2".parse().expect("a version");
                    assert!(
                        compat::accept(SCHEMA_VERSION, stamped).is_err(),
                        "{} must be refused by the current reader",
                        path.display()
                    );
                } else {
                    assert_eq!(
                        manifest["schema"],
                        json!(kind.schema()),
                        "{} is stamped with the current schema",
                        path.display()
                    );
                }
                continue;
            }
            // One validator per file: the public pair holds thousands of rows.
            let validator = schemas.validator(schema);
            for (number, line) in text.lines().enumerate() {
                let row: Value = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("{}:{}: not JSON: {e}", path.display(), number + 1));
                let errors: Vec<String> = validator
                    .iter_errors(&row)
                    .map(|e| format!("{e} at {}", e.instance_path()))
                    .collect();
                assert!(
                    errors.is_empty(),
                    "{}:{}: {errors:?}",
                    path.display(),
                    number + 1
                );
            }
        }
    }
}

#[test]
fn the_diff_golden_is_the_specification_example() {
    let root = repository();
    for (name, _) in Output::Diff.files() {
        let example = fs::read(root.join("spec/examples/diff-basic").join(name))
            .unwrap_or_else(|e| panic!("spec/examples/diff-basic/{name}: {e}"));
        let golden = fs::read(root.join("fixtures/diff-basic/expected").join(name))
            .unwrap_or_else(|e| panic!("fixtures/diff-basic/expected/{name}: {e}"));
        assert!(
            example == golden,
            "{name}: the golden differs from the example"
        );
    }
}

/// A changed row needs properties, each side of a property is a value, and a removed or rekeyed row carries none.
#[test]
fn diff_entity_rows_the_specification_forbids_fail() {
    let schemas = Schemas::load();
    for (schema, row) in [
        (
            "diff-entity",
            json!({"change": "changed", "old": "cargo:d:a().", "new": "cargo:d:a()."}),
        ),
        (
            "diff-entity",
            json!({"change": "moved", "old": "cargo:d:a().", "new": "cargo:d:b()."}),
        ),
        (
            "diff-entity",
            json!({"change": "removed", "old": "cargo:d:a().", "name": "a", "properties": {"lines": {"old": 1, "new": 2}}}),
        ),
        (
            "diff-entity",
            json!({"change": "added", "old": "cargo:d:a().", "new": "cargo:d:a().", "name": "a"}),
        ),
        (
            "diff-entity",
            json!({"change": "rekeyed", "old": "cargo:d:a().", "new": "cargo:d:a().~1", "properties": {"lines": {"old": 1, "new": 2}}}),
        ),
        (
            "diff-entity",
            json!({"change": "changed", "old": "cargo:d:a().", "new": "cargo:d:a().", "properties": {}}),
        ),
        (
            "diff-entity",
            json!({"change": "changed", "old": "cargo:d:a().", "new": "cargo:d:a().", "properties": {"lines": {}}}),
        ),
        (
            "diff-entity",
            json!({"change": "changed", "old": "cargo:d:a().", "new": "cargo:d:a().", "name": "a", "properties": {"lines": {"old": 1, "new": 2}}}),
        ),
        (
            "diff-entity",
            json!({"change": "changed", "old": "cargo:d:a().", "new": "cargo:d:a().", "properties": {"outline": {"old": {"call": 0}, "new": {}}}}),
        ),
    ] {
        schemas.assert_invalid(schema, &row);
    }
}

/// An external is never changed and never has properties.
#[test]
fn diff_external_rows_the_specification_forbids_fail() {
    let schemas = Schemas::load();
    for (schema, row) in [
        (
            "diff-external",
            json!({"change": "added", "new": "cargo:d:x().", "package": "cargo:d", "properties": {}}),
        ),
        (
            "diff-external",
            json!({"change": "changed", "old": "cargo:d:x().", "new": "cargo:d:x()."}),
        ),
        (
            "diff-external",
            json!({"change": "added", "new": "cargo:d:x()."}),
        ),
    ] {
        schemas.assert_invalid(schema, &row);
    }
}

/// A row carries its discriminators, and `endpoint` agrees with which ends are single.
#[test]
fn diff_relation_rows_the_specification_forbids_fail() {
    let schemas = Schemas::load();
    for (schema, row) in [
        (
            "diff-relation",
            json!({"change": "added", "from": {"new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "endpoint": "from", "rel": "ref"}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"old": "cargo:d:a().", "new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "rel": "call", "resolution": "declared", "src": "adapter:x/syntax"}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"old": "cargo:d:a().", "new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "rel": "call", "mechanism": "unknown", "src": "adapter:x/syntax"}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"old": "cargo:d:a().", "new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "rel": "call", "mechanism": "unknown", "resolution": "declared"}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "rel": "ref", "roles": []}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"old": "cargo:d:a().", "new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "endpoint": "both", "rel": "ref", "roles": []}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "endpoint": "to", "rel": "ref", "roles": []}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"old": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "endpoint": "from", "rel": "ref", "roles": []}),
        ),
        (
            "diff-relation",
            json!({"change": "removed", "from": {"new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "endpoint": "from", "rel": "ref", "roles": []}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"old": "cargo:d:a().", "new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "rel": "ref", "roles": [], "src": "core:reference-in-body"}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"old": "cargo:d:a().", "new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "rel": "call", "mechanism": "unknown", "resolution": "direct", "src": "adapter:x/syntax"}),
        ),
        (
            "diff-relation",
            json!({"change": "added", "from": {"old": "cargo:d:a().", "new": "cargo:d:a()."}, "to": {"old": "cargo:d:b().", "new": "cargo:d:b()."}, "rel": "call", "mechanism": "static", "resolution": "declared", "src": "adapter:x/calls"}),
        ),
    ] {
        schemas.assert_invalid(schema, &row);
    }
}

/// A measure is at most 1 000 000, every feature is present, a rank is 1 to 3; a snapshot manifest is not a diff manifest.
#[test]
fn diff_candidate_and_manifest_rows_the_specification_forbids_fail() {
    let schemas = Schemas::load();
    for (schema, row) in [
        (
            "diff-candidate",
            json!({"old": "cargo:d:a().", "new": "cargo:d:b().", "rank": 1, "score": 1_000_001, "name": 0, "file": 0, "parents": 0, "size": 0, "outgoing": 0, "incoming": 0}),
        ),
        (
            "diff-candidate",
            json!({"old": "cargo:d:a().", "new": "cargo:d:b().", "rank": 1, "score": 500_000, "name": 0, "file": 0, "parents": 0, "size": 0, "outgoing": 0}),
        ),
        (
            "diff-candidate",
            json!({"old": "cargo:d:a().", "new": "cargo:d:b().", "rank": 4, "score": 500_000, "name": 0, "file": 0, "parents": 0, "size": 0, "outgoing": 0, "incoming": 0}),
        ),
        ("diff-manifest", json!({"format": "gosanja-snapshot"})),
    ] {
        schemas.assert_invalid(schema, &row);
    }
}

/// The first paragraph after a Markdown document's title, its lines joined by spaces.
fn opening(path: &str) -> String {
    let text = fs::read_to_string(repository().join(path))
        .unwrap_or_else(|e| panic!("{path} must exist: {e}"));
    let mut lines = text
        .lines()
        .skip_while(|l| l.starts_with('#') || l.is_empty());
    let mut paragraph = Vec::new();
    for line in lines.by_ref() {
        if line.is_empty() {
            break;
        }
        paragraph.push(line.trim());
    }
    paragraph.join(" ")
}

/// Both documents of the diff begin with what a row says and what it does not, before the
/// command or the files, so that no reader meets a row first.
#[test]
fn both_diff_documents_open_with_what_a_row_states() {
    for path in ["spec/diff.md", "docs/diff.md"] {
        let first = opening(path);
        for phrase in [
            "one snapshot holds a statement and the other does not",
            "never says that the code changed",
            "gap counts per kind",
        ] {
            assert!(
                first.contains(phrase),
                "{path} opens without \"{phrase}\": {first}"
            );
        }
    }
}

/// An added or removed row carries what its snapshot row has by its variant, and a property
/// that is compared only when both sides observed it has both sides.
#[test]
fn diff_entity_rows_carry_what_their_variant_and_property_require() {
    let schemas = Schemas::load();
    for row in [
        json!({"change": "added", "new": "cargo:d:f().", "name": "f", "file": "src/lib.rs"}),
        json!({"change": "removed", "old": "cargo:d:f().~1", "name": "f", "role": "callable", "member_of": "cargo:d:f()."}),
        json!({"change": "added", "new": "cargo:d:m/", "name": "m", "virtual": true}),
        json!({"change": "changed", "old": "cargo:d:m/", "new": "cargo:d:m/", "properties": {"file": {"new": "src/lib.rs"}}}),
        json!({"change": "changed", "old": "cargo:d:f().", "new": "cargo:d:f().", "properties": {"lines": {"old": 2}}}),
        json!({"change": "changed", "old": "cargo:d:f().", "new": "cargo:d:f().", "properties": {"outline": {"new": {}}}}),
        json!({"change": "changed", "old": "cargo:d:f().", "new": "cargo:d:f().", "properties": {"kind": {"note": "x"}}}),
    ] {
        schemas.assert_invalid("diff-entity", &row);
    }
    for row in [
        json!({"change": "removed", "old": "cargo:d:f().", "name": "f", "role": "callable"}),
        json!({"change": "added", "new": "cargo:d:m/", "name": "m", "role": "container"}),
        json!({"change": "added", "new": "cargo:d:g().", "name": "g", "group": true}),
        json!({"change": "added", "new": "cargo:d:m/", "name": "m", "role": "container", "virtual": true}),
        json!({"change": "removed", "old": "cargo:d:f().~1", "name": "f", "role": "callable", "member_of": "cargo:d:f().", "file": "src/a.rs"}),
        json!({"change": "changed", "old": "cargo:d:f().", "new": "cargo:d:f().", "properties": {"visibility": {"new": "pub"}}}),
        json!({"change": "changed", "old": "cargo:d:f().", "new": "cargo:d:f().", "properties": {"lines": {"old": 2, "new": 3}, "later": 1}}),
        json!({"change": "added", "new": "cargo:d:f().", "name": "f", "role": "callable", "file": "src/lib.rs", "later": true}),
    ] {
        schemas.assert_valid("diff-entity", &row);
    }
}
