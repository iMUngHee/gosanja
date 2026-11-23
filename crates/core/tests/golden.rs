//! The checked-in fixtures against their goldens, and the known fact of each fixture, found by
//! name and file rather than by key.

mod support;

use gosanja_core::write;
use gosanja_schema::position::Site;
use gosanja_schema::snapshot::{EntityRow, GapKind, Rel};
use support::{build_fixture, diagnostics, fixture, read_dir, repo};

fn named<'a>(snapshot: &'a gosanja_core::graph::Snapshot, name: &str, file: &str) -> &'a EntityRow {
    let found: Vec<_> = snapshot
        .entities
        .iter()
        .filter(|e| e.name == name && e.def.as_ref().is_some_and(|d| d.file == file))
        .collect();
    assert_eq!(found.len(), 1, "one definition of {name} in {file}");
    found[0]
}

#[test]
fn every_fixture_matches_its_golden_and_is_deterministic() {
    for name in ["rust-basic", "go-basic", "ts-basic"] {
        let first = write::serialise(&build_fixture(name));
        let second = write::serialise(&build_fixture(name));
        assert_eq!(first, second, "{name}: two runs differ");
        assert_eq!(
            first,
            read_dir(&fixture(name).join("expected")),
            "{name}: golden"
        );
        assert!(
            diagnostics(&build_fixture(name)).is_empty(),
            "{name}: validates"
        );
    }
}

#[test]
fn rust_basic_equals_the_specification_example() {
    let written = write::serialise(&build_fixture("rust-basic"));
    assert_eq!(written, read_dir(&repo("spec/examples/rust-basic")));
}

/// Criterion 3: a `ref` from `render` to `empty` with a site on the line of the call.
#[test]
fn rust_basic_render_refers_to_empty_on_the_line_of_the_call() {
    let snapshot = build_fixture("rust-basic");
    let render = named(&snapshot, "render", "src/views.rs");
    let empty = named(&snapshot, "empty", "src/views.rs");
    let refs: Vec<_> = snapshot
        .relations
        .iter()
        .filter(|r| r.from == render.key && r.to == empty.key && r.rel == Rel::Ref)
        .collect();
    assert_eq!(refs.len(), 1);
    let sites = refs[0].sites.as_deref().expect("a ref has sites");
    assert!(
        sites
            .iter()
            .any(|s| s.file() == "src/views.rs" && s.range().start().0 == 4)
    );
}

/// Criterion 4, index-only part: both other indexers give bodies, so the recorded reference
/// is a `ref` row between the two named entities.
#[test]
fn go_and_typescript_known_facts_come_from_the_index_alone() {
    for (fixture_name, file, caller, site) in [
        (
            "go-basic",
            "main.go",
            "main",
            Site::new("main.go", gosanja_schema::position::Range::new(7, 5, 7, 10)),
        ),
        (
            "ts-basic",
            "src/index.ts",
            "run",
            Site::new(
                "src/index.ts",
                gosanja_schema::position::Range::new(5, 9, 5, 14),
            ),
        ),
    ] {
        let snapshot = build_fixture(fixture_name);
        let greet = named(&snapshot, "greet", file);
        let from = named(&snapshot, caller, file);
        let refs: Vec<_> = snapshot
            .relations
            .iter()
            .filter(|r| r.from == from.key && r.to == greet.key && r.rel == Rel::Ref)
            .collect();
        assert_eq!(refs.len(), 1, "{fixture_name}");
        assert_eq!(
            refs[0].sites.as_deref(),
            Some(&[site][..]),
            "{fixture_name}"
        );
        assert!(
            !snapshot
                .gaps
                .iter()
                .any(|g| g.gap == GapKind::UnattributedReferences),
            "{fixture_name}"
        );
    }
}
