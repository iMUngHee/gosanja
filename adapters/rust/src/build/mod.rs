//! Build targets and the compilation evidence for their files.

mod dep;
mod units;
pub use dep::dep_info;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Output;

use gosanja_schema::contract::{
    Anchor, BuildRow, Diagnostic, FileFact, LayerResult, LayerStatus, SkippedFile, TargetFact,
};
use gosanja_schema::snapshot::{FileTest, TargetRoot};
use scip::types::{Index, PositionEncoding, SymbolRole};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug)]
struct Target {
    package: String,
    identity: Value,
    row: TargetFact,
}

fn key_component(value: &str) -> String {
    use std::fmt::Write as _;
    let mut key = String::new();
    for ch in value.chars() {
        if ch.is_whitespace() || ch.is_control() || matches!(ch, '%' | '\u{feff}') {
            for byte in ch.encode_utf8(&mut [0; 4]).as_bytes() {
                let _ = write!(key, "%{byte:02X}");
            }
        } else {
            key.push(ch);
        }
    }
    key
}

fn target_root(file: &str, index: &Index) -> Option<TargetRoot> {
    let document = index
        .documents
        .iter()
        .find(|doc| doc.relative_path == file)?;
    if !valid_encoding(document) {
        return None;
    }
    let mut roots = document
        .occurrences
        .iter()
        .filter(|occurrence| {
            occurrence.symbol.ends_with(" crate/")
                && occurrence.symbol_roles & SymbolRole::Definition as i32 != 0
                && crate::syntax::convert(occurrence, Some)
                    .is_some_and(|(range, _)| range.0[..2] == [0, 0])
        })
        .map(|occurrence| occurrence.symbol.as_str());
    let symbol = roots.next()?;
    roots
        .all(|candidate| candidate == symbol)
        .then(|| TargetRoot {
            symbol: symbol.into(),
            anchor: Some(Anchor {
                file: file.into(),
                line: 0,
                col: Some(0),
                name: None,
            }),
        })
}

fn targets(
    meta: &Value,
    root: &Path,
    index: &Index,
) -> (BTreeMap<String, Target>, Option<&'static str>) {
    let mut targets = BTreeMap::new();
    let mut diagnostic = None;
    for package in meta["packages"].as_array().into_iter().flatten() {
        if !meta["workspace_members"]
            .as_array()
            .is_some_and(|ids| ids.contains(&package["id"]))
        {
            continue;
        }
        let (Some(package_id), Some(package_name)) =
            (package["id"].as_str(), package["name"].as_str())
        else {
            continue;
        };
        for target in package["targets"].as_array().into_iter().flatten() {
            let (Some(src), Some(name), Some(kinds)) = (
                target["src_path"].as_str(),
                target["name"].as_str(),
                target["kind"].as_array(),
            ) else {
                continue;
            };
            let src = Path::new(src);
            let normal = units::lexical(src);
            let Ok(relative) = normal.strip_prefix(root) else {
                continue;
            };
            if src.canonicalize().is_ok_and(|path| !path.starts_with(root)) {
                continue;
            }
            let kinds: Option<Vec<_>> = kinds.iter().map(Value::as_str).collect();
            let Some(kind) = kinds.as_deref().and_then(normalize_kind) else {
                diagnostic = Some("target-kind-unknown");
                continue;
            };
            let key = format!(
                "cargo:{}:{kind}:{}",
                key_component(package_name),
                key_component(name)
            );
            if key.len() > 1024 {
                return (BTreeMap::new(), Some("target-key-invalid"));
            }
            let features: BTreeSet<_> = target["required-features"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let row = TargetFact {
                key: key.clone(),
                kind: kind.into(),
                name: name.into(),
                root: relative
                    .to_str()
                    .and_then(|file| target_root(&file.replace('\\', "/"), index)),
                required_features: (!features.is_empty()).then(|| features.into_iter().collect()),
                test: matches!(kind, "test" | "bench").then_some(true),
            };
            if targets
                .insert(
                    key,
                    Target {
                        package: package_id.into(),
                        identity: target.clone(),
                        row,
                    },
                )
                .is_some()
            {
                return (BTreeMap::new(), Some("duplicate-target"));
            }
        }
    }
    (targets, diagnostic)
}

fn valid_encoding(doc: &scip::types::Document) -> bool {
    matches!(
        doc.position_encoding.enum_value(),
        Ok(PositionEncoding::UTF8CodeUnitOffsetFromLineStart
            | PositionEncoding::UTF16CodeUnitOffsetFromLineStart
            | PositionEncoding::UTF32CodeUnitOffsetFromLineStart)
    )
}

fn files(index: &Index, root: &Path) -> BTreeMap<String, PathBuf> {
    index
        .documents
        .iter()
        .filter(|doc| valid_encoding(doc))
        .filter_map(|doc| {
            let file = doc.relative_path.as_str();
            if file.contains('\\') || file.split('/').any(|s| matches!(s, "" | "." | "..")) {
                return None;
            }
            let canonical = root.join(file).canonicalize().ok()?;
            (canonical.starts_with(root) && canonical.is_file() && fs::read(&canonical).is_ok())
                .then(|| (file.to_owned(), canonical))
        })
        .collect()
}

#[derive(Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts establish or veto test evidence"
)]
struct Evidence {
    targets: BTreeSet<String>,
    eligible: bool,
    production: bool,
    contradicts: bool,
    target_only: bool,
    unadmitted: bool,
    aliased: bool,
}

