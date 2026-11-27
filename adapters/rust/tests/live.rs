//! The adapter against the real `rust-analyzer`, in the caller's own environment. Every test
//! here is ignored in the gate and run by `scripts/check-live.sh`, which first checks that
//! the indexer is the version the fixtures were made with. Build and output directories lie
//! under `/var/tmp`; the workspaces are written by the tests.
#![cfg(unix)]

#[expect(dead_code, reason = "these runs use part of the shared harness")]
mod support;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use support::{adapter_binary, engine, listing};

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in("/var/tmp").expect("a directory under /var/tmp")
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }
}

/// Writes `Cargo.lock` without touching anything else of the tree.
fn lock(root: &Path) {
    let status = Command::new("cargo")
        .args(["generate-lockfile", "--offline"])
        .current_dir(root)
        .status()
        .expect("cargo runs");
    assert!(
        status.success(),
        "cargo generate-lockfile in {}",
        root.display()
    );
}

fn read_only(root: &Path, on: bool) {
    for entry in walk(root) {
        let meta = fs::symlink_metadata(&entry).expect("stat");
        if meta.file_type().is_symlink() {
            continue;
        }
        let mode = match (meta.is_dir(), on) {
            (true, true) => 0o555,
            (true, false) => 0o755,
            (false, true) => 0o444,
            (false, false) => 0o644,
        };
        fs::set_permissions(&entry, fs::Permissions::from_mode(mode)).expect("chmod");
    }
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_owned()];
    let mut i = 0;
    while i < out.len() {
        let path = out[i].clone();
        if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
            let mut children: Vec<_> = fs::read_dir(&path)
                .expect("readable")
                .map(|e| e.expect("an entry").path())
                .collect();
            children.sort();
            out.extend(children);
        }
        i += 1;
    }
    out
}

