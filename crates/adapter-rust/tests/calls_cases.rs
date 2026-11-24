//! Corroboration on a pinned compiler's two-function module and SCIP index.
use gosanja_adapter_rust::calls::{
    debug::{File, Module, RootFiles},
    rows::{Rows, rows},
};
use md5::{Digest, Md5};
use protobuf::Message;
use scip::types::Index;
use std::{collections::BTreeSet, fs};
use tempfile::tempdir;

const IR: &str = include_str!("../../../fixtures/ir/rows/rows.ll");
const SOURCE: &str = include_str!("../../../fixtures/ir/rows/src/lib.rs");
const INDEX: &[u8] = include_bytes!("../../../fixtures/ir/rows/index.scip");
const FILE: &str = "src/lib.rs";
const CALL: &str = "  %_0 = call i32 @_RNvCsjCVZgSmFKGc_4rows4leaf(), !dbg !19";
const SITE: &str = "!19 = !DILocation(line: 3, column: 5, scope: !17)";

fn checksum(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Md5::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(hex, "{byte:02x}").unwrap();
            hex
        })
}
fn index() -> Index {
    Index::parse_from_bytes(INDEX).unwrap()
}
fn run(ir: &str, source: &str, index: &Index) -> Rows {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join(FILE), source).unwrap();
    let ir = ir.replace("@ROOT@", dir.path().to_str().unwrap()).replace(
        "34cb2f2a1489a93bacc40e216e394fb0",
        &checksum(source.as_bytes()),
    );
    let module = Module::read(&ir).unwrap();
    rows(
        &[(&module, "build")],
        index,
        &mut RootFiles::new(dir.path()),
        &BTreeSet::from(["rows".into()]),
    )
}
fn check(result: &Rows, count: usize, code: Option<&str>) {
    assert_eq!(result.evidence.len(), count, "{result:?}");
    let actual: Vec<_> = result
        .skipped
        .iter()
        .map(|s| (s.file.as_str(), s.code.as_str()))
        .collect();
    assert_eq!(
        actual,
        code.map(|c| (FILE, c)).into_iter().collect::<Vec<_>>()
    );
}
#[test]
fn direct_call_has_written_anchors_and_site() {
    let result = run(IR, SOURCE, &index());
    check(&result, 1, None);
    let row = &result.evidence[0];
    assert_eq!(
        (&*row.caller.file, row.caller.line, row.caller.col),
        (FILE, 1, Some(7))
    );
    assert_eq!(
        (&*row.callee.file, row.callee.line, row.callee.col),
        (FILE, 0, Some(7))
    );
    let site = row.site.as_ref().unwrap();
    assert_eq!((&*site.file, site.line, site.col), (FILE, 2, Some(4)));
}
#[test]
fn debug_locations_and_targets_determine_the_gap() {
    let cases = [
        (
            IR.replace("line: 3, column: 5", "line: 0, column: 5"),
            Some("no-debug-location"),
        ),
        (
            IR.replace(CALL, &CALL.replace(", !dbg !19", "")),
            Some("no-debug-location"),
        ),
        (
            IR.replace(CALL, &CALL.replace("@_RNvCsjCVZgSmFKGc_4rows4leaf", "%ptr")),
            Some("indirect-call"),
        ),
        (IR.replace(CALL, "  %_0 = call i32 %ptr()"), None),
        (IR.replace(" #0 !dbg !17 !guid", " #0 !guid"), None),
        (
            IR.replace("filename: \"src/lib.rs\"", "filename: \"outside.rs\""),
            None,
        ),
        (
            IR.replace("line: 3, column: 5", "line: 3, column: 0"),
            Some("uncorroborated"),
        ),
        (
            IR.replace(SITE, &SITE.replace("scope: !17", "scope: !8")),
            Some("uncorroborated"),
        ),
    ];
    for (ir, gap) in cases {
        check(&run(&ir, SOURCE, &index()), 0, gap);
    }
}
#[test]
fn instruction_count_uses_distinct_written_references() {
    let ir = IR.replace(CALL, &format!("{CALL}\n{}", CALL.replace("%_0", "%second")));
    let mut idx = index();
    let doc = &mut idx.documents[0];
    let repeated = doc.occurrences.last().unwrap().clone();
    doc.occurrences.push(repeated);
    check(&run(&ir, SOURCE, &idx), 0, Some("uncorroborated"));
    check(&run(IR, SOURCE, &idx), 1, None);
    check(
        &run(IR, &SOURCE.replace("leaf()\n", "leaf  \n"), &idx),
        0,
        Some("uncorroborated"),
    );
    check(
        &run(
            IR,
            &SOURCE.replace("leaf()\n", "leaf::<Vec<Vec<u32>>>()\n"),
            &idx,
        ),
        1,
        None,
    );
}
#[test]
fn unreadable_source_reports_parse_failed() {
    for text in [format!("\u{feff}{SOURCE}"), format!("{SOURCE} fn {{")] {
        check(&run(IR, &text, &index()), 0, Some("parse-failed"));
    }
}
#[test]
fn definition_and_reference_must_both_be_indexed() {
    for role in [0, 1] {
        let mut idx = index();
        idx.documents[0]
            .occurrences
            .retain(|o| !o.symbol.ends_with("leaf().") || o.symbol_roles & 1 != role);
        check(&run(IR, SOURCE, &idx), 0, Some("uncorroborated"));
    }
}
#[test]
fn root_records_require_matching_bytes_and_carriable_paths() {
    let dir = tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut files = RootFiles::new(&root);
    let base = File {
        directory: root.display().to_string(),
        filename: String::new(),
        md5: Some(checksum(b"text")),
    };
    fs::create_dir(root.join("@")).unwrap();
    for name in ["@/file.rs", "a:b.rs", "extensionless"] {
        fs::write(root.join(name), b"text").unwrap();
        let f = File {
            filename: name.into(),
            ..base.clone()
        };
        assert_eq!(files.get(&f).unwrap().path, name);
        assert!(
            files
                .get(&File {
                    md5: None,
                    ..f.clone()
                })
                .is_none()
        );
        assert!(
            files
                .get(&File {
                    md5: Some("0".repeat(32)),
                    ..f.clone()
                })
                .is_none()
        );
        assert!(
            files
                .get(&File {
                    directory: "relative".into(),
                    ..f
                })
                .is_none()
        );
    }
    for name in [
        "<unknown>",
        "missing",
        "a:b.rs/@/hash",
        r"C:\src\lib.rs",
        r"back\slash",
    ] {
        if name.contains('\\') && cfg!(unix) {
            fs::write(root.join(name), b"text").unwrap();
        }
        assert!(
            files
                .get(&File {
                    filename: name.into(),
                    ..base.clone()
                })
                .is_none()
        );
    }
    #[cfg(unix)]
    {
        let outside = tempdir().unwrap();
        let path = outside.path().join("real");
        fs::write(&path, b"text").unwrap();
        std::os::unix::fs::symlink(path, root.join("link")).unwrap();
        assert!(
            files
                .get(&File {
                    filename: "link".into(),
                    ..base
                })
                .is_none()
        );
    }
}