fn hash(path: &Path) -> Option<[u8; 32]> {
    fs::read(path)
        .ok()
        .map(|bytes| Sha256::digest(bytes).into())
}

/// Positive item evidence and the build rows from one set of checks.
#[derive(Debug, Default)]
pub struct Build {
    rows: Vec<BuildRow>,
    skipped: Vec<SkippedFile>,
    diagnostic: Option<&'static str>,
    failure: Option<&'static str>,
    /// Exact document names for which syntax may use a test gate.
    pub eligible: BTreeSet<String>,
    /// Exact document names whose item evidence could not be decided.
    pub withheld: BTreeSet<String>,
}

impl Build {
    fn failed(files: &BTreeMap<String, PathBuf>, code: &'static str) -> Self {
        Self {
            failure: Some(code),
            withheld: files.keys().cloned().collect(),
            ..Self::default()
        }
    }

    /// Write the build side-car and return its result entry.
    ///
    /// # Errors
    /// If the side-car could not be written.
    pub fn publish(self, out: &Path) -> io::Result<LayerResult> {
        if self.failure.is_none() {
            let mut file = io::BufWriter::new(fs::File::create(out.join("build.jsonl"))?);
            for row in self.rows {
                serde_json::to_writer(&mut file, &row)?;
                file.write_all(b"\n")?;
            }
            file.flush()?;
        }
        Ok(LayerResult {
            status: if self.failure.is_some() {
                LayerStatus::Failed
            } else {
                LayerStatus::Ok
            },
            diagnostics: self.failure.or(self.diagnostic).map(|code| {
                vec![Diagnostic {
                    code: code.into(),
                    message: None,
                }]
            }),
            skipped: (!self.skipped.is_empty()).then_some(self.skipped),
            merged: None,
        })
    }
}

type Launch<'a> = dyn FnMut(&str, &[OsString], &[(&str, &OsStr)]) -> io::Result<Output> + 'a;

