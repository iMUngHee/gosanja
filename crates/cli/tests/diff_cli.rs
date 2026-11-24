//! `gosanja diff`: the files it writes, the inputs it refuses and the outputs it protects.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

const FILES: [&str; 5] = [
    "manifest.json",
    "entities.jsonl",
    "externals.jsonl",
    "relations.jsonl",
    "candidates.jsonl",
];

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
        .join(format!("diff-{}-{n}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("clear scratch");
    }
    dir
}

/// Runs `gosanja` with an empty environment: a diff reads nothing but its arguments.
fn gosanja(args: &[&dyn AsRef<std::ffi::OsStr>]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gosanja"))
        .args(args.iter().map(AsRef::as_ref))
        .env_clear()
        .output()
        .expect("runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("exited")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A refusal of the inputs or the output, exit 2, and not a refusal of the command line.
fn assert_refused(output: &Output, what: &str) {
    assert_eq!(code(output), 2, "{what}: {}", stderr(output));
    assert!(
        !stderr(output).contains("usage:"),
        "{what}: not a usage error: {}",
        stderr(output)
    );
}

fn old() -> PathBuf {
    repo("fixtures/diff-basic/old/expected")
}

fn new() -> PathBuf {
    repo("fixtures/diff-basic/new/expected")
}

/// Copies the files of `from` into a new directory `to`.
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for entry in fs::read_dir(from).expect("readable") {
        let entry = entry.expect("entry");
        fs::copy(entry.path(), to.join(entry.file_name())).expect("copy");
    }
}

/// A copy of the old fixture snapshot with `edit` applied to one of its files.
fn edited(name: &str, file: &str, edit: impl Fn(&str) -> String) -> PathBuf {
    let dir = scratch(name);
    copy(&old(), &dir);
    let path = dir.join(file);
    let before = fs::read_to_string(&path).expect("read");
    let after = edit(&before);
    assert_ne!(before, after, "the edit changed nothing");
    fs::write(path, after).expect("write");
    dir
}

/// Every path under `dir` with the bytes of each file, a link recorded by its target.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        let meta = fs::symlink_metadata(&path).expect("exists");
        let name = path.strip_prefix(dir).expect("below").to_path_buf();
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&path).expect("link");
            found.insert(name, target.as_os_str().as_encoded_bytes().to_vec());
        } else if meta.is_dir() {
            found.insert(name, Vec::new());
            for entry in fs::read_dir(&path).expect("readable") {
                pending.push(entry.expect("entry").path());
            }
        } else {
            found.insert(name, fs::read(&path).expect("readable"));
        }
    }
    found
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("readable")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn the_fixture_pair_gives_the_golden_on_every_run() {
    let first = scratch("first");
    let second = scratch("second");
    for out in [&first, &second] {
        let output = gosanja(&[&"diff", &old(), &new(), &"--out", out]);
        assert_eq!(code(&output), 0, "{}", stderr(&output));
    }
    let mut expected: Vec<String> = FILES.iter().map(|f| (*f).to_owned()).collect();
    expected.sort();
    assert_eq!(names(&first), expected, "exactly the five files");
    for file in FILES {
        let written = fs::read(first.join(file)).expect("written");
        assert_eq!(
            written,
            fs::read(second.join(file)).expect("written"),
            "{file}: two runs"
        );
        assert_eq!(
            written,
            fs::read(repo("fixtures/diff-basic/expected").join(file)).expect("golden"),
            "{file}: the golden"
        );
        assert_eq!(
            written,
            fs::read(repo("spec/examples/diff-basic").join(file)).expect("example"),
            "{file}: the hand-derived example"
        );
    }
}

