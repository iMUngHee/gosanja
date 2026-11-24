//! CLI-level adapter protocol, publication and source-tree preservation.
#![cfg(unix)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::tempdir;

fn binary() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gosanja"));
    Command::env_remove(&mut cmd, "GOSANJA_FAKE_MODE");
    cmd
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}
fn cli(mode: &str, root: &Path, out: &Path, cache: &Path) -> Output {
    let mut cmd = binary();
    cmd.args(["index", "--adapter"])
        .arg(fixture().join("fake-adapter/adapter.sh"))
        .arg("--root")
        .arg(root)
        .arg("--out")
        .arg(out);
    configure(&mut cmd, mode, cache);
    cmd.output().unwrap()
}
fn configure(cmd: &mut Command, mode: &str, cache: &Path) {
    Command::env(cmd, "GOSANJA_FAKE_MODE", mode);
    Command::env(cmd, "XDG_CACHE_HOME", cache);
    Command::env(cmd, "HOME", cache.parent().unwrap());
}
fn check(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// Include directories, dotfiles, permission bits and each regular file's content digest.
fn listing(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        let meta = fs::symlink_metadata(&path).unwrap();
        let bytes = if meta.is_dir() {
            pending.extend(fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            vec![]
        } else {
            assert!(meta.is_file());
            Sha256::digest(fs::read(&path).unwrap()).to_vec()
        };
        entries.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            (meta.permissions().mode(), bytes),
        );
    }
    entries
}

#[test]
fn standalone_describe_and_conflicting_index_sources() {
    let program = fixture().join("fake-adapter/adapter.sh");
    let result = binary()
        .args(["adapter", "describe"])
        .arg(&program)
        .output()
        .unwrap();
    check(&result, 0);
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["name"], "fake");
    assert_eq!(value["contract"], "0.3");
    let mut cmd = binary();
    cmd.args(["adapter", "describe"]).arg(program);
    Command::env(&mut cmd, "GOSANJA_FAKE_MODE", "incompatible");
    check(&cmd.output().unwrap(), 2);
    for args in [
        vec![
            "index",
            "--scip",
            "x",
            "--adapter",
            "a",
            "--root",
            ".",
            "--out",
            "o",
        ],
        vec!["adapter", "describe"],
    ] {
        check(&binary().args(args).output().unwrap(), 2);
    }
}

#[test]
fn fatal_outcomes_leave_the_complete_previous_snapshot() {
    let temp = tempdir().unwrap();
    let out = temp.path().join("snapshot");
    let cache = temp.path().join("cache");
    let root = fixture().join("rust-basic");
    check(&cli("good", &root, &out, &cache), 0);
    let previous = listing(&out);
    for (mode, status) in [
        ("incompatible", 2),
        ("old-contract", 2),
        ("nonzero", 3),
        ("no-result", 3),
        ("not-json", 3),
        ("invalid-result", 3),
        ("failed-base", 3),
        ("absent-base", 3),
        ("no-index", 3),
        ("broken-scip", 3),
    ] {
        check(&cli(mode, &root, &out, &cache), status);
        assert_eq!(listing(&out), previous, "{mode}");
    }
}

#[test]
fn optional_outcomes_publish_normalised_gaps_and_only_valid_rows() {
    let temp = tempdir().unwrap();
    let cache = temp.path().join("cache");
    let root = fixture().join("rust-basic");
    let out = temp.path().join("snapshot");
    for (mode, kind, detail) in [
        ("not-offered", "missing-layer", None),
        ("reported-a", "failed-layer", Some("reported-failed")),
        ("no-status", "failed-layer", Some("no-status")),
        ("no-file", "failed-layer", Some("file-missing")),
        ("bad-row", "failed-layer", Some("row-invalid")),
    ] {
        check(&cli(mode, &root, &out, &cache), 0);
        let gaps = fs::read_to_string(out.join("gaps.jsonl")).unwrap();
        let gap = gaps
            .lines()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .find(|v| v["src"] == "adapter:fake/syntax")
            .unwrap();
        assert_eq!(gap["gap"], kind);
        assert_eq!(gap["detail"].as_str(), detail);
        assert_eq!(
            gap["code"].as_str(),
            (mode == "reported-a").then_some("parser-failed")
        );
        assert_eq!(fs::read(out.join("syntax.jsonl")).unwrap().len(), 0);
    }
    for (a, b) in [("reported-a", "reported-b"), ("good", "extra")] {
        let other = temp.path().join(b);
        check(&cli(a, &root, &out, &cache), 0);
        check(&cli(b, &root, &other, &cache), 0);
        assert_eq!(listing(&out), listing(&other));
        if a == "good" {
            let row: Value =
                serde_json::from_slice(&fs::read(out.join("syntax.jsonl")).unwrap()).unwrap();
            assert_eq!(row["visibility"], json!("pub"));
        }
    }
}

