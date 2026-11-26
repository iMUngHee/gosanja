// The fake adapter run with build rows and test rows, shared by the build cases of the core.

#[allow(dead_code, reason = "each test binary uses a different subset")]
mod build_run {
    use super::support::{self, DEF, doc, index, occ};
    use gosanja_core::{sidecar, write};
    use serde_json::Value;
    use std::{collections::BTreeMap, fs};

    /// The contract the fake adapter declares; `None` keeps its default.
    pub const CONTRACT: Option<&str> = None;

    pub const CRATE: &str = "s m p 1 crate/";
    pub const ALPHA: &str = "s m p 1 alpha().";
    pub const BETA: &str = "s m p 1 beta().";
    pub const PACKAGE: &str = "s m p 1 pkg/";
    pub const LIB: &str = "pub fn alpha() {}\npub fn beta() {}\n";

    /// What the fake adapter is given and asked.
    #[derive(Clone, Default)]
    pub struct Spec {
        pub documents: Vec<scip::types::Document>,
        pub sources: Vec<(&'static str, String)>,
        pub syntax: Vec<String>,
        pub calls: Vec<String>,
        pub build: Vec<String>,
        pub build_entry: Option<String>,
        pub offered: Option<&'static str>,
        pub request: Option<&'static str>,
        pub mode: Option<&'static str>,
        pub encoding: Option<&'static str>,
        pub merged: Option<String>,
    }

    /// The published snapshot, as text per file.
    pub struct Out {
        pub tables: BTreeMap<String, String>,
    }

    impl Out {
        pub fn rows(&self, name: &str) -> Vec<Value> {
            self.tables
                .get(name)
                .map(|t| {
                    t.lines()
                        .map(|l| serde_json::from_str(l).unwrap())
                        .collect()
                })
                .unwrap_or_default()
        }
        pub fn manifest(&self) -> Value {
            serde_json::from_str(&self.tables["manifest.json"]).unwrap()
        }
        pub fn layer(&self, name: &str) -> Value {
            self.manifest()["scopes"][0]["layers"][name].clone()
        }
        pub fn gaps(&self, kind: &str) -> Vec<Value> {
            self.rows("gaps.jsonl")
                .into_iter()
                .filter(|g| g["gap"] == kind)
                .collect()
        }
        pub fn layer_gaps(&self, layer: &str) -> Vec<Value> {
            self.rows("gaps.jsonl")
                .into_iter()
                .filter(|g| g["layer"] == layer)
                .collect()
        }
        pub fn file(&self, path: &str) -> Value {
            self.rows("files.jsonl")
                .into_iter()
                .find(|f| f["path"] == path)
                .unwrap_or_else(|| panic!("no file {path}"))
        }
        pub fn entity(&self, key: &str) -> Value {
            self.rows("entities.jsonl")
                .into_iter()
                .find(|e| e["key"] == key)
                .unwrap_or_else(|| panic!("no entity {key}"))
        }
        pub fn member_in(&self, symbol: &str, file: &str) -> String {
            self.rows("entities.jsonl")
                .into_iter()
                .find(|e| e["symbol"] == symbol && e["def"]["file"] == file)
                .unwrap_or_else(|| panic!("no definition of {symbol} in {file}"))["key"]
                .as_str()
                .unwrap()
                .to_owned()
        }
        pub fn target(&self, key: &str) -> Value {
            self.rows("targets.jsonl")
                .into_iter()
                .find(|t| t["key"] == key)
                .unwrap_or(Value::Null)
        }
        /// The validator's codes for this snapshot.
        pub fn validation(&self) -> Vec<String> {
            let mut files = write::SnapshotFiles::default();
            for (name, slot) in [
                ("manifest.json", &mut files.manifest),
                ("files.jsonl", &mut files.files),
                ("entities.jsonl", &mut files.entities),
                ("externals.jsonl", &mut files.externals),
                ("relations.jsonl", &mut files.relations),
                ("syntax.jsonl", &mut files.syntax),
                ("gaps.jsonl", &mut files.gaps),
            ] {
                *slot = self.tables.get(name).map(|t| t.clone().into_bytes());
            }
            fill_targets(&mut files, self.tables.get("targets.jsonl"));
            gosanja_core::validate::validate(&files, None)
                .unwrap()
                .into_iter()
                .map(|d| format!("{}:{}", d.file, d.code))
                .collect()
        }
    }

    fn fill_targets(files: &mut write::SnapshotFiles, targets: Option<&String>) {
        files.targets = targets.map(|t| t.clone().into_bytes());
    }

