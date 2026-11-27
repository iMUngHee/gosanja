//! Which targets the calls layer compiles, how it compiles them, and what Cargo reports.

use serde_json::Value;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Why the layer failed: a code of `result.json` and a message for standard error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
}

impl Failure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// The kind of a selected target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Bin,
    Lib,
}

impl Kind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bin => "bin",
            Self::Lib => "lib",
        }
    }
}

/// A target the layer compiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub package: String,
    pub version: String,
    pub package_id: String,
    pub kind: Kind,
    pub name: String,
    /// The target's kinds as Cargo's metadata gives them, to match its artifact message.
    pub kinds: Vec<String>,
}

impl Selected {
    /// The id of its build context: `<package>-<kind>-<target name>`, every character
    /// outside `[A-Za-z0-9._-]` replaced by `_`.
    #[must_use]
    pub fn context_id(&self) -> String {
        format!("{}-{}-{}", self.package, self.kind.as_str(), self.name)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }
}

const LIB_KINDS: [&str; 6] = ["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];

/// The targets of the workspace members that the layer compiles.
///
/// In the order of package name, target kind and target name: each member's library,
/// whatever its crate types, and each binary whose required features all lie in the closure of `default` over the
/// package's own feature table.
///
/// # Errors
/// When the metadata lacks what a member's targets need.
pub fn select(meta: &Value) -> Result<Vec<Selected>, Failure> {
    let missing = |what: &str| Failure::new("build-failed", format!("metadata lacks {what}"));
    let members = meta["workspace_members"]
        .as_array()
        .ok_or_else(|| missing("workspace members"))?;
    let packages = meta["packages"]
        .as_array()
        .ok_or_else(|| missing("packages"))?;
    let mut out = Vec::new();
    for member in members {
        let package = packages
            .iter()
            .find(|p| p["id"] == *member)
            .ok_or_else(|| missing("a workspace member"))?;
        let text =
            |v: &Value, what: &str| v.as_str().map(str::to_owned).ok_or_else(|| missing(what));
        let name = text(&package["name"], "a package name")?;
        let version = text(&package["version"], "a package version")?;
        let package_id = text(&package["id"], "a package id")?;
        let enabled = default_closure(&package["features"]);
        let targets = package["targets"]
            .as_array()
            .ok_or_else(|| missing("targets"))?;
        for target in targets {
            let kinds: Vec<String> = target["kind"]
                .as_array()
                .ok_or_else(|| missing("a target kind"))?
                .iter()
                .filter_map(|k| k.as_str().map(str::to_owned))
                .collect();
            let kind = if kinds.iter().any(|k| LIB_KINDS.contains(&k.as_str())) {
                Kind::Lib
            } else if kinds.iter().any(|k| k == "bin") {
                Kind::Bin
            } else {
                continue;
            };
            if kind == Kind::Bin {
                let required = target["required-features"].as_array();
                let met = required.is_none_or(|r| {
                    r.iter()
                        .all(|f| f.as_str().is_some_and(|f| enabled.contains(f)))
                });
                if !met {
                    continue;
                }
            }
            out.push(Selected {
                package: name.clone(),
                version: version.clone(),
                package_id: package_id.clone(),
                kind,
                name: text(&target["name"], "a target name")?,
                kinds,
            });
        }
    }
    out.sort_by(|a, b| {
        (&a.package, a.kind.as_str(), &a.name).cmp(&(&b.package, b.kind.as_str(), &b.name))
    });
    Ok(out)
}

/// `default` and every entry reached from it in a feature table; empty when the table has
/// no `default`.
fn default_closure(features: &Value) -> BTreeSet<String> {
    let mut reached = BTreeSet::new();
    let Some(table) = features.as_object().filter(|t| t.contains_key("default")) else {
        return reached;
    };
    let mut pending = vec!["default".to_owned()];
    while let Some(entry) = pending.pop() {
        if let Some(entries) = table.get(&entry).and_then(Value::as_array) {
            pending.extend(
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|e| !reached.contains(*e))
                    .map(str::to_owned),
            );
        }
        reached.insert(entry);
    }
    reached
}

