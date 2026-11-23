//! Installed adapters: how the engine finds them, lists them and chooses one by marker file.
//!
//! Every `gosanja` here runs as a process whose environment is cleared and then built by the
//! test: the binary, hard-linked into a directory of its own, a `PATH` of links to the utilities the
//! fake adapter needs, and a home, a cache and an adapter search path inside the test's
//! temporary directory. Fake adapters are scripts the test writes, each running
//! `fixtures/fake-adapter/adapter.sh` with options; every `gosanja` process gets its own
//! directory of invocation records.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, MutexGuard};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

/// One test at a time writes fake adapters and starts processes: a script being written while
/// another thread forks could still be open for writing in that child when it is executed.
static SERIAL: Mutex<()> = Mutex::new(());

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository")
}

fn fake_adapter() -> PathBuf {
    repository().join("fixtures/fake-adapter/adapter.sh")
}

/// A shell word that stands for `value` exactly.
fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn write_executable(path: &Path, text: &str) {
    fs::write(path, text).expect("write an executable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("make it executable");
}

/// Options of one fake adapter; `None` keeps the fake's default.
#[derive(Clone, Default)]
struct Fake {
    /// The name its manifest gives; the file's name when absent.
    manifest_name: Option<String>,
    markers: Option<String>,
    version_json: Option<String>,
    languages: Option<String>,
    layers: Option<String>,
    encoding: Option<String>,
    requires: Option<String>,
    mode: Option<String>,
}

impl Fake {
    fn markers(markers: &str) -> Self {
        Self {
            markers: Some(markers.to_owned()),
            ..Self::default()
        }
    }

    fn mode(mut self, mode: &str) -> Self {
        self.mode = Some(mode.to_owned());
        self
    }
}

/// Writes a fake adapter script to `path`, recording under `label`.
fn write_fake(path: &Path, name: &str, label: &str, fake: &Fake) {
    let manifest_name = fake
        .manifest_name
        .clone()
        .unwrap_or_else(|| name.to_owned());
    let mut options = vec![("--name", Some(manifest_name))];
    options.extend([
        ("--markers", fake.markers.clone()),
        ("--version-json", fake.version_json.clone()),
        ("--languages", fake.languages.clone()),
        ("--layers", fake.layers.clone()),
        ("--encoding", fake.encoding.clone()),
        ("--requires", fake.requires.clone()),
        ("--mode", fake.mode.clone()),
    ]);
    let mut script = format!(
        "#!/bin/sh\nexec /bin/sh {} --record \"$GOSANJA_TEST_RECORDS\"/{}",
        quoted(&fake_adapter().to_string_lossy()),
        quoted(label)
    );
    for (option, value) in options {
        if let Some(value) = value {
            let _ = write!(script, " {option} {}", quoted(&value));
        }
    }
    script.push_str(" \"$@\"\n");
    write_executable(path, &script);
}

/// Writes `gosanja-adapter-<name>` into `dir`, recording under `label`.
fn install_in(dir: &Path, name: &str, label: &str, fake: &Fake) -> PathBuf {
    let path = dir.join(format!("gosanja-adapter-{name}"));
    write_fake(&path, name, label, fake);
    path
}

/// The finished run of one `gosanja` process and the records of the adapters it started.
struct Run {
    output: Output,
    records: PathBuf,
}

impl Run {
    fn code(&self) -> i32 {
        self.output.status.code().expect("exited")
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    /// The protocol commands the adapter recording under `label` was asked for, in order.
    fn record(&self, label: &str) -> Vec<String> {
        match fs::read_to_string(self.records.join(label)) {
            Ok(text) => text.lines().map(str::to_owned).collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => panic!("record {label}: {e}"),
        }
    }

    #[track_caller]
    fn expect(&self, code: i32) -> &Self {
        assert_eq!(
            self.code(),
            code,
            "stdout:\n{}\nstderr:\n{}",
            self.stdout(),
            self.stderr()
        );
        self
    }

    #[track_caller]
    fn says(&self, text: &str) -> &Self {
        assert!(
            self.stderr().contains(text),
            "{text:?} not in:\n{}",
            self.stderr()
        );
        self
    }
}

/// A temporary world: the binary, its `PATH`, a home, a cache and adapter directories.
struct World {
    temp: TempDir,
    /// Holds the copied `gosanja` and nothing the test did not put there.
    bin: PathBuf,
    /// The `PATH` of every run.
    tools: PathBuf,
    /// The entries of `GOSANJA_ADAPTER_PATH`, in order.
    search: Vec<OsString>,
    cache: PathBuf,
    runs: std::cell::Cell<usize>,
    _serial: MutexGuard<'static, ()>,
}

impl World {
    fn new() -> Self {
        let serial = SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Under the target directory, so that the binary can be hard-linked: a link is never
        // open for writing, so executing it cannot fail as "text file busy".
        let temp =
            tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory");
        let base = temp.path().canonicalize().expect("canonical");
        let bin = base.join("bin");
        let tools = base.join("tools");
        let adapters = base.join("adapters");
        for dir in [
            &bin,
            &tools,
            &adapters,
            &base.join("home"),
            &base.join("records"),
        ] {
            fs::create_dir(dir).expect("mkdir");
        }
        fs::hard_link(env!("CARGO_BIN_EXE_gosanja"), bin.join("gosanja")).expect("link gosanja");
        let path = std::env::var_os("PATH").expect("the test has a PATH");
        for tool in ["cp", "dirname", "cat"] {
            let found = std::env::split_paths(&path)
                .filter(|d| d.is_absolute())
                .map(|d| d.join(tool))
                .find(|p| executable(p))
                .unwrap_or_else(|| panic!("{tool} on the test's PATH"));
            symlink(found, tools.join(tool)).expect("link a utility");
        }
        Self {
            bin,
            tools,
            search: vec![adapters.into_os_string()],
            cache: base.join("cache"),
            temp,
            runs: std::cell::Cell::new(0),
            _serial: serial,
        }
    }

    fn base(&self) -> PathBuf {
        self.temp.path().canonicalize().expect("canonical")
    }

    /// The first adapter search directory.
    fn adapters(&self) -> PathBuf {
        PathBuf::from(&self.search[0])
    }

    /// A new directory under the world.
    fn dir(&self, name: &str) -> PathBuf {
        let dir = self.base().join(name);
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    /// Installs a fake in the first search directory, recording under its own name.
    fn install(&self, name: &str, fake: &Fake) -> PathBuf {
        install_in(&self.adapters(), name, name, fake)
    }

    /// A copy of `fixtures/rust-basic` without its goldens.
    fn project(&self, name: &str) -> PathBuf {
        let root = self.dir(name);
        let source = repository().join("fixtures/rust-basic");
        fs::copy(source.join("Cargo.toml"), root.join("Cargo.toml")).expect("copy");
        fs::copy(source.join("index.scip"), root.join("index.scip")).expect("copy");
        fs::create_dir(root.join("src")).expect("mkdir");
        for file in ["lib.rs", "views.rs"] {
            fs::copy(source.join("src").join(file), root.join("src").join(file)).expect("copy");
        }
        root
    }

    /// A snapshot written at `out` by `--scip`, which starts no program.
    fn snapshot(&self, root: &Path, out: &Path) {
        self.run(&[
            "index".as_ref(),
            "--scip".as_ref(),
            repository()
                .join("fixtures/rust-basic/index.scip")
                .as_os_str(),
            "--root".as_ref(),
            root.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ])
        .expect(0);
    }

    fn command(&self) -> (Command, PathBuf) {
        let n = self.runs.get();
        self.runs.set(n + 1);
        let records = self.base().join("records").join(n.to_string());
        fs::create_dir(&records).expect("a record directory");
        let mut command = Command::new(self.bin.join("gosanja"));
        command
            .env_clear()
            .env("PATH", &self.tools)
            .env("HOME", self.base().join("home"))
            .env("XDG_CACHE_HOME", &self.cache)
            .env(
                "GOSANJA_ADAPTER_PATH",
                std::env::join_paths(&self.search).expect("joinable"),
            )
            .env("GOSANJA_TEST_RECORDS", &records)
            .current_dir(self.base());
        (command, records)
    }

    fn run(&self, args: &[&OsStr]) -> Run {
        self.run_with(args, |_| {})
    }

    fn run_with(&self, args: &[&OsStr], adjust: impl Fn(&mut Command)) -> Run {
        let (mut command, records) = self.command();
        command.args(args);
        adjust(&mut command);
        Run {
            output: command.output().expect("gosanja runs"),
            records,
        }
    }

    /// `gosanja index --root <root> --out <out>`, choosing automatically.
    fn index(&self, root: &Path, out: &Path) -> Run {
        self.run(&[
            "index".as_ref(),
            "--root".as_ref(),
            root.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ])
    }

    /// `gosanja index --adapter <adapter> --root <root> --out <out>`.
    fn index_with(&self, adapter: &OsStr, root: &Path, out: &Path) -> Run {
        self.run(&[
            "index".as_ref(),
            "--adapter".as_ref(),
            adapter,
            "--root".as_ref(),
            root.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ])
    }

    fn list(&self) -> Run {
        self.run(&["adapter".as_ref(), "list".as_ref()])
    }

    fn describe(&self, adapter: &OsStr) -> Run {
        self.run(&["adapter".as_ref(), "describe".as_ref(), adapter])
    }
}

/// What the tree listing records for one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Directory(u32),
    File(u32, Vec<u8>),
    /// A link's own target, as `read_link` gives it.
    Link(PathBuf),
}

/// Every entry under `root`, `root` included, by its path relative to `root`; links are not
/// followed.
fn listing(root: &Path) -> BTreeMap<PathBuf, Node> {
    fn walk(root: &Path, relative: &Path, out: &mut BTreeMap<PathBuf, Node>) {
        let path = root.join(relative);
        let meta = fs::symlink_metadata(&path).expect("inspectable");
        let mode = meta.permissions().mode() & 0o7777;
        let node = if meta.file_type().is_symlink() {
            Node::Link(fs::read_link(&path).expect("a link"))
        } else if meta.is_dir() {
            let mut names: Vec<_> = fs::read_dir(&path)
                .expect("readable")
                .map(|e| e.expect("an entry").file_name())
                .collect();
            names.sort();
            for name in names {
                walk(root, &relative.join(name), out);
            }
            Node::Directory(mode)
        } else {
            Node::File(
                mode,
                Sha256::digest(fs::read(&path).expect("readable")).to_vec(),
            )
        };
        out.insert(relative.to_path_buf(), node);
    }
    let mut out = BTreeMap::new();
    walk(root, Path::new(""), &mut out);
    out
}

/// Whether permissions bind this process; they do not for the super-user.
fn permissions_bind(world: &World) -> bool {
    let probe = world.dir("permission-probe");
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o000)).expect("chmod");
    let binds = fs::read_dir(&probe).is_err();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).expect("chmod");
    if !binds {
        println!("skipped: permissions do not bind the super-user, who may read any directory");
    }
    binds
}