// Move the three real SCIP occurrences to the corresponding tokens in a source variant.
// A marker before the call token allows the IR column to vary separately from its index range.
fn variant(source: &str, scope: &str) -> Rows {
    let (ir, source, idx) = variant_inputs(source, scope);
    run(&ir, &source, &idx)
}
fn variant_inputs(source: &str, scope: &str) -> (String, String, Index) {
    let marked = source.find('§').unwrap();
    let source = source.replace('§', "");
    let point = |offset: usize| {
        let prefix = &source[..offset];
        (
            i32::try_from(prefix.bytes().filter(|b| *b == b'\n').count()).unwrap(),
            i32::try_from(prefix.rsplit('\n').next().unwrap().len()).unwrap(),
        )
    };
    let (line, column) = point(marked);
    let mut ir = IR.replace(
        SITE,
        &format!(
            "!19 = !DILocation(line: {}, column: {}, scope: !17)",
            line + 1,
            column + 1
        ),
    );
    let mut idx = index();
    let doc = &mut idx.documents[0];
    doc.occurrences.retain(|o| o.symbol.ends_with("()."));
    for occ in &mut doc.occurrences {
        let is_def = occ.symbol_roles & 1 != 0;
        let caller = occ.symbol.ends_with("caller().");
        let name = if caller { "caller" } else { "leaf" };
        let offset = if is_def {
            source.find(&format!("fn {name}")).unwrap() + 3
        } else {
            source[marked..].find("leaf").map_or(marked, |n| marked + n)
        };
        let (l, c) = point(offset);
        let width = if source[offset..].starts_with(name) {
            i32::try_from(name.len()).unwrap()
        } else {
            1
        };
        occ.typed_range = None;
        occ.typed_enclosing_range = None;
        occ.range = vec![l, c, c + width];
        occ.enclosing_range.clear();
        if is_def {
            let (el, ec) = if caller {
                point(source.trim_end().len())
            } else {
                point(source[offset..].find('}').unwrap() + offset + 1)
            };
            occ.enclosing_range = vec![l, 0, el, ec];
            let id = if caller { "!17 = " } else { "!8 = " };
            let record = ir
                .lines()
                .find(|line| line.starts_with(id))
                .unwrap()
                .to_owned();
            let original = if caller { 2 } else { 1 };
            let at = if caller && scope.starts_with('{') {
                line + 1
            } else {
                l + 1
            };
            let mut new = record.replace(&format!("line: {original},"), &format!("line: {at},"));
            if caller {
                new = new.replace("name: \"caller\"", &format!("name: \"{scope}\""));
            }
            ir = ir.replace(&record, &new);
        }
    }
    (ir, source, idx)
}
#[test]
fn written_attributes_and_signatures_control_both_ends() {
    for (before, after, allowed) in [
        ("#[inline]", "", true),
        ("#[cfg(unix)]", "", true),
        ("#[unsafe(no_mangle)]", "", true),
        ("#[cfg_attr(unix, inline, allow(dead_code))]", "", true),
        ("#[test]", "", false),
        ("#[rustfmt::skip]", "", false),
        ("#[user_attribute]", "", false),
        ("#[cfg_attr(any(), cfg_attr(unix, generate))]", "", false),
        ("#[generate] impl T {", "}", false),
        ("fn other() {} ", "", false),
        ("m!(); ", "", false),
        ("macro_rules! m { () => {} } ", "", false),
        ("#[derive(Clone)] struct T; ", "", false),
        ("#[derive(Clone)]\nstruct T { field:u8 } ", "", false),
    ] {
        let text = format!(
            "pub fn leaf() -> u32 {{ 1 }}\n{before}fn caller() {{\n    §leaf();\n}}{after}"
        );
        check(
            &variant(&text, "caller"),
            usize::from(allowed),
            (!allowed).then_some("uncorroborated"),
        );
    }
}
#[test]
fn source_item_owns_site_and_anonymous_scope() {
    for (body, scope, allowed) in [
        ("m!(§leaf())", "caller", true),
        ("let f = || §leaf();", "{closure#0}", true),
        ("let f = async { §leaf() };", "{async_block#0}", true),
        ("let f = async || §leaf();", "{async_closure#0}", true),
        ("m!(|| §leaf());", "{closure#0}", false),
        ("fn inner() { §leaf(); }", "caller", false),
        ("const C:u32 = §leaf();", "caller", false),
        ("static S:u32 = §leaf();", "caller", false),
        ("macro_rules! m { () => { §leaf() } }", "caller", false),
        (
            "#[derive(Clone)]\nstruct T { a: [u32; §leaf()] }",
            "caller",
            false,
        ),
        ("fn inner() {} let f = || §leaf();", "{closure#0}", false),
        ("let x = §leaf::<u8>();", "caller<u8>", true),
        ("let x = 1 §+ 2;", "caller", true),
    ] {
        let text = format!("pub fn leaf() -> u32 {{ 1 }}\nfn caller() {{\n{body}\n}}");
        check(
            &variant(&text, scope),
            usize::from(allowed),
            (!allowed).then_some("uncorroborated"),
        );
    }
}
#[test]
fn utf8_debug_and_index_columns_become_character_columns() {
    let result = variant(
        "/*한*/ fn leaf() {}\n/*é*/ fn caller() {\n/*한글*/ §leaf();\n}",
        "caller",
    );
    check(&result, 1, None);
    let row = &result.evidence[0];
    assert_eq!(
        (
            row.caller.col,
            row.callee.col,
            row.site.as_ref().unwrap().col
        ),
        (Some(9), Some(9), Some(7))
    );
}
#[test]
fn only_declarations_in_member_paths_get_a_gap() {
    let start = IR.find("define i32").unwrap();
    let end = IR[start..].find("\n}").unwrap() + start + 2;
    for (target, member) in [
        ("_RNvCsjCVZgSmFKGc_4rows4leaf", true),
        ("_RNvMC4rowsNtB2_1T4leaf", true),
        (
            "_RNvXCsjCVZgSmFKGc_4rowsINtCsjhmpXLHu1Ea_7outside4WrapIBm_hEENtB2_5Local4work",
            true,
        ),
        (
            "_RNvXs0_CsjCVZgSmFKGc_4rowsRNtB5_4ItemNtCsjhmpXLHu1Ea_7outside8External4work",
            true,
        ),
        ("_RNvXs1_CsjCVZgSmFKGc_4rowsmNtB5_5Local4work", true),
        (
            "_RNvXs_CsjCVZgSmFKGc_4rowsNtB4_4ItemNtCsjhmpXLHu1Ea_7outside8External4work",
            true,
        ),
        (
            "_RNvXs2_CsjCVZgSmFKGc_4rowsQINtCsjhmpXLHu1Ea_7outside4WraptENtB5_5Local4work",
            true,
        ),
        ("_RNvXs5_CsjCVZgSmFKGc_4rowsFEmNtB5_5Local4work", true),
        // The receiver's generic argument is a type local to a different trait impl.
        (
            "_RNvXNvXCsjCVZgSmFKGc_4rowsNtB5_4ItemNtCsjhmpXLHu1Ea_7outside8External4workINtBA_4WrapNtB2_6NestedENtB5_5Local2go",
            true,
        ),
        ("_RNvC5other4leaf", false),
        ("_RINvC5other4leafNtC4rows1TE", false),
        ("_RNvXC4rowsmINtC5other4FromNtB2_1TE4from", false),
        ("_ZN4rows4leafE", false),
        ("_Read", false),
    ] {
        let ir = IR
            .replace(&IR[start..end], &format!("declare i32 @{target}()"))
            .replace(CALL, &CALL.replace("_RNvCsjCVZgSmFKGc_4rows4leaf", target));
        check(
            &run(&ir, SOURCE, &index()),
            0,
            member.then_some("callee-unresolved"),
        );
    }
}

