// A temporary root and the fake adapter run through the command line, shared by the build
// cases of the command line.

#[cfg(unix)]
#[allow(dead_code, reason = "each test binary uses a different subset")]
mod build_cli {
    use serde_json::Value;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::{Command, Output},
    };

    /// The contract the fake adapter declares; `None` keeps its default.
    pub const CONTRACT: Option<&str> = None;

    pub fn repository() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// A temporary world: a copy of `fixtures/rust-basic`'s sources as the root.
    pub struct World {
        pub tmp: tempfile::TempDir,
        pub root: PathBuf,
    }

    impl World {
        pub fn rust_basic() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("root");
            fs::create_dir_all(root.join("src")).unwrap();
            for file in ["Cargo.toml", "src/lib.rs", "src/views.rs"] {
                fs::copy(
                    repository().join("fixtures/rust-basic").join(file),
                    root.join(file),
                )
                .unwrap();
            }
            Self { tmp, root }
        }

        pub fn with_files(files: &[(&str, &str)]) -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("root");
            fs::create_dir_all(&root).unwrap();
            for (path, text) in files {
                let file = root.join(path);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, text).unwrap();
            }
            Self { tmp, root }
        }

        /// A script that runs the fake adapter with `options` before its own arguments.
        pub fn adapter(&self, name: &str, options: &[(&str, String)]) -> PathBuf {
            let mut line = format!(
                "exec /bin/sh '{}'",
                repository()
                    .join("fixtures/fake-adapter/adapter.sh")
                    .display()
            );
            let mut all: Vec<(&str, String)> = options.to_vec();
            if let Some(contract) = CONTRACT {
                all.push(("--contract", contract.to_owned()));
            }
            for (option, value) in all {
                line.push_str(&[" ", option, " '", &value, "'"].concat());
            }
            let path = self.tmp.path().join(format!("adapter-{name}"));
            fs::write(&path, format!("#!/bin/sh\n{line} \"$@\"\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        /// Writes `lines` as a JSON Lines file and returns its path.
        pub fn lines(&self, name: &str, lines: &[String]) -> String {
            let path = self.tmp.path().join(name);
            fs::write(
                &path,
                lines.iter().fold(String::new(), |mut out, l| {
                    out.push_str(l);
                    out.push('\n');
                    out
                }),
            )
            .unwrap();
            path.display().to_string()
        }

        pub fn index(&self, adapter: &Path, out: &str, layers: Option<&str>) -> (Output, PathBuf) {
            let out = self.tmp.path().join(out);
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_gosanja"));
            cmd.env_remove("GOSANJA_FAKE_MODE")
                .args(["index", "--adapter"])
                .arg(adapter)
                .arg("--root")
                .arg(&self.root)
                .arg("--out")
                .arg(&out)
                .env("XDG_CACHE_HOME", self.tmp.path().join("cache"))
                .env("HOME", self.tmp.path());
            if let Some(layers) = layers {
                cmd.args(["--layers", layers]);
            }
            (cmd.output().unwrap(), out)
        }

        pub fn diff(&self, old: &Path, new: &Path) -> (Output, PathBuf) {
            let out = self.tmp.path().join("diff");
            let output = Command::new(env!("CARGO_BIN_EXE_gosanja"))
                .arg("diff")
                .arg(old)
                .arg(new)
                .arg("--out")
                .arg(&out)
                .output()
                .unwrap();
            (output, out)
        }
    }

    pub fn rows(dir: &Path, name: &str) -> Vec<Value> {
        fs::read_to_string(dir.join(name))
            .map(|t| {
                t.lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn manifest(dir: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
    }

    pub fn layer(dir: &Path, name: &str) -> Value {
        manifest(dir)["scopes"][0]["layers"][name].clone()
    }

    pub fn entity(dir: &Path, key: &str) -> Value {
        rows(dir, "entities.jsonl")
            .into_iter()
            .find(|e| e["key"] == key)
            .unwrap_or_else(|| panic!("no entity {key}"))
    }

    pub fn ok(output: &Output) {
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A minimal SCIP index: one tool, documents of `(path, occurrences)`, each occurrence
    /// `(symbol, roles, range, enclosing)`, positions in UTF-8.
    /// An occurrence: symbol, roles, range and enclosing range.
    pub type Occurrence<'a> = (&'a str, i32, Vec<i32>, Vec<i32>);

    pub fn scip(documents: &[(&str, Vec<Occurrence<'_>>)]) -> Vec<u8> {
        fn varint(mut v: u64, out: &mut Vec<u8>) {
            loop {
                let byte = (v & 0x7f) as u8;
                v >>= 7;
                if v == 0 {
                    out.push(byte);
                    return;
                }
                out.push(byte | 0x80);
            }
        }
        fn field(number: u64, bytes: &[u8], out: &mut Vec<u8>) {
            varint(number << 3 | 2, out);
            varint(bytes.len() as u64, out);
            out.extend_from_slice(bytes);
        }
        fn packed(values: &[i32]) -> Vec<u8> {
            let mut out = Vec::new();
            for &v in values {
                varint(u64::try_from(v).unwrap(), &mut out);
            }
            out
        }
        let mut tool = Vec::new();
        field(1, b"t", &mut tool);
        field(2, b"1", &mut tool);
        let mut metadata = Vec::new();
        field(2, &tool, &mut metadata);
        let mut index = Vec::new();
        field(1, &metadata, &mut index);
        for (path, occurrences) in documents {
            let mut document = Vec::new();
            field(1, path.as_bytes(), &mut document);
            for (symbol, roles, range, enclosing) in occurrences {
                let mut occurrence = Vec::new();
                field(1, &packed(range), &mut occurrence);
                field(2, symbol.as_bytes(), &mut occurrence);
                varint(3 << 3, &mut occurrence);
                varint(u64::try_from(*roles).unwrap(), &mut occurrence);
                if !enclosing.is_empty() {
                    field(7, &packed(enclosing), &mut occurrence);
                }
                field(2, &occurrence, &mut document);
            }
            field(4, b"test", &mut document);
            varint(6 << 3, &mut document);
            varint(1, &mut document);
            field(2, &document, &mut index);
        }
        index
    }
}