fn manifest(snapshot: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(snapshot.join("manifest.json")).expect("a manifest"))
        .expect("JSON")
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

const RUST: &str = r#"["Cargo.toml"]"#;
const GO: &str = r#"["go.mod"]"#;

/// A refusal that must leave the snapshot at `out` and the project as they were.
#[track_caller]
fn refused_unchanged(world: &World, root: &Path, out: &Path, code: i32) -> Run {
    let project = listing(root);
    let snapshot = listing(out);
    let run = world.index(root, out);
    run.expect(code);
    assert_eq!(listing(root), project, "the project changed");
    assert_eq!(listing(out), snapshot, "the snapshot changed");
    run
}

// Automatic choice.

#[test]
fn automatic_choice_gives_the_snapshot_of_naming_the_adapter_and_asks_each_adapter_once() {
    let world = World::new();
    let rust = world.install("rust", &Fake::markers(RUST));
    world.install("go", &Fake::markers(GO));
    let root = world.project("project");
    let before = listing(&root);
    let (a, b) = (world.base().join("a"), world.base().join("b"));

    let automatic = world.index(&root, &a);
    automatic.expect(0);
    assert_eq!(automatic.record("rust"), strings(&["describe", "index"]));
    assert_eq!(automatic.record("go"), strings(&["describe"]));

    let explicit = world.index_with(rust.as_os_str(), &root, &b);
    explicit.expect(0);
    assert_eq!(explicit.record("rust"), strings(&["describe", "index"]));
    assert_eq!(explicit.record("go"), Vec::<String>::new());

    world
        .run(&[
            "validate".as_ref(),
            a.as_os_str(),
            "--root".as_ref(),
            root.as_os_str(),
        ])
        .expect(0);
    assert_eq!(listing(&a), listing(&b));
    assert_eq!(manifest(&a)["scopes"][0]["adapter"]["name"], "rust");
    assert_eq!(listing(&root), before);
}