#[test]
fn repeated_and_conflicting_definition_places() {
    for end in ["caller().", "leaf()."] {
        for conflict in [false, true] {
            let mut idx = index();
            let doc = &mut idx.documents[0];
            let mut other = doc
                .occurrences
                .iter()
                .find(|o| o.symbol_roles & 1 != 0 && o.symbol.ends_with(end))
                .unwrap()
                .clone();
            if conflict {
                let mut info = doc
                    .symbols
                    .iter()
                    .find(|s| s.symbol == other.symbol)
                    .unwrap()
                    .clone();
                other.symbol.push_str("other.");
                info.symbol.clone_from(&other.symbol);
                "other".clone_into(&mut info.display_name);
                doc.symbols.push(info);
            }
            doc.occurrences.push(other);
            check(
                &run(IR, SOURCE, &idx),
                usize::from(!conflict),
                conflict.then_some("uncorroborated"),
            );
        }
    }
    for reverse in [false, true] {
        let mut idx = index();
        let doc = &mut idx.documents[0];
        let mut caller = doc
            .occurrences
            .iter()
            .find(|o| o.symbol.ends_with("caller()."))
            .unwrap()
            .clone();
        caller.enclosing_range.clear();
        caller.typed_enclosing_range = None;
        doc.occurrences.push(caller);
        if reverse {
            doc.occurrences.reverse();
        }
        check(&run(IR, SOURCE, &idx), 1, None);
    }
}
#[test]
fn occurrence_ranges_must_be_valid_in_the_preferred_encoding() {
    use scip::types::{SingleLineRange, occurrence::Typed_range};
    for (typed, legacy, ok) in [(7, 50, true), (50, 7, false)] {
        let mut idx = index();
        let def = idx.documents[0]
            .occurrences
            .iter_mut()
            .find(|o| o.symbol_roles & 1 != 0 && o.symbol.ends_with("leaf()."))
            .unwrap();
        def.typed_range = Some(Typed_range::SingleLineRange(SingleLineRange {
            line: 0,
            start_character: typed,
            end_character: typed + 4,
            ..Default::default()
        }));
        def.range = vec![0, legacy, legacy + 4];
        check(
            &run(IR, SOURCE, &idx),
            usize::from(ok),
            (!ok).then_some("uncorroborated"),
        );
    }
    let mut idx = index();
    let doc = &mut idx.documents[0];
    let def = doc
        .occurrences
        .iter_mut()
        .find(|o| o.symbol_roles & 1 != 0 && o.symbol.ends_with("leaf()."))
        .unwrap();
    let valid = def.clone();
    def.typed_enclosing_range = None;
    def.enclosing_range = vec![
        0,
        0,
        i32::try_from(SOURCE.lines().next().unwrap().len() + 1).unwrap(),
    ];
    check(&run(IR, SOURCE, &idx), 0, Some("uncorroborated"));
    idx.documents[0].occurrences.push(valid);
    check(&run(IR, SOURCE, &idx), 1, None);
}
#[test]
fn different_documents_and_parse_error_precedence() {
    for (site, callee, expected) in [
        (SOURCE, SOURCE, None),
        (SOURCE, "\u{feff}", Some("callee.rs")),
        ("\u{feff}", "\u{feff}", Some(FILE)),
    ] {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join(FILE), site).unwrap();
        fs::write(dir.path().join("callee.rs"), callee).unwrap();
        let mut idx = index();
        let mut other = idx.documents[0].clone();
        other.relative_path = "callee.rs".into();
        idx.documents.push(other);
        let entry = IR.lines().find(|s| s.starts_with("!8 = ")).unwrap();
        let ir=format!("{}\n!21 = !DIFile(filename: \"callee.rs\", directory: \"@ROOT@\", checksumkind: CSK_MD5, checksum: \"{}\")\n",
            IR.replace(entry,&entry.replace("file: !9","file: !21")).replace("34cb2f2a1489a93bacc40e216e394fb0",&checksum(site.as_bytes())),checksum(callee.as_bytes())).replace("@ROOT@",dir.path().to_str().unwrap());
        let m = Module::read(&ir).unwrap();
        let result = rows(
            &[(&m, "z"), (&m, "a"), (&m, "a")],
            &idx,
            &mut RootFiles::new(dir.path()),
            &BTreeSet::new(),
        );
        if let Some(file) = expected {
            assert_eq!(result.evidence.len(), 0);
            assert_eq!(result.skipped.len(), 1);
            assert_eq!(
                (&*result.skipped[0].file, &*result.skipped[0].code),
                (file, "parse-failed")
            );
        } else {
            check(&result, 3, None);
            assert_eq!(
                result
                    .evidence
                    .iter()
                    .map(|e| e.build.as_str())
                    .collect::<Vec<_>>(),
                ["a", "a", "z"]
            );
            assert!(
                result
                    .evidence
                    .iter()
                    .all(|e| e.caller.file == FILE && e.callee.file == "callee.rs")
            );
        }
    }
}