fn probe(launch: &mut Launch<'_>, tool: &str, args: &[&str]) -> Option<String> {
    let output = launch(
        tool,
        &args.iter().map(OsString::from).collect::<Vec<_>>(),
        &[],
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

/// Run the four checks needed by build or syntax. The launcher supplies the project
/// environment before applying the check's overrides.
#[expect(
    clippy::too_many_lines,
    reason = "the four checks share one ordered coverage and source sampling loop"
)]
pub fn collect(
    root: &Path,
    out: &Path,
    meta: &Value,
    index: &Index,
    syntax_hashes: Option<&BTreeMap<String, [u8; 32]>>,
    launch: &mut Launch<'_>,
) -> Build {
    let documents = files(index, root);
    let (targets, diagnostic) = targets(meta, root, index);
    if let Some(code @ ("duplicate-target" | "target-key-invalid")) = diagnostic {
        return Build::failed(&documents, code);
    }
    let Some(sysroot) = probe(launch, "rustc", &["--print", "sysroot"]) else {
        return Build::failed(&documents, "check-failed");
    };
    let compiler = Path::new(sysroot.trim()).join(if cfg!(windows) {
        "bin/rustc.exe"
    } else {
        "bin/rustc"
    });
    let Some(version) = compiler
        .to_str()
        .filter(|_| compiler.is_absolute())
        .and_then(|compiler| probe(launch, compiler, &["-vV"]))
    else {
        return Build::failed(&documents, "check-failed");
    };
    let Some(host) = version.lines().find_map(|line| line.strip_prefix("host: ")) else {
        return Build::failed(&documents, "check-failed");
    };
    let before: BTreeMap<_, _> = documents
        .values()
        .filter_map(|path| hash(path).map(|sum| (path.clone(), sum)))
        .collect();
    let mut unstable: BTreeSet<_> = documents
        .iter()
        .filter_map(|(file, path)| {
            let parsed = syntax_hashes.and_then(|hashes| hashes.get(file));
            parsed
                .filter(|parsed| Some(*parsed) != before.get(path))
                .map(|_| path.clone())
        })
        .collect();
    let mut runs = Vec::new();
    for (i, extra) in [
        &["--all-targets"][..],
        &["--all-targets", "--profile", "test"],
        &["--all-targets", "--all-features"],
        &["--lib", "--bins"],
    ]
    .into_iter()
    .enumerate()
    {
        let run_dir = out.join("work").join(format!("check-{i}"));
        let target = run_dir.join("target");
        let build = run_dir.join("build");
        let env = [
            ("RUSTC", compiler.as_os_str()),
            ("RUSTC_WRAPPER", OsStr::new("")),
            ("RUSTC_WORKSPACE_WRAPPER", OsStr::new("")),
            ("CARGO_TARGET_DIR", target.as_os_str()),
            ("CARGO_BUILD_BUILD_DIR", build.as_os_str()),
        ];
        let args: Vec<_> = [
            "check",
            "--workspace",
            "--locked",
            "--message-format=json",
            "-v",
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .map(OsString::from)
        .collect();
        let Ok(result) = launch("cargo", &args, &env) else {
            if i < 2 {
                return Build::failed(&documents, "check-failed");
            }
            runs.push(units::Run {
                complete: false,
                units: Vec::new(),
            });
            continue;
        };
        if !result.status.success() && i < 2 {
            for message in result
                .stdout
                .split(|byte| *byte == b'\n')
                .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
            {
                if let Some(rendered) = message["message"]["rendered"].as_str() {
                    eprint!("{rendered}");
                }
            }
            eprintln!("{}", String::from_utf8_lossy(&result.stderr));
            return Build::failed(&documents, "check-failed");
        }
        let run = units::read(&result, &compiler, host, root, &target, &build);
        for path in run
            .units
            .iter()
            .flat_map(|unit| &unit.files)
            .map(|listing| listing.canonical.clone())
            .filter(|path| path.starts_with(root))
            .chain(documents.values().cloned())
        {
            if hash(&path).as_ref() != before.get(&path) || !before.contains_key(&path) {
                unstable.insert(path);
            }
        }
        runs.push(run);
    }
    summarise(root, documents, targets, diagnostic, &runs, &unstable)
}

#[expect(
    clippy::too_many_lines,
    reason = "membership and test marks share the same accounting of compiler evidence"
)]
fn summarise(
    root: &Path,
    documents: BTreeMap<String, PathBuf>,
    targets: BTreeMap<String, Target>,
    diagnostic: Option<&'static str>,
    runs: &[units::Run],
    unstable: &BTreeSet<PathBuf>,
) -> Build {
    let admitted: BTreeSet<_> = documents.values().cloned().collect();
    let mut seen = BTreeSet::new();
    let mut evidence = BTreeMap::<PathBuf, Evidence>::new();
    let mut membership_incomplete = false;
    for (run, data) in runs.iter().enumerate() {
        for unit in &data.units {
            let target = targets.iter().find(|(_, target)| {
                target.package == unit.package
                    && ["name", "kind", "src_path"]
                        .iter()
                        .all(|field| target.identity[field] == unit.target[field])
            });
            if let Some((key, _)) = target {
                seen.insert(key);
            }
            if unit.opaque {
                membership_incomplete = true;
                continue;
            }
            let root_admitted = unit.target["src_path"]
                .as_str()
                .is_some_and(|path| admitted.contains(&units::lexical(Path::new(path))));
            for listing in &unit.files {
                if !admitted.contains(&listing.canonical) {
                    continue;
                }
                let ev = evidence
                    .entry(listing.canonical.clone())
                    .or_insert_with(|| Evidence {
                        target_only: true,
                        ..Evidence::default()
                    });
                ev.aliased |= listing.aliased;
                if let Some((key, _)) = target {
                    ev.targets.insert(key.clone());
                }
                if unit.test && run >= 2 {
                    continue;
                }
                ev.unadmitted |= !root_admitted;
                if unit.test {
                    ev.eligible |= target.is_some();
                    ev.target_only &= target
                        .is_some_and(|(_, t)| matches!(t.row.kind.as_str(), "bench" | "test"));
                } else {
                    ev.production = true;
                    ev.contradicts |= unit.enables_test;
                }
            }
        }
    }
    for (file, path) in &documents {
        if units::lexical(&root.join(file)) != *path
            && let Some(ev) = evidence.get_mut(path)
        {
            ev.aliased = true;
        }
    }
    let complete = runs.iter().all(|run| run.complete) && seen.len() == targets.len();
    membership_incomplete |= !complete;
    let mut layer = Build {
        rows: targets
            .into_values()
            .map(|target| BuildRow::Target(target.row))
            .collect(),
        diagnostic,
        ..Build::default()
    };
    for (file, path) in documents {
        let ev = evidence.get(&path);
        if !complete
            || unstable.contains(&path)
            || ev.is_some_and(|ev| ev.contradicts || (ev.unadmitted && ev.eligible))
        {
            layer.withheld.insert(file.clone());
        }
        let eligible =
            !layer.withheld.contains(&file) && ev.is_some_and(|ev| ev.eligible && !ev.aliased);
        if eligible {
            layer.eligible.insert(file.clone());
        }
        let code = if let Some(ev) = ev.filter(|ev| !ev.targets.is_empty()) {
            let test = (eligible && !ev.production).then_some(if ev.target_only {
                FileTest::Target
            } else {
                FileTest::Cfg
            });
            layer.rows.push(BuildRow::File(FileFact {
                file: file.clone(),
                targets: Some(ev.targets.iter().cloned().collect()),
                test,
                constraint: None,
            }));
            membership_incomplete.then_some("targets-incomplete")
        } else {
            Some("target-unknown")
        };
        if let Some(code) = code {
            layer.skipped.push(SkippedFile {
                file,
                code: code.into(),
            });
        }
    }
    layer
}

/// Cargo may report several library crate types for one target.
#[must_use]
pub fn normalize_kind(kinds: &[&str]) -> Option<&'static str> {
    match kinds {
        ["custom-build"] => Some("build"),
        ["proc-macro"] => Some("proc-macro"),
        ["bin"] => Some("bin"),
        ["test"] => Some("test"),
        ["bench"] => Some("bench"),
        ["example"] => Some("example"),
        _ if !kinds.is_empty()
            && kinds
                .iter()
                .all(|kind| matches!(*kind, "lib" | "rlib" | "dylib" | "cdylib" | "staticlib")) =>
        {
            Some("lib")
        }
        _ => None,
    }
}