#[test]
fn a_describe_that_would_answer_differently_is_asked_once() {
    let world = World::new();
    world.install("rust", &Fake::markers(RUST).mode("changed-describe"));
    let root = world.project("project");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(0);
    assert_eq!(run.record("rust"), strings(&["describe", "index"]));
}

#[test]
fn the_marker_decides_and_not_the_search_order() {
    let mut world = World::new();
    let first = world.dir("first");
    world.search.insert(0, first.clone().into_os_string());
    install_in(
        &first,
        "go",
        "go",
        &Fake {
            encoding: Some("utf-16".into()),
            ..Fake::markers(GO)
        },
    );
    world.install("rust", &Fake::markers(RUST));
    let root = world.project("project");
    let out = world.base().join("out");
    let run = world.index(&root, &out);
    run.expect(0);
    assert_eq!(run.record("rust"), strings(&["describe", "index"]));
    assert_eq!(run.record("go"), strings(&["describe"]));
    assert_eq!(manifest(&out)["scopes"][0]["adapter"]["name"], "rust");
}

// Selection rules.

/// A world with `rust` and `go` installed, an empty project and a snapshot already at `out`.
fn two_adapters() -> (World, PathBuf, PathBuf) {
    let world = World::new();
    world.install("rust", &Fake::markers(RUST));
    world.install("go", &Fake::markers(GO));
    let root = world.dir("project");
    fs::write(root.join("README"), "no marker here\n").expect("write");
    let out = world.base().join("out");
    world.snapshot(&root, &out);
    (world, root, out)
}

