//! The four build-layer checks share one pair of Cargo directories. Each case here runs the
//! adapter twice on one project: as it is, and with a `cargo` that empties those directories
//! before every check, which is what four checks in directories of their own would see. What
//! the two runs publish has to be equal, whether the first reused the directories or gave up.

use super::*;
use std::os::unix::fs::PermissionsExt as _;

const LAYERS: &str = "base,syntax,build";
const LIB: &str = "cargo:probe:lib:probe";

/// A build script that Cargo runs again in every check and that always does the same.
const SAME_EVERY_TIME: &str = r#"use std::{env, fs, path::Path};
fn main() {
    println!("cargo::rerun-if-changed=missing");
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("gen.rs"), "pub fn g() {}").unwrap();
}
"#;

/// One package named `probe` with a build script, and an index that names `sources`.
fn project(build: &str, sources: &[(&str, &str)]) -> Case {
    let case = Case::new();
    fs::remove_dir_all(&case.root).expect("remove the default fixture copy");
    fs::create_dir_all(case.root.join("src")).unwrap();
    fs::write(
        case.root.join("Cargo.toml"),
        "[package]\nname = 'probe'\nversion = '0.1.0'\nedition = '2021'\n",
    )
    .unwrap();
    let root = |file: &str| Document {
        relative_path: file.into(),
        language: "rust".into(),
        position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
        occurrences: vec![definition(
            "rust-analyzer cargo probe 0.1.0 crate/",
            [0, 0, 0],
            [0, 0, 0],
        )],
        ..Document::default()
    };
    let mut index =
        Index::parse_from_bytes(&fs::read(fixture().join("index.scip")).unwrap()).unwrap();
    index.documents = vec![root("build.rs")];
    fs::write(case.root.join("build.rs"), build).unwrap();
    for (file, text) in sources {
        fs::write(case.root.join(file), text).unwrap();
        index.documents.push(if *file == "src/lib.rs" {
            root(file)
        } else {
            Document {
                occurrences: Vec::new(),
                ..root(file)
            }
        });
    }
    fs::write(
        case.root.join("index.scip"),
        index.write_to_bytes().unwrap(),
    )
    .unwrap();
    ok(&case
        .command(&case.bin.join("cargo"))
        .current_dir(&case.root)
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap());
    case
}

struct Published {
    out: PathBuf,
    log: String,
}

impl Published {
    fn layers(&self) -> Value {
        let result: Value =
            serde_json::from_slice(&fs::read(self.out.join("result.json")).unwrap()).unwrap();
        result["layers"].clone()
    }

    /// The checks that ran again in directories of their own.
    fn retried(&self) -> Vec<usize> {
        (0..4)
            .filter(|run| self.out.join(format!("work/check-{run}")).exists())
            .collect()
    }

    fn file(&self, path: &str) -> Value {
        table(&self.out, "build.jsonl")
            .into_iter()
            .find(|row| row["row"] == "file" && row["file"] == path)
            .unwrap_or_else(|| panic!("no file row for {path}"))
    }
}

fn publish(case: &Case, name: &str) -> Published {
    let out = case.temp.path().join(name);
    fs::create_dir(&out).unwrap();
    let output = case
        .adapter()
        .arg("index")
        .arg("--root")
        .arg(&case.root)
        .arg("--out")
        .arg(&out)
        .args(["--layers", LAYERS])
        .output()
        .unwrap();
    ok(&output);
    Published {
        out,
        log: String::from_utf8(output.stderr).unwrap(),
    }
}

