//! The Rust types and the JSON Schemas under `spec/schema` must describe the same shapes.
//!
//! Every row a type can serialise has to pass its schema, the snapshots kept in the repository
//! have to pass too, and a few rows the specification forbids have to fail.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gosanja_schema::contract::{
    AdapterManifest, AdapterResult, Anchor, CallEvidence, CallSite, Diagnostic, LayerName,
    LayerResult, LayerStatus, Requirement, SyntaxFact,
};
use gosanja_schema::position::{PositionEncoding, Range, Site};
use gosanja_schema::snapshot::{
    AdapterInfo, BuildContext, Counts, Def, EntityRow, Evidence, ExternalRow, FORMAT, FailedDetail,
    FileRow, GapKind, GapRow, Layer, LayerState, Layers, Manifest, Mechanism, OutlineKind,
    OutlineNode, Package, Project, RefRole, Rel, RelationRow, Resolution, Role, Scope, SyntaxRow,
    Tool,
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
        assert_eq!(count, 11, "the specification names eleven schemas");
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
        schema: "0.1".into(),
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
                contract: "0.1".into(),
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
fn every_gap_kind_passes_its_schema() {
    let schemas = Schemas::load();

    let gaps = [
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
    ];
    for gap in &gaps {
        schemas.assert_valid("gap", &to_value(gap));
    }
}

#[test]
fn every_adapter_file_type_passes_its_schema() {
    let schemas = Schemas::load();

    let manifest = AdapterManifest {
        contract: "0.1".into(),
        name: "fake".into(),
        version: "0.0.1".into(),
        languages: vec!["rust".into()],
        position_encoding: PositionEncoding::Utf16,
        layers: vec![LayerName::Base, LayerName::Syntax, LayerName::Calls],
        requires: Some(vec![Requirement {
            tool: "rust-analyzer".into(),
            version: None,
        }]),
    };
    schemas.assert_valid("adapter-manifest", &to_value(&manifest));

    let result = AdapterResult {
        contract: "0.1".into(),
        layers: BTreeMap::from([
            (
                "base".to_owned(),
                LayerResult {
                    status: LayerStatus::Ok,
                    diagnostics: None,
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
    schemas.assert_valid("adapter-result", &json!({"contract": "0.1", "layers": {}}));

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
    schemas.assert_invalid("adapter-result", &json!({"contract": "0.1", "layers": {}, "builds": [{"id": "release", "toolchain": "/home/dev/rustc"}]}));
    schemas.assert_valid("adapter-result", &json!({"contract": "0.1", "layers": {}, "builds": [{"id": "release", "cwd": "ignored, not copied"}]}));
    // A qualified feature name is not a path; an absolute path is refused in any spelling.
    schemas.assert_valid("adapter-result", &json!({"contract": "0.1", "layers": {}, "builds": [{"id": "release", "features": ["ascii/std", "derive"]}]}));
    for absolute in [
        "/abs/feature",
        "~/feature",
        "C:\\feature",
        "\\\\host\\share",
    ] {
        schemas.assert_invalid("adapter-result", &json!({"contract": "0.1", "layers": {}, "builds": [{"id": "release", "features": [absolute]}]}));
    }
    schemas.assert_invalid(
        "adapter-result",
        &json!({"contract": "0.1", "layers": {}, "builds": [{"id": "a/b"}]}),
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
    schemas.assert_invalid("adapter-result", &json!({"contract": "0.1", "layers": {"base": {"status": "failed", "diagnostics": [{"code": "Could not run"}]}}}));
    // A version is two integers.
    schemas.assert_invalid("adapter-manifest", &json!({"contract": "0.1.0", "name": "x", "version": "1", "languages": ["go"], "position_encoding": "utf-8", "layers": ["base"]}));
}

/// Maps a file of a snapshot directory to the schema of its rows.
fn schema_of(file_name: &str) -> Option<&'static str> {
    Some(match file_name {
        "manifest.json" => "snapshot-manifest",
        "files.jsonl" => "file",
        "entities.jsonl" => "entity",
        "externals.jsonl" => "external",
        "relations.jsonl" => "relation",
        "syntax.jsonl" => "syntax",
        "gaps.jsonl" => "gap",
        _ => return None,
    })
}

fn snapshot_directories() -> Vec<PathBuf> {
    let root = repository();
    let mut found = Vec::new();
    for parent in ["spec/examples", "fixtures"] {
        let Ok(entries) = fs::read_dir(root.join(parent)) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("readable directory entry").path();
            for candidate in [path.clone(), path.join("expected")] {
                if candidate.join("manifest.json").is_file() {
                    found.push(candidate);
                }
            }
        }
    }
    found.sort();
    found
}

#[test]
fn snapshots_in_the_repository_pass_the_schemas() {
    let schemas = Schemas::load();
    let directories = snapshot_directories();
    assert!(
        directories
            .iter()
            .any(|d| d.ends_with("spec/examples/rust-basic")),
        "the hand-written example must exist, found {directories:?}"
    );
    for directory in directories {
        for name in [
            "manifest.json",
            "files.jsonl",
            "entities.jsonl",
            "externals.jsonl",
            "relations.jsonl",
            "syntax.jsonl",
            "gaps.jsonl",
        ] {
            let path = directory.join(name);
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} must exist: {e}", path.display()));
            let schema = schema_of(name).expect("every snapshot file has a schema");
            if name == "manifest.json" {
                schemas.assert_valid(
                    schema,
                    &serde_json::from_str(&text).expect("manifest is JSON"),
                );
                continue;
            }
            for (number, line) in text.lines().enumerate() {
                let row: Value = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("{}:{}: not JSON: {e}", path.display(), number + 1));
                schemas.assert_valid(schema, &row);
            }
        }
    }
}
