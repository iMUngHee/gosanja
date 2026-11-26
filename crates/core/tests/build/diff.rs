// Diff behavior for targets, file facts and test marks.

include!("run.rs");

mod build_diff {
    use super::build_run::{self, Out, file, standard, target};
    use serde_json::{Value, json};
    use std::path::Path;

    /// Writes a snapshot's files into a fresh directory.
    pub fn materialize(out: &Out) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in &out.tables {
            std::fs::write(dir.path().join(name), text).unwrap();
        }
        dir
    }

    /// The diff manifest and its row files.
    pub fn diff(old: &Out, new: &Out) -> (Value, Vec<(&'static str, Vec<u8>)>) {
        let (old, new) = (materialize(old), materialize(new));
        let comparison = gosanja_core::diff::compare(old.path(), new.path()).unwrap();
        let files = gosanja_core::write::serialise_diff(&comparison);
        let manifest = files.iter().find(|(n, _)| *n == "manifest.json").unwrap();
        (serde_json::from_slice(&manifest.1).unwrap(), files)
    }

    pub fn rows_of(files: &[(&'static str, Vec<u8>)], name: &str) -> usize {
        files
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(0, |(_, b)| String::from_utf8_lossy(b).lines().count())
    }

    pub fn no_rows(files: &[(&'static str, Vec<u8>)]) {
        for table in [
            "entities.jsonl",
            "externals.jsonl",
            "relations.jsonl",
            "candidates.jsonl",
        ] {
            assert_eq!(rows_of(files, table), 0, "{table}");
        }
    }

    /// Two runs of one tree that differ in their build rows and test rows.
    pub fn pair(old_build: Vec<String>, new_build: Vec<String>, new_tests: bool) -> (Out, Out) {
        let mut old = standard();
        old.build = old_build;
        let mut new = standard();
        new.build = new_build;
        if new_tests {
            new.syntax = vec![
                json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 0, "col": 7}, "test": "cfg"})
                    .to_string(),
            ];
        }
        (build_run::run(&old).unwrap(), build_run::run(&new).unwrap())
    }

    pub fn lib(test: Option<&str>) -> Vec<String> {
        let mut facts = json!({"targets": ["lib"]});
        if let Some(test) = test {
            facts["test"] = test.into();
        }
        vec![target("lib", None), file("src/lib.rs", facts)]
    }

    pub fn repository(path: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }
}

#[test]
fn build_differences_give_no_rows() {
    use build_diff::{diff, lib, no_rows, pair};
    use build_run::{file, target};
    use serde_json::json;
    let mut other = lib(Some("target"));
    other.push(target("bin", None));
    other.push(file(
        "tests/it.rs",
        json!({"targets": ["bin"], "test": "target", "constraint": "x"}),
    ));
    let (old, new) = pair(lib(None), other, true);
    let (manifest, files) = diff(&old, &new);
    no_rows(&files);
    assert_eq!(manifest["not_compared"], json!(["build", "test"]));
    // The pair differs where it should, so the absence of rows means something.
    assert_ne!(old.rows("targets.jsonl"), new.rows("targets.jsonl"));
    assert_ne!(old.rows("files.jsonl"), new.rows("files.jsonl"));
    assert_ne!(old.rows("entities.jsonl"), new.rows("entities.jsonl"));
}

#[test]
fn build_syntax_only_mark_lists_test() {
    use build_diff::{diff, no_rows};
    use serde_json::json;
    // Without build in either snapshot, only the syntax-only mark is left to disclose.
    let mut plain = build_run::standard();
    plain.request = Some("base,syntax,calls");
    let mut marked = plain.clone();
    marked.syntax = vec![
        json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 0, "col": 7}, "test": "cfg"})
            .to_string(),
    ];
    let (old, new) = (
        build_run::run(&plain).unwrap(),
        build_run::run(&marked).unwrap(),
    );
    let (manifest, files) = diff(&old, &new);
    no_rows(&files);
    assert_eq!(manifest["not_compared"], json!(["test"]));
    assert_eq!(new.entity("m:p:alpha().")["test"], "cfg");
}

#[test]
fn build_diff_schema_is_0_4() {
    use build_diff::{diff, pair};
    let (old, new) = pair(vec![], vec![], false);
    let (manifest, _) = diff(&old, &new);
    assert_eq!(manifest["schema"], "0.4");
    assert_eq!(manifest["old"]["schema"], old.manifest()["schema"]);
}

#[test]
fn build_changed_attribute_still_a_row() {
    use build_diff::{diff, rows_of};
    use serde_json::json;
    let row = |attribute: &str| {
        let mut spec = build_run::standard();
        spec.syntax = vec![
            json!({"anchor": {"file": "src/lib.rs", "line": 0, "col": 7}, "attributes": [attribute]})
                .to_string(),
        ];
        build_run::run(&spec).unwrap()
    };
    let (_, files) = diff(&row("inline"), &row("cold"));
    assert_eq!(rows_of(&files, "entities.jsonl"), 1);
}

#[test]
fn build_diff_public_refused() {
    let root = build_diff::repository("fixtures/diff-public");
    let sides: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect();
    assert!(sides.len() >= 2, "{sides:?}");
    gosanja_core::diff::compare(&sides[0], &sides[1]).unwrap_err();
}