#[test]
fn references_must_have_a_unique_innermost_enclosing_function() {
    let mut idx = index();
    let doc = &mut idx.documents[0];
    let mut outer = doc
        .occurrences
        .iter()
        .find(|o| o.symbol.ends_with("caller()."))
        .unwrap()
        .clone();
    let mut info = doc
        .symbols
        .iter()
        .find(|s| s.symbol == outer.symbol)
        .unwrap()
        .clone();
    info.symbol.push_str("outer().");
    "outer".clone_into(&mut info.display_name);
    outer.symbol.clone_from(&info.symbol);
    outer.typed_range = None;
    outer.range = vec![0, 0, 3];
    outer.typed_enclosing_range = None;
    outer.enclosing_range = vec![0, 0, 3, 1];
    doc.symbols.push(info);
    doc.occurrences.push(outer);
    check(&run(IR, SOURCE, &idx), 1, None);
    idx.documents[0]
        .occurrences
        .last_mut()
        .unwrap()
        .enclosing_range = vec![0, 0, 2, 8];
    check(&run(IR, SOURCE, &idx), 0, Some("uncorroborated"));
}
#[test]
fn two_instructions_with_two_references_in_one_body_are_both_kept() {
    let source = SOURCE.replace("    leaf()", "    leaf()+leaf()");
    let mut idx = index();
    let doc = &mut idx.documents[0];
    let mut second = doc
        .occurrences
        .iter()
        .find(|o| o.symbol.ends_with("leaf().") && o.symbol_roles & 1 == 0)
        .unwrap()
        .clone();
    second.typed_range = None;
    second.range = vec![2, 11, 15];
    doc.occurrences.push(second);
    let ir = IR.replace(
        CALL,
        &format!(
            "{CALL}\n{}",
            CALL.replace("%_0", "%second").replace("!19", "!21")
        ),
    ) + "\n!21 = !DILocation(line: 3, column: 12, scope: !17)\n";
    let result = run(&ir, &source, &idx);
    check(&result, 2, None);
    let mut columns: Vec<_> = result
        .evidence
        .iter()
        .map(|r| r.site.as_ref().unwrap().col.unwrap())
        .collect();
    columns.sort_unstable();
    assert_eq!(columns, [4, 11]);
}
#[test]
fn positions_within_utf8_characters_and_outside_the_owner_are_rejected() {
    let text = SOURCE.replace("    leaf()", "é; leaf()");
    // Four bytes precede leaf in both versions. Byte column two splits the first character.
    check(
        &run(
            &IR.replace(SITE, &SITE.replace("column: 5", "column: 2")),
            &text,
            &index(),
        ),
        0,
        Some("uncorroborated"),
    );
    let mut idx = index();
    let leaf = idx.documents[0]
        .occurrences
        .iter_mut()
        .find(|o| o.symbol_roles & 1 != 0 && o.symbol.ends_with("leaf()."))
        .unwrap();
    leaf.typed_range = None;
    leaf.range = vec![0, 3, 6];
    check(
        &run(IR, &SOURCE.replacen("pub", "/*é*/", 1), &idx),
        0,
        Some("uncorroborated"),
    );
    let text = SOURCE.replace("    leaf()\n}", "    leaf() } ");
    let mut idx = index();
    let caller = idx.documents[0]
        .occurrences
        .iter_mut()
        .find(|o| o.symbol.ends_with("caller()."))
        .unwrap();
    caller.typed_enclosing_range = None;
    caller.enclosing_range = vec![1, 0, 2, 12];
    check(
        &run(
            &IR.replace(SITE, &SITE.replace("column: 5", "column: 14")),
            &text,
            &idx,
        ),
        0,
        Some("uncorroborated"),
    );
}
#[test]
fn generic_instances_inlined_sites_and_namespace_aliases_keep_source_identity() {
    let definition = IR
        .lines()
        .find(|s| s.starts_with("define i32") && s.contains("6caller"))
        .unwrap();
    let sub = IR.lines().find(|s| s.starts_with("!17 =")).unwrap();
    let instance = definition
        .replace("6caller", "7caller2")
        .replace("!17", "!21");
    let ir = IR.replace(
        sub,
        &sub.replace("name: \"caller\"", "name: \"caller<u8>\""),
    ) + &format!(
        "\n{instance}\nstart:\n{}\n  ret i32 %_0\n}}\n{}\n!22 = !DILocation(line: 3, column: 5, scope: !21)\n",
        CALL.replace("!19", "!22"),
        sub.replace("!17 =", "!21 =")
            .replace("6caller", "7caller2")
            .replace("name: \"caller\"", "name: \"caller<char>\"")
    );
    check(&run(&ir, SOURCE, &index()), 2, None);
    let ir = IR
        .replace("name: \"rows\"", "name: \"different_module\"")
        .replace(
            "!19 = !DILocation(line: 3, column: 5, scope: !17)",
            "!19 = !DILocation(line: 3, column: 5, scope: !17, inlinedAt: !16)",
        )
        .replace(definition, &definition.replace("!dbg !17", "!dbg !8"));
    let result = run(&ir, SOURCE, &index());
    check(&result, 1, None);
    assert_eq!(result.evidence[0].caller.line, 1);
}