    /// The standard tree: a library with two functions and two crate roots, a binary, an
    /// integration test, and two package clauses for a merged package.
    pub fn standard() -> Spec {
        Spec {
            documents: vec![
                doc(
                    "src/lib.rs",
                    vec![
                        occ(CRATE, DEF, &[0, 0, 0], &[0, 0, 2, 0]),
                        occ(ALPHA, DEF, &[0, 7, 12], &[0, 0, 17]),
                        occ(BETA, DEF, &[1, 7, 11], &[1, 0, 16]),
                    ],
                ),
                doc(
                    "src/main.rs",
                    vec![occ(CRATE, DEF, &[0, 0, 0], &[0, 0, 1, 0])],
                ),
                doc(
                    "tests/it.rs",
                    vec![occ("s m p 1 helper().", DEF, &[0, 3, 9], &[0, 0, 14])],
                ),
                doc("a.go", vec![occ(PACKAGE, DEF, &[0, 8, 11], &[])]),
                doc("b.go", vec![occ(PACKAGE, DEF, &[0, 8, 11], &[])]),
            ],
            sources: vec![
                ("src/lib.rs", LIB.to_owned()),
                ("src/main.rs", "fn main() {}\n".to_owned()),
                ("tests/it.rs", "fn helper() {}\n".to_owned()),
                ("a.go", "package pkg\n".to_owned()),
                ("b.go", "package pkg\n".to_owned()),
            ],
            merged: Some(format!(r#"["{PACKAGE}"]"#)),
            ..Spec::default()
        }
    }

    /// Runs the fake adapter on a fresh root and builds the snapshot the engine publishes.
    pub fn run(spec: &Spec) -> Result<Out, gosanja_core::adapter::AdapterError> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        fs::create_dir(&root).unwrap();
        for (path, text) in &spec.sources {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        let write_lines = |name: &str, lines: &[String]| {
            let path = tmp.path().join(name);
            fs::write(
                &path,
                lines.iter().fold(String::new(), |mut out, l| {
                    out.push_str(l);
                    out.push('\n');
                    out
                }),
            )
            .unwrap();
            path.into_os_string()
        };
        let index_file = tmp.path().join("index.scip");
        fs::write(&index_file, index("t", spec.documents.clone())).unwrap();
        let mut args: Vec<std::ffi::OsString> = vec![
            support::repo("fixtures/fake-adapter/adapter.sh").into_os_string(),
            "--mode".into(),
            spec.mode.unwrap_or("good").into(),
            "--index-file".into(),
            index_file.into_os_string(),
            "--syntax-file".into(),
            write_lines("syntax.jsonl", &spec.syntax),
            "--calls-file".into(),
            write_lines("calls.jsonl", &spec.calls),
            "--build-file".into(),
            write_lines("build.jsonl", &spec.build),
        ];
        let mut option = |name: &str, value: Option<String>| {
            if let Some(value) = value {
                args.push(name.into());
                args.push(value.into());
            }
        };
        option("--build-entry", spec.build_entry.clone());
        option("--layers", spec.offered.map(str::to_owned));
        option("--encoding", spec.encoding.map(str::to_owned));
        option("--merged", spec.merged.clone());
        option("--contract", CONTRACT.map(str::to_owned));
        let described = gosanja_core::adapter::describe(
            gosanja_core::adapter::Invocation::new(support::shell(), args).unwrap(),
        )?;
        let output = match spec.request {
            None => described.run(&root, tmp.path())?,
            Some(list) => described.run_with_layers(
                &root,
                tmp.path(),
                list.parse::<gosanja_schema::contract::LayerRequest>()
                    .expect("a layer list"),
            )?,
        };
        let sources = write::FsSource::new(&root).unwrap();
        let snapshot = sidecar::build(output, &sources, "t").unwrap();
        let tables = write::serialise(&snapshot)
            .named()
            .into_iter()
            .map(|(name, bytes)| {
                (
                    name.to_owned(),
                    String::from_utf8(bytes.unwrap_or_default().to_vec()).unwrap(),
                )
            })
            .collect();
        Ok(Out { tables })
    }

    /// A target row of the build layer.
    pub fn target(key: &str, root: Option<Value>) -> String {
        let mut row = serde_json::json!({"row": "target", "key": key, "kind": "lib", "name": key});
        if let Some(root) = root {
            row["root"] = root;
        }
        row.to_string()
    }

    /// A root at `symbol`, anchored at `[line, col]` of `file` when given.
    pub fn root(symbol: &str, anchor: Option<(&str, u32, u32)>) -> Value {
        match anchor {
            None => serde_json::json!({"symbol": symbol}),
            Some((file, line, col)) => {
                serde_json::json!({"symbol": symbol, "anchor": {"file": file, "line": line, "col": col}})
            }
        }
    }

    pub fn file(path: &str, extra: Value) -> String {
        let mut row = serde_json::json!({"row": "file", "file": path});
        if let Value::Object(fields) = extra {
            row.as_object_mut().unwrap().extend(fields);
        }
        row.to_string()
    }

    pub fn skipped(entries: &[(&str, &str)]) -> String {
        let skipped: Vec<_> = entries
            .iter()
            .map(|(file, code)| serde_json::json!({"file": file, "code": code}))
            .collect();
        serde_json::json!({"status": "ok", "skipped": skipped}).to_string()
    }
}