#[test]
fn no_rekeyed_row_has_equal_keys() {
    let out = scratch("rekeyed");
    let output = gosanja(&[&"diff", &old(), &new(), &"--out", &out]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let mut rekeyed = 0;
    for file in ["entities.jsonl", "externals.jsonl"] {
        for line in fs::read_to_string(out.join(file)).expect("written").lines() {
            let row: Value = serde_json::from_str(line).expect("JSON");
            if row["change"] == "rekeyed" {
                rekeyed += 1;
                assert_ne!(row["old"], row["new"], "{file}: {line}");
            }
        }
    }
    assert!(rekeyed > 0, "the fixture has rekeyed rows to check");
}

#[test]
fn a_snapshot_compared_with_itself_gives_five_files_and_no_row() {
    let out = scratch("itself");
    let output = gosanja(&[&"diff", &old(), &old(), &"--out", &out]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    for file in &FILES[1..] {
        assert_eq!(fs::read(out.join(file)).expect("written"), b"", "{file}");
    }
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).expect("written")).unwrap();
    assert_eq!(
        manifest["counts"],
        serde_json::json!({
            "entities": {"added": 0, "removed": 0, "changed": 0, "rekeyed": 0},
            "externals": {"added": 0, "removed": 0, "rekeyed": 0},
            "relations": {"added": 0, "removed": 0},
            "candidates": 0
        })
    );
    assert_eq!(manifest["format"], "gosanja-diff");
    assert_eq!(manifest["old"], manifest["new"]);
}