#[test]
fn read_only_input_and_refused_publication_targets_are_preserved() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("project");
    let cache = temp.path().join("cache");
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    for path in ["Cargo.toml", "src/views.rs", "src/lib.rs", "index.scip"] {
        fs::copy(fixture().join("rust-basic").join(path), root.join(path)).unwrap();
    }
    fs::write(root.join(".gitignore"), "ignored.bin\n").unwrap();
    fs::write(root.join("ignored.bin"), [0, 255]).unwrap();
    let paths = listing(&root);
    for path in paths.keys().rev() {
        let p = root.join(path);
        let mode = if p.is_dir() { 0o555 } else { 0o444 };
        fs::set_permissions(&p, fs::Permissions::from_mode(mode)).unwrap();
    }
    let before = listing(&root);
    let out = temp.path().join("snapshot");
    let success = cli("good", &root, &out, &cache);
    let after = listing(&root);
    // Restore directory writability for TempDir cleanup, including when assertions fail.
    for p in [&root, &src] {
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    check(&success, 0);
    assert_eq!(after, before);
    let surrounding = listing(temp.path());
    for target in [&root, &src.join("out"), temp.path()] {
        check(&cli("good", &root, target, &cache), 2);
        assert_eq!(listing(temp.path()), surrounding);
    }
    let foreign = temp.path().join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("manifest.json"), "{}").unwrap();
    let before = listing(&foreign);
    check(&cli("good", &root, &foreign, &cache), 2);
    assert_eq!(listing(&foreign), before);
}

#[cfg(target_os = "linux")]
#[test]
fn cache_is_outside_the_project_even_when_a_missing_directory_has_a_symlink_parent() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("project");
    fs::create_dir(&root).unwrap();
    let alias = tmp.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let before = listing(&root);
    let out = tmp.path().join("result");
    check(&cli("good", &root, &out, &alias.join("missing")), 3);
    assert_eq!(listing(&root), before);
    assert!(!out.exists());
    let cache = tmp.path().join("new cache");
    let run = cli("show-cache", &fixture().join("rust-basic"), &out, &cache);
    check(&run, 0);
    let stderr = String::from_utf8(run.stderr).unwrap();
    let scratch = Path::new(stderr.trim());
    assert_eq!(scratch.parent(), Some(cache.join("gosanja").as_path()));
    assert!(!scratch.exists(), "temporary index must be cleaned up");
}

#[cfg(target_os = "linux")]
#[test]
fn invalid_cache_paths_report_why_preparation_failed() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("project");
    fs::create_dir(&root).unwrap();
    let file = root.join("cache-config");
    fs::write(&file, b"config").unwrap();
    let before = listing(&root);
    let output = tmp.path().join("out");

    // Refuse a traversal before mkdir can create either intermediate directory.
    let cache = tmp.path().join("intermediate/../other");
    let result = cli("good", &root, &output, &cache);
    check(&result, 3);
    assert!(!tmp.path().join("intermediate").exists());
    assert!(!tmp.path().join("other").exists());
    // The traversal is refused as written, also when it would resolve.
    fs::create_dir(tmp.path().join("intermediate")).unwrap();
    let result = cli("good", &root, &output, &cache);
    check(&result, 3);
    assert!(!tmp.path().join("other").exists());
    assert!(!output.exists());

    let cache = file.join("subdir");
    let expected = cache.canonicalize().unwrap_err().to_string();
    let result = cli("good", &root, &output, &cache);
    check(&result, 3);
    assert!(String::from_utf8_lossy(&result.stderr).contains(&expected));
    assert_eq!(listing(&root), before);
}

#[test]
fn base_failure_names_only_the_first_diagnostic_as_the_reason() {
    let temp = tempdir().unwrap();
    for (mode, code, message) in [
        ("failed-base-message", true, true),
        ("failed-base-code", true, false),
        ("failed-base", false, false),
    ] {
        let result = cli(
            mode,
            &fixture().join("rust-basic"),
            &temp.path().join("out"),
            &temp.path().join("cache"),
        );
        check(&result, 3);
        let err = String::from_utf8(result.stderr).unwrap();
        assert!(err.contains("base layer failed"), "{err}");
        assert_eq!(err.contains("base layer failed: lockfile-missing"), code);
        assert_eq!(err.contains("missing project lockfile"), message);
        assert!(!err.contains("base layer failed: other-code"));
        if !code {
            assert!(!err.contains("base layer failed:"));
        }
    }
}

