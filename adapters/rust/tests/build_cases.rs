use gosanja_adapter_rust::build::{dep_info, normalize_kind};

#[test]
fn build_targets_normalize_cargo_kinds() {
    assert_eq!(normalize_kind(&["lib", "cdylib"]), Some("lib"));
    assert_eq!(normalize_kind(&["custom-build"]), Some("build"));
    assert_eq!(normalize_kind(&["example"]), Some("example"));
    assert_eq!(normalize_kind(&["lib", "bin"]), None);
    assert_eq!(normalize_kind(&["future"]), None);
    assert_eq!(normalize_kind(&[]), None);
}

#[test]
fn build_units_depfile_consistency() {
    let outputs = ["/work/demo.d".into(), "/work/libdemo.rmeta".into()];
    let valid = "/work/demo.d: src/lib.rs src/with\\ space.rs\n\n/work/libdemo.rmeta: src/lib.rs src/with\\ space.rs\n\nsrc/lib.rs:\nsrc/with\\ space.rs:\n# env-dep:FOO=bar\n";
    assert_eq!(
        dep_info(valid, &outputs),
        Some(vec!["src/lib.rs".into(), "src/with space.rs".into()])
    );
    for invalid in [
        valid.replace("src/lib.rs:\n", "src/lib.rs:\nsrc/lib.rs:\n"),
        valid.replace("src/lib.rs:\n", "src/lib.rs:\nother.rs:\n"),
        valid.replace("src/lib.rs src/", "src/lib.rs src/lib.rs src/"),
        valid.replace("src/lib.rs", "src/evil:\nlib.rs"),
        valid.replace("# env-dep", "unexpected\n# env-dep"),
        valid
            .replace("src/lib.rs src/", "src/lib.rs src/lib.rs src/")
            .replace("src/lib.rs:\n", "src/lib.rs:\nsrc/lib.rs:\n"),
        valid.replacen("src/lib.rs src/", "src/other.rs src/", 1),
    ] {
        assert_eq!(dep_info(&invalid, &outputs), None, "{invalid}");
    }
}

#[test]
fn test_marks_cfg_gates_in_the_file() {
    use gosanja_adapter_rust::syntax::cfg_test_positions;
    let code = r"#[cfg(not(not(test)))] fn double_negative() {}
#[cfg(test)] mod child { pub fn inherited() {} }
#[cfg(any(all(), test))] fn production_too() {}
#[cfg(any())] fn never_built() {}
#[cfg(test)] #[test] fn gate_and_attribute() {}
#[cfg(any(test, not(test)))] fn always_built() {}
#[cfg(all(test, not(test)))] fn contradiction() {}
#[cfg(all(all(), test))] fn all_with_true() {}
#[cfg(all(any(), test))] fn all_with_false() {}";
    assert_eq!(
        cfg_test_positions(code)
            .iter()
            .map(|(line, _)| *line)
            .collect::<Vec<_>>(),
        [0, 1, 1, 4, 7]
    );
}

#[test]
fn test_marks_none_for_open_predicates() {
    use gosanja_adapter_rust::syntax::cfg_test_positions;
    assert_eq!(
        cfg_test_positions(
            r"#[cfg(other(test))] fn unknown() {}
#[cfg(not())] fn missing_operand() {}
#[cfg(not(test, not(test)))] fn extra_operand() {}
#[cfg(all(test, unix))] fn open() {}
#[cfg_attr(test, cfg(test))] fn nested() {}"
        ),
        Vec::<(u32, u32)>::new()
    );
}

#[test]
fn test_marks_enclosing_rewriting_attribute() {
    use gosanja_adapter_rust::syntax::cfg_test_positions;
    assert_eq!(
        cfg_test_positions(
            "#[cfg(test)] mod outer { #[tokio::test] fn kept() {} }\n\
         #[retype(u8)] impl Host<u16> { #[cfg(test)] fn hidden() {} }\n\
         #[cfg_attr(test, allow(dead_code))] impl Host<u16> { #[cfg(any(test))] fn kept() {} }\n\
         #[cfg(any(test, all(test, unix)))] fn unknown() {}\n\
         #[test] fn bare() {}"
        )
        .iter()
        .map(|&(line, _)| line)
        .collect::<Vec<_>>(),
        [0, 0, 2]
    );
}
