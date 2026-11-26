//! The `gosanja` binary: exit codes and diagnostics of `index` and `validate`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// A fresh directory path under the target directory; the directory itself does not exist.
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("cli-{}-{n}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("clear scratch");
    }
    dir
}

fn gosanja(args: &[&dyn AsRef<std::ffi::OsStr>]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gosanja"))
        .args(args.iter().map(AsRef::as_ref))
        .output()
        .expect("runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("exited")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A copy of the rust-basic golden with `edit` applied to one of its files.
fn corrupted(name: &str, file: &str, edit: impl Fn(&str) -> String) -> PathBuf {
    let dir = scratch(name);
    fs::create_dir_all(&dir).expect("mkdir");
    for entry in fs::read_dir(repo("fixtures/rust-basic/expected")).expect("golden") {
        let entry = entry.expect("entry");
        fs::copy(entry.path(), dir.join(entry.file_name())).expect("copy");
    }
    let path = dir.join(file);
    let before = fs::read_to_string(&path).expect("read");
    let after = edit(&before);
    assert_ne!(before, after, "the edit changed nothing");
    fs::write(path, after).expect("write");
    dir
}

#[test]
fn index_then_validate_succeeds_on_the_fixture() {
    let out = scratch("snapshot");
    let root = repo("fixtures/rust-basic");
    let index = gosanja(&[
        &"index",
        &"--scip",
        &root.join("index.scip"),
        &"--root",
        &root,
        &"--out",
        &out,
    ]);
    assert_eq!(code(&index), 0, "{}", stderr(&index));
    let validate = gosanja(&[&"validate", &out, &"--root", &root]);
    assert_eq!(code(&validate), 0, "{}", stderr(&validate));
}

/// A corruption: name, file, edit, expected exit code and a diagnostic it must name.
type Case = (
    &'static str,
    &'static str,
    Box<dyn Fn(&str) -> String>,
    i32,
    &'static str,
);

#[test]
fn corrupted_goldens_fail_validation_with_a_named_diagnostic() {
    let root = repo("fixtures/rust-basic");
    let cases: [Case; 4] = [
        (
            "dangling",
            "relations.jsonl",
            Box::new(|t| {
                t.replacen(
                    r#""to":"cargo:demo:views/""#,
                    r#""to":"cargo:demo:nowhere/""#,
                    1,
                )
            }),
            1,
            "dangling-key",
        ),
        (
            "outside",
            "entities.jsonl",
            Box::new(|t| t.replace(r#""name":[2,7,2,13]"#, r#""name":[99,7,99,13]"#)),
            1,
            "position-outside-file",
        ),
        (
            "major",
            "manifest.json",
            Box::new(|t| t.replace(r#""schema":"0.5""#, r#""schema":"7.0""#)),
            2,
            "7.0",
        ),
        (
            "member",
            "entities.jsonl",
            Box::new(|t| {
                t.replace(
                    r#""name":"render","parent":"cargo:demo:views/","#,
                    r#""name":"render","parent":"cargo:demo:views/","member_of":"cargo:demo:views/","#,
                )
            }),
            1,
            "member-of-no-group",
        ),
    ];
    for (name, file, edit, expected, diagnostic) in cases {
        let dir = corrupted(name, file, edit);
        for with_root in [false, true] {
            let output = if with_root {
                gosanja(&[&"validate", &dir, &"--root", &root])
            } else {
                gosanja(&[&"validate", &dir])
            };
            assert_eq!(code(&output), expected, "{name}: {}", stderr(&output));
            assert!(
                stderr(&output).contains(diagnostic),
                "{name}: {}",
                stderr(&output)
            );
        }
    }
}

#[test]
fn another_unstable_minor_is_refused() {
    let dir = corrupted("minor", "manifest.json", |t| {
        t.replace(r#""schema":"0.5""#, r#""schema":"0.1""#)
    });
    let output = gosanja(&[&"validate", &dir]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(stderr(&output).contains("0.x versions are only compatible with themselves"));
}

#[test]
fn source_changes_are_reported_with_the_root() {
    let root = scratch("root");
    fs::create_dir_all(&root).expect("mkdir");
    for entry in ["Cargo.toml", "src"] {
        let from = repo("fixtures/rust-basic").join(entry);
        if from.is_dir() {
            fs::create_dir_all(root.join(entry)).expect("mkdir");
            for file in fs::read_dir(&from).expect("src") {
                let file = file.expect("entry");
                fs::copy(file.path(), root.join(entry).join(file.file_name())).expect("copy");
            }
        } else {
            fs::copy(&from, root.join(entry)).expect("copy");
        }
    }
    fs::write(root.join("src/lib.rs"), "//! changed\n").expect("write");
    let golden = repo("fixtures/rust-basic/expected");
    let output = gosanja(&[&"validate", &golden, &"--root", &root]);
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("source-mismatch"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn usage_errors_exit_2_and_input_failures_exit_3() {
    let root = repo("fixtures/rust-basic");
    assert_eq!(code(&gosanja(&[])), 2);
    assert_eq!(code(&gosanja(&[&"frobnicate"])), 2);
    assert_eq!(code(&gosanja(&[&"index", &"--root", &root])), 2);
    assert_eq!(code(&gosanja(&[&"validate", &"--bogus", &root])), 2);
    let garbage = scratch("garbage.scip");
    fs::write(&garbage, [0xff, 0xff, 0xff]).expect("write");
    let out = scratch("never");
    let failed = gosanja(&[
        &"index", &"--scip", &garbage, &"--root", &root, &"--out", &out,
    ]);
    assert_eq!(code(&failed), 3, "{}", stderr(&failed));
    assert!(!out.exists());
    let missing = gosanja(&[
        &"index",
        &"--scip",
        &scratch("absent.scip"),
        &"--root",
        &root,
        &"--out",
        &out,
    ]);
    assert_eq!(code(&missing), 3, "{}", stderr(&missing));
}

#[test]
fn the_version_is_checked_before_any_table_is_read() {
    let dir = corrupted("order", "manifest.json", |t| {
        t.replace(r#""schema":"0.5""#, r#""schema":"0.1""#)
    });
    fs::remove_file(dir.join("relations.jsonl")).expect("remove");
    fs::create_dir(dir.join("relations.jsonl")).expect("a directory in its place");
    let output = gosanja(&[&"validate", &dir]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
}

#[test]
fn an_unreadable_table_is_a_validation_failure() {
    let dir = corrupted("unreadable", "relations.jsonl", |t| format!("{t} "));
    fs::remove_file(dir.join("relations.jsonl")).expect("remove");
    fs::create_dir(dir.join("relations.jsonl")).expect("a directory in its place");
    let output = gosanja(&[&"validate", &dir]);
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("relations.jsonl"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn rows_are_checked_against_the_normative_schemas() {
    let dir = corrupted("schema", "relations.jsonl", |t| {
        t.replacen(r#""roles":[],"#, "", 1)
    });
    let output = gosanja(&[&"validate", &dir]);
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(stderr(&output).contains("schema"), "{}", stderr(&output));
}

#[test]
fn the_version_is_refused_before_the_root_is_resolved() {
    let dir = corrupted("root-order", "manifest.json", |t| {
        t.replace(r#""schema":"0.5""#, r#""schema":"0.1""#)
    });
    let output = gosanja(&[&"validate", &dir, &"--root", &scratch("no-such-root")]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
}

/// A table that blocks whoever opens it for reading, since no writer ever comes.
#[cfg(unix)]
fn fifo(path: &Path) {
    fs::remove_file(path).expect("remove");
    let made = Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
}

#[cfg(unix)]
#[test]
fn a_refused_version_is_decided_without_opening_any_table() {
    let dir = corrupted("manifest-only", "manifest.json", |t| {
        t.replace(r#""schema":"0.5""#, r#""schema":"0.1""#)
    });
    for table in [
        "files",
        "entities",
        "externals",
        "relations",
        "syntax",
        "gaps",
    ] {
        fifo(&dir.join(format!("{table}.jsonl")));
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_gosanja"))
        .arg("validate")
        .arg(&dir)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("runs");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("waits") {
            break status;
        }
        if std::time::Instant::now() > deadline {
            child.kill().expect("kills");
            panic!("validate opened a table before refusing the version");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(2));
}

#[test]
fn the_project_is_named_after_its_root() {
    let out = scratch("named");
    let root = repo("fixtures/rust-basic");
    let index = gosanja(&[
        &"index",
        &"--scip",
        &root.join("index.scip"),
        &"--root",
        &root,
        &"--out",
        &out,
    ]);
    assert_eq!(code(&index), 0, "{}", stderr(&index));
    let manifest = fs::read_to_string(out.join("manifest.json")).expect("manifest");
    assert!(
        manifest.contains(r#""project":{"name":"rust-basic"}"#),
        "{manifest}"
    );
}

#[test]
fn validate_refuses_frozen_public_snapshots() {
    for version in ["v0.20.0", "v0.21.0"] {
        let frozen = repo("fixtures/diff-public").join(version);
        let output = gosanja(&[&"validate", &frozen]);
        assert_eq!(code(&output), 2, "{}", stderr(&output));
        let message = stderr(&output);
        assert!(
            message.contains("0.2") && message.contains("0.5"),
            "{message}"
        );
    }
}