#[test]
fn closure_scope_cannot_borrow_a_site_in_the_outer_function() {
    let source = "pub fn leaf() -> u32 { 7 }\nfn caller() {\n  fn inner() {\n    let c = || 0;\n  }\n  §leaf();\n}";
    // The fixture's reader receives the outer site but the nested function's closure line.
    let base = source.replace('§', "");
    let mut idx = index();
    let caller = idx.documents[0]
        .occurrences
        .iter_mut()
        .find(|o| o.symbol.ends_with("caller()."))
        .unwrap();
    caller.typed_range = None;
    caller.range = vec![1, 3, 9];
    caller.typed_enclosing_range = None;
    caller.enclosing_range = vec![1, 0, 6, 1];
    let call = idx.documents[0]
        .occurrences
        .iter_mut()
        .find(|o| o.symbol.ends_with("leaf().") && o.symbol_roles & 1 == 0)
        .unwrap();
    call.typed_range = None;
    call.range = vec![5, 2, 6];
    let sp = IR.lines().find(|s| s.starts_with("!17 =")).unwrap();
    let ir = IR
        .replace(
            sp,
            &sp.replace("name: \"caller\"", "name: \"{closure#0}\"")
                .replace("line: 2", "line: 4"),
        )
        .replace(
            SITE,
            &SITE.replace("line: 3, column: 5", "line: 6, column: 3"),
        );
    check(&run(&ir, &base, &idx), 0, Some("uncorroborated"));
}
#[test]
fn inner_attributes_and_impl_methods_use_their_written_extent() {
    for (text, allowed) in [
        (
            "pub fn leaf() {}\nimpl T { #![generate] fn caller() {\n §leaf();\n} }",
            false,
        ),
        (
            "pub fn leaf() {}\nimpl Trait for u32 { fn caller() {\n §leaf();\n} }",
            true,
        ),
        (
            "pub fn leaf() {}\nfn outer() {\n fn caller() {\n §leaf();\n} }",
            true,
        ),
    ] {
        check(
            &variant(text, "caller"),
            usize::from(allowed),
            (!allowed).then_some("uncorroborated"),
        );
    }
}

