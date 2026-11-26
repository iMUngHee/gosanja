// Test marks from item and file evidence.

include!("run.rs");

#[test]
fn build_test_precedence() {
    use build_run::{file, run, standard, target};
    use serde_json::json;
    use support::{DEF, doc, occ};
    let mut spec = standard();
    spec.documents[2] = doc(
        "tests/it.rs",
        vec![
            occ("s m p 1 helper().", DEF, &[0, 3, 9], &[0, 0, 14]),
            occ("s m p 1 other().", DEF, &[1, 3, 8], &[1, 0, 13]),
        ],
    );
    spec.sources[2] = ("tests/it.rs", "fn helper() {}\nfn other() {}\n".to_owned());
    spec.syntax = vec![
        json!({"row": "test", "anchor": {"file": "tests/it.rs", "line": 0, "col": 3}, "test": "cfg"})
            .to_string(),
    ];
    spec.build = vec![
        target("it", None),
        file("tests/it.rs", json!({"targets": ["it"], "test": "target"})),
    ];
    let out = run(&spec).unwrap();
    assert_eq!(out.entity("m:p:helper().")["test"], "cfg");
    assert_eq!(out.entity("m:p:other().")["test"], "target");
    assert!(out.entity("m:p:alpha().").get("test").is_none());
    assert_eq!(out.validation(), Vec::<String>::new());
}

#[test]
fn build_test_decls_common_mark() {
    use build_run::{file, run, skipped, standard};
    use serde_json::json;
    let both = {
        let mut spec = standard();
        spec.build = vec![
            file("a.go", json!({"test": "file"})),
            file("b.go", json!({"test": "file"})),
        ];
        spec.build_entry = Some(skipped(&[
            ("a.go", "target-unknown"),
            ("b.go", "target-unknown"),
        ]));
        spec
    };
    let out = run(&both).unwrap();
    assert!(out.entity("m:p:pkg/").get("decls").is_some());
    assert_eq!(out.entity("m:p:pkg/")["test"], "file");
    assert_eq!(out.validation(), Vec::<String>::new());
    let mut one = both;
    one.build.pop();
    one.build_entry = Some(skipped(&[("a.go", "target-unknown")]));
    let out = run(&one).unwrap();
    assert!(out.entity("m:p:pkg/").get("test").is_none());
    // A test row at a clause site gives the folded package nothing.
    let mut row = one;
    row.syntax = vec![
        json!({"row": "test", "anchor": {"file": "b.go", "line": 0, "col": 8}, "test": "cfg"})
            .to_string(),
    ];
    let out = run(&row).unwrap();
    assert!(out.entity("m:p:pkg/").get("decls").is_some());
    assert!(out.entity("m:p:pkg/").get("test").is_none());
}

#[test]
fn build_test_group_members() {
    use build_run::{CRATE, file, run, skipped, standard};
    use serde_json::json;
    let mut spec = standard();
    spec.build = vec![
        file("src/lib.rs", json!({"test": "cfg"})),
        file("src/main.rs", json!({"test": "target"})),
    ];
    spec.build_entry = Some(skipped(&[
        ("src/lib.rs", "target-unknown"),
        ("src/main.rs", "target-unknown"),
    ]));
    let out = run(&spec).unwrap();
    let group = out
        .rows("entities.jsonl")
        .into_iter()
        .find(|e| e["symbol"] == CRATE && e["group"] == true)
        .unwrap();
    assert!(group.get("test").is_none(), "{group}");
    assert_eq!(
        out.entity(&out.member_in(CRATE, "src/lib.rs"))["test"],
        "cfg"
    );
    assert_eq!(
        out.entity(&out.member_in(CRATE, "src/main.rs"))["test"],
        "target"
    );
    assert_eq!(out.validation(), Vec::<String>::new());
}

#[test]
fn build_test_attribute_value_kept() {
    use build_run::{file, run, standard, target};
    use serde_json::json;
    let mut spec = standard();
    spec.syntax = vec![
        json!({"row": "test", "anchor": {"file": "src/lib.rs", "line": 0, "col": 7}, "test": "attribute"})
            .to_string(),
    ];
    spec.build = vec![
        target("lib", None),
        file("src/lib.rs", json!({"targets": ["lib"], "test": "target"})),
    ];
    let out = run(&spec).unwrap();
    assert_eq!(out.entity("m:p:alpha().")["test"], "attribute");
    assert_eq!(out.entity("m:p:beta().")["test"], "target");
    assert_eq!(out.validation(), Vec::<String>::new());
}
