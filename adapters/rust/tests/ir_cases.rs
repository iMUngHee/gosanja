//! The IR reader on a module the pinned toolchain emitted (`fixtures/ir/reader/app.ll`, a
//! binary of `fixtures/rust-calls`), and on that module edited one way at a time.

use gosanja_adapter_rust::calls::debug::{Module, Refused};
use gosanja_adapter_rust::calls::ir::Target;
use std::path::Path;

const TOOLCHAIN: &str = "rustc 1.99.1 (58cf8f9c3 2026-10-13)";
const MAIN: &str = "_RNvCsjoC0h555x8z_9calls_app4main";
const DIRECT: &str = "_RNvCshPFz110IhNm_9calls_lib6direct";
const LANG_START: &str = "_RINvNtCs8ZKwes1zzDd_3std2rt10lang_startuECsjoC0h555x8z_9calls_app";
const CALL_DIRECT: &str = "  %total = call i32 @_RNvCshPFz110IhNm_9calls_lib6direct(), !dbg !152\n";

fn app() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ir/reader/app.ll");
    std::fs::read_to_string(path).expect("the reader fixture")
}

fn edited(from: &str, to: &str) -> String {
    let text = app();
    assert_eq!(text.matches(from).count(), 1, "one place to edit: {from}");
    text.replacen(from, to, 1)
}

fn unreadable(text: &str) -> String {
    match Module::read(text) {
        Err(Refused::Unreadable { error, .. }) => error.to_string(),
        other => panic!("expected an unreadable module, got {other:?}"),
    }
}

fn function<'m>(m: &'m Module, symbol: &str) -> &'m gosanja_adapter_rust::calls::debug::Function {
    m.functions
        .iter()
        .find(|f| f.symbol == symbol)
        .unwrap_or_else(|| panic!("{symbol} is defined"))
}

#[test]
fn the_emitted_module_reads() {
    let m = Module::read(&app()).unwrap();
    assert_eq!(m.toolchain, "rustc 1.99.1 (58cf8f9c3 2026-10-13)");
    assert_eq!(m.triple, "x86_64-unknown-linux-gnu");
    assert_eq!(m.functions.len(), 9);
    assert!(m.declared.contains(DIRECT));
    let main = function(&m, MAIN);
    let targets: Vec<_> = main.instructions.iter().map(|i| i.target.clone()).collect();
    assert_eq!(
        targets,
        [
            Target::Direct(DIRECT.into()),
            Target::Direct("shared_entry".into())
        ]
    );
    let site = main.instructions[0].location.unwrap();
    assert_eq!((site.line, site.column), (6, 17));
    let sp = m.subprogram(m.scope_subprogram(site.scope));
    assert_eq!(sp.name, "main");
    let file = m.file(m.scope_file(site.scope).unwrap());
    assert_eq!(
        (file.directory.as_str(), file.filename.as_str()),
        ("@ROOT@", "calls-app/src/main.rs")
    );
    assert!(file.md5.as_deref().is_some_and(|s| s.len() == 32));
    // The entry point the compiler generates has no `!dbg`, and neither has its call.
    let entry = function(&m, "main");
    assert_eq!(entry.subprogram, None);
    assert_eq!(entry.instructions.len(), 1);
    assert_eq!(entry.instructions[0].location, None);
    assert_eq!(m.definition("main"), Some(None));
    assert_eq!(m.definition(DIRECT), None, "only declared");
    assert_eq!(
        m.definition(LANG_START)
            .flatten()
            .map(|s| (s.name.as_str(), s.line)),
        Some(("lang_start<()>", 199))
    );
}

#[test]
fn an_invoke_over_two_lines_is_one_call() {
    let m = Module::read(&edited(
        CALL_DIRECT,
        "  %total = invoke i32 @_RNvCshPFz110IhNm_9calls_lib6direct()\n          to label %start unwind label %start, !dbg !152\n",
    ))
    .unwrap();
    let main = function(&m, MAIN);
    assert_eq!(main.instructions.len(), 2);
    assert_eq!(main.instructions[0].target, Target::Direct(DIRECT.into()));
    assert_eq!(main.instructions[0].location.map(|l| l.line), Some(6));
}

#[test]
fn an_invoke_without_its_continuation_does_not_parse() {
    unreadable(&edited(
        CALL_DIRECT,
        "  %total = invoke i32 @_RNvCshPFz110IhNm_9calls_lib6direct(), !dbg !152\n",
    ));
}