/// What differs between two listings: entries added, removed or changed.
fn difference(
    before: &BTreeMap<PathBuf, (u32, Vec<u8>)>,
    after: &BTreeMap<PathBuf, (u32, Vec<u8>)>,
) -> Vec<PathBuf> {
    let mut paths: Vec<_> = before
        .keys()
        .chain(after.keys())
        .filter(|p| before.get(*p) != after.get(*p))
        .cloned()
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Runs the adapter on `root` into a fresh output directory; the result and the output.
fn adapter(root: &Path, env: &[(&str, &OsStr)]) -> (Value, tempfile::TempDir) {
    let out = scratch();
    let mut command = Command::new(adapter_binary());
    command
        .arg("index")
        .arg("--root")
        .arg(root)
        .arg("--out")
        .arg(out.path());
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("the adapter runs");
    assert!(
        output.status.success(),
        "exit {:?}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value =
        serde_json::from_slice(&fs::read(out.path().join("result.json")).expect("result.json"))
            .expect("JSON");
    if result["layers"]["base"]["status"] != "ok" {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }
    (result, out)
}

/// Runs `gosanja index` with the adapter on `root`; the snapshot directory. The Cargo home
/// is made for the run and holds no configuration, as for the goldens, so that flags in the
/// contributor's own Cargo home cannot decide the calls layer; the projects run here have
/// path dependencies only.
fn snapshot(root: &Path, cache: &Path) -> PathBuf {
    let out = cache.join("snapshot");
    let output = Command::new(engine())
        .arg("index")
        .arg("--adapter")
        .arg(adapter_binary())
        .arg("--root")
        .arg(root)
        .arg("--out")
        .arg(&out)
        .env("XDG_CACHE_HOME", cache.join("cache"))
        .env("CARGO_HOME", cache.join("cargo-home"))
        .output()
        .expect("gosanja runs");
    assert!(
        output.status.success(),
        "exit {:?}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    out
}

fn base(result: &Value) -> &Value {
    &result["layers"]["base"]
}

fn table(dir: &Path, name: &str) -> Vec<Value> {
    fs::read_to_string(dir.join(name))
        .expect("a table")
        .lines()
        .map(|l| serde_json::from_str(l).expect("JSON"))
        .collect()
}

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn the_real_indexer_gives_the_golden_snapshot_on_a_relocated_read_only_copy() {
    let temp = scratch();
    let root = temp.path().join("rust-flow");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-flow");
    copy(&fixture, &root);
    read_only(&root, true);
    let before = listing(&root);
    let snap = snapshot(&root, temp.path());
    let after = listing(&root);
    read_only(&root, false);
    assert_eq!(difference(&before, &after), Vec::<PathBuf>::new());
    let golden = fixture.join("expected");
    for entry in fs::read_dir(&golden).expect("the golden") {
        let name = entry.expect("an entry").file_name();
        assert_eq!(
            fs::read(snap.join(&name)).expect("written"),
            fs::read(golden.join(&name)).expect("checked in"),
            "{}",
            name.to_string_lossy()
        );
    }
    assert_eq!(
        fs::read_dir(&snap).expect("a snapshot").count(),
        fs::read_dir(&golden).expect("the golden").count()
    );
}

/// The sources of the fixture, without its index and its golden.
fn copy(fixture: &Path, root: &Path) {
    for path in walk(fixture) {
        let relative = path.strip_prefix(fixture).expect("inside");
        let first = relative
            .components()
            .next()
            .map(|c| c.as_os_str().to_owned());
        if first
            .as_deref()
            .is_some_and(|f| f == "expected" || f == "index.scip")
        {
            continue;
        }
        let to = root.join(relative);
        if path.is_dir() {
            fs::create_dir_all(&to).expect("mkdir");
        } else {
            fs::copy(&path, &to).expect("copy");
        }
    }
}

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn a_build_script_that_writes_into_its_source_is_the_only_change() {
    let temp = scratch();
    let root = temp.path().join("ws");
    write(
        &root,
        &[
            (
                "Cargo.toml",
                "[workspace]\n\n[package]\nname = \"scripted\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            (
                "build.rs",
                "fn main() {\n    std::fs::write(\"src/from_build.txt\", \"written by build.rs\\n\").unwrap();\n}\n",
            ),
            ("src/lib.rs", "pub fn f() {}\n"),
        ],
    );
    lock(&root);
    let before = listing(&root);
    let (result, _out) = adapter(&root, &[]);
    assert_eq!(base(&result)["status"], "ok", "{result}");
    assert_eq!(
        difference(&before, &listing(&root)),
        [PathBuf::from("src/from_build.txt")]
    );
}

/// Build directories configured beneath the workspace root.
const BUILD_DIRS: &str = "[build]\ntarget-dir = \"inner-target\"\nbuild-dir = \"inner-build\"\n";

/// A locked library without project code.
fn plain_workspace(root: &Path, config: Option<&str>) {
    write(
        root,
        &[
            (
                "Cargo.toml",
                "[workspace]\n\n[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("src/lib.rs", "pub fn f() -> u8 { 1 }\n"),
        ],
    );
    if let Some(config) = config {
        write(root, &[(".cargo/config.toml", config)]);
    }
    lock(root);
}

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn build_directories_configured_in_the_tree_stay_out_of_it() {
    let temp = scratch();
    let root = temp.path().join("ws");
    plain_workspace(&root, Some(BUILD_DIRS));
    let before = listing(&root);
    let (result, _out) = adapter(&root, &[]);
    assert_eq!(base(&result)["status"], "ok", "{result}");
    assert_eq!(difference(&before, &listing(&root)), Vec::<PathBuf>::new());
}

/// `inotifywait` watching `root` recursively; the events it reported when stopped.
struct Watch(Child, Arc<Mutex<Vec<String>>>);

impl Watch {
    fn start(root: &Path) -> Self {
        Self::start_for(root, "create,modify,delete,move")
    }

    fn start_for(root: &Path, events: &str) -> Self {
        let mut child = Command::new("inotifywait")
            .args(["-m", "-r", "-e", events, "--format", "%w%f %e"])
            .arg(root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("inotifywait runs; scripts/check-live.sh requires it");
        // Wait until the watches are in place.
        let mut stderr = BufReader::new(child.stderr.take().expect("stderr"));
        let mut line = String::new();
        while stderr.read_line(&mut line).expect("readable") > 0
            && !line.contains("Watches established")
        {
            line.clear();
        }
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let stdout = child.stdout.take().expect("stdout");
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                sink.lock().expect("unpoisoned").push(line);
            }
        });
        Self(child, events)
    }

    fn stop(mut self) -> Vec<String> {
        std::thread::sleep(Duration::from_millis(500));
        self.0.kill().expect("stopped");
        self.0.wait().expect("reaped");
        std::thread::sleep(Duration::from_millis(100));
        self.1.lock().expect("unpoisoned").clone()
    }
}

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn destinations_in_the_callers_environment_stay_out_of_the_tree() {
    let temp = scratch();
    let root = temp.path().join("ws");
    // The same workspace as with the configured build directories.
    plain_workspace(&root, Some(BUILD_DIRS));
    // A control: the watch sees a file created and removed in the tree.
    let watch = Watch::start(&root);
    fs::write(root.join("src/control.txt"), "x").expect("write");
    fs::remove_file(root.join("src/control.txt")).expect("rm");
    let control = watch.stop();
    assert!(
        control.iter().any(|e| e.ends_with("control.txt CREATE")),
        "{control:?}"
    );
    assert!(
        control.iter().any(|e| e.ends_with("control.txt DELETE")),
        "{control:?}"
    );

    let before = listing(&root);
    let watch = Watch::start(&root);
    let inside = |name: &str| root.join(name).into_os_string();
    let (log_file, tmp_dir, target_dir) = (inside("ra.log"), inside("tmp"), inside("target"));
    let (result, _out) = adapter(
        &root,
        &[
            ("RA_LOG_FILE", log_file.as_os_str()),
            ("RA_LOG", OsStr::new("info")),
            ("TMPDIR", tmp_dir.as_os_str()),
            ("CARGO_TARGET_DIR", target_dir.as_os_str()),
        ],
    );
    let events = watch.stop();
    assert_eq!(base(&result)["status"], "ok", "{result}");
    assert_eq!(events, Vec::<String>::new());
    assert_eq!(difference(&before, &listing(&root)), Vec::<PathBuf>::new());
}

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn compiler_flags_from_the_projects_configuration_stay_in_force() {
    let temp = scratch();
    let root = temp.path().join("ws");
    let emitted = root.join("emitted.d");
    write(
        &root,
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"app\", \"mac\"]\nresolver = \"2\"\n",
            ),
            (
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmac = { path = \"../mac\" }\n",
            ),
            ("app/src/lib.rs", "#[mac::keep]\npub fn kept() {}\n"),
            (
                "mac/Cargo.toml",
                "[package]\nname = \"mac\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\nproc-macro = true\n",
            ),
            (
                "mac/src/lib.rs",
                "use proc_macro::TokenStream;\n\n#[proc_macro_attribute]\npub fn keep(_: TokenStream, item: TokenStream) -> TokenStream {\n    item\n}\n",
            ),
        ],
    );
    write(
        &root,
        &[(
            ".cargo/config.toml",
            &format!(
                "[build]\nrustflags = [\"--emit=dep-info={}\"]\n",
                emitted.display()
            ),
        )],
    );
    lock(&root);
    // A control: the pinned indexer run directly, with fresh redirected build directories,
    // creates the file.
    let control = scratch();
    let status = Command::new("rust-analyzer")
        .arg("scip")
        .arg(".")
        .arg("--output")
        .arg(control.path().join("index.scip"))
        .current_dir(&root)
        .env("CARGO_TARGET_DIR", control.path().join("target"))
        .env("CARGO_BUILD_BUILD_DIR", control.path().join("build"))
        .stderr(Stdio::null())
        .status()
        .expect("rust-analyzer runs");
    assert!(status.success());
    assert!(
        emitted.is_file(),
        "the control did not create {}",
        emitted.display()
    );
    fs::remove_file(&emitted).expect("rm");

    let before = listing(&root);
    let (result, _out) = adapter(&root, &[]);
    assert_eq!(base(&result)["status"], "ok", "{result}");
    assert_eq!(
        difference(&before, &listing(&root)),
        [PathBuf::from("emitted.d")]
    );
}

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn a_path_dependency_outside_the_root_is_an_external() {
    let temp = scratch();
    write(
        temp.path(),
        &[
            (
                "sibling/Cargo.toml",
                "[package]\nname = \"sibling\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("sibling/src/lib.rs", "pub fn letter() -> u8 { b'a' }\n"),
            (
                "ws/Cargo.toml",
                "[workspace]\n\n[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nsibling = { path = \"../sibling\" }\n",
            ),
            (
                "ws/src/lib.rs",
                "pub fn uses() -> u8 { sibling::letter() }\n",
            ),
        ],
    );
    let root = temp.path().join("ws");
    lock(&root);
    let cache = scratch();
    let snap = snapshot(&root, cache.path());
    let manifest: Value =
        serde_json::from_slice(&fs::read(snap.join("manifest.json")).expect("a manifest"))
            .expect("JSON");
    assert_eq!(manifest["scopes"][0]["layers"]["base"], "present");
    let files: Vec<_> = table(&snap, "files.jsonl")
        .into_iter()
        .map(|f| f["path"].clone())
        .collect();
    assert_eq!(files, [Value::from("src/lib.rs")]);
    let syntax = table(&snap, "syntax.jsonl");
    assert!(
        syntax.iter().all(|r| r["key"]
            .as_str()
            .is_some_and(|k| k.starts_with("cargo:user:"))),
        "{syntax:?}"
    );
    let letter = table(&snap, "externals.jsonl")
        .into_iter()
        .find(|e| {
            e["key"]
                .as_str()
                .is_some_and(|k| k.starts_with("cargo:sibling:") && k.ends_with("letter()."))
        })
        .expect("an external for sibling's letter");
    assert!(
        table(&snap, "relations.jsonl")
            .iter()
            .any(|r| r["from"] == "cargo:user:uses()." && r["to"] == letter["key"]),
        "no relation to {letter}"
    );
}

/// A local HTTP listener that answers 404 and counts the requests.
fn listener() -> (String, Arc<Mutex<Vec<String>>>) {
    let socket = TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = format!("http://{}", socket.local_addr().expect("an address"));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in socket.incoming().map_while(Result::ok) {
            let mut stream = stream;
            let mut head = [0; 4096];
            let n = stream.read(&mut head).unwrap_or(0);
            let line = String::from_utf8_lossy(&head[..n])
                .lines()
                .next()
                .unwrap_or("")
                .to_owned();
            sink.lock().expect("unpoisoned").push(line);
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    (address, requests)
}

fn toolchains() -> String {
    let output = Command::new("rustup")
        .args(["toolchain", "list"])
        .output()
        .expect("rustup runs");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn a_pinned_toolchain_that_is_not_installed_is_not_installed() {
    let temp = scratch();
    let root = temp.path().join("ws");
    plain_workspace(&root, None);
    write(
        &root,
        &[(
            "rust-toolchain.toml",
            "[toolchain]\nchannel = \"nightly-2001-01-01\"\n",
        )],
    );
    let (server, requests) = listener();
    // The rustup proxies come first; the pin takes effect without RUSTUP_TOOLCHAIN.
    let path = std::env::join_paths(
        ["/usr/bin".into(), "/usr/lib/rustup/bin".into()]
            .into_iter()
            .chain(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            )),
    )
    .expect("joinable");
    let listed = toolchains();
    // A control: the proxy with automatic installation left on reaches the listener.
    let control = Command::new("cargo")
        .arg("--version")
        .current_dir(&root)
        .env("PATH", &path)
        .env("RUSTUP_DIST_SERVER", &server)
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("RUSTUP_AUTO_INSTALL")
        .output()
        .expect("the proxy runs");
    assert!(!control.status.success());
    let reached = requests.lock().expect("unpoisoned").len();
    assert!(reached > 0, "the control made no request");

    let out = scratch();
    let output = Command::new(adapter_binary())
        .arg("index")
        .arg("--root")
        .arg(&root)
        .arg("--out")
        .arg(out.path())
        .env("PATH", &path)
        .env("RUSTUP_DIST_SERVER", &server)
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("RUSTUP_AUTO_INSTALL")
        .output()
        .expect("the adapter runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value =
        serde_json::from_slice(&fs::read(out.path().join("result.json")).expect("result.json"))
            .expect("JSON");
    assert_eq!(
        base(&result)["diagnostics"],
        serde_json::json!([{"code": "toolchain-missing"}])
    );
    assert_eq!(
        requests.lock().expect("unpoisoned").len(),
        reached,
        "the adapter reached the listener"
    );
    assert_eq!(toolchains(), listed);
}

/// The published crate of the live check: its archive's name and SHA-256.
const RYU: &str = "ryu-1.0.23";
const RYU_SHA256: &str = "9774ba4a74de5f7b1c1451ed6cd5285a32eddb5cccb8cc655a4e50009e06477f";

/// What the first run of the finished adapter gave on the crate: the number of rows of
/// `calls.jsonl`, the `skipped` entries of `result.json` as (file, code), and the rows that
/// `sample` draws, each checked by hand against the source (see the research record).
struct Baseline {
    rows: usize,
    skipped: &'static [(&'static str, &'static str)],
    sample: &'static [&'static str],
}

/// The first run of the finished adapter, on 2026-11-25. `None` would make the test fail and
/// print what it observed, as it did for that run.
const RYU_BASELINE: Option<Baseline> = Some(Baseline {
    rows: 49,
    skipped: &[("src/pretty/mod.rs", "uncorroborated")],
    sample: &[
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/common.rs","line":63,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":128,"col":43},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/common.rs","line":63,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":170,"col":16},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/common.rs","line":89,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":167,"col":16},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":23,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":152,"col":69},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":28,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":219,"col":27},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":28,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":236,"col":31},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":28,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":241,"col":31},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":28,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":279,"col":27},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":28,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":284,"col":27},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":33,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":262,"col":24},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":33,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":266,"col":28},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":55,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":159,"col":39},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":55,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":162,"col":22},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s.rs","line":90,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":77,"col":14},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s.rs","line":131,"col":12},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s_intrinsics.rs","line":55,"col":7},"callee":{"file":"src/d2s_intrinsics.rs","line":38,"col":14},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s_intrinsics.rs","line":57,"col":4},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s_intrinsics.rs","line":77,"col":14},"callee":{"file":"src/d2s_intrinsics.rs","line":70,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s_intrinsics.rs","line":85,"col":19},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/d2s_intrinsics.rs","line":77,"col":14},"callee":{"file":"src/d2s_intrinsics.rs","line":70,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/d2s_intrinsics.rs","line":87,"col":4},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/common.rs","line":63,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":107,"col":32},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/common.rs","line":63,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":73,"col":42},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/common.rs","line":80,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":71,"col":16},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":100,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":103,"col":13},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":100,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":104,"col":13},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":100,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":108,"col":34},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":43,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":92,"col":39},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":43,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":94,"col":22},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":74,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":75,"col":13},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":74,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":77,"col":13},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s.rs","line":39,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":74,"col":7},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s.rs","line":84,"col":17},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/f2s_intrinsics.rs","line":43,"col":7},"callee":{"file":"src/f2s_intrinsics.rs","line":26,"col":3},"mechanism":"static","resolution":"direct","site":{"file":"src/f2s_intrinsics.rs","line":44,"col":4},"build":"ryu-lib-ryu"}"#,
        r#"{"caller":{"file":"src/pretty/mantissa.rs","line":4,"col":14},"callee":{"file":"src/pretty/mantissa.rs","line":27,"col":14},"mechanism":"static","resolution":"direct","site":{"file":"src/pretty/mantissa.rs","line":23,"col":4},"build":"ryu-lib-ryu"}"#,
    ],
});