#[test]
fn another_schema_version_is_refused_before_anything_is_written() {
    let input = edited("version", "manifest.json", |m| {
        m.replace(r#""schema":"0.3""#, r#""schema":"0.1""#)
    });
    for (a, b) in [(&input, &new()), (&old(), &input)] {
        let out = scratch("refused");
        let output = gosanja(&[&"diff", a, b, &"--out", &out]);
        assert_refused(&output, "another schema");
        assert!(!out.exists(), "nothing is written");
    }
}

/// An edit of one file of a copied snapshot.
type Edit = Box<dyn Fn(&str) -> String>;

#[test]
fn an_invalid_input_is_refused_with_its_diagnostic() {
    let stays = r#"{"key":"cargo:diff-basic:util/stays()."#;
    let cases: [(&str, &str, Edit, &str); 5] = [
        (
            "duplicate",
            "entities.jsonl",
            Box::new(|t: &str| {
                let first = t.lines().next().unwrap();
                let twin = first.replacen(r#""name":""#, r#""name":"twin-"#, 1);
                format!("{first}\n{twin}\n{}", t.split_once('\n').unwrap().1)
            }),
            "duplicate-key",
        ),
        (
            "dangling",
            "relations.jsonl",
            Box::new(|t: &str| {
                t.replacen("cargo:diff-basic:hidden().", "cargo:diff-basic:gone().", 1)
            }),
            "dangling-key",
        ),
        (
            "cycle",
            "entities.jsonl",
            Box::new(|t: &str| {
                t.replacen(
                    r#""name":"util","def""#,
                    r#""name":"util","parent":"cargo:diff-basic:util/stays().","def""#,
                    1,
                )
            }),
            "parent-cycle",
        ),
        (
            "local",
            "entities.jsonl",
            Box::new(move |t: &str| {
                let line = t.lines().find(|l| l.starts_with(stays)).unwrap();
                let symbol = line
                    .split(r#""symbol":""#)
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap();
                t.replacen(symbol, "local 4", 1)
            }),
            "invalid-symbol",
        ),
        (
            "malformed",
            "entities.jsonl",
            Box::new(move |t: &str| {
                let line = t.lines().find(|l| l.starts_with(stays)).unwrap();
                let symbol = line
                    .split(r#""symbol":""#)
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap();
                t.replacen(symbol, "rust-analyzer cargo", 1)
            }),
            "invalid-symbol",
        ),
    ];
    for (name, file, edit, diagnostic) in cases {
        let input = edited(name, file, edit);
        if name == "duplicate" {
            // One more row than the manifest counts would be reported first, as `count-mismatch`.
            let path = input.join("manifest.json");
            let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let rows = manifest["counts"]["entities"].as_u64().unwrap();
            manifest["counts"]["entities"] = (rows + 1).into();
            fs::write(&path, format!("{manifest}\n")).unwrap();
        }
        let out = scratch("invalid");
        let output = gosanja(&[&"diff", &input, &new(), &"--out", &out]);
        assert_eq!(code(&output), 3, "{name}: {}", stderr(&output));
        assert!(
            stderr(&output).contains(diagnostic),
            "{name}: names {diagnostic}: {}",
            stderr(&output)
        );
        assert!(!out.exists(), "{name}: nothing is written");
    }
}

#[test]
fn a_snapshot_with_no_scope_or_two_is_refused() {
    let none = edited("no-scope", "manifest.json", |m| {
        let mut v: Value = serde_json::from_str(m).unwrap();
        v["scopes"] = serde_json::json!([]);
        format!("{v}\n")
    });
    let two = edited("two-scopes", "manifest.json", |m| {
        let mut v: Value = serde_json::from_str(m).unwrap();
        let scope = v["scopes"][0].clone();
        let mut second = scope.clone();
        second["path"] = "other".into();
        v["scopes"] = serde_json::json!([scope, second]);
        format!("{v}\n")
    });
    for input in [none, two] {
        let out = scratch("scopes");
        let output = gosanja(&[&"diff", &input, &new(), &"--out", &out]);
        assert_refused(&output, &input.display().to_string());
        assert!(!out.exists());
    }
}

/// Copies of the two fixture snapshots side by side in a scratch directory.
fn inputs(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = scratch(name);
    let (a, b) = (base.join("old"), base.join("new"));
    copy(&old(), &a);
    copy(&new(), &b);
    (base, a, b)
}

#[test]
fn an_output_that_is_contains_or_lies_inside_an_input_is_refused() {
    let (base, a, b) = inputs("overlap");
    let before = tree(&base);
    for out in [
        a.clone(),
        b.clone(),
        a.join("inside"),
        b.join("inside"),
        base.clone(),
    ] {
        let output = gosanja(&[&"diff", &a, &b, &"--out", &out]);
        assert_refused(&output, &out.display().to_string());
    }
    assert_eq!(tree(&base), before, "inputs and outputs unchanged");
}

#[cfg(unix)]
#[test]
fn an_overlap_through_a_link_is_refused() {
    let (base, a, b) = inputs("linked");
    let links = scratch("links");
    fs::create_dir_all(&links).expect("mkdir");
    std::os::unix::fs::symlink(&a, links.join("to-old")).expect("link");
    std::os::unix::fs::symlink(&b, links.join("to-new")).expect("link");
    std::os::unix::fs::symlink(&base, links.join("to-both")).expect("link");
    let before = (tree(&base), tree(&links));
    for out in [
        links.join("to-old"),
        links.join("to-new"),
        links.join("to-old").join("inside"),
        links.join("to-both"),
        links.join("to-both").join("new").join(".."),
    ] {
        let output = gosanja(&[&"diff", &a, &b, &"--out", &out]);
        assert_refused(&output, &out.display().to_string());
    }
    assert_eq!(
        (tree(&base), tree(&links)),
        before,
        "inputs and outputs unchanged"
    );
}

#[test]
fn an_earlier_diff_is_replaced_and_a_snapshot_is_not() {
    let out = scratch("earlier");
    copy(&repo("fixtures/diff-basic/expected"), &out);
    fs::write(out.join("stale"), "left from an earlier run").expect("write");
    let output = gosanja(&[&"diff", &old(), &new(), &"--out", &out]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(!out.join("stale").exists(), "replaced whole");
    assert_eq!(names(&out).len(), 5);

    let held = scratch("held-snapshot");
    copy(&old(), &held);
    let before = tree(&held);
    let output = gosanja(&[&"diff", &old(), &new(), &"--out", &held]);
    assert_refused(&output, "a snapshot at --out");
    assert_eq!(tree(&held), before, "a snapshot is left as it was");
}

#[test]
fn a_snapshot_never_replaces_a_diff() {
    let out = scratch("held-diff");
    copy(&repo("fixtures/diff-basic/expected"), &out);
    let before = tree(&out);
    let root = repo("fixtures/rust-basic");
    let output = gosanja(&[
        &"index",
        &"--scip",
        &root.join("index.scip"),
        &"--root",
        &root,
        &"--out",
        &out,
    ]);
    assert_refused(&output, "a diff at a snapshot's --out");
    assert_eq!(tree(&out), before, "the diff is left as it was");
}

#[test]
fn diff_needs_two_inputs_and_an_output() {
    for args in [
        vec!["diff"],
        vec!["diff", "a"],
        vec!["diff", "a", "b"],
        vec!["diff", "a", "b", "c", "--out", "d"],
        vec!["diff", "a", "b", "--root", "c", "--out", "d"],
    ] {
        let os: Vec<&dyn AsRef<std::ffi::OsStr>> = args
            .iter()
            .map(|a| a as &dyn AsRef<std::ffi::OsStr>)
            .collect();
        let output = gosanja(&os);
        assert_eq!(code(&output), 2, "{args:?}: {}", stderr(&output));
        assert!(
            stderr(&output).contains("gosanja diff <old> <new> --out <dir>"),
            "{args:?}"
        );
    }
}

#[test]
fn another_schema_version_is_refused_whatever_its_tables_hold() {
    for (name, damage) in [
        (
            "unreadable",
            Box::new(|dir: &Path| {
                fs::remove_file(dir.join("entities.jsonl")).unwrap();
                fs::create_dir(dir.join("entities.jsonl")).unwrap();
            }) as Box<dyn Fn(&Path)>,
        ),
        (
            "malformed",
            Box::new(|dir: &Path| {
                fs::write(dir.join("relations.jsonl"), b"{not json\n\xff\xfe\n").unwrap();
            }),
        ),
        (
            "missing",
            Box::new(|dir: &Path| {
                fs::remove_file(dir.join("syntax.jsonl")).unwrap();
            }),
        ),
    ] {
        let input = edited(name, "manifest.json", |m| {
            m.replace(r#""schema":"0.3""#, r#""schema":"9.9""#)
        });
        damage(&input);
        for (a, b) in [(&input, &new()), (&old(), &input)] {
            let out = scratch("version-first");
            let output = gosanja(&[&"diff", a, b, &"--out", &out]);
            assert_refused(&output, name);
            assert!(
                stderr(&output).contains("9.9"),
                "{name}: a version refusal: {}",
                stderr(&output)
            );
            assert!(!out.exists(), "{name}: nothing is written");
        }
    }
}

#[test]
fn diff_refuses_frozen_public_snapshots() {
    let root = repo("fixtures/diff-public");
    let out = scratch("frozen");
    let result = gosanja(&[
        &"diff",
        &root.join("v0.20.0"),
        &root.join("v0.21.0"),
        &"--out",
        &out,
    ]);
    assert_refused(&result, "frozen public snapshots");
    let message = stderr(&result);
    assert!(
        message.contains("unsupported snapshot schema 0.2"),
        "{message}"
    );
    assert!(!out.exists());
}

#[test]
fn validate_refuses_frozen_public_snapshots() {
    let root = repo("fixtures/diff-public");
    for side in ["v0.20.0", "v0.21.0"] {
        let result = gosanja(&[&"validate", &root.join(side)]);
        assert_refused(&result, side);
        let message = stderr(&result);
        assert!(
            message.contains("written with version 0.2, this reader implements 0.3"),
            "{side}: {message}"
        );
    }
}

#[test]
fn a_diff_of_the_previous_schema_at_the_output_is_replaced() {
    use gosanja_schema::{
        compat::{Refusal, accept},
        diff::SCHEMA_VERSION,
    };
    let out = scratch("previous-schema");
    copy(&repo("fixtures/diff-basic/expected"), &out);
    let manifest = out.join("manifest.json");
    let text = fs::read_to_string(&manifest).expect("read");
    // A diff written before this schema: diff 0.1 over snapshot 0.2 sides from contract 0.2.
    let previous = text
        .replacen(r#""schema":"0.2""#, r#""schema":"0.1""#, 1)
        .replace(r#""schema":"0.3""#, r#""schema":"0.2""#)
        .replace(r#""contract":"0.3""#, r#""contract":"0.2""#);
    for stamp in [
        r#""schema":"0.1""#,
        r#""schema":"0.2""#,
        r#""contract":"0.2""#,
    ] {
        let expected = if stamp.contains("0.1") { 1 } else { 2 };
        assert_eq!(previous.matches(stamp).count(), expected, "{stamp}");
    }
    fs::write(&manifest, previous).expect("write");
    let prior_manifest: Value =
        serde_json::from_slice(&fs::read(&manifest).expect("read")).expect("JSON");
    let writer = prior_manifest["schema"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        accept(SCHEMA_VERSION, writer),
        Err(Refusal::UnstableMinor {
            reader: SCHEMA_VERSION,
            writer
        })
    );

    let output = gosanja(&[&"diff", &old(), &new(), &"--out", &out]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("read")).expect("JSON");
    assert_eq!(written["schema"], SCHEMA_VERSION.to_string());
    assert_eq!(tree(&out), tree(&repo("fixtures/diff-basic/expected")));
}