#[test]
fn an_understood_line_that_does_not_parse_fails() {
    let text = app();
    let location = text
        .lines()
        .find(|l| l.starts_with("!152 = !DILocation("))
        .unwrap();
    unreadable(&edited(location, location.trim_end_matches(')')));
    unreadable(&edited(CALL_DIRECT, "  %total = call i32 0(), !dbg !152\n"));
    unreadable(&edited(
        "define hidden void @_RNvCsjoC0h555x8z_9calls_app4main() unnamed_addr #0 !dbg !144 !guid !151 {",
        "define hidden void @_RNvCsjoC0h555x8z_9calls_app4main() unnamed_addr #0 !dbg !144 !guid !151",
    ));
}

#[test]
fn a_reference_to_a_missing_node_or_one_of_the_wrong_kind_fails() {
    let missing = unreadable(&edited(CALL_DIRECT, &CALL_DIRECT.replace("!152", "!99999")));
    assert!(missing.contains("!99999"), "{missing}");
    let wrong = unreadable(&edited(CALL_DIRECT, &CALL_DIRECT.replace("!152", "!34")));
    assert!(wrong.contains("DIFile"), "{wrong}");
}

#[test]
fn a_module_that_ends_inside_a_function_fails() {
    let text = app();
    let cut = text.find(CALL_DIRECT).unwrap();
    let e = unreadable(&text[..cut]);
    assert!(e.contains("ends inside a function"), "{e}");
}

#[test]
fn a_string_that_is_not_utf8_fails() {
    unreadable(&edited(
        "filename: \"calls-app/src/main.rs/@/6ioqopafrauawqaowcojr5zfo\"",
        "filename: \"calls-app/src/m\\FFin.rs/@/6ioqopafrauawqaowcojr5zfo\"",
    ));
}

#[test]
fn an_empty_or_cut_module_fails() {
    unreadable("");
    let text = app();
    let first = text.find("\n}\n").unwrap() + 3;
    unreadable(&text[..first]);
    let compile_unit = text.find("!33 = distinct !DICompileUnit(").unwrap();
    unreadable(&text[..compile_unit]);
}

#[test]
fn a_module_without_a_target_triple_fails() {
    let e = unreadable(&edited(
        "target triple = \"x86_64-unknown-linux-gnu\"\n",
        "",
    ));
    assert!(e.contains("target triple"), "{e}");
}

#[test]
fn a_module_of_another_producer_fails() {
    let e = unreadable(&edited(
        "producer: \"clang LLVM (rustc version 1.99.1 (58cf8f9c3 2026-10-13))\"",
        "producer: \"clang version 19.1.0\"",
    ));
    assert!(e.contains("another producer"), "{e}");
}

#[test]
fn a_second_compile_unit_fails() {
    let text = app();
    let unit = text
        .lines()
        .find(|l| l.starts_with("!33 = distinct !DICompileUnit("))
        .unwrap();
    let e = unreadable(&format!("{text}{}\n", unit.replacen("!33", "!9033", 1)));
    assert!(e.contains("second compile unit"), "{e}");
}

#[test]
fn an_optimised_module_is_refused_as_such() {
    let text = edited("isOptimized: false", "isOptimized: true");
    assert_eq!(
        Module::read(&text).unwrap_err(),
        Refused::Optimised {
            toolchain: TOOLCHAIN.into()
        }
    );
    let text = edited("isOptimized: false, ", "");
    assert_eq!(
        Module::read(&text).unwrap_err(),
        Refused::Optimised {
            toolchain: TOOLCHAIN.into()
        }
    );
    // Unreadable comes first.
    let text = text.replace("target triple = \"x86_64-unknown-linux-gnu\"\n", "");
    unreadable(&text);
}

#[test]
fn unknown_lines_kinds_and_fields_are_passed_over() {
    let text = app();
    let subprogram = text
        .lines()
        .find(|l| l.starts_with("!144 = distinct !DISubprogram("))
        .unwrap();
    let with_field = subprogram.replacen("(name: ", "(futureField: 7, name: ", 1);
    let text = text.replacen(subprogram, &with_field, 1)
        + "$future = comdat any\nsome future line\n!9000 = !DIFutureKind(x: 1, y: \"\\FF\")\n";
    let m = Module::read(&text).unwrap();
    assert_eq!(function(&m, MAIN).instructions.len(), 2);
}

#[test]
fn escapes_in_strings_decode() {
    let text = app();
    let file = text
        .lines()
        .find(|l| l.starts_with("!37 = !DIFile("))
        .unwrap();
    let escaped = file.replacen("std/src/rt.rs", "std/src/r\\5Ct\\22.rs", 1);
    let m = Module::read(&text.replacen(file, &escaped, 1)).unwrap();
    let id = function(&m, LANG_START).subprogram.unwrap();
    let file = m.file(m.subprogram(id).file.unwrap());
    assert!(
        file.filename.ends_with("std/src/r\\t\".rs"),
        "{}",
        file.filename
    );
}