/// The local Cargo home, whose registry cache holds the crate archive.
fn local_cargo_home() -> PathBuf {
    std::env::var_os("CARGO_HOME").map_or_else(
        || PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".cargo"),
        PathBuf::from,
    )
}

/// The registry of `home` whose cache holds the archive, by its directory name.
fn registry_holding(home: &Path, archive: &str) -> String {
    fs::read_dir(home.join("registry/cache"))
        .expect("a registry cache in the local Cargo home")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .find(|name| {
            home.join("registry/cache")
                .join(name)
                .join(archive)
                .is_file()
        })
        .unwrap_or_else(|| {
            panic!(
                "{archive} is not in the local Cargo cache; `cargo fetch` in a project that \
                 depends on it puts it there"
            )
        })
}

/// SHA-256 of a file, by `sha256sum`: the adapter's test dependencies hold no hash of that
/// kind, and the live check runs on Linux only.
fn sha256(path: &Path) -> String {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum runs");
    assert!(output.status.success(), "sha256sum failed");
    String::from_utf8(output.stdout).expect("UTF-8")[..64].to_owned()
}

/// Where a sparse registry index keeps the entry of a package.
fn index_entry(name: &str) -> PathBuf {
    let name = name.to_lowercase();
    match name.len() {
        1 => Path::new("1").join(&name),
        2 => Path::new("2").join(&name),
        3 => Path::new("3").join(&name[..1]).join(&name),
        _ => Path::new(&name[..2]).join(&name[2..4]).join(&name),
    }
}

