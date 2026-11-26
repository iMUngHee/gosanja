//! The syntax layer, fact by fact, against `fixtures/rust-flow` and against source strings.
//!
//! Every expected value below was derived by hand from the source text, not from the adapter's
//! output: lines are zero-based and columns count characters.

use std::path::{Path, PathBuf};

use gosanja_adapter_rust::syntax;
use gosanja_schema::contract::SyntaxFact;
use serde_json::{Value, json};

const APP: &str = "app/src/main.rs";
const KIT: &str = "kit/src/lib.rs";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-flow")
}

fn index(path: &Path) -> scip::types::Index {
    use protobuf::Message;
    scip::types::Index::parse_from_bytes(&std::fs::read(path).expect("an index"))
        .expect("a SCIP index")
}

fn flow_layer() -> syntax::Layer {
    let root = fixture().canonicalize().expect("the fixture");
    syntax::layer(&root, &index(&root.join("index.scip")))
}

#[test]
fn token_sites_follow_use_trees_and_direct_impl_members() {
    let text = concat!(
        "pub use crate::outer::{self, first as named, second as _, nested::*};\n",
        "mod inner { use super::Trait; }\n",
        "fn local() { use crate::other; }\n",
        "opaque! { use crate::hidden; impl Hidden { fn hidden() {} } }\n",
        "impl crate::Generic<u8> { fn method() { fn nested() {} } const VALUE: u8 = 0; }\n",
        "impl path::Trait for Generic<u8> { type Item = u8; }\n",
        "impl Trait for &Generic<u8> { fn borrowed() {} }\n",
        "impl !Trait for Generic<u8> { fn negative() {} }\n",
        "impl Generic<u8> {}\n",
        "impl Trait for <Generic<u8> as Path>::Assoc { fn qualified() {} }\n",
    );
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("lib.rs"), text).unwrap();
    let index = index_of(vec![document("lib.rs", vec![])]);
    syntax::publish(syntax::layer(&root, &index), &root).unwrap();
    let output = std::fs::read_to_string(root.join("syntax.jsonl")).unwrap();
    let rows: Vec<Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| matches!(row["row"].as_str(), Some("impl" | "imports")))
        .collect();
    let token = |line: usize, name: &str| {
        let col = text.lines().nth(line).unwrap().find(name).unwrap();
        json!([line, col, line, col + name.len()])
    };
    let extent = |line: usize| json!([line, 0, line, text.lines().nth(line).unwrap().len()]);
    assert_eq!(
        rows,
        vec![
            json!({"row":"imports","file":"lib.rs","sites":[
                token(0,"crate"), token(0,"outer"), token(0,"self"), token(0,"first"),
                token(0,"named"), token(0,"second"), token(0,"nested"),
                token(1,"super"), token(1,"Trait"), token(2,"crate"), token(2,"other")
            ]}),
            json!({"row":"impl","file":"lib.rs","extent":extent(4),"members":[
                token(4,"method"),token(4,"VALUE")
            ],"self_type":token(4,"Generic")}),
            json!({"row":"impl","file":"lib.rs","extent":extent(5),"members":[
                token(5,"Item")
            ],"self_type":token(5,"Generic"),"trait_type":token(5,"Trait")}),
            json!({"row":"impl","file":"lib.rs","extent":extent(6),"members":[
                token(6,"borrowed")
            ],"trait_type":token(6,"Trait")}),
            json!({"row":"impl","file":"lib.rs","extent":extent(9),"members":[
                token(9,"qualified")
            ],"trait_type":token(9,"Trait")}),
        ]
    );
}

fn at<'a>(rows: &'a [SyntaxFact], file: &str, line: u32, col: u32) -> &'a SyntaxFact {
    rows.iter()
        .find(|r| r.anchor.file == file && r.anchor.line == line && r.anchor.col == Some(col))
        .unwrap_or_else(|| panic!("no row at {file}:{line}:{col}"))
}

fn outline(row: &SyntaxFact) -> Value {
    serde_json::to_value(row.outline.as_ref().expect("an outline")).expect("serialisable")
}

/// One outline node on one line: kind, detail, line, start, end, callee columns, children.
fn node(
    k: &str,
    detail: Option<&str>,
    line: u32,
    start: u32,
    end: u32,
    callee: Option<(u32, u32)>,
    children: Vec<Value>,
) -> Value {
    let mut v = json!({"k": k, "range": [line, start, line, end]});
    if let Some(d) = detail {
        v["detail"] = json!(d);
    }
    if let Some((a, b)) = callee {
        v["callee"] = json!([line, a, line, b]);
    }
    if !children.is_empty() {
        v["children"] = Value::Array(children);
    }
    v
}