#[test]
fn records_that_lead_back_to_themselves_fail() {
    let location = "!57 = !DILocation(line: 205, column: 5, scope: !36)";
    // A lexical block that is its own scope, and two that hold each other.
    for blocks in [
        "!999 = distinct !DILexicalBlock(scope: !999, line: 1, column: 1)",
        "!999 = distinct !DILexicalBlock(scope: !998, line: 1, column: 1)\n!998 = distinct !DILexicalBlock(scope: !999, line: 1, column: 1)",
    ] {
        let text = edited(
            location,
            &format!("!57 = !DILocation(line: 205, column: 5, scope: !999)\n{blocks}"),
        );
        let e = unreadable(&text);
        assert!(e.contains("holds itself"), "{e}");
    }
    // A location inlined at itself, and two inlined at each other.
    for inlined in [
        "!57 = !DILocation(line: 205, column: 5, scope: !36, inlinedAt: !57)",
        "!57 = !DILocation(line: 205, column: 5, scope: !36, inlinedAt: !997)\n!997 = !DILocation(line: 1, column: 1, scope: !36, inlinedAt: !57)",
    ] {
        let e = unreadable(&edited(location, inlined));
        assert!(e.contains("returns to itself"), "{e}");
    }
}

#[test]
fn a_call_head_with_text_of_several_bytes_before_a_quote_does_not_parse() {
    let text = edited(CALL_DIRECT, "  %total = call i32 é\"x\"(), !dbg !152\n");
    let e = unreadable(&text);
    assert!(e.contains("call"), "{e}");
}

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ir/reader/app.ll")
}

fn unreadable_file(path: &Path) -> String {
    match Module::read_file(path) {
        Err(Refused::Unreadable { error, toolchain }) => {
            assert_eq!(toolchain, None);
            error.to_string()
        }
        other => panic!("expected an unreadable module, got {other:?}"),
    }
}

#[test]
fn a_module_reads_the_same_from_its_file_as_from_its_text() {
    let from_file = Module::read_file(&fixture_path()).unwrap();
    let from_text = Module::read(&app()).unwrap();
    assert_eq!(format!("{from_file:?}"), format!("{from_text:?}"));
    assert_eq!(function(&from_file, MAIN).instructions.len(), 2);
}

#[test]
fn a_file_that_cannot_be_read_fails() {
    let dir = tempfile::tempdir().unwrap();
    let missing = unreadable_file(&dir.path().join("missing.ll"));
    assert!(!missing.starts_with("line "), "{missing}");
    unreadable_file(dir.path());
}

#[test]
fn a_file_that_is_not_utf8_fails_wherever_the_bytes_are() {
    let dir = tempfile::tempdir().unwrap();
    let text = app();
    // The first pass reads every line, so the byte is found wherever it is: in a line the
    // reader passes over, in an instruction, in a metadata node.
    for anchor in [
        "target datalayout",
        CALL_DIRECT,
        "!144 = distinct !DISubprogram(",
    ] {
        let at = text.find(anchor).unwrap() + anchor.len() - 2;
        let mut bytes = text.clone().into_bytes();
        bytes[at] = 0xFF;
        let path = dir.path().join("bad.ll");
        std::fs::write(&path, bytes).unwrap();
        let line = text[..at].lines().count();
        let error = unreadable_file(&path);
        assert!(error.starts_with(&format!("line {line}: ")), "{error}");
    }
}

#[test]
fn a_field_the_reader_does_not_keep_is_still_checked() {
    let text = app();
    let subprogram = text
        .lines()
        .find(|l| l.starts_with("!144 = distinct !DISubprogram("))
        .unwrap();
    for bad in [
        "(futureField: 7, futureField: 8, name: ",
        "(futureField: \"\\zz\", name: ",
        "(future field: 7, name: ",
    ] {
        let edited = subprogram.replacen("(name: ", bad, 1);
        let error = unreadable(&text.replacen(subprogram, &edited, 1));
        assert!(
            error.contains("DISubprogram node that does not parse"),
            "{error}"
        );
    }
}

#[test]
fn a_node_of_a_kind_nothing_asks_for_is_passed_over_even_when_malformed() {
    let text =
        app() + "!9001 = !DINamespace(name: \"n\", name: \"again\")\n!9002 = !DICompositeType(\n";
    Module::read(&text).unwrap();
}