/// Fills `home` with what the local Cargo home holds of the registry packages `lock` names:
/// their index entries and archives, and the index's own `config.json`. Nothing else, so no
/// configuration. Returns how many archives were copied and how many the local cache lacks.
fn private_cargo_home(local: &Path, registry: &str, lock: &Path, home: &Path) -> (usize, usize) {
    let index = local.join("registry/index").join(registry);
    let cache = local.join("registry/cache").join(registry);
    let to_index = home.join("registry/index").join(registry);
    let to_cache = home.join("registry/cache").join(registry);
    fs::create_dir_all(&to_cache).expect("mkdir");
    fs::create_dir_all(&to_index).expect("mkdir");
    fs::copy(index.join("config.json"), to_index.join("config.json")).expect("config.json");
    let text = fs::read_to_string(lock).expect("Cargo.lock");
    let (mut copied, mut absent) = (0, 0);
    for package in text.split("[[package]]").skip(1) {
        let field = |key: &str| {
            package
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{key} = \"")))
                .map(|v| v.trim_end_matches('"').to_owned())
        };
        let (Some(name), Some(version)) = (field("name"), field("version")) else {
            continue;
        };
        if !field("source").is_some_and(|s| s.starts_with("registry+")) {
            continue;
        }
        let entry = Path::new(".cache").join(index_entry(&name));
        if index.join(&entry).is_file() && !to_index.join(&entry).is_file() {
            fs::create_dir_all(to_index.join(&entry).parent().expect("a parent")).expect("mkdir");
            fs::copy(index.join(&entry), to_index.join(&entry)).expect("an index entry");
        }
        let archive = format!("{name}-{version}.crate");
        if cache.join(&archive).is_file() {
            fs::copy(cache.join(&archive), to_cache.join(&archive)).expect("an archive");
            copied += 1;
        } else {
            absent += 1;
        }
    }
    (copied, absent)
}

/// The first Cargo configuration file that a build in `dir` with `cargo_home` would read:
/// `.cargo/config` or `.cargo/config.toml` in the resolved directory or any directory above
/// it up to the root of the file system, or `config` or `config.toml` in the Cargo home.
fn cargo_configuration(dir: &Path, cargo_home: &Path) -> Option<PathBuf> {
    let resolved = dir.canonicalize().expect("a directory");
    resolved
        .ancestors()
        .map(|d| d.join(".cargo"))
        .chain(std::iter::once(cargo_home.to_owned()))
        .flat_map(|d| [d.join("config"), d.join("config.toml")])
        .find(|p| p.exists())
}