fn leaf(k: &str, detail: Option<&str>, line: u32, start: u32, end: u32) -> Value {
    node(k, detail, line, start, end, None, vec![])
}

fn call(
    line: u32,
    start: u32,
    end: u32,
    callee: Option<(u32, u32)>,
    children: Vec<Value>,
) -> Value {
    node("call", None, line, start, end, callee, children)
}

fn only_row(source: &str) -> SyntaxFact {
    let rows = syntax::rows("a.rs", source).expect("parses");
    assert_eq!(rows.len(), 1, "{rows:?}");
    rows.into_iter().next().expect("one row")
}

// Items.

#[test]
fn every_item_kind_has_one_row_at_its_name_and_rows_come_in_order() {
    let layer = flow_layer();
    let anchors: Vec<_> = layer
        .rows
        .iter()
        .map(|r| {
            (
                r.anchor.file.as_str(),
                r.anchor.line,
                r.anchor.col.expect("a column"),
            )
        })
        .collect();
    // The file modules at 0:0, then every item in source order; no row for the impl blocks
    // (A9, K19, K23), `mod extra;` (A3), the variants, the fields, or `foreign_item` in the
    // extern block (K43).
    let expected = [
        (APP, 0, 0),   // file module
        (APP, 5, 13),  // marker, macro_rules
        (APP, 8, 7),   // Shell, struct
        (APP, 13, 6),  // INNER_VALUE, constant
        (APP, 14, 3),  // outer, function
        (APP, 16, 9),  // flow, async function
        (APP, 17, 7),  // nested, function in a body
        (APP, 18, 17), // item, macro_rules in a body
        (APP, 41, 3),  // main
        (KIT, 0, 0),   // file module
        (KIT, 2, 13),  // slot, macro_rules
        (KIT, 5, 11),  // Op, struct
        (KIT, 6, 9),   // Flag, enum
        (KIT, 7, 10),  // Bits, union
        (KIT, 8, 9),   // Count, type alias
        (KIT, 9, 10),  // LIMIT, constant
        (KIT, 10, 15), // TOTAL, static
        (KIT, 12, 10), // Adjust, trait
        (KIT, 13, 9),  // Output, associated type
        (KIT, 14, 10), // START, associated constant
        (KIT, 15, 7),  // required, trait method
        (KIT, 20, 11), // apply, method
        (KIT, 24, 9),  // Output in the impl
        (KIT, 25, 10), // START in the impl
        (KIT, 26, 7),  // required in the impl
        (KIT, 33, 7),  // step
        (KIT, 34, 7),  // make
        (KIT, 35, 20), // r#type, raw identifier: the column of `r#`
        (KIT, 36, 18), // foreign_abi
        (KIT, 38, 8),  // a, inline module
        (KIT, 39, 24), // restricted
        (KIT, 40, 18), // shared
        (KIT, 46, 3),  // inactive, under #[cfg(any())]
        (KIT, 48, 13), // one, macro_rules
        (KIT, 50, 13), // int, macro_rules
        (KIT, 52, 7),  // ping
        (KIT, 56, 3),  // pong
    ];
    assert_eq!(anchors, expected);
    assert!(
        layer
            .rows
            .iter()
            .all(|r| r.anchor.name.is_none() && r.body.is_none())
    );
}

#[test]
fn visibility_is_taken_as_written() {
    let layer = flow_layer();
    let visibility = |file, line, col| at(&layer.rows, file, line, col).visibility.as_deref();
    assert_eq!(visibility(KIT, 39, 24), Some("pub(in crate::a)"));
    assert_eq!(visibility(KIT, 40, 18), Some("pub(crate)"));
    assert_eq!(visibility(KIT, 33, 7), Some("pub"));
    assert_eq!(visibility(APP, 14, 3), None);
    assert_eq!(
        only_row("pub( in  crate ) fn f() {}").visibility.as_deref(),
        Some("pub( in  crate )")
    );
}

