//! The calls layer: calls the compiler emitted, read from the textual LLVM IR of each
//! selected target and corroborated by the base layer's index.

pub mod build;
pub mod debug;
pub mod ir;
pub mod rows;

use build::{Built, Failure};
use debug::{Module, Refused};
use gosanja_schema::contract::{CallEvidence, Diagnostic, LayerResult, LayerStatus, SkippedFile};
use gosanja_schema::snapshot::BuildContext;
use serde_json::Value;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Output;

/// The file the layer writes its rows to.
pub const FILE: &str = "calls.jsonl";

/// The layer's entry of `result.json`, its build contexts, and the version of `rustc` that
/// a module named, if one was read that far.
#[derive(Debug)]
pub struct Outcome {
    pub layer: LayerResult,
    pub builds: Option<Vec<BuildContext>>,
    pub rustc: Option<String>,
}

impl Outcome {
    /// A failed layer: no rows, no build context.
    #[must_use]
    pub fn failed(failure: &Failure, rustc: Option<String>) -> Self {
        eprintln!("calls: {}", failure.code);
        if !failure.message.is_empty() {
            eprintln!("{}", failure.message);
        }
        Self {
            layer: LayerResult {
                status: LayerStatus::Failed,
                diagnostics: Some(vec![Diagnostic {
                    code: failure.code.into(),
                    message: None,
                }]),
                skipped: None,
            },
            builds: None,
            rustc,
        }
    }
}

/// Compiles the selected targets of the workspace that `meta`, Cargo's locked metadata,
/// describes, writing their IR under `<out>/work/calls`. Runs before the base layer's
/// indexer.
///
/// # Errors
/// An I/O error when the directory for the IR cannot be made.
pub fn compile(
    out: &Path,
    meta: &Value,
    cargo: &mut dyn FnMut(&[OsString]) -> io::Result<Output>,
) -> io::Result<Result<Vec<Built>, Failure>> {
    if out.as_os_str().as_encoded_bytes().contains(&b',') {
        return Ok(Err(Failure::new(
            "build-unsupported",
            format!(
                "the output directory {} holds a comma, which rustc's --emit cannot name",
                out.display()
            ),
        )));
    }
    let targets = match build::select(meta) {
        Ok(t) => t,
        Err(f) => return Ok(Err(f)),
    };
    if targets.is_empty() {
        return Ok(Ok(Vec::new()));
    }
    if let Some(id) = build::shared_id(&targets) {
        return Ok(Err(Failure::new(
            "build-context-ambiguous",
            format!("two selected targets give the build context id {id}"),
        )));
    }
    let calls = out.join("work").join("calls");
    fs::create_dir_all(&calls)?;
    Ok(build::compile(cargo, &calls, &targets))
}

/// The crate names of every target of the workspace members.
#[must_use]
pub fn member_crates(meta: &Value) -> BTreeSet<String> {
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["id"].as_str().is_some_and(|id| members.contains(id)))
        .flat_map(|p| p["targets"].as_array().into_iter().flatten())
        .filter_map(|t| t["name"].as_str())
        .map(|name| name.replace('-', "_"))
        .collect()
}

/// Rows and omissions made from the modules, each with its build context id.
pub type Rows = (Vec<CallEvidence>, Vec<SkippedFile>);

/// Reads the IR of the built targets after the indexer has run, makes the rows with
/// `rows`, and writes `calls.jsonl` into `out` when the layer succeeds.
///
/// # Errors
/// When `calls.jsonl` cannot be written.
pub fn finish(
    out: &Path,
    built: Result<Vec<Built>, Failure>,
    rows: impl FnOnce(&[(&Module, &str)]) -> Rows,
) -> io::Result<Outcome> {
    let built = match built {
        Ok(b) => b,
        Err(f) => return Ok(Outcome::failed(&f, None)),
    };
    let mut modules = Vec::with_capacity(built.len());
    let mut refusals = Vec::new();
    let mut rustc = None;
    for b in &built {
        let read = fs::read(&b.ir)
            .map_err(|e| e.to_string())
            .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()));
        let module = match read {
            Ok(text) => Module::read(&text),
            Err(e) => Err(Refused::Unreadable {
                error: ir::Unreadable::new(0, e),
                toolchain: None,
            }),
        };
        match module {
            Ok(m) => {
                rustc.get_or_insert_with(|| version(&m.toolchain));
                modules.push(m);
            }
            Err(r) => {
                if let Some(t) = r.toolchain() {
                    rustc.get_or_insert_with(|| version(t));
                }
                refusals.push((b.target.context_id(), r));
            }
        }
    }
    let unreadable = refusals
        .iter()
        .find(|(_, r)| matches!(r, Refused::Unreadable { .. }));
    if let Some((id, Refused::Unreadable { error, .. })) = unreadable {
        let failure = Failure::new("ir-unreadable", format!("{id}: {error}"));
        return Ok(Outcome::failed(&failure, rustc));
    }
    if let Some((id, _)) = refusals.first() {
        let failure = Failure::new(
            "build-unsupported",
            format!("{id}: the module says it was optimised"),
        );
        return Ok(Outcome::failed(&failure, rustc));
    }
    let builds: Vec<BuildContext> = built
        .iter()
        .zip(&modules)
        .map(|(b, m)| BuildContext {
            id: b.target.context_id(),
            toolchain: Some(m.toolchain.clone()),
            target: Some(m.triple.clone()),
            profile: Some("dev".into()),
            features: Some(b.features.clone()),
        })
        .collect();
    let ids: Vec<String> = builds.iter().map(|c| c.id.clone()).collect();
    let pairs: Vec<(&Module, &str)> = modules.iter().zip(ids.iter().map(String::as_str)).collect();
    let (evidence, skipped) = rows(&pairs);
    write(&evidence, &out.join(FILE))?;
    Ok(Outcome {
        layer: LayerResult {
            status: LayerStatus::Ok,
            diagnostics: None,
            skipped: (!skipped.is_empty()).then_some(skipped),
        },
        builds: (!builds.is_empty()).then_some(builds),
        rustc,
    })
}

/// The version part of a toolchain, `rustc 1.99.1 (…)` giving `1.99.1 (…)`.
fn version(toolchain: &str) -> String {
    toolchain
        .strip_prefix("rustc ")
        .unwrap_or(toolchain)
        .to_owned()
}

fn write(rows: &[CallEvidence], path: &Path) -> io::Result<()> {
    let mut out = io::BufWriter::new(fs::File::create(path)?);
    for row in rows {
        serde_json::to_writer(&mut out, row)?;
        out.write_all(b"\n")?;
    }
    out.flush()
}