#[test]
fn no_marker_present_names_every_usable_adapter_and_the_alternatives() {
    let (world, root, out) = two_adapters();
    let run = refused_unchanged(&world, &root, &out, 2);
    run.says("rust: markers Cargo.toml")
        .says("go: markers go.mod")
        .says("--adapter")
        .says("--scip");
    assert_eq!(run.record("rust"), strings(&["describe"]));
}

#[test]
fn nothing_usable_installed_is_said_so() {
    let world = World::new();
    let root = world.dir("project");
    fs::write(root.join("Cargo.toml"), "").expect("write");
    let out = world.base().join("out");
    world.snapshot(&root, &out);
    refused_unchanged(&world, &root, &out, 2).says("no usable adapter is installed");
    world.install("rust", &Fake::markers(RUST).mode("describe-failed"));
    let run = refused_unchanged(&world, &root, &out, 2);
    run.says("no usable adapter is installed");
    assert_eq!(run.record("rust"), strings(&["describe"]));
}

#[test]
fn a_marker_is_a_regular_file_after_following_links() {
    let (world, root, out) = two_adapters();
    fs::create_dir(root.join("Cargo.toml")).expect("mkdir");
    refused_unchanged(&world, &root, &out, 2).says("rust: markers Cargo.toml");
    fs::remove_dir(root.join("Cargo.toml")).expect("rmdir");
    symlink("absent", root.join("Cargo.toml")).expect("link");
    refused_unchanged(&world, &root, &out, 2).says("rust: markers Cargo.toml");
    fs::remove_file(root.join("Cargo.toml")).expect("unlink");
    // A target that runs through a regular file does not exist either.
    symlink("README/child", root.join("Cargo.toml")).expect("link");
    refused_unchanged(&world, &root, &out, 2).says("rust: markers Cargo.toml");
    fs::remove_file(root.join("Cargo.toml")).expect("unlink");
    symlink("README", root.join("Cargo.toml")).expect("link");
    let run = world.index(&root, &out);
    run.expect(0);
    assert_eq!(run.record("rust"), strings(&["describe", "index"]));
}

#[test]
fn several_candidates_are_named_in_name_order() {
    let (world, root, out) = two_adapters();
    fs::write(root.join("Cargo.toml"), "").expect("write");
    fs::write(root.join("go.mod"), "").expect("write");
    let run = refused_unchanged(&world, &root, &out, 2);
    run.says("go, rust").says("--adapter");
    assert_eq!(run.record("rust"), strings(&["describe"]));
    assert_eq!(run.record("go"), strings(&["describe"]));

    let named = world.index_with("go".as_ref(), &root, &out);
    named.expect(0);
    assert_eq!(named.record("go"), strings(&["describe", "index"]));
    assert_eq!(named.record("rust"), Vec::<String>::new());
}