#[test]
fn a_distinct_callee_with_the_same_display_name_on_one_line_is_ambiguous() {
    let mut idx = index();
    let doc = &mut idx.documents[0];
    let mut other = doc
        .occurrences
        .iter()
        .find(|o| o.symbol_roles & 1 != 0 && o.symbol.ends_with("leaf()."))
        .unwrap()
        .clone();
    let mut info = doc
        .symbols
        .iter()
        .find(|s| s.symbol == other.symbol)
        .unwrap()
        .clone();
    info.symbol.push_str("leaf().");
    other.symbol.clone_from(&info.symbol);
    other.typed_range = None;
    other.range = vec![0, 1, 5];
    doc.symbols.push(info);
    doc.occurrences.push(other);
    check(&run(IR, SOURCE, &idx), 0, Some("uncorroborated"));
}
#[test]
fn references_in_different_innermost_functions_cannot_share_an_instruction() {
    let text = SOURCE.replace("    leaf()", "    fn inner() { leaf(); } leaf()");
    let mut idx = index();
    let doc = &mut idx.documents[0];
    let mut nested = doc
        .occurrences
        .iter()
        .find(|o| o.symbol.ends_with("caller()."))
        .unwrap()
        .clone();
    let mut info = doc
        .symbols
        .iter()
        .find(|s| s.symbol == nested.symbol)
        .unwrap()
        .clone();
    info.symbol.push_str("inner().");
    "inner".clone_into(&mut info.display_name);
    nested.symbol.clone_from(&info.symbol);
    nested.typed_range = None;
    nested.range = vec![2, 7, 12];
    nested.typed_enclosing_range = None;
    nested.enclosing_range = vec![2, 4, 26];
    let call = doc
        .occurrences
        .iter_mut()
        .find(|o| o.symbol_roles & 1 == 0 && o.symbol.ends_with("leaf()."))
        .unwrap();
    call.typed_range = None;
    call.range = vec![2, 17, 21];
    let mut other = call.clone();
    other.range = vec![2, 27, 31];
    doc.occurrences.extend([nested, other]);
    doc.symbols.push(info);
    check(
        &run(
            &IR.replace(SITE, &SITE.replace("column: 5", "column: 28")),
            &text,
            &idx,
        ),
        0,
        Some("uncorroborated"),
    );
}
#[test]
fn admission_of_the_site_and_callee_is_independent() {
    for prefix in ["!8 =", "!17 ="] {
        let sp = IR.lines().find(|s| s.starts_with(prefix)).unwrap();
        let ir = IR.replace(sp, &sp.replace("file: !9", "file: !21"))
            + "\n!21 = !DIFile(filename: \"absent.rs\", directory: \"@ROOT@\", checksumkind: CSK_MD5, checksum: \"34cb2f2a1489a93bacc40e216e394fb0\")\n";
        check(&run(&ir, SOURCE, &index()), 0, None);
    }
}

