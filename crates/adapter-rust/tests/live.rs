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
    let result =
        serde_json::from_slice(&fs::read(out.path().join("result.json")).expect("result.json"))
            .expect("JSON");
    (result, out)
}

/// Runs `gosanja index` with the adapter on `root`; the snapshot directory.
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
        let mut child = Command::new("inotifywait")
            .args([
                "-m",
                "-r",
                "-e",
                "create,modify,delete,move",
                "--format",
                "%w%f %e",
            ])
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
