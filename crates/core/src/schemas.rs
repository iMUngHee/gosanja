//! The normative JSON Schemas under `spec/schema/`, bundled into the binary and compiled once.
//!
//! Both the adapter runner and the snapshot validator check rows against these, so there is
//! exactly one copy of each schema and one registry resolving the references between them.

use std::sync::LazyLock;

use jsonschema::{Registry, Validator};
use serde_json::{Value, json};

/// One normative schema, named after its file under `spec/schema/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schema {
    /// `adapter-manifest.schema.json`
    AdapterManifest,
    /// `adapter-result.schema.json`
    AdapterResult,
    /// `syntax-fact.schema.json`
    SyntaxFact,
    /// `call-evidence.schema.json`
    CallEvidence,
    /// `snapshot-manifest.schema.json`
    SnapshotManifest,
    /// `diff-manifest.schema.json`
    DiffManifest,
    /// `diff-entity.schema.json`
    DiffEntity,
    /// `diff-external.schema.json`
    DiffExternal,
    /// `diff-relation.schema.json`
    DiffRelation,
    /// `diff-candidate.schema.json`
    DiffCandidate,
    /// `file.schema.json`
    File,
    /// `entity.schema.json`
    Entity,
    /// `external.schema.json`
    External,
    /// `relation.schema.json`
    Relation,
    /// `syntax.schema.json`
    Syntax,
    /// `gap.schema.json`
    Gap,
    /// `build.schema.json`
    Build,
    /// `target.schema.json`
    Target,
}

const BASE: &str = "https://gosanja.invalid/schema/";

macro_rules! bundled {
    ($($variant:ident => $stem:literal),* $(,)?) => {
        const ALL: &[(Schema, &str, &str)] = &[$((
            Schema::$variant,
            $stem,
            include_str!(concat!("../../../spec/schema/", $stem, ".schema.json")),
        )),*];
    };
}

bundled! {
    AdapterManifest => "adapter-manifest",
    AdapterResult => "adapter-result",
    SyntaxFact => "syntax-fact",
    CallEvidence => "call-evidence",
    SnapshotManifest => "snapshot-manifest",
    DiffManifest => "diff-manifest",
    DiffEntity => "diff-entity",
    DiffExternal => "diff-external",
    DiffRelation => "diff-relation",
    DiffCandidate => "diff-candidate",
    File => "file",
    Entity => "entity",
    External => "external",
    Relation => "relation",
    Syntax => "syntax",
    Gap => "gap",
    Build => "build",
    Target => "target",
}

static VALIDATORS: LazyLock<Vec<(Schema, Validator)>> = LazyLock::new(|| {
    let mut registry = Registry::new();
    for (_, stem, text) in ALL {
        let document: Value = serde_json::from_str(text).expect("a bundled schema is JSON");
        registry = registry
            .add(format!("{BASE}{stem}.schema.json"), document)
            .expect("a bundled schema registers");
    }
    let registry = registry
        .prepare()
        .expect("the references between bundled schemas resolve");
    ALL.iter()
        .map(|(schema, stem, _)| {
            let validator = jsonschema::options()
                .with_registry(&registry)
                .build(&json!({ "$ref": format!("{BASE}{stem}.schema.json") }))
                .expect("a bundled schema compiles");
            (*schema, validator)
        })
        .collect()
});

/// The compiled validator of `schema`.
///
/// # Panics
///
/// Only if a bundled schema were not a valid JSON Schema; the tests of this module compile
/// every one.
#[must_use]
pub fn validator(schema: Schema) -> &'static Validator {
    VALIDATORS
        .iter()
        .find_map(|(s, v)| (*s == schema).then_some(v))
        .expect("every schema is bundled")
}

/// How a row fits a schema, as a reader needs to know under `spec/README.md` rule 5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fit {
    /// The row passes.
    Valid,
    /// Every error sits on a field whose string value no enumeration of the schema holds, so
    /// the row fails only because of values from a newer minor. An error the unknown value
    /// causes elsewhere in the schema, such as a conditional `const` on that same field,
    /// sits on that field too.
    UnknownValueOnly,
    /// Some error sits on another field; the first such error, with its location.
    Invalid(String),
}

/// Classifies `value` against `schema`.
#[must_use]
pub fn fit(schema: Schema, value: &Value) -> Fit {
    let errors: Vec<_> = validator(schema).iter_errors(value).collect();
    if errors.is_empty() {
        return Fit::Valid;
    }
    let unknown: Vec<String> = errors
        .iter()
        .filter(|e| {
            matches!(
                e.kind(),
                jsonschema::error::ValidationErrorKind::Enum { .. }
            ) && e.instance().is_string()
        })
        .map(|e| e.instance_path().to_string())
        .collect();
    match errors
        .iter()
        .find(|e| !unknown.contains(&e.instance_path().to_string()))
    {
        None => Fit::UnknownValueOnly,
        Some(e) => Fit::Invalid(format!("{e} at {}", e.instance_path())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_compiles_and_checks_shapes() {
        for (schema, _, _) in ALL {
            let _compiled = validator(*schema);
        }
        let file = json!({"path":"src/a.rs","bytes":1,"lines":1,"sha256":"0".repeat(64)});
        assert!(validator(Schema::File).is_valid(&file));
        assert!(!validator(Schema::File).is_valid(&json!({"path":"/abs","bytes":1})));
        let gap = json!({"gap":"symbol-collision","src":"core:identity"});
        assert!(!validator(Schema::Gap).is_valid(&gap), "keys are required");
    }

    #[test]
    fn errors_on_an_unknown_value_belong_to_it_and_others_do_not() {
        let gap = |extra: Value| {
            let mut row = json!({"gap":"missing-layer","layer":"syntax","src":"adapter:x/syntax"});
            if let (Value::Object(row), Value::Object(extra)) = (&mut row, extra) {
                row.extend(extra);
            }
            fit(Schema::Gap, &row)
        };
        assert_eq!(gap(json!({})), Fit::Valid);
        assert_eq!(gap(json!({"layer":"future"})), Fit::UnknownValueOnly);
        assert!(
            matches!(gap(json!({"layer":"calls"})), Fit::Invalid(_)),
            "a known value"
        );
        assert!(
            matches!(gap(json!({"layer":7})), Fit::Invalid(_)),
            "not a string"
        );
        let both = gap(json!({"layer":"future","count":-1}));
        assert!(
            matches!(&both, Fit::Invalid(e) if e.contains("/count")),
            "{both:?}"
        );
    }
}