#[test]
fn indexed_columns_use_each_documents_declared_unit() {
    use scip::types::PositionEncoding::*;
    let (ir, source, base) = variant_inputs(
        "/*🐈*/ fn leaf() {}\n/*🐈*/ fn caller() {\n/*🐈*/ §leaf();\n}",
        "caller",
    );
    for (encoding, shorter) in [
        (UTF8CodeUnitOffsetFromLineStart, 0),
        (UTF16CodeUnitOffsetFromLineStart, 2),
        (UTF32CodeUnitOffsetFromLineStart, 3),
    ] {
        let mut idx = base.clone();
        let doc = &mut idx.documents[0];
        doc.position_encoding = encoding.into();
        for o in &mut doc.occurrences {
            for col in &mut o.range[1..] {
                *col -= shorter;
            }
            if o.symbol_roles & 1 != 0 && o.enclosing_range[2] == 0 {
                o.enclosing_range[3] -= shorter;
            }
        }
        let result = run(&ir, &source, &idx);
        check(&result, 1, None);
        assert_eq!(result.evidence[0].site.as_ref().unwrap().col, Some(6));
        assert_eq!(result.evidence[0].callee.col, Some(9));
    }
    let mut idx = base;
    idx.documents[0].position_encoding = UnspecifiedPositionEncoding.into();
    check(&run(&ir, &source, &idx), 0, Some("uncorroborated"));
}
#[test]
fn lookup_uses_descriptor_and_deterministic_kind() {
    use scip::types::symbol_information::Kind;
    let mut idx = index();
    let doc = &mut idx.documents[0];
    for info in &mut doc.symbols {
        "unused display".clone_into(&mut info.display_name);
    }
    let mut duplicate = doc
        .symbols
        .iter()
        .find(|s| s.symbol.ends_with("caller()."))
        .unwrap()
        .clone();
    duplicate.kind = Kind::Variable.into();
    doc.symbols.push(duplicate);
    check(&run(IR, SOURCE, &idx), 1, None);
    idx.documents[0].symbols.reverse();
    check(&run(IR, SOURCE, &idx), 1, None);
    let call = idx.documents[0]
        .occurrences
        .iter_mut()
        .find(|o| o.symbol_roles & 1 == 0 && o.symbol.ends_with("leaf()."))
        .unwrap();
    call.symbol = call.symbol.replace("leaf().", "caller().");
    check(&run(IR, SOURCE, &idx), 0, Some("uncorroborated"));
}
#[test]
fn written_checks_apply_to_the_callee_and_the_closure_scope() {
    for (text, scope) in [
        (
            "#[generated]\nfn leaf(){}\nfn caller(){\n §leaf();\n}",
            "caller",
        ),
        (
            "#![generated]\nfn leaf(){}\nfn caller(){\n §leaf();\n}",
            "caller",
        ),
    ] {
        check(&variant(text, scope), 0, Some("uncorroborated"));
    }
    for source in [
        "pub fn leaf(){}\nfn caller(){\n  macro_rules! saved { () => {} }\n  §leaf();\n}",
        "pub fn leaf(){}\nfn caller(){\n  #[derive(Clone)] struct T;\n  §leaf();\n}",
    ] {
        let (ir, text, idx) = variant_inputs(source, "{closure#0}");
        check(&run(&ir, &text, &idx), 1, None);
        let sub = ir.lines().find(|s| s.starts_with("!17 =")).unwrap();
        check(
            &run(
                &ir.replace(sub, &sub.replace("line: 4", "line: 3")),
                &text,
                &idx,
            ),
            0,
            Some("uncorroborated"),
        );
    }
    let sp = IR.lines().find(|s| s.starts_with("!17 =")).unwrap();
    for bad in [
        sp.replace("line: 2", "line: 1"),
        sp.replace("name: \"caller\"", "name: \"leaf\""),
    ] {
        check(
            &run(&IR.replace(sp, &bad), SOURCE, &index()),
            0,
            Some("uncorroborated"),
        );
    }
}
#[test]
fn checksummed_compile_unit_suffix_is_not_stripped() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("code/@");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("unit"), b"source").unwrap();
    let ir = IR
        .replace("src/lib.rs/@/rows.e49e8bfc74d1a7ca-cgu.0", "code/@/unit")
        .replace("file: !9, line: 1", "file: !7, line: 1")
        .replace("@ROOT@", dir.path().to_str().unwrap());
    let module = Module::read(&ir).unwrap();
    let (_, unit) = module
        .files()
        .find(|(_, file)| file.filename == "code/@/unit")
        .unwrap();
    let mut files = RootFiles::new(dir.path());
    assert!(files.get(unit).is_none());
    assert_eq!(
        files
            .get(&File {
                md5: Some(checksum(b"source")),
                ..unit.clone()
            })
            .unwrap()
            .path,
        "code/@/unit"
    );
}
#[test]
fn an_innermost_function_does_not_disambiguate_equal_outer_extents() {
    let mut idx = index();
    let doc = &mut idx.documents[0];
    let caller = doc
        .occurrences
        .iter()
        .find(|o| o.symbol.ends_with("caller()."))
        .unwrap()
        .clone();
    let info = doc
        .symbols
        .iter()
        .find(|s| s.symbol == caller.symbol)
        .unwrap()
        .clone();
    for (i, name) in ["outer", "other"].iter().enumerate() {
        let mut symbol = info.clone();
        symbol.symbol = symbol.symbol.replace("caller", name);
        let mut def = caller.clone();
        def.symbol.clone_from(&symbol.symbol);
        def.typed_range = None;
        def.typed_enclosing_range = None;
        let col = i32::try_from(i).unwrap();
        def.range = vec![0, col, col + 1];
        def.enclosing_range = vec![0, 0, 3, 1];
        doc.occurrences.push(def);
        doc.symbols.push(symbol);
    }
    check(&run(IR, SOURCE, &idx), 0, Some("uncorroborated"));
}

#[test]
fn an_inlined_location_must_name_the_written_callers_scope_file() {
    let caller = IR.lines().find(|s| s.starts_with("!17 =")).unwrap();
    let module = IR
        .replace(caller, &caller.replace("file: !9", "file: !23"))
        .replace(SITE, &SITE.replace("scope: !17", "scope: !24"))
        + "\n!23 = !DIFile(filename: \"other.rs\", directory: \"@ROOT@\")\n!24 = !DILexicalBlockFile(scope: !17, file: !9, discriminator: 0)\n";
    check(&run(&module, SOURCE, &index()), 0, Some("uncorroborated"));
}

#[test]
fn generic_callee_instances_resolve_to_one_written_method() {
    let (ir, source, idx) = variant_inputs(
        "struct T; impl T { fn leaf<U>() {} }\nfn caller() {\n T::§leaf::<u8>();\n}",
        "caller",
    );
    let sp = ir.lines().find(|s| s.starts_with("!8 =")).unwrap();
    for ty in ["u8", "char"] {
        let module = ir.replace(
            sp,
            &sp.replace("name: \"leaf\"", &format!("name: \"leaf<{ty}>\"")),
        );
        let result = run(&module, &source, &idx);
        check(&result, 1, None);
        assert_eq!(result.evidence[0].callee.col, Some(22));
    }
}

#[test]
fn instruction_positions_at_the_end_of_an_item_are_outside_it() {
    for (line, ok) in [(" leaf(); }", false), (" fn nested() {} leaf();", true)] {
        let source = format!(
            "pub fn leaf() {{}}\nfn caller() {{\n{line}{}",
            if ok { "\n}" } else { "" }
        );
        let marked = source.replacen("leaf();", "§leaf();", 1);
        let (ir, source, idx) = variant_inputs(&marked, "caller");
        let loc = ir.lines().find(|s| s.starts_with("!19 =")).unwrap();
        let col = if ok {
            line.find('}').unwrap() + 2
        } else {
            line.len() + 1
        };
        let module = ir.replace(
            loc,
            &format!("!19 = !DILocation(line: 3, column: {col}, scope: !17)"),
        );
        check(
            &run(&module, &source, &idx),
            usize::from(ok),
            (!ok).then_some("uncorroborated"),
        );
    }
}