#[test]
fn an_adapter_with_two_present_markers_is_one_candidate() {
    let world = World::new();
    world.install("rust", &Fake::markers(r#"["Cargo.toml","index.scip"]"#));
    let root = world.project("project");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(0);
    assert_eq!(run.record("rust"), strings(&["describe", "index"]));
}

#[test]
fn names_and_paths_given_to_adapter() {
    let (world, root, out) = two_adapters();
    let searched = world.adapters().to_string_lossy().into_owned();
    let (project, snapshot) = (listing(&root), listing(&out));
    let unchanged = || {
        assert_eq!(listing(&root), project);
        assert_eq!(listing(&out), snapshot);
    };
    // Every tier of the search order is named: GOSANJA_ADAPTER_PATH, the executable's
    // directory and PATH.
    world
        .index_with("python".as_ref(), &root, &out)
        .expect(2)
        .says(&searched)
        .says(&world.bin.to_string_lossy())
        .says(&world.tools.to_string_lossy());
    unchanged();
    world
        .index_with("Rust".as_ref(), &root, &out)
        .expect(2)
        .says(&searched);
    unchanged();
    // A bare word that is a program on `PATH` but no installed adapter is not run.
    write_fake(&world.tools.join("frob"), "frob", "frob", &Fake::default());
    let bare = world.index_with("frob".as_ref(), &root, &out);
    bare.expect(2);
    assert_eq!(bare.record("frob"), Vec::<String>::new());
    unchanged();
    // A path still names a program, installed or not.
    let elsewhere = world.dir("elsewhere");
    let program = install_in(&elsewhere, "rust", "elsewhere", &Fake::markers(RUST));
    let by_path = world.index_with(program.as_os_str(), &root, &out);
    by_path.expect(0);
    assert_eq!(by_path.record("elsewhere"), strings(&["describe", "index"]));
    assert_eq!(by_path.record("rust"), Vec::<String>::new());
}

#[test]
fn an_adapter_without_markers_runs_only_when_named() {
    let world = World::new();
    world.install("plain", &Fake::default());
    let root = world.project("project");
    let out = world.base().join("out");
    world.snapshot(&root, &out);
    refused_unchanged(&world, &root, &out, 2).says("plain: no markers");
    let named = world.index_with("plain".as_ref(), &root, &out);
    named.expect(0);
    assert_eq!(named.record("plain"), strings(&["describe", "index"]));
}

#[test]
fn a_missing_required_tool_does_not_affect_the_choice() {
    let world = World::new();
    world.install(
        "rust",
        &Fake {
            requires: Some(r#"[{"tool":"absent-tool","version":"1"}]"#.into()),
            ..Fake::markers(RUST)
        },
    );
    let root = world.project("project");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(0);
    assert_eq!(run.record("rust"), strings(&["describe", "index"]));
}

#[test]
fn a_marker_that_cannot_be_inspected_decides_nothing() {
    let (world, root, out) = two_adapters();
    fs::write(root.join("go.mod"), "").expect("write");
    symlink("Cargo.toml", root.join("Cargo.toml")).expect("a link to itself");
    let run = refused_unchanged(&world, &root, &out, 3);
    run.says(&root.join("Cargo.toml").to_string_lossy());
    assert!(!run.record("go").contains(&"index".to_owned()));

    drop(world);
    let world = World::new();
    let root = world.project("project");
    symlink("loop", root.join("loop")).expect("a link to itself");
    let out = world.base().join("out");
    world.snapshot(&root, &out);
    for markers in [r#"["Cargo.toml","loop"]"#, r#"["loop","Cargo.toml"]"#] {
        world.install("rust", &Fake::markers(markers));
        let run = refused_unchanged(&world, &root, &out, 3);
        run.says(&root.join("loop").to_string_lossy());
        assert_eq!(run.record("rust"), strings(&["describe"]), "{markers}");
    }
}

#[test]
fn the_root_is_checked_before_any_adapter_is_described() {
    let (world, root, out) = two_adapters();
    let snapshot = listing(&out);
    let quiet = |run: &Run| {
        assert_eq!(run.record("rust"), Vec::<String>::new());
        assert_eq!(run.record("go"), Vec::<String>::new());
        assert_eq!(listing(&out), snapshot);
    };
    let missing = world.base().join("missing");
    let run = world.index(&missing, &out);
    run.expect(3).says(&missing.to_string_lossy());
    quiet(&run);
    assert!(!missing.exists());

    let file = root.join("README");
    let before = fs::read(&file).expect("read");
    let run = world.index(&file, &out);
    // Said as such, not as a failed lookup inside it.
    run.expect(3)
        .says(&format!("the root {} is not a directory", file.display()));
    quiet(&run);
    assert_eq!(fs::read(&file).expect("read"), before);

    if !permissions_bind(&world) {
        return;
    }
    fs::write(root.join("Cargo.toml"), "").expect("write");
    let project = listing(&root);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o600)).expect("chmod");
    let run = world.index(&root, &out);
    let mode = fs::metadata(&root).expect("stat").permissions().mode() & 0o777;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("chmod");
    run.expect(3).says(&root.to_string_lossy());
    assert_eq!(mode, 0o600);
    quiet(&run);
    assert_eq!(listing(&root), project);
}

#[test]
fn a_root_that_can_be_searched_but_not_read_still_chooses() {
    let world = World::new();
    world.install("rust", &Fake::markers(RUST));
    if !permissions_bind(&world) {
        return;
    }
    let root = world.project("project");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o300)).expect("chmod");
    let run = world.index(&root, &world.base().join("out"));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("chmod");
    run.expect(0);
    assert_eq!(run.record("rust"), strings(&["describe", "index"]));
}

#[test]
fn explicit_sources_start_no_other_adapter() {
    let world = World::new();
    world.install("go", &Fake::markers(GO));
    world.install("watcher", &Fake::markers(RUST));
    let root = world.project("project");
    fs::write(root.join("go.mod"), "").expect("write");
    let out = world.base().join("out");
    let go = world.adapters().join("gosanja-adapter-go");
    for run in [
        world.index_with("go".as_ref(), &root, &out),
        world.describe("go".as_ref()),
        world.index_with(go.as_os_str(), &root, &out),
    ] {
        run.expect(0);
        assert_eq!(run.record("watcher"), Vec::<String>::new());
    }
    let scip = world.run(&[
        "index".as_ref(),
        "--scip".as_ref(),
        repository()
            .join("fixtures/rust-basic/index.scip")
            .as_os_str(),
        "--root".as_ref(),
        root.as_os_str(),
        "--out".as_ref(),
        out.as_os_str(),
    ]);
    scip.expect(0);
    assert_eq!(scip.record("watcher"), Vec::<String>::new());
    assert_eq!(scip.record("go"), Vec::<String>::new());
}

#[test]
fn no_index_source_with_no_installed_adapters_is_a_refusal() {
    let world = World::new();
    let root = world.dir("project");
    let out = world.base().join("snapshot");
    world
        .index(&root, &out)
        .expect(2)
        .says("no usable adapter is installed");
    assert!(!out.exists());
}