#[test]
fn duplicate_build_ids_fail_before_diagnostics_or_the_base_status() {
    let temp = tempdir().unwrap();
    let result = cli(
        "duplicate-builds-failed-base",
        &fixture().join("rust-basic"),
        &temp.path().join("out"),
        &temp.path().join("cache"),
    );
    check(&result, 3);
    let err = String::from_utf8(result.stderr).unwrap();
    assert!(err.contains("duplicate build context id"), "{err}");
    assert!(!err.contains("missing project lockfile"), "{err}");
    assert!(!err.contains("base layer failed"), "{err}");
}

#[test]
fn an_old_result_is_refused_for_its_contract_before_the_schema() {
    let temp = tempdir().unwrap();
    let out = temp.path().join("out");
    let result = cli(
        "wrong-result-contract",
        &fixture().join("rust-basic"),
        &out,
        &temp.path().join("cache"),
    );
    check(&result, 3);
    let err = String::from_utf8(result.stderr).unwrap();
    assert!(
        err.contains("result contract differs from describe"),
        "{err}"
    );
    assert!(!err.contains("calls"), "{err}");
    assert!(!out.exists());
}

#[test]
fn skipped_files_survive_in_order_only_on_retained_layers() {
    let temp = tempdir().unwrap();
    let root = fixture().join("rust-basic");
    let cache = temp.path().join("cache");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let skip = |file, layer, code| json!({"gap":"skipped-file","file":file,"layer":layer,"code":code,"src":format!("adapter:fake/{layer}")});
    for (mode, out) in [("skipped", &first), ("skipped-reverse", &second)] {
        check(&cli(mode, &root, out, &cache), 0);
        let rows: Vec<Value> = fs::read_to_string(out.join("gaps.jsonl"))
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .filter(|v| v["gap"] == "skipped-file")
            .collect();
        assert_eq!(
            rows,
            [
                skip("absent.rs", "calls", "unreadable"),
                skip("src/views.rs", "base", "unreadable"),
                skip("src/views.rs", "syntax", "parse-error"),
                skip("src/views.rs", "syntax", "unreadable")
            ]
        );
        check(
            &binary()
                .arg("validate")
                .arg(out)
                .arg("--root")
                .arg(&root)
                .output()
                .unwrap(),
            0,
        );
    }
    assert_eq!(listing(&first), listing(&second));
    for mode in ["skip-failed", "skip-duplicate", "skip-extent"] {
        check(&cli(mode, &root, &first, &cache), 0);
        let gaps = fs::read_to_string(first.join("gaps.jsonl")).unwrap();
        assert!(gaps.contains("failed-layer"), "{mode}: {gaps}");
        assert!(!gaps.contains("skipped-file"), "{mode}: {gaps}");
        assert!(!gaps.contains("unknown-value"));
    }
    check(&cli("skip-invalid", &root, &first, &cache), 3);
}

#[test]
fn unrequested_adapter_diagnostics_do_not_escape() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    for (selection, expected) in [("base,syntax", false), ("calls,base", true)] {
        let out = temp.path().join(selection);
        let mut command = binary();
        command
            .args(["index", "--adapter"])
            .arg(fixture().join("fake-adapter/adapter.sh"))
            .arg("--root")
            .arg(&root)
            .arg("--out")
            .arg(&out)
            .args(["--layers", selection]);
        configure(&mut command, "failing-calls", &temp.path().join("cache"));
        let output = command.output().unwrap();
        check(&output, 0);
        let stderr = String::from_utf8(output.stderr).unwrap();
        for marker in ["probe-calls-error", "the calls probe reported a failure"] {
            assert_eq!(stderr.contains(marker), expected, "{stderr}");
        }
        let gaps = fs::read_to_string(out.join("gaps.jsonl")).unwrap_or_default();
        assert_eq!(gaps.contains("probe-calls-error"), expected, "{gaps}");
        assert_eq!(gaps.contains("failed-layer"), expected, "{gaps}");
        let manifest: Value =
            serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["scopes"][0]["layers"]["calls"],
            if expected { "failed" } else { "not-requested" }
        );
    }
}