/// Run the adapter as it is, then with every check starting from empty directories, and
/// require that both publish the same. Returns the first run.
fn same_as_on_its_own(case: &Case) -> Published {
    let shared = publish(case, "shared");
    let cargo = case.bin.join("cargo");
    let real = case.bin.join("real-cargo");
    fs::rename(&cargo, &real).unwrap();
    let rm = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join("rm"))
        .find(|path| path.is_file())
        .unwrap();
    fs::write(
        &cargo,
        format!(
            r#"#!/bin/sh
if [ "$1" = check ] && [ "${{CARGO_TARGET_DIR##*/work/}}" = checks/target ]; then
    '{}' -rf "$CARGO_TARGET_DIR" "$CARGO_BUILD_BUILD_DIR"
fi
exec '{}' "$@"
"#,
            rm.display(),
            real.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    let alone = publish(case, "alone");
    assert_eq!(shared.layers(), alone.layers());
    for name in ["build.jsonl", "syntax.jsonl"] {
        assert_eq!(table(&shared.out, name), table(&alone.out, name), "{name}");
    }
    shared
}

fn retry_reason(published: &Published, check: usize, reason: &str) {
    let line = format!(
        "gosanja-adapter-rust: build check {check} runs again in directories of its own: {reason}"
    );
    assert!(published.log.contains(&line), "{}", published.log);
}

#[test]
fn check_reuse_keeps_the_capture_check_apart() {
    let case = project(SAME_EVERY_TIME, &[("src/lib.rs", "pub fn kept() {}\n")]);
    let shared = same_as_on_its_own(&case);
    assert!(shared.out.join("work/unit-target").is_dir());
    assert!(shared.out.join("work/checks/target").is_dir());
    assert_eq!(shared.retried(), Vec::<usize>::new(), "{}", shared.log);
    assert!(!shared.log.contains("runs again"), "{}", shared.log);
    assert_eq!(shared.file("src/lib.rs")["targets"], json!([LIB]));
}

#[test]
fn check_reuse_gives_up_when_a_script_reads_its_earlier_out_dir() {
    let case = project(
        r#"use std::{env, fs, path::Path};
fn main() {
    println!("cargo::rerun-if-changed=missing");
    println!("cargo::rustc-check-cfg=cfg(first)");
    let seen = Path::new(&env::var("OUT_DIR").unwrap()).join("seen");
    if !seen.exists() {
        println!("cargo::rustc-cfg=first");
    }
    fs::write(seen, "").unwrap();
}
"#,
        &[
            (
                "src/lib.rs",
                "#[cfg(first)]\nmod gated;\npub fn kept() {}\n",
            ),
            ("src/gated.rs", "pub fn gated() {}\n"),
        ],
    );
    let shared = same_as_on_its_own(&case);
    assert_eq!(shared.retried(), [1, 2, 3], "{}", shared.log);
    retry_reason(
        &shared,
        1,
        "a build script ran again and printed different directives",
    );
    assert_eq!(shared.file("src/gated.rs")["targets"], json!([LIB]));
}

/// The script's second run drops a `rustc-check-cfg` directive. Cargo's message for the
/// script and its files stay the same; only what it printed differs.
const DROPS_CHECK_CFG: &str = r#"use std::{env, fs, path::Path};
fn main() {
    println!("cargo::rerun-if-changed=missing");
    println!("cargo::rustc-cfg=flag");
    let seen = Path::new(&env::var("OUT_DIR").unwrap()).join("seen");
    if !seen.exists() {
        println!("cargo::rustc-check-cfg=cfg(flag)");
    }
    fs::write(seen, "").unwrap();
}
"#;

#[test]
fn check_reuse_gives_up_when_only_a_printed_directive_differs() {
    let case = project(
        DROPS_CHECK_CFG,
        &[("src/lib.rs", "#[cfg(flag)]\npub fn kept() {}\n")],
    );
    let shared = same_as_on_its_own(&case);
    assert_eq!(shared.retried(), [1, 2, 3], "{}", shared.log);
    retry_reason(
        &shared,
        1,
        "a build script ran again and printed different directives",
    );
}

#[test]
fn check_reuse_gives_up_when_the_shared_check_fails() {
    let case = project(
        DROPS_CHECK_CFG,
        &[(
            "src/lib.rs",
            "#![deny(unexpected_cfgs)]\n#[cfg(flag)]\npub fn kept() {}\n",
        )],
    );
    let shared = same_as_on_its_own(&case);
    assert_eq!(shared.retried(), [1, 2, 3], "{}", shared.log);
    retry_reason(&shared, 1, "the check did not succeed");
    assert_eq!(shared.layers()["build"]["status"], "ok");
}

#[test]
fn check_reuse_gives_up_on_a_link_in_out_dir() {
    let case = project(
        r#"use std::{env, fs, os::unix::fs::symlink, path::Path};
fn main() {
    println!("cargo::rerun-if-changed=missing");
    let out = env::var("OUT_DIR").unwrap();
    let out = Path::new(&out);
    for name in ["a.rs", "b.rs"] {
        fs::write(out.join(name), "pub fn same() {}").unwrap();
    }
    let current = out.join("current.rs");
    let target = if current.exists() { "b.rs" } else { "a.rs" };
    let _ = fs::remove_file(&current);
    symlink(out.join(target), current).unwrap();
}
"#,
        &[("src/lib.rs", "pub fn kept() {}\n")],
    );
    let shared = same_as_on_its_own(&case);
    assert_eq!(shared.retried(), [1, 2, 3], "{}", shared.log);
    retry_reason(
        &shared,
        1,
        "a build script ran again and its OUT_DIR differs or cannot be compared",
    );
}

#[test]
fn check_reuse_gives_up_when_a_script_writes_another_file() {
    let case = project(
        r#"use std::{env, fs, path::Path};
fn main() {
    println!("cargo::rerun-if-changed=missing");
    let gen = Path::new(&env::var("OUT_DIR").unwrap()).join("gen.rs");
    let text = if gen.exists() { "pub fn again() {}" } else { "pub fn g() {}" };
    fs::write(gen, text).unwrap();
}
"#,
        &[("src/lib.rs", "pub fn kept() {}\n")],
    );
    let shared = same_as_on_its_own(&case);
    assert_eq!(shared.retried(), [1, 2, 3], "{}", shared.log);
    retry_reason(
        &shared,
        1,
        "a build script ran again and its OUT_DIR differs or cannot be compared",
    );
}

/// The first check starts from empty directories, so nothing it does can come from reuse.
#[test]
fn check_reuse_does_not_repeat_a_failing_first_check() {
    let case = project(
        SAME_EVERY_TIME,
        &[(
            "src/lib.rs",
            "compile_error!(\"the controlled check failure\");\n",
        )],
    );
    let failed = publish(&case, "failed");
    assert_eq!(failed.layers()["build"]["status"], "failed");
    assert_eq!(failed.retried(), Vec::<usize>::new(), "{}", failed.log);
    assert!(!failed.log.contains("runs again"), "{}", failed.log);
}