/// Thirty rows drawn from `rows`, which are sorted: indices floor(i × (N − 1) / 29) for i
/// from 0 to 29, or every row when there are fewer than thirty.
fn sample(rows: &[String]) -> Vec<String> {
    if rows.len() < 30 {
        return rows.to_vec();
    }
    (0..30)
        .map(|i| rows[i * (rows.len() - 1) / 29].clone())
        .collect()
}

/// Runs `command` to its end, reading the process's high-water mark of resident memory from
/// `/proc` while it runs: the exit status, the wall time and the last mark read, in KiB. The
/// mark is that of the process itself, not of the tools it starts.
fn measured(command: &mut Command) -> (std::process::ExitStatus, Duration, Option<u64>) {
    let start = std::time::Instant::now();
    let mut child = command.spawn().expect("the process starts");
    let status_file = PathBuf::from(format!("/proc/{}/status", child.id()));
    let mut peak = None;
    loop {
        if let Some(mark) = fs::read_to_string(&status_file).ok().and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("VmHWM:"))
                .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
        }) {
            peak = Some(mark);
        }
        if let Some(status) = child.try_wait().expect("waitable") {
            return (status, start.elapsed(), peak);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The toolchain `rust-toolchain.toml` of this repository names.
fn pinned_toolchain() -> String {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rust-toolchain.toml");
    fs::read_to_string(file)
        .expect("rust-toolchain.toml")
        .lines()
        .find_map(|l| {
            l.strip_prefix("channel = \"")
                .map(|v| v.trim_end_matches('"').to_owned())
        })
        .expect("a channel")
}

/// A command with only the environment the live check gives: the caller's `PATH` and home,
/// rustup's home, the pinned toolchain, the private Cargo home, and Cargo offline.
fn live_command(program: &Path, cargo_home: &Path, toolchain: &str) -> Command {
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let rustup_home =
        std::env::var_os("RUSTUP_HOME").map_or_else(|| home.join(".rustup"), PathBuf::from);
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").expect("PATH"))
        .env("HOME", &home)
        .env("RUSTUP_HOME", rustup_home)
        .env("RUSTUP_TOOLCHAIN", toolchain)
        .env("CARGO_HOME", cargo_home)
        .env("CARGO_NET_OFFLINE", "true")
        .env("TMPDIR", "/var/tmp");
    command
}

/// The crate unpacked in `temp` from the local Cargo cache, its archive's digest checked, and
/// a private Cargo home beside it: the crate's directory, the Cargo home, and how many locked
/// archives were copied and how many the local cache lacks.
fn unpacked_ryu(temp: &Path) -> (PathBuf, PathBuf, usize, usize) {
    let local = local_cargo_home();
    let archive_name = format!("{RYU}.crate");
    let registry = registry_holding(&local, &archive_name);
    let archive = temp.join(&archive_name);
    let cached = local
        .join("registry/cache")
        .join(&registry)
        .join(&archive_name);
    fs::copy(cached, &archive).expect("copy the archive");
    assert_eq!(
        sha256(&archive),
        RYU_SHA256,
        "the archive is not the published one"
    );
    let unpacked = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(temp)
        .status()
        .expect("tar runs");
    assert!(unpacked.success(), "tar failed");
    let root = temp.join(RYU);
    let cargo_home = temp.join("cargo-home");
    let (copied, absent) =
        private_cargo_home(&local, &registry, &root.join("Cargo.lock"), &cargo_home);
    (root, cargo_home, copied, absent)
}

/// Stops when Cargo would read any configuration for the crate, after a control in which a
/// configuration one directory above the crate has to be found, by name.
fn no_configuration_above(temp: &Path, root: &Path, cargo_home: &Path) {
    let above = temp.join(".cargo/config.toml");
    fs::create_dir_all(above.parent().expect("a parent")).expect("mkdir");
    fs::write(&above, "[build]\n").expect("write");
    assert_eq!(
        cargo_configuration(root, cargo_home),
        Some(above.canonicalize().expect("written")),
        "the control configuration was not found"
    );
    fs::remove_dir_all(above.parent().expect("a parent")).expect("remove the control");
    if let Some(file) = cargo_configuration(root, cargo_home) {
        panic!(
            "{} would be in force for the build of {RYU}; the counts are not compared",
            file.display()
        );
    }
}

/// What one run of the adapter on the crate gave.
struct Observed {
    builds: Value,
    rows: Vec<String>,
    skipped: Vec<(String, String)>,
    ir_bytes: u64,
    elapsed: Duration,
    peak: Option<u64>,
}

/// Runs the adapter on `root` into `out`, measured, and reads what it wrote.
fn observed(root: &Path, out: &Path, cargo_home: &Path, toolchain: &str) -> Observed {
    let (status, elapsed, peak) = measured(
        live_command(adapter_binary(), cargo_home, toolchain)
            .arg("index")
            .arg("--root")
            .arg(root)
            .arg("--out")
            .arg(out)
            .stdout(Stdio::null()),
    );
    assert!(status.success(), "the adapter exited with {status}");
    let result: Value =
        serde_json::from_slice(&fs::read(out.join("result.json")).expect("result.json"))
            .expect("JSON");
    for layer in ["base", "syntax", "calls"] {
        assert_eq!(result["layers"][layer]["status"], "ok", "{result}");
    }
    let skipped = result["layers"]["calls"]["skipped"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|e| {
                    (
                        e["file"].as_str().expect("a file").to_owned(),
                        e["code"].as_str().expect("a code").to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Observed {
        builds: result["builds"].clone(),
        rows: fs::read_to_string(out.join("calls.jsonl"))
            .expect("calls.jsonl")
            .lines()
            .map(str::to_owned)
            .collect(),
        skipped,
        ir_bytes: fs::read_dir(out.join("work/calls"))
            .expect("the IR directory")
            .map(|e| e.expect("an entry").metadata().expect("metadata").len())
            .sum(),
        elapsed,
        peak,
    }
}

/// `gosanja index` with the adapter on `root`, then `gosanja validate`: the snapshot's call
/// relations, their evidence entries, and its gaps counted by kind and code.
fn snapshot_summary(
    root: &Path,
    cache: &Path,
    cargo_home: &Path,
    toolchain: &str,
) -> (usize, usize, BTreeMap<String, usize>) {
    fs::create_dir_all(cache).expect("mkdir");
    let snapshot = cache.join("snapshot");
    let indexed = live_command(engine(), cargo_home, toolchain)
        .env("XDG_CACHE_HOME", cache.join("cache"))
        .arg("index")
        .arg("--adapter")
        .arg(adapter_binary())
        .arg("--root")
        .arg(root)
        .arg("--out")
        .arg(&snapshot)
        .output()
        .expect("gosanja runs");
    assert!(
        indexed.status.success(),
        "{}",
        String::from_utf8_lossy(&indexed.stderr)
    );
    let validated = Command::new(engine())
        .arg("validate")
        .arg(&snapshot)
        .arg("--root")
        .arg(root)
        .output()
        .expect("gosanja validate runs");
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    let calls: Vec<Value> = table(&snapshot, "relations.jsonl")
        .into_iter()
        .filter(|r| r["src"] == "adapter:rust/calls")
        .collect();
    let evidence = calls
        .iter()
        .map(|r| r["evidence"].as_array().map_or(0, Vec::len))
        .sum();
    let mut gaps = BTreeMap::new();
    for gap in table(&snapshot, "gaps.jsonl") {
        let kind = format!(
            "{} {}",
            gap["gap"].as_str().unwrap_or_default(),
            gap["code"].as_str().unwrap_or_default()
        );
        *gaps.entry(kind.trim().to_owned()).or_insert(0) += 1;
    }
    (calls.len(), evidence, gaps)
}

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn a_public_crate_keeps_its_counts() {
    let temp = scratch();
    let (root, cargo_home, copied, absent) = unpacked_ryu(temp.path());
    no_configuration_above(temp.path(), &root, &cargo_home);
    let toolchain = pinned_toolchain();
    let out = temp.path().join("out");
    fs::create_dir(&out).expect("mkdir");
    let run = observed(&root, &out, &cargo_home, &toolchain);
    let (relations, evidence, gaps) =
        snapshot_summary(&root, &temp.path().join("engine"), &cargo_home, &toolchain);
    let drawn = sample(&run.rows);
    eprintln!(
        "{RYU}: archives copied {copied}, absent from the local cache {absent}\n\
         builds {}\nrows {}\nskipped {:?}\n\
         call relations {relations} with {evidence} evidence entries\ngaps {gaps:?}\n\
         adapter wall time {:.1} s, peak resident memory {} KiB, IR {} bytes\nsample:\n{}",
        run.builds,
        run.rows.len(),
        run.skipped,
        run.elapsed.as_secs_f64(),
        run.peak
            .map_or_else(|| "not read".to_owned(), |p| p.to_string()),
        run.ir_bytes,
        drawn.join("\n")
    );

    let Some(baseline) = RYU_BASELINE else {
        panic!("no baseline is recorded for {RYU}; the observed values are printed above");
    };
    assert_eq!(run.rows.len(), baseline.rows, "rows of calls.jsonl");
    let expected: Vec<(String, String)> = baseline
        .skipped
        .iter()
        .map(|(f, c)| ((*f).to_owned(), (*c).to_owned()))
        .collect();
    assert_eq!(run.skipped, expected, "skipped entries");
    assert_eq!(drawn.len(), baseline.sample.len(), "sampled rows");
    for row in baseline.sample {
        assert!(
            run.rows.iter().any(|r| r == row),
            "a sampled row is missing: {row}"
        );
    }
}

#[test]
#[ignore = "needs inotifywait; run by scripts/check-live.sh"]
fn omitted_syntax_does_not_open_its_indexed_source() {
    use protobuf::Message;
    use scip::types::{Document, Index};
    for (request, reads_syntax) in [
        ("base,calls", false),
        ("base", false),
        ("base,syntax", true),
    ] {
        let c = support::Case::new();
        let file = c.root.join("probed.rs");
        fs::write(&file, "fn probed() {}\n").unwrap();
        let index_path = c.root.join("index.scip");
        let mut index = Index::parse_from_bytes(&fs::read(&index_path).unwrap()).unwrap();
        let mut document = Document::new();
        document.relative_path = "probed.rs".into();
        index.documents.push(document);
        fs::write(index_path, index.write_to_bytes().unwrap()).unwrap();
        let watch = Watch::start_for(&c.root, "open");
        let result = c.run(c.index().args(["--layers", request]));
        let events = watch.stop();
        assert_eq!(result["layers"]["base"]["status"], "ok", "{result}");
        assert_eq!(
            result["layers"].get("syntax").is_some(),
            reads_syntax,
            "{result}"
        );
        if request.contains("calls") {
            assert_eq!(result["layers"]["calls"]["status"], "ok", "{result}");
        }
        assert_eq!(
            events
                .iter()
                .filter(|event| event.ends_with("/probed.rs OPEN"))
                .count(),
            usize::from(reads_syntax),
            "{events:?}"
        );
    }
}

const SPLIT_FEATURES: &[(&str, &str)] = &[
    (
        "Cargo.toml",
        r#"[workspace]
resolver = "3"
[package]
name = "consumer"
version = "0.1.0"
edition = "2024"
[dependencies]
shared = { path = "shared", features = ["normal"] }
[build-dependencies]
shared = { path = "shared", features = ["build"] }"#,
    ),
    (
        "src/lib.rs",
        "pub fn run() { shared::normal_only(); shared::common(); }\n",
    ),
    (
        "build.rs",
        "fn main() { shared::build_only(); shared::common(); }\n",
    ),
    (
        "shared/Cargo.toml",
        r#"[package]
name = "shared"
version = "0.1.0"
edition = "2024"
[features]
normal = []
build = []"#,
    ),
    (
        "shared/build.rs",
        r#"fn main() {
    let mut generated = String::new();
    if cfg!(feature = "normal") {
        println!("cargo::rustc-cfg=normal_context");
        generated.push_str("fn generated_normal() {}");
    }
    if cfg!(feature = "build") {
        println!("cargo::rustc-cfg=build_context");
        generated.push_str("fn generated_build() {}");
    }
    let directory = std::env::var_os("OUT_DIR").unwrap();
    std::fs::write(std::path::Path::new(&directory).join("generated.rs"), generated).unwrap();
}"#,
    ),
    (
        "shared/src/lib.rs",
        r#"include!(concat!(env!("OUT_DIR"), "/generated.rs"));
#[cfg(normal_context)]
pub fn normal_only() { generated_normal(); }
#[cfg(build_context)]
pub fn build_only() { generated_build(); }
pub fn common() {}
"#,
    ),
];

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn one_index_preserves_normal_and_build_dependency_contexts() {
    use protobuf::Message as _;
    use scip::types::{Index, SymbolRole};

    let root = scratch();
    write(root.path(), SPLIT_FEATURES);
    lock(root.path());
    read_only(root.path(), true);
    let before = listing(root.path());
    let (result, out) = adapter(root.path(), &[]);
    assert_eq!(base(&result)["status"], "ok", "{result}");
    let bytes = fs::read(out.path().join("index.scip")).expect("one published index");
    let index = Index::parse_from_bytes(&bytes).expect("SCIP");
    let document = |path: &str| {
        let mut matches = index
            .documents
            .iter()
            .filter(|doc| doc.relative_path == path);
        let doc = matches.next().unwrap_or_else(|| panic!("missing {path}"));
        assert!(matches.next().is_none(), "duplicated document {path}");
        doc
    };
    let reference = |path: &str, name: &str| {
        let suffix = format!(" {name}().");
        let matches: std::collections::BTreeSet<_> = document(path)
            .occurrences
            .iter()
            .filter(|occ| {
                occ.symbol_roles & SymbolRole::Definition as i32 == 0
                    && occ.symbol.ends_with(&suffix)
            })
            .map(|occ| occ.symbol.as_str())
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "{path} has references {matches:?} to {name}"
        );
        matches.into_iter().next().expect("one reference")
    };
    let normal = reference("src/lib.rs", "normal_only");
    let build = reference("build.rs", "build_only");
    let normal_common = reference("src/lib.rs", "common");
    let build_common = reference("build.rs", "common");
    assert_ne!(normal_common, build_common);
    assert_ne!(
        reference("shared/src/lib.rs", "generated_normal"),
        reference("shared/src/lib.rs", "generated_build")
    );
    for symbol in [normal, build, normal_common, build_common] {
        assert!(
            document("shared/src/lib.rs").occurrences.iter().any(|occ| {
                occ.symbol == symbol && occ.symbol_roles & SymbolRole::Definition as i32 != 0
            }),
            "missing definition of {symbol}"
        );
    }
    assert_eq!(
        difference(&before, &listing(root.path())),
        [] as [PathBuf; 0]
    );
    read_only(root.path(), false);
}

/// A workspace whose procedural-macro member another member uses, beside a dependency that
/// Cargo builds twice so that the adapter writes its split graph. `cargo check --workspace`
/// checks the macro member on its own, emitting only metadata, and also links it for the
/// member that expands it.
const PROC_MACRO_MEMBER: &[(&str, &str)] = &[
    (
        "Cargo.toml",
        r#"[workspace]
members = ["mac", "shared"]
resolver = "3"
[package]
name = "consumer"
version = "0.1.0"
edition = "2024"
[dependencies]
mac = { path = "mac" }
shared = { path = "shared", features = ["normal"] }
[build-dependencies]
shared = { path = "shared", features = ["build"] }"#,
    ),
    (
        "src/lib.rs",
        "mac::make!();\npub fn run() { made(); shared::common(); }\n",
    ),
    ("build.rs", "fn main() { shared::common(); }\n"),
    (
        "mac/Cargo.toml",
        r#"[package]
name = "mac"
version = "0.1.0"
edition = "2024"
[lib]
proc-macro = true"#,
    ),
    (
        "mac/src/lib.rs",
        "use proc_macro::TokenStream;\n\n#[proc_macro]\npub fn make(_: TokenStream) -> TokenStream {\n    \"pub fn made() {}\".parse().unwrap()\n}\n",
    ),
    (
        "shared/Cargo.toml",
        r#"[package]
name = "shared"
version = "0.1.0"
edition = "2024"
[features]
normal = []
build = []"#,
    ),
    ("shared/src/lib.rs", "pub fn common() {}\n"),
];

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn a_proc_macro_member_checked_on_its_own_keeps_the_split_index() {
    use protobuf::Message as _;
    use scip::types::{Index, SymbolRole};

    let root = scratch();
    write(root.path(), PROC_MACRO_MEMBER);
    lock(root.path());
    let (result, out) = adapter(root.path(), &[]);
    assert_eq!(base(&result)["status"], "ok", "{result}");

    let graph: Value = serde_json::from_slice(
        &fs::read(out.path().join("work/rust-project/rust-project.json")).expect("the split graph"),
    )
    .expect("JSON");
    let mut units: Vec<&Value> = graph["crates"]
        .as_array()
        .expect("crates")
        .iter()
        .filter(|unit| {
            unit["display_name"]
                .as_str()
                .is_some_and(|name| name.starts_with("mac__gosanja_unit_"))
        })
        .collect();
    // `--all-targets` also builds the macro's test harness, which is no procedural macro.
    let harnesses = units
        .iter()
        .filter(|unit| {
            unit["cfg"]
                .as_array()
                .is_some_and(|cfg| cfg.iter().any(|c| c == "test"))
        })
        .inspect(|unit| assert_eq!(unit["is_proc_macro"], false, "{unit}"))
        .count();
    assert_eq!(harnesses, 1, "{units:?}");
    units.retain(|unit| {
        !unit["cfg"]
            .as_array()
            .is_some_and(|cfg| cfg.iter().any(|c| c == "test"))
    });
    assert_eq!(
        units.len(),
        2,
        "the checked and the linked macro unit: {units:?}"
    );
    assert!(
        units.iter().all(|unit| unit["is_proc_macro"] == true),
        "{units:?}"
    );
    let libraries: Vec<&str> = units
        .iter()
        .filter_map(|unit| unit["proc_macro_dylib_path"].as_str())
        .collect();
    assert_eq!(
        libraries.len(),
        1,
        "only the linked unit has a library: {units:?}"
    );
    assert!(Path::new(libraries[0]).is_file(), "{}", libraries[0]);

    let bytes = fs::read(out.path().join("index.scip")).expect("one published index");
    let index = Index::parse_from_bytes(&bytes).expect("SCIP");
    let consumer = index
        .documents
        .iter()
        .find(|doc| doc.relative_path == "src/lib.rs")
        .expect("the consumer's document");
    assert!(
        consumer.occurrences.iter().any(|occ| {
            occ.symbol_roles & SymbolRole::Definition as i32 == 0
                && occ.symbol.ends_with(" made().")
        }),
        "the call into the expanded function has no reference"
    );
}

/// A workspace at `ws/` using two procedural macros from outside it: `direct-mac` writes its
/// declaration and takes its output from `pm-helper`; `gen-mac` has its declaration written by
/// a macro of `decl-helper`. `shared` is built twice so that the adapter writes its split graph.
const OUTSIDE_PROC_MACROS: &[(&str, &str)] = &[
    (
        "ws/Cargo.toml",
        r#"[workspace]
resolver = "3"
[package]
name = "consumer"
version = "0.1.0"
edition = "2024"
[dependencies]
direct-mac = { path = "../direct-mac" }
gen-mac = { path = "../gen-mac" }
shared = { path = "../shared", features = ["normal"] }
[build-dependencies]
shared = { path = "../shared", features = ["build"] }"#,
    ),
    (
        "ws/src/lib.rs",
        "direct_mac::make_direct!();\ngen_mac::make_gen!();\npub fn run() { direct_made(); gen_made(); shared::common(); }\n",
    ),
    ("ws/build.rs", "fn main() { shared::common(); }\n"),
    (
        "direct-mac/Cargo.toml",
        r#"[package]
name = "direct-mac"
version = "0.1.0"
edition = "2024"
[lib]
proc-macro = true
[dependencies]
pm-helper = { path = "../pm-helper" }"#,
    ),
    (
        "direct-mac/src/lib.rs",
        "use proc_macro::TokenStream;\n\n#[proc_macro]\npub fn make_direct(_: TokenStream) -> TokenStream {\n    pm_helper::text().parse().unwrap()\n}\n",
    ),
    (
        "pm-helper/Cargo.toml",
        "[package]\nname = \"pm-helper\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    ),
    (
        "pm-helper/src/lib.rs",
        "pub fn text() -> &'static str {\n    \"pub fn direct_made() {}\"\n}\n",
    ),
    (
        "gen-mac/Cargo.toml",
        r#"[package]
name = "gen-mac"
version = "0.1.0"
edition = "2024"
[lib]
proc-macro = true
[dependencies]
decl-helper = { path = "../decl-helper" }"#,
    ),
    (
        "gen-mac/src/lib.rs",
        "extern crate proc_macro;\n\ndecl_helper::declare!(make_gen, \"pub fn gen_made() {}\");\n",
    ),
    (
        "decl-helper/Cargo.toml",
        "[package]\nname = \"decl-helper\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    ),
    (
        "decl-helper/src/lib.rs",
        "#[macro_export]\nmacro_rules! declare {\n    ($name:ident, $out:literal) => {\n        #[proc_macro]\n        pub fn $name(_: proc_macro::TokenStream) -> proc_macro::TokenStream {\n            $out.parse().unwrap()\n        }\n    };\n}\n",
    ),
    (
        "shared/Cargo.toml",
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[features]\nnormal = []\nbuild = []\n",
    ),
    ("shared/src/lib.rs", "pub fn common() {}\n"),
];

#[test]
#[ignore = "needs the real rust-analyzer; run by scripts/check-live.sh"]
fn an_outside_proc_macro_loses_only_the_dependencies_it_cannot_use() {
    use protobuf::Message as _;
    use scip::types::{Index, SymbolRole};

    let temp = scratch();
    write(temp.path(), OUTSIDE_PROC_MACROS);
    let root = temp.path().join("ws");
    lock(&root);
    let (result, out) = adapter(&root, &[]);
    assert_eq!(base(&result)["status"], "ok", "{result}");

    let graph: Value = serde_json::from_slice(
        &fs::read(out.path().join("work/rust-project/rust-project.json")).expect("the split graph"),
    )
    .expect("JSON");
    let crates = graph["crates"].as_array().expect("crates");
    let named = |package: &str| -> Vec<&Value> {
        crates
            .iter()
            .filter(|unit| {
                unit["display_name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with(&format!("{package}__gosanja_unit_")))
            })
            .collect()
    };
    let [direct] = named("direct-mac")[..] else {
        panic!("one direct-mac unit: {crates:?}")
    };
    assert_eq!(direct["deps"], serde_json::json!([]), "{direct}");
    assert!(named("pm-helper").is_empty(), "pm-helper left the graph");
    let [generated] = named("gen-mac")[..] else {
        panic!("one gen-mac unit: {crates:?}")
    };
    assert_eq!(
        generated["deps"].as_array().map(Vec::len),
        Some(1),
        "{generated}"
    );
    assert_eq!(named("decl-helper").len(), 1, "decl-helper stays");

    let bytes = fs::read(out.path().join("index.scip")).expect("one published index");
    let index = Index::parse_from_bytes(&bytes).expect("SCIP");
    let consumer = index
        .documents
        .iter()
        .find(|doc| doc.relative_path == "src/lib.rs")
        .expect("the consumer's document");
    for name in ["direct_made", "gen_made"] {
        let suffix = format!(" {name}().");
        assert!(
            consumer.occurrences.iter().any(|occ| {
                occ.symbol_roles & SymbolRole::Definition as i32 == 0
                    && occ.symbol.ends_with(&suffix)
            }),
            "the call into {name} has no reference"
        );
    }
}