fn attribute_case(source: &str, kept: bool) {
    let rows = variant(source, "caller");
    check(
        &rows,
        usize::from(kept),
        (!kept).then_some("uncorroborated"),
    );
    if kept {
        let row = &rows.evidence[0];
        let point = |before: &str, after: usize| {
            let prefix = &source[..source.find(before).unwrap() + after];
            let line = u32::try_from(prefix.matches('\n').count()).unwrap();
            let column = u32::try_from(prefix.rsplit('\n').next().unwrap().len()).unwrap();
            (FILE, line, Some(column))
        };
        for (anchor, expected) in [
            (&row.caller, point("fn caller", 3)),
            (&row.callee, point("fn leaf", 3)),
        ] {
            assert_eq!(
                (anchor.file.as_str(), anchor.line, anchor.col),
                expected,
                "{source}"
            );
        }
        let site = row.site.as_ref().unwrap();
        assert_eq!(
            (site.file.as_str(), site.line, site.col),
            point("§", 0),
            "{source}"
        );
    }
}

#[test]
fn only_allowed_inner_attributes_keep_calls() {
    for (attr, kept) in [
        ("no_std", true),
        ("no_main", true),
        ("no_implicit_prelude", true),
        ("recursion_limit = \"8\"", true),
        ("type_length_limit = \"16\"", true),
        ("crate_name = \"rows\"", true),
        ("crate_type = \"lib\"", true),
        ("windows_subsystem = \"windows\"", true),
        ("feature(doc_cfg)", true),
        ("feature(doc_notable_trait, doc_auto_cfg)", true),
        ("cfg_attr(docsrs_regex, feature(doc_cfg))", true),
        (
            "cfg_attr(any(), cfg_attr(any(), no_std), recursion_limit = \"8\")",
            true,
        ),
        ("cfg_attr(true, no_std)", true),
        ("my_attr", false),
        ("tool::attr", false),
        ("feature(try_trait_v2)", false),
        ("feature(doc_cfg, try_trait_v2)", false),
        ("cfg_attr(any(), my_attr)", false),
        ("cfg_attr(any(), no_std, my_attr)", false),
        ("cfg_attr(false, my_attr)", false),
        ("no_builtins", false),
    ] {
        let source = format!(
            "#![{attr}]\npub fn leaf() {{}}\npub fn caller() {{\n    §leaf();\n    leaf();\n}}"
        );
        attribute_case(&source, kept);
        attribute_case(
            &source.replace("§leaf();\n    leaf();", "leaf();\n    §leaf();"),
            kept,
        );
    }
    attribute_case(
        "pub fn leaf() {}\n#[no_std]\npub fn caller() {\n    §leaf();\n}",
        false,
    );
    attribute_case(
        "#!/usr/bin/env rust-script\n#![no_std]\npub fn leaf() {}\npub fn caller() {\n    §leaf();\n}",
        true,
    );
}

#[test]
fn nested_attribute_owners_keep_unrelated_sites() {
    for (node, kept, control) in [
        ("#[my_attr] let x = §leaf();", false, true),
        ("#[my_attr] §leaf();", false, true),
        (
            "match 0 {\n        #[my_attr] 0 => §leaf(),\n        _ => (),\n    };",
            false,
            true,
        ),
        ("let f = #[my_attr] || §leaf();", false, true),
        ("become §leaf();", false, true),
        ("#[allow(unused)] let x = §leaf();", true, true),
        ("#![my_attr]\n    §leaf();", false, false),
        (
            "{\n        #![my_attr]\n        §leaf();\n    }",
            false,
            true,
        ),
        (
            "match §leaf() {\n        #![my_attr]\n        _ => (),\n    };",
            false,
            true,
        ),
        (
            "let _ = |#[cfg_attr(false, my_attr)]\n        box x| ();\n    §leaf();",
            false,
            false,
        ),
        ("{ #![cfg_attr(false, my_attr)] }\n    §leaf();", true, true),
        ("m!(§leaf());", true, true),
        ("m!(#[my_attr] §leaf());", true, true),
    ] {
        let source = format!("pub fn leaf() {{}}\npub fn caller() {{\n    {node}\n    leaf();\n}}");
        attribute_case(&source, kept);
        attribute_case(
            &source
                .replacen('§', "", 1)
                .replace("\n    leaf();\n}", "\n    §leaf();\n}"),
            control,
        );
    }
    for source in [
        "pub fn leaf() {}\nmod m {\n    #![my_attr]\n    fn caller() {\n        §leaf();\n    }\n}",
        "pub fn leaf() {}\nimpl T {\n    #![my_attr]\n    fn caller() {\n        §leaf();\n    }\n}",
        "pub fn leaf() {\n    #![my_attr]\n}\npub fn caller() {\n    §leaf();\n}",
    ] {
        attribute_case(source, false);
    }
    attribute_case(
        "pub fn leaf() {}\nfn blocked() {\n    let _ = |#[cfg_attr(false, my_attr)]\n        box x| ();\n}\npub fn caller() {\n    §leaf();\n}",
        true,
    );
}
