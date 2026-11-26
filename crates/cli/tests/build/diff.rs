// Build facts excluded from command-line diffs.

include!("world.rs");

#[cfg(unix)]
#[test]
fn build_not_compared_on_the_command_line() {
    use build_cli::{World, manifest, ok, rows};
    use serde_json::json;
    let world = World::rust_basic();
    let build = |name: &str, test: &str| {
        world.lines(
            name,
            &[
                json!({"row": "target", "key": "cargo:demo:lib:demo", "kind": "lib", "name": "demo"})
                    .to_string(),
                json!({"row": "file", "file": "src/views.rs", "targets": ["cargo:demo:lib:demo"], "test": test})
                    .to_string(),
            ],
        )
    };
    let run = |name: &str, test: &str| {
        let adapter = world.adapter(
            name,
            &[("--build-file", build(&format!("{name}.jsonl"), test))],
        );
        let (output, out) = world.index(&adapter, name, None);
        ok(&output);
        out
    };
    let old = run("old", "target");
    let new = run("new", "cfg");
    let (output, diff) = world.diff(&old, &new);
    ok(&output);
    for table in [
        "entities.jsonl",
        "externals.jsonl",
        "relations.jsonl",
        "candidates.jsonl",
    ] {
        assert!(rows(&diff, table).is_empty(), "{table}");
    }
    let manifest = manifest(&diff);
    assert_eq!(manifest["not_compared"], json!(["build", "test"]));
    assert_eq!(manifest["schema"], "0.5");
}