/// The first id that two selected targets share, if any.
#[must_use]
pub fn shared_id(targets: &[Selected]) -> Option<String> {
    let mut seen = BTreeSet::new();
    targets
        .iter()
        .map(Selected::context_id)
        .find(|id| !seen.insert(id.clone()))
}

/// A target as Cargo built it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    pub target: Selected,
    pub ir: PathBuf,
    /// The features of its artifact message, sorted.
    pub features: Vec<String>,
}

/// The arguments of the one `cargo` invocation for a target, writing its IR to `ir`.
#[must_use]
pub fn arguments(target: &Selected, ir: &Path) -> Vec<OsString> {
    let mut emit = OsString::from("--emit=llvm-ir=");
    emit.push(ir);
    let mut args: Vec<OsString> = [
        "rustc",
        "--locked",
        "--offline",
        "--message-format=json",
        "-p",
    ]
    .map(OsString::from)
    .into();
    args.push(format!("{}@{}", target.package, target.version).into());
    match target.kind {
        Kind::Lib => args.push("--lib".into()),
        Kind::Bin => args.extend(["--bin".into(), OsString::from(&target.name)]),
    }
    args.push("--".into());
    args.push(emit);
    for flag in [
        "codegen-units=1",
        "debuginfo=line-tables-only",
        "opt-level=0",
    ] {
        args.extend(["-C".into(), flag.into()]);
    }
    args
}

/// Compiles every target, writing the IR of the `n`th to `<calls>/<n>.ll`.
///
/// What Cargo reported is checked in this order: a target that does not compile, then one with more
/// than one artifact message, then one with none or without its file.
///
/// # Errors
/// The failure of the first of those situations that holds, for the first target it holds
/// for.
pub fn compile(
    cargo: &mut dyn FnMut(&[OsString]) -> io::Result<Output>,
    calls: &Path,
    targets: &[Selected],
) -> Result<Vec<Built>, Failure> {
    let mut reports = Vec::with_capacity(targets.len());
    for (n, target) in targets.iter().enumerate() {
        let ir = calls.join(format!("{n}.ll"));
        let output = cargo(&arguments(target, &ir)).map_err(|e| failed(target, &e.to_string()))?;
        if !output.status.success() {
            return Err(failed(target, &String::from_utf8_lossy(&output.stderr)));
        }
        reports.push((ir, artifacts(&output.stdout, target)));
    }
    for (target, (_, found)) in targets.iter().zip(&reports) {
        if found.len() > 1 {
            return Err(Failure::new(
                "build-unsupported",
                format!(
                    "Cargo reported {} artifacts for {}",
                    found.len(),
                    target.context_id()
                ),
            ));
        }
    }
    let mut built = Vec::with_capacity(targets.len());
    for (target, (ir, found)) in targets.iter().zip(reports) {
        let Some(features) = found.into_iter().next().filter(|_| ir.is_file()) else {
            return Err(Failure::new(
                "ir-missing",
                format!("no IR for {} at {}", target.context_id(), ir.display()),
            ));
        };
        built.push(Built {
            target: target.clone(),
            ir,
            features,
        });
    }
    Ok(built)
}

fn failed(target: &Selected, detail: &str) -> Failure {
    Failure::new(
        "build-failed",
        format!(
            "{} did not compile. `cargo fetch` restores registry and git dependencies missing \
             from the local cache; a path dependency has to be present.\n{detail}",
            target.context_id()
        ),
    )
}

/// The features of each `compiler-artifact` message for `target`, sorted.
fn artifacts(stdout: &[u8], target: &Selected) -> Vec<Vec<String>> {
    let mut found = Vec::new();
    for line in stdout.split(|b| *b == b'\n') {
        let Ok(message) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-artifact" || message["package_id"] != *target.package_id {
            continue;
        }
        let kinds: Option<Vec<&str>> = message["target"]["kind"]
            .as_array()
            .map(|k| k.iter().filter_map(Value::as_str).collect());
        if message["target"]["name"] != *target.name
            || kinds.as_deref()
                != Some(&target.kinds.iter().map(String::as_str).collect::<Vec<_>>())
        {
            continue;
        }
        let mut features: Vec<String> = message["features"]
            .as_array()
            .map(|f| {
                f.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        features.sort();
        found.push(features);
    }
    found
}