// Search order and what counts as an adapter.

#[test]
fn the_first_copy_of_a_name_is_the_adapter() {
    let mut world = World::new();
    let second = world.dir("second");
    world.search.push(second.clone().into_os_string());
    install_in(&world.adapters(), "rust", "first", &Fake::markers(RUST));
    install_in(&second, "rust", "second", &Fake::markers(RUST));
    let root = world.project("project");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(0);
    assert_eq!(run.record("first"), strings(&["describe", "index"]));
    assert_eq!(run.record("second"), Vec::<String>::new());

    // GOSANJA_ADAPTER_PATH, then the directory of the executable, then PATH.
    drop(world);
    let world = World::new();
    install_in(
        &world.adapters(),
        "rust",
        "adapter-path",
        &Fake::markers(RUST),
    );
    install_in(&world.bin, "rust", "executable", &Fake::markers(RUST));
    install_in(&world.tools, "rust", "path", &Fake::markers(RUST));
    let root = world.project("project");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(0);
    assert_eq!(run.record("adapter-path"), strings(&["describe", "index"]));
    assert_eq!(run.record("executable"), Vec::<String>::new());
    assert_eq!(run.record("path"), Vec::<String>::new());
    fs::remove_file(world.adapters().join("gosanja-adapter-rust")).expect("rm");
    let run = world.index(&root, &world.base().join("out2"));
    run.expect(0);
    assert_eq!(run.record("executable"), strings(&["describe", "index"]));
    assert_eq!(run.record("path"), Vec::<String>::new());
}

#[test]
fn an_unusable_first_copy_hides_every_later_one() {
    let mut world = World::new();
    let second = world.dir("second");
    world.search.push(second.clone().into_os_string());
    let first = install_in(
        &world.adapters(),
        "rust",
        "first",
        &Fake::markers(RUST).mode("describe-failed"),
    );
    install_in(&second, "rust", "second", &Fake::markers(RUST));
    let root = world.project("project");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(2)
        .says(&first.to_string_lossy())
        .says("no usable adapter is installed");
    assert_eq!(run.record("second"), Vec::<String>::new());
}

#[test]
fn an_unreadable_search_directory_stops_discovery_before_anything_runs() {
    let mut world = World::new();
    if !permissions_bind(&world) {
        return;
    }
    let ahead = world.dir("ahead");
    let locked = world.dir("locked");
    let later = world.dir("later");
    world.search = vec![
        ahead.clone().into_os_string(),
        locked.clone().into_os_string(),
        later.clone().into_os_string(),
    ];
    install_in(&ahead, "watcher", "watcher", &Fake::default());
    install_in(&later, "rust", "rust", &Fake::markers(RUST));
    let root = world.project("project");
    let out = world.base().join("out");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    let runs = [
        world.index(&root, &out),
        world.index_with("rust".as_ref(), &root, &out),
        world.list(),
    ];
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod");
    for run in &runs {
        run.expect(3).says(&locked.to_string_lossy());
        assert_eq!(run.record("rust"), Vec::<String>::new());
        assert_eq!(run.record("watcher"), Vec::<String>::new());
    }
    assert!(!out.exists());
}

#[test]
fn a_file_with_an_adapter_name_that_cannot_be_inspected_stops_discovery() {
    let mut world = World::new();
    let second = world.dir("second");
    world.search.push(second.clone().into_os_string());
    let looping = world.adapters().join("gosanja-adapter-rust");
    symlink(&looping, &looping).expect("a link to itself");
    install_in(&second, "rust", "second", &Fake::markers(RUST));
    let root = world.project("project");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(3).says(&looping.to_string_lossy());
    assert_eq!(run.record("second"), Vec::<String>::new());

    fs::remove_file(&looping).expect("rm");
    symlink("absent", &looping).expect("a dangling link");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(0);
    assert_eq!(run.record("second"), strings(&["describe", "index"]));

    // A link whose target runs through a regular file dangles as well.
    fs::remove_file(&looping).expect("rm");
    fs::write(world.adapters().join("regular"), "a file\n").expect("write");
    symlink("regular/child", &looping).expect("a dangling link");
    let run = world.index(&root, &world.base().join("out-2"));
    run.expect(0);
    assert_eq!(run.record("second"), strings(&["describe", "index"]));
}

#[test]
fn empty_and_relative_entries_are_ignored_and_one_with_a_space_is_searched() {
    let mut world = World::new();
    let spaced = world.dir("with space");
    world.dir("relative");
    install_in(
        &world.base().join("relative"),
        "relative",
        "relative",
        &Fake::default(),
    );
    install_in(&world.base(), "here", "here", &Fake::default());
    install_in(&spaced, "spaced", "spaced", &Fake::default());
    world.search = vec![OsString::new(), "relative".into(), spaced.into_os_string()];
    let run = world.list();
    run.expect(0);
    let stdout = run.stdout();
    assert!(stdout.starts_with("spaced "), "{stdout}");
    assert!(!stdout.contains("relative"), "{stdout}");
    assert!(!stdout.contains("here"), "{stdout}");
    assert_eq!(run.record("relative"), Vec::<String>::new());
    assert_eq!(run.record("here"), Vec::<String>::new());
}

