use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use tempfile::TempDir;

pub struct Case {
    pub temp: TempDir,
    pub root: PathBuf,
    pub out: PathBuf,
    pub bin: PathBuf,
}
pub fn copy_tree(src: &Path, dest: &Path) {
    fs::create_dir(dest).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let e = entry.unwrap();
        let to = dest.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &to);
        } else {
            fs::copy(e.path(), to).unwrap();
        }
    }
}
pub fn listing(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    use std::os::unix::fs::PermissionsExt;
    let mut result = BTreeMap::new();
    let mut paths = vec![root.to_owned()];
    while let Some(path) = paths.pop() {
        let meta = fs::symlink_metadata(&path).unwrap();
        let bytes = if meta.is_dir() {
            paths.extend(fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            Vec::new()
        } else if meta.is_symlink() {
            fs::read_link(&path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else {
            fs::read(&path).unwrap()
        };
        result.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            (meta.permissions().mode(), bytes),
        );
    }
    result
}
impl Case {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let out = temp.path().join("out");
        let bin = temp.path().join("bin");
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        copy_tree(&fixtures.join("rust-flow"), &root);
        for dir in [
            &out,
            &bin,
            &temp.path().join("home"),
            &temp.path().join("cargo-home"),
        ] {
            fs::create_dir(dir).unwrap();
        }
        let cargo = Path::new(env!("CARGO"));
        symlink(cargo, bin.join("cargo")).unwrap();
        symlink(cargo.with_file_name("rustc"), bin.join("rustc")).unwrap();
        // `cc` links the binaries of the calls build; it finds `ld` on `PATH`.
        for name in ["cp", "env", "cc", "ld"] {
            let target = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|d| d.join(name))
                .find(|p| p.is_file())
                .unwrap();
            symlink(target, bin.join(name)).unwrap();
        }
        symlink(
            fixtures.join("stub-tools/rust-analyzer"),
            bin.join("rust-analyzer"),
        )
        .unwrap();
        Self {
            temp,
            root,
            out,
            bin,
        }
    }
    pub fn command(&self, program: &Path) -> Command {
        let mut c = Command::new(program);
        c.current_dir(self.temp.path())
            .env_clear()
            .env("PATH", &self.bin)
            .env("HOME", self.temp.path().join("home"))
            .env("CARGO_HOME", self.temp.path().join("cargo-home"))
            .env("XDG_CACHE_HOME", self.temp.path().join("cache"))
            .env("GOSANJA_ADAPTER_PATH", &self.bin);
        c
    }
    pub fn adapter(&self) -> Command {
        self.command(adapter_binary())
    }
    pub fn index(&self) -> Command {
        let mut c = self.adapter();
        c.arg("index")
            .arg("--root")
            .arg(&self.root)
            .arg("--out")
            .arg(&self.out);
        c
    }
    pub fn run(&self, c: &mut Command) -> Value {
        let o = c.output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&fs::read(self.out.join("result.json")).unwrap()).unwrap()
    }
    pub fn stub(&self, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let script = self.bin.join("case-indexer");
        fs::write(&script, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        fs::rename(script, self.bin.join("rust-analyzer")).unwrap();
    }
}
pub fn engine() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let o = Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "build",
                "--locked",
                "-p",
                "gosanja",
                "--bin",
                "gosanja",
                "--message-format=json",
            ])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let exe = String::from_utf8(o.stdout)
            .unwrap()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find_map(|v| v["executable"].as_str().map(PathBuf::from))
            .unwrap();
        linked_program(&exe)
    })
}

fn linked_program(executable: &Path) -> PathBuf {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
        .keep();
    let p = dir.join(executable.file_name().unwrap());
    fs::hard_link(executable, &p).unwrap();
    p
}
pub fn adapter_binary() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| linked_program(Path::new(env!("CARGO_BIN_EXE_gosanja-adapter-rust"))))
}
