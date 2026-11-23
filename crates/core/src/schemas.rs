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
    File => "file",
    Entity => "entity",
    External => "external",
    Relation => "relation",
    Syntax => "syntax",
    Gap => "gap",
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
}