#[test]
fn search_entries_that_are_no_directory_are_ignored() {
    let mut world = World::new();
    world.install("rust", &Fake::default());
    let file = world.base().join("a-file");
    fs::write(&file, "not a directory\n").expect("write");
    world.search = vec![
        world.base().join("missing").into_os_string(),
        file.clone().into_os_string(),
        file.join("child").into_os_string(),
        world.adapters().into_os_string(),
    ];
    let run = world.list();
    run.expect(0);
    assert!(run.stdout().starts_with("rust "), "{}", run.stdout());
}

#[test]
fn a_search_entry_that_cannot_be_inspected_stops_discovery() {
    let mut world = World::new();
    if !permissions_bind(&world) {
        return;
    }
    world.install("rust", &Fake::markers(RUST));
    let locked = world.dir("locked");
    let behind = locked.join("inner");
    world.search.insert(0, behind.clone().into_os_string());
    let root = world.project("project");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    let runs = [world.list(), world.index(&root, &world.base().join("out"))];
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod");
    for run in &runs {
        run.expect(3).says(&behind.to_string_lossy());
        assert_eq!(run.record("rust"), Vec::<String>::new());
    }
}

#[test]
fn adapter_takes_list_or_describe_only() {
    let world = World::new();
    world.install("rust", &Fake::default());
    let run = world.run(&["adapter".as_ref(), "show".as_ref(), "rust".as_ref()]);
    run.expect(2).says("usage:");
    assert_eq!(run.record("rust"), Vec::<String>::new());
    world
        .run(&["adapter".as_ref(), "list".as_ref(), "rust".as_ref()])
        .expect(2);
}

#[test]
fn only_executable_files_with_an_adapter_name_are_adapters() {
    let world = World::new();
    let adapters = world.adapters();
    let plain = world.install("plain", &Fake::default());
    fs::set_permissions(&plain, fs::Permissions::from_mode(0o644)).expect("chmod");
    fs::create_dir(adapters.join("gosanja-adapter-folder")).expect("mkdir");
    write_fake(
        &adapters.join("gosanja-adapter-x.sh"),
        "x",
        "x",
        &Fake::default(),
    );
    let target = world.dir("elsewhere").join("program");
    write_fake(&target, "linked", "linked", &Fake::default());
    symlink(&target, adapters.join("gosanja-adapter-linked")).expect("link");
    let run = world.list();
    run.expect(0);
    let names: Vec<_> = run
        .stdout()
        .lines()
        .filter(|l| !l.starts_with(' '))
        .map(|l| l.split(' ').next().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(names, strings(&["linked"]));
}

// Unusable adapters.

#[test]
fn unusable_adapters_are_reported_and_never_chosen() {
    let world = World::new();
    world.install("rust", &Fake::markers(RUST));
    let unusable: [(&str, Fake, i32); 5] = [
        ("silent", Fake::markers(RUST).mode("describe-failed"), 3),
        ("shapeless", Fake::markers(RUST).mode("invalid-manifest"), 3),
        ("chatty", Fake::markers(RUST).mode("extra-stdout"), 3),
        ("old", Fake::markers(RUST).mode("old-contract"), 2),
        (
            "misnamed",
            Fake {
                manifest_name: Some("other".into()),
                ..Fake::markers(RUST)
            },
            2,
        ),
    ];
    for (name, fake, _) in &unusable {
        world.install(name, fake);
    }
    let root = world.project("project");
    let run = world.index(&root, &world.base().join("out"));
    run.expect(0);
    assert_eq!(run.record("rust"), strings(&["describe", "index"]));
    let list = world.list();
    list.expect(0);
    for (name, _, code) in &unusable {
        let path = world.adapters().join(format!("gosanja-adapter-{name}"));
        let line = format!("{name} path={} unusable: ", path.display());
        let stdout = list.stdout();
        let reason = stdout
            .lines()
            .find_map(|l| l.strip_prefix(&line))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| panic!("{line} with a reason in\n{stdout}"));
        // Automatic choice reports the same reason on standard error.
        run.says(&format!(
            "the adapter {name} at {} is unusable: {reason}",
            path.display()
        ));
        let out = world.base().join(format!("out-{name}"));
        world.index_with(name.as_ref(), &root, &out).expect(*code);
        assert!(!out.exists());
        world.describe(name.as_ref()).expect(*code);
    }
    let described = world.describe("rust".as_ref());
    described.expect(0);
    let manifest: serde_json::Value =
        serde_json::from_str(&described.stdout()).expect("a manifest");
    assert_eq!(
        manifest,
        serde_json::json!({
            "contract": "0.2",
            "name": "rust",
            "version": "1",
            "languages": ["rust"],
            "position_encoding": "utf-8",
            "layers": ["base", "syntax", "calls"],
            "markers": ["Cargo.toml"]
        })
    );
}