#[test]
fn qualifiers_are_the_keywords_before_the_item_keyword() {
    let layer = flow_layer();
    let qualifiers = |file, line, col| at(&layer.rows, file, line, col).qualifiers.clone();
    let strings = |q: &[&str]| Some(q.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    assert_eq!(qualifiers(KIT, 35, 20), strings(&["const", "unsafe"]));
    assert_eq!(qualifiers(APP, 16, 9), strings(&["async"]));
    assert_eq!(qualifiers(KIT, 36, 18), strings(&[r#"extern "C""#]));
    assert_eq!(qualifiers(KIT, 10, 15), strings(&["mut"]));
    assert_eq!(qualifiers(KIT, 33, 7), None);
    // What the fixture does not hold.
    let table: [(&str, &[&str]); 7] = [
        ("extern fn f() {}", &["extern"]),
        ("extern  \"C\"  fn f() {}", &[r#"extern "C""#]),
        ("extern r\"C\" fn f() {}", &[r#"extern r"C""#]),
        (
            "pub const async unsafe extern \"C\" fn f() {}",
            &["const", "async", "unsafe", r#"extern "C""#],
        ),
        ("auto trait T {}", &["auto"]),
        ("unsafe trait T {}", &["unsafe"]),
        ("unsafe auto trait T {}", &["unsafe", "auto"]),
    ];
    for (source, expected) in table {
        assert_eq!(only_row(source).qualifiers, strings(expected), "{source}");
    }
    let rows = syntax::rows(
        "a.rs",
        "impl T for S { default fn f() {} default const C: u8 = 0; default type A = u8; }",
    )
    .expect("parses");
    let qualifiers: Vec<_> = rows.iter().map(|r| r.qualifiers.clone()).collect();
    assert_eq!(
        qualifiers,
        [
            strings(&["default"]),
            strings(&["default"]),
            strings(&["default"])
        ]
    );
}

#[test]
fn attributes_are_the_outer_ones_as_written_without_docs() {
    let layer = flow_layer();
    let attributes = |file, line, col| at(&layer.rows, file, line, col).attributes.clone();
    assert_eq!(
        attributes(KIT, 33, 7),
        Some(vec![
            "#[inline]".to_owned(),
            r#"#[must_use = "use the next count"]"#.to_owned()
        ])
    );
    assert_eq!(
        attributes(KIT, 46, 3),
        Some(vec!["#[cfg(any())]".to_owned()])
    );
    assert_eq!(
        attributes(KIT, 48, 13),
        Some(vec!["#[macro_export]".to_owned()])
    );
    assert_eq!(attributes(APP, 14, 3), None);
    let row = only_row(
        "/** block */\n#[ inline ]\n/// line\n#[doc(hidden)]\nfn f() { #![allow(unused)] }",
    );
    assert_eq!(row.attributes, Some(vec!["#[ inline ]".to_owned()]));
}

#[test]
fn constructs_without_a_row_give_none() {
    for source in [
        "impl S {}",
        "mod m;",
        "use std::fmt;",
        "extern crate core;",
        "extern \"C\" { fn x(); static Y: u8; }",
        "const _: () = ();",
    ] {
        assert_eq!(
            syntax::rows("a.rs", source).expect("parses"),
            [],
            "{source}"
        );
    }
    let rows = syntax::rows("a.rs", "enum E { A, B(u8) } struct S { f: u8 }").expect("parses");
    let names: Vec<_> = rows.iter().map(|r| r.anchor.col).collect();
    assert_eq!(names, [Some(5), Some(27)]);
}

// The outline.

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the hand-derived tree of the function is one literal"
)]
fn the_function_with_every_construct_has_the_literal_outline() {
    let layer = flow_layer();
    let expected = json!([
        leaf("opaque", Some("item!"), 19, 4, 12),
        node(
            "branch",
            Some("let-else"),
            20,
            4,
            43,
            None,
            vec![node(
                "arm",
                Some("else"),
                20,
                27,
                42,
                None,
                vec![leaf("exit", Some("return"), 20, 29, 40),]
            )]
        ),
        node(
            "branch",
            Some("if"),
            21,
            4,
            44,
            None,
            vec![node(
                "arm",
                Some("else"),
                21,
                21,
                44,
                None,
                vec![leaf("branch", Some("if"), 21, 21, 44),]
            )]
        ),
        node(
            "branch",
            Some("match"),
            22,
            4,
            41,
            None,
            vec![leaf("arm", None, 22, 14, 31), leaf("arm", None, 22, 32, 39),]
        ),
        node(
            "loop",
            Some("loop"),
            23,
            4,
            19,
            None,
            vec![leaf("exit", Some("break"), 23, 11, 16)]
        ),
        node(
            "loop",
            Some("while"),
            24,
            4,
            29,
            None,
            vec![leaf("exit", Some("continue"), 24, 18, 26)]
        ),
        node(
            "loop",
            Some("while"),
            25,
            4,
            46,
            None,
            vec![leaf("exit", Some("break"), 25, 38, 43)]
        ),
        leaf("loop", Some("for"), 26, 4, 19),
        leaf("exit", Some("?"), 27, 12, 16),
        node(
            "closure",
            Some("closure"),
            28,
            12,
            24,
            None,
            vec![call(28, 16, 24, Some((16, 21)), vec![])]
        ),
        node(
            "await",
            None,
            29,
            12,
            29,
            None,
            vec![leaf("closure", Some("async"), 29, 12, 23)]
        ),
        node(
            "await",
            None,
            30,
            12,
            34,
            None,
            vec![leaf("closure", Some("async"), 30, 12, 28)]
        ),
        // After `/*é*/`: the callee `step` starts at character 28 (byte 29).
        call(31, 18, 35, Some((28, 32)), vec![]),
        call(32, 4, 15, Some((7, 12)), vec![]),
        call(33, 4, 22, Some((4, 9)), vec![]),
        // `make()(x)`: the outer call has no callee token, the inner one has `make`.
        call(34, 4, 13, None, vec![call(34, 4, 10, Some((4, 8)), vec![])]),
        call(35, 4, 13, None, vec![]),
        leaf("opaque", Some("stringify!"), 36, 12, 25),
        leaf("opaque", Some("assert!"), 37, 4, 17),
        // Type position, then pattern position.
        leaf("opaque", Some("flow_kit::int!"), 37, 26, 42),
        node(
            "branch",
            Some("match"),
            37,
            45,
            86,
            None,
            vec![
                node(
                    "arm",
                    None,
                    37,
                    55,
                    77,
                    None,
                    vec![leaf("opaque", Some("flow_kit::one!"), 37, 55, 71)]
                ),
                leaf("arm", None, 37, 78, 84),
            ]
        ),
        call(
            38,
            4,
            14,
            Some((4, 8)),
            vec![call(38, 9, 13, Some((9, 10)), vec![])]
        ),
    ]);
    assert_eq!(outline(at(&layer.rows, APP, 16, 9)), expected);
    // The nested function and the nested macro definition have rows of their own, and the
    // call inside `nested` is in its outline, not in `flow`'s.
    assert_eq!(
        outline(at(&layer.rows, APP, 17, 7)),
        json!([call(17, 18, 26, Some((18, 23)), vec![])])
    );
    assert_eq!(at(&layer.rows, APP, 18, 17).outline, None);
}

#[test]
fn macros_in_item_position_go_to_the_innermost_outlined_owner() {
    let layer = flow_layer();
    assert_eq!(
        outline(at(&layer.rows, APP, 0, 0)),
        json!([
            leaf("opaque", Some("marker!"), 6, 0, 10),
            leaf("opaque", Some("marker!"), 10, 4, 14)
        ])
    );
    assert_eq!(
        outline(at(&layer.rows, KIT, 0, 0)),
        json!([leaf("opaque", Some("slot!"), 3, 0, 8)])
    );
    assert_eq!(
        outline(at(&layer.rows, KIT, 12, 10)),
        json!([leaf("opaque", Some("slot!"), 16, 4, 12)])
    );
    let file_module = at(&layer.rows, APP, 0, 0);
    assert_eq!(
        (
            &file_module.visibility,
            &file_module.qualifiers,
            &file_module.attributes
        ),
        (&None, &None, &None)
    );
}

#[test]
fn raw_token_syntax_is_one_verbatim_node_and_an_empty_statement_is_nothing() {
    assert_eq!(
        outline(&only_row("fn forward() { become f(); }")),
        json!([leaf("opaque", Some("verbatim"), 0, 15, 25)])
    );
    assert_eq!(
        outline(&only_row("fn forward() { ; f(); }")),
        json!([call(0, 17, 20, Some((17, 18)), vec![])])
    );
}

// What the layer cannot process is reported, not lost.

/// A definition occurrence: symbol, name range `[line, start, end]` in bytes, enclosing range.
fn definition(symbol: &str, name: [i32; 3], enclosing: &[i32]) -> scip::types::Occurrence {
    scip::types::Occurrence {
        symbol: symbol.to_owned(),
        symbol_roles: scip::types::SymbolRole::Definition as i32,
        range: name.to_vec(),
        enclosing_range: enclosing.to_vec(),
        ..Default::default()
    }
}

fn document(path: &str, occurrences: Vec<scip::types::Occurrence>) -> scip::types::Document {
    scip::types::Document {
        relative_path: path.to_owned(),
        occurrences,
        ..Default::default()
    }
}

fn index_of(documents: Vec<scip::types::Document>) -> scip::types::Index {
    scip::types::Index {
        documents,
        ..Default::default()
    }
}

fn skipped(layer: &syntax::Layer) -> Vec<(&str, &str)> {
    layer
        .skipped
        .iter()
        .map(|s| (s.file.as_str(), s.code.as_str()))
        .collect()
}

/// Whether permissions bind this process; they do not for the super-user.
fn permissions_bind(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let probe = dir.join("probe");
    std::fs::write(&probe, "x").expect("write");
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let binds = std::fs::read(&probe).is_err();
    std::fs::remove_file(&probe).expect("rm");
    if !binds {
        println!("skipped: permissions do not bind the super-user, who may read any file");
    }
    binds
}

#[test]
fn files_that_cannot_be_processed_are_listed_and_the_rest_is_kept() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().expect("a directory");
    let root = temp.path().join("root");
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    let root = root.canonicalize().expect("resolved");
    let write = |path: &str, bytes: &[u8]| std::fs::write(root.join(path), bytes).expect("write");
    write("src/good.rs", b"pub fn good() {}\n");
    write("src/broken.rs", b"fn broken( {\n");
    write("src/latin.rs", b"fn caf\xe9() {}\n");
    write("src/bom.rs", "\u{feff}fn bom() {}\n".as_bytes());
    write("src/locked.rs", b"fn locked() {}\n");
    write("src/notes.txt", b"not rust\n");
    std::fs::write(temp.path().join("outside.rs"), "fn outside( {\n").expect("write");
    symlink(temp.path().join("outside.rs"), root.join("src/escape.rs")).expect("link");
    let lock = permissions_bind(&root);
    if lock {
        std::fs::set_permissions(
            root.join("src/locked.rs"),
            std::fs::Permissions::from_mode(0o000),
        )
        .expect("chmod");
    }
    let paths = [
        "src/good.rs",
        "src/broken.rs",
        "src/latin.rs",
        "src/bom.rs",
        "src/locked.rs",
        "src/notes.txt",
        "src/escape.rs",
        "/abs/broken.rs",
        "src/../src/broken.rs",
        "src/good.rs",
    ];
    let index = index_of(paths.iter().map(|p| document(p, vec![])).collect());
    let layer = syntax::layer(&root, &index);
    let mut expected = vec![
        ("src/bom.rs", "byte-order-mark"),
        ("src/broken.rs", "parse-failed"),
        ("src/latin.rs", "unreadable"),
    ];
    if lock {
        expected.push(("src/locked.rs", "unreadable"));
    } else {
        // Readable to the super-user, so it gives its row.
        expected.sort_unstable();
    }
    assert_eq!(skipped(&layer), expected);
    let anchors: Vec<_> = layer
        .rows
        .iter()
        .map(|r| (r.anchor.file.as_str(), r.anchor.line))
        .collect();
    let mut want = vec![("src/good.rs", 0)];
    if !lock {
        want.push(("src/locked.rs", 0));
    }
    assert_eq!(
        anchors, want,
        "one row per readable file, a document listed twice read once"
    );
}

/// A file with two functions whose outlines are one call each, and its index.
fn two_functions(narrow: &[&[i32]]) -> syntax::Layer {
    let temp = tempfile::tempdir().expect("a directory");
    let root = temp.path().canonicalize().expect("resolved");
    // line 0: fn wide() { f(); }       line 1: fn other() { g(); }
    std::fs::write(
        root.join("a.rs"),
        "fn wide() { f(); }\nfn other() { g(); }\n",
    )
    .expect("write");
    let mut occurrences: Vec<_> = narrow
        .iter()
        .map(|enclosing| definition("s m p 1 wide().", [0, 3, 7], enclosing))
        .collect();
    occurrences.push(definition("s m p 1 other().", [1, 3, 8], &[1, 0, 19]));
    syntax::layer(&root, &index_of(vec![document("a.rs", occurrences)]))
}

#[test]
fn an_outline_outside_the_index_extent_is_withheld_and_reported() {
    // `f()` lies at 0:12-15; an extent that ends at 0:10 does not hold it.
    let narrow: &[i32] = &[0, 0, 10];
    let wide: &[i32] = &[0, 0, 18];
    let past_the_file: &[i32] = &[0, 0, 9, 0];
    let outlined = |layer: &syntax::Layer| -> Vec<bool> {
        layer.rows.iter().map(|r| r.outline.is_some()).collect()
    };

    let layer = two_functions(&[narrow]);
    assert_eq!(outlined(&layer), [false, true]);
    assert_eq!(skipped(&layer), [("a.rs", "outline-outside-definition")]);
    assert!(
        layer.notes.iter().any(|n| n.contains("wide")),
        "{:?}",
        layer.notes
    );

    // Two observations of one definition: the greater extent counts, in either order.
    for order in [[narrow, wide], [wide, narrow]] {
        let layer = two_functions(&order);
        assert_eq!(outlined(&layer), [true, true], "{order:?}");
        assert_eq!(skipped(&layer), [], "{order:?}");
    }
    // An observation that cannot be converted is dropped, as the engine drops it.
    for order in [[narrow, past_the_file], [past_the_file, narrow]] {
        let layer = two_functions(&order);
        assert_eq!(outlined(&layer), [false, true], "{order:?}");
        assert_eq!(
            skipped(&layer),
            [("a.rs", "outline-outside-definition")],
            "{order:?}"
        );
    }
    // Without an enclosing range, only the nesting rules apply.
    let layer = two_functions(&[&[]]);
    assert_eq!(outlined(&layer), [true, true]);
}

/// `fn chain() { x.m().m()…; }` with `n` calls, each holding the one before it.
fn chain(n: usize) -> String {
    format!("fn chain() {{ x{}; }}\n", ".m()".repeat(n))
}

#[test]
fn an_outline_deeper_than_32_is_withheld_and_reported() {
    let temp = tempfile::tempdir().expect("a directory");
    let root = temp.path().canonicalize().expect("resolved");
    for (file, n) in [("a.rs", 70), ("b.rs", 32), ("c.rs", 33)] {
        std::fs::write(root.join(file), chain(n)).expect("write");
    }
    std::fs::write(root.join("d.rs"), "fn plain() { f(); }\n").expect("write");
    let index = index_of(
        ["a.rs", "b.rs", "c.rs", "d.rs"]
            .iter()
            .map(|f| document(f, vec![]))
            .collect(),
    );
    let layer = syntax::layer(&root, &index);
    let outlined: Vec<_> = layer
        .rows
        .iter()
        .map(|r| (r.anchor.file.as_str(), r.outline.is_some()))
        .collect();
    assert_eq!(
        outlined,
        [
            ("a.rs", false),
            ("b.rs", true),
            ("c.rs", false),
            ("d.rs", true)
        ]
    );
    assert_eq!(
        skipped(&layer),
        [("a.rs", "outline-too-deep"), ("c.rs", "outline-too-deep")]
    );
}

#[test]
fn item_position_macros_in_source_strings_go_to_the_innermost_owner() {
    let outlines = |source: &str| -> Vec<(u32, u32, Option<Value>)> {
        syntax::rows("a.rs", source)
            .expect("parses")
            .into_iter()
            .map(|r| {
                let o = r
                    .outline
                    .map(|o| serde_json::to_value(o).expect("serialisable"));
                (r.anchor.line, r.anchor.col.expect("a column"), o)
            })
            .collect()
    };
    // A trait inside a function owns its macro; the function does not.
    assert_eq!(
        outlines("fn f() { trait T { m! {} } }"),
        [
            (0, 3, Some(json!([]))),
            (0, 15, Some(json!([leaf("opaque", Some("m!"), 0, 19, 24)])))
        ]
    );
    // An impl block inside an inline module: the module.
    assert_eq!(
        outlines("mod a { impl S { m! {} } }"),
        [(0, 4, Some(json!([leaf("opaque", Some("m!"), 0, 17, 22)])))]
    );
    // An extern block at file scope: the file's module.
    assert_eq!(
        outlines("extern \"C\" { m!(); }"),
        [(0, 0, Some(json!([leaf("opaque", Some("m!"), 0, 13, 17)])))]
    );
    // The initializer of a constant has no outline, so its macro gives no node.
    assert_eq!(
        outlines("const X: () = { impl S { m! {} } };"),
        [(0, 6, None)]
    );
    assert_eq!(outlines("static Y: () = { m! {} };"), [(0, 7, None)]);
    // An impl block inside a function: the function, nested by position.
    assert_eq!(
        outlines("fn f() { if x { impl S { m! {} } } }"),
        [(
            0,
            3,
            Some(json!([node(
                "branch",
                Some("if"),
                0,
                9,
                34,
                None,
                vec![leaf("opaque", Some("m!"), 0, 25, 30)]
            )]))
        )]
    );
    // An item syn keeps as raw tokens, inside a body.
    assert_eq!(
        outlines("fn f() { macro m() {} }"),
        [(
            0,
            3,
            Some(json!([leaf("opaque", Some("verbatim"), 0, 9, 21)]))
        )]
    );
}

#[test]
fn extract_writes_the_rows_and_gives_the_result_entry() {
    use gosanja_schema::contract::{LayerStatus, SyntaxLine};
    let root = fixture();
    let out = tempfile::tempdir().expect("a directory");
    let entry =
        syntax::extract(&root, out.path(), &index(&root.join("index.scip"))).expect("written");
    assert_eq!(
        (entry.status, entry.diagnostics, entry.skipped),
        (LayerStatus::Ok, None, None)
    );
    let written: Vec<SyntaxLine> = std::fs::read_to_string(out.path().join("syntax.jsonl"))
        .expect("syntax.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).expect("a row"))
        .collect();
    let layer = flow_layer();
    let expected: Vec<_> = layer
        .rows
        .into_iter()
        .map(SyntaxLine::Fact)
        .chain(layer.tokens)
        .collect();
    assert_eq!(written, expected);
}

#[test]
fn a_local_definition_at_the_anchor_does_not_hide_the_extent() {
    // The engine makes no entity of a local symbol, so the function's narrow extent is the
    // one it checks the outline against.
    let temp = tempfile::tempdir().expect("a directory");
    let root = temp.path().canonicalize().expect("resolved");
    std::fs::write(root.join("a.rs"), "fn wide() { f(); }\n").expect("write");
    // Nor of a parameter, whose descriptor ends in `)`.
    for other in ["local 1", "s m p 1 wide().(x)"] {
        for order in [true, false] {
            let mut occurrences = vec![
                definition("s m p 1 wide().", [0, 3, 7], &[0, 0, 10]),
                definition(other, [0, 3, 7], &[0, 0, 18]),
            ];
            if !order {
                occurrences.reverse();
            }
            let layer = syntax::layer(&root, &index_of(vec![document("a.rs", occurrences)]));
            assert_eq!(layer.rows[0].outline, None, "{other} {order}");
            assert_eq!(
                skipped(&layer),
                [("a.rs", "outline-outside-definition")],
                "{other} {order}"
            );
        }
    }
}

#[test]
fn a_closure_s_parameter_and_return_types_are_part_of_its_outline() {
    let ty = |start| leaf("opaque", Some("ty!"), 0, start, start + 5);
    assert_eq!(
        outline(&only_row(
            "fn f() { let c = |v: ty!()| -> ty!() { id(v) }; }"
        )),
        json!([node(
            "closure",
            Some("closure"),
            0,
            17,
            46,
            None,
            vec![ty(21), ty(31), call(0, 39, 44, Some((39, 41)), vec![]),]
        )])
    );
    assert_eq!(
        outline(&only_row(
            "fn f() { let c = async |v: ty!()| -> ty!() { id(v) }; }"
        )),
        json!([node(
            "closure",
            Some("closure"),
            0,
            17,
            52,
            None,
            vec![ty(27), ty(37), call(0, 45, 50, Some((45, 47)), vec![]),]
        )])
    );
}

// The forms in which an index gives ranges: typed fields, single-line or multi-line, before
// the legacy integer arrays of three or four elements, for name and enclosing range alike.

fn single(line: i32, start: i32, end: i32) -> scip::types::SingleLineRange {
    scip::types::SingleLineRange {
        line,
        start_character: start,
        end_character: end,
        ..Default::default()
    }
}

fn multi(sl: i32, sc: i32, el: i32, ec: i32) -> scip::types::MultiLineRange {
    scip::types::MultiLineRange {
        start_line: sl,
        start_character: sc,
        end_line: el,
        end_character: ec,
        ..Default::default()
    }
}

/// The layer of one file `a.rs` with `text`, indexed by `occurrences`; whether the outline of
/// its first row was withheld for lying outside the extent.
fn withheld(text: &str, occurrences: Vec<scip::types::Occurrence>) -> bool {
    let temp = tempfile::tempdir().expect("a directory");
    let root = temp.path().canonicalize().expect("resolved");
    std::fs::write(root.join("a.rs"), text).expect("write");
    let layer = syntax::layer(&root, &index_of(vec![document("a.rs", occurrences)]));
    let out = layer.rows[0].outline.is_none();
    assert_eq!(
        skipped(&layer),
        if out {
            vec![("a.rs", "outline-outside-definition")]
        } else {
            vec![]
        }
    );
    out
}

#[test]
fn every_range_form_of_the_index_is_read_and_typed_fields_come_first() {
    use scip::types::occurrence::{Typed_enclosing_range, Typed_range};
    const ONE: &str = "fn wide() { f(); }\n"; // f() at 0:12-15
    const THREE: &str = "fn wide() {\n    f();\n}\n"; // f() at 1:4-7
    let symbol = "s m p 1 wide().";
    let typed = |name: Typed_range, enclosing: Option<Typed_enclosing_range>| {
        let mut o = definition(symbol, [0, 0, 0], &[]);
        o.range = Vec::new();
        o.typed_range = Some(name);
        o.typed_enclosing_range = enclosing;
        o
    };
    // Typed single-line name and enclosing range: the narrow extent counts.
    let narrow_one = Typed_enclosing_range::SingleLineEnclosingRange(single(0, 0, 10));
    assert!(withheld(
        ONE,
        vec![typed(
            Typed_range::SingleLineRange(single(0, 3, 7)),
            Some(narrow_one.clone())
        )]
    ));
    // Typed multi-line name and enclosing range.
    let narrow_three = Typed_enclosing_range::MultiLineEnclosingRange(multi(0, 0, 1, 4));
    assert!(withheld(
        THREE,
        vec![typed(
            Typed_range::MultiLineRange(multi(0, 3, 0, 7)),
            Some(narrow_three)
        )]
    ));
    let wide_three = Typed_enclosing_range::MultiLineEnclosingRange(multi(0, 0, 2, 1));
    assert!(!withheld(
        THREE,
        vec![typed(
            Typed_range::MultiLineRange(multi(0, 3, 0, 7)),
            Some(wide_three)
        )]
    ));
    // Legacy: three elements for the name, four for a multi-line enclosing range.
    assert!(withheld(
        THREE,
        vec![definition(symbol, [0, 3, 7], &[0, 0, 1, 4])]
    ));
    assert!(!withheld(
        THREE,
        vec![definition(symbol, [0, 3, 7], &[0, 0, 2, 1])]
    ));
    // Typed fields win over the legacy arrays of the same occurrence.
    let mut both = definition(symbol, [0, 3, 7], &[0, 0, 18]);
    both.typed_enclosing_range = Some(narrow_one);
    assert!(withheld(ONE, vec![both]));
    let mut both = definition(symbol, [1, 0, 2], &[0, 0, 10]);
    both.typed_range = Some(Typed_range::SingleLineRange(single(0, 3, 7)));
    assert!(
        withheld(ONE, vec![both]),
        "the typed name puts it at the anchor"
    );
}

#[test]
fn a_definition_without_an_enclosing_range_still_takes_part_in_the_lookup() {
    // Two symbols defined at the anchor make it ambiguous, as in the engine, even when one has
    // no enclosing range; then only the nesting rules apply.
    const ONE: &str = "fn wide() { f(); }\n";
    assert!(!withheld(
        ONE,
        vec![
            definition("s m p 1 wide().", [0, 3, 7], &[]),
            definition("s m p 1 other().", [0, 3, 7], &[0, 0, 10]),
        ]
    ));
}

#[test]
fn a_reference_at_the_anchor_is_not_a_definition() {
    const ONE: &str = "fn wide() { f(); }\n";
    let mut reference = definition("s m p 1 elsewhere().", [0, 3, 7], &[0, 0, 18]);
    reference.symbol_roles = 0;
    assert!(withheld(
        ONE,
        vec![
            definition("s m p 1 wide().", [0, 3, 7], &[0, 0, 10]),
            reference
        ]
    ));
}

#[test]
fn a_macro_with_a_name_after_the_bang_is_an_invocation_unless_it_is_macro_rules() {
    let rows = syntax::rows("a.rs", "foo! bar { }\n").expect("parses");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        (rows[0].anchor.line, rows[0].anchor.col),
        (0, Some(0)),
        "the file's module"
    );
    assert_eq!(
        outline(&rows[0]),
        json!([leaf("opaque", Some("foo!"), 0, 0, 12)])
    );
}

#[test]
fn patterns_and_types_kept_as_raw_tokens_are_verbatim_nodes() {
    // syn keeps a `box` pattern and a `dyn*` type as raw tokens.
    assert_eq!(
        outline(&only_row("fn f() { let box y = b; }")),
        json!([leaf("opaque", Some("verbatim"), 0, 13, 18)])
    );
    assert_eq!(
        outline(&only_row("fn f() { let p: dyn* T = x; }")),
        json!([leaf("opaque", Some("verbatim"), 0, 16, 22)])
    );
}