// Listing.

#[test]
fn the_listing_shows_each_adapter_on_its_own_lines() {
    let world = World::new();
    let found = world.tools.join("tool-a");
    write_executable(&found, "#!/bin/sh\n");
    world.install(
        "rust",
        &Fake {
            version_json: Some(r#""1.2.3""#.into()),
            languages: Some(r#"["rust"]"#.into()),
            layers: Some(r#"["base","syntax","calls"]"#.into()),
            requires: Some(
                r#"[{"tool":"tool-a","version":">=1"},{"tool":"tool-b"},{"tool":"/opt/tool-c"}]"#
                    .into(),
            ),
            ..Fake::markers(r#"["Cargo.toml","rust-toolchain.toml"]"#)
        },
    );
    world.install(
        "go",
        &Fake {
            version_json: Some(r#""0.9""#.into()),
            languages: Some(r#"["go","templ"]"#.into()),
            layers: Some(r#"["base"]"#.into()),
            ..Fake::markers(GO)
        },
    );
    let run = world.list();
    run.expect(0);
    let adapters = world.adapters();
    let expected = format!(
        "go 0.9 contract=0.2 languages=go,templ layers=base markers=go.mod path={go}\n\
         rust 1.2.3 contract=0.2 languages=rust layers=base,syntax,calls markers=Cargo.toml,rust-toolchain.toml path={rust}\n\
         \x20 requires tool-a >=1: found {found}\n\
         \x20 requires tool-b: missing\n\
         \x20 requires /opt/tool-c: not checked\n",
        go = adapters.join("gosanja-adapter-go").display(),
        rust = adapters.join("gosanja-adapter-rust").display(),
        found = found.display(),
    );
    assert_eq!(run.stdout(), expected);
    println!("{}", run.stdout());
}

#[test]
fn an_empty_listing_names_the_searched_directories() {
    let world = World::new();
    let run = world.list();
    run.expect(0);
    let stdout = run.stdout();
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some("no adapters installed"));
    let rest: Vec<_> = lines.collect();
    for dir in [world.adapters(), world.bin.clone(), world.tools.clone()] {
        assert!(
            rest.iter().any(|l| l.contains(&*dir.to_string_lossy())),
            "{} in\n{stdout}",
            dir.display()
        );
    }
}

#[test]
fn listed_values_stay_on_one_line() {
    let world = World::new();
    world.install(
        "alpha",
        &Fake {
            version_json: Some(r#""1\n2\u2028\u2029\u00853""#.into()),
            requires: Some(r#"[{"tool":"a\nb"}]"#.into()),
            ..Fake::default()
        },
    );
    world.install("beta", &Fake::default());
    let run = world.list();
    run.expect(0);
    let stdout = run.stdout();
    // `lines` splits at `\n` only; the Unicode line and paragraph separators must not appear.
    assert!(
        !stdout.contains(['\u{2028}', '\u{2029}', '\u{85}']),
        "{stdout:?}"
    );
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{stdout}");
    assert!(
        lines[0].starts_with(r"alpha 1\u{a}2\u{2028}\u{2029}\u{85}3 contract=0.2 "),
        "{stdout}"
    );
    assert_eq!(lines[1], r"  requires a\u{a}b: not checked");
    assert!(lines[2].starts_with("beta 1 contract=0.2 "), "{stdout}");
}

// A failed base layer under automatic choice, and the cache beside `--out`.

#[test]
fn a_failed_base_under_automatic_choice_leaves_the_snapshot_alone() {
    let world = World::new();
    world.install("rust", &Fake::markers(RUST).mode("failed-base-code"));
    let root = world.project("project");
    let out = world.base().join("out");
    world.snapshot(&root, &out);
    let snapshot = listing(&out);

    let run = world.index(&root, &out);
    run.expect(3).says("base layer failed: lockfile-missing");
    assert_eq!(run.record("rust"), strings(&["describe", "index"]));
    assert_eq!(listing(&out), snapshot);

    let beneath = |out: &Path| {
        let cache = out.join("cache");
        world.run_with(
            &[
                "index".as_ref(),
                "--root".as_ref(),
                root.as_os_str(),
                "--out".as_ref(),
                out.as_os_str(),
            ],
            |command| {
                command.env("XDG_CACHE_HOME", &cache);
            },
        )
    };
    let run = beneath(&out);
    run.expect(3).says("lies in the output");
    assert_eq!(run.record("rust"), Vec::<String>::new());
    assert_eq!(listing(&out), snapshot);

    let absent = world.base().join("absent");
    let run = beneath(&absent);
    run.expect(3).says("lies in the output");
    assert_eq!(run.record("rust"), Vec::<String>::new());
    assert!(!absent.exists());
}
