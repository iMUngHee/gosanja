mod capture;
mod graph;
mod remap;

pub(crate) use capture::wrapper as compiler_wrapper;

use crate::project::Project;
use crate::result::Problem;
use gosanja_schema::snapshot::Tool;
use protobuf::Message;
use scip::types::Index;
use serde_json::Value;
use std::{
    collections::HashSet,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

fn command(
    project: &Project<'_>,
    name: &str,
    args: &[&OsStr],
    code: &'static str,
) -> Result<Vec<u8>, Problem> {
    let output = project
        .run(name, args)
        .map_err(|e| Problem::new(code, e.to_string()))?;
    if !output.status.success() {
        return Err(Problem::new(code, String::from_utf8_lossy(&output.stderr)));
    }
    Ok(output.stdout)
}
fn metadata(project: &Project<'_>, flag: &str, code: &'static str) -> Result<Value, Problem> {
    let args = ["metadata", flag, "--format-version", "1"].map(OsStr::new);
    serde_json::from_slice(&command(project, "cargo", &args, code)?)
        .map_err(|e| Problem::new(code, e.to_string()))
}

fn check_documents(index: &Index, meta: &Value, root: &Path) -> Result<(), Problem> {
    if index.documents.is_empty() {
        return Err(Problem::new(
            "index-incomplete",
            "index contains no documents",
        ));
    }
    let paths: HashSet<_> = index
        .documents
        .iter()
        .map(|d| Path::new(&d.relative_path))
        .collect();
    let Some(members) = meta["workspace_members"].as_array() else {
        return Err(Problem::new(
            "index-incomplete",
            "metadata lacks workspace members",
        ));
    };
    let Some(packages) = meta["packages"].as_array() else {
        return Err(Problem::new("index-incomplete", "metadata lacks packages"));
    };
    for member in members {
        let Some(targets) = packages
            .iter()
            .find(|p| p["id"] == *member)
            .and_then(|p| p["targets"].as_array())
        else {
            return Err(Problem::new(
                "index-incomplete",
                "metadata lacks workspace member targets",
            ));
        };
        for target in targets {
            let Some(kinds) = target["kind"].as_array() else {
                return Err(Problem::new(
                    "index-incomplete",
                    "metadata lacks target kind",
                ));
            };
            let required = kinds.iter().any(|k| {
                matches!(
                    k.as_str(),
                    Some("lib" | "proc-macro" | "bin" | "cdylib" | "dylib" | "rlib" | "staticlib")
                )
            }) && target["required-features"]
                .as_array()
                .is_none_or(Vec::is_empty);
            if required {
                let Some(src) = target["src_path"].as_str() else {
                    return Err(Problem::new(
                        "index-incomplete",
                        "metadata lacks target source path",
                    ));
                };
                let src = Path::new(src);
                let resolved = src.canonicalize().unwrap_or_else(|_| src.to_owned());
                if let Ok(relative) = resolved.strip_prefix(root)
                    && !paths.contains(relative)
                {
                    return Err(Problem::new(
                        "index-incomplete",
                        format!("missing target document {}", relative.display()),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The project's child environment and Cargo's locked metadata, handed to work that has to
/// run once the base layer's preconditions hold and before its indexer.
pub type BeforeIndex<'f> = dyn FnMut(&Project<'_>, &Value) + 'f;

pub fn build(
    root: &Path,
    out: &Path,
    tools: &mut Vec<Tool>,
    before_index: &mut BeforeIndex<'_>,
) -> io::Result<Result<Index, Problem>> {
    if !root.join("Cargo.toml").is_file() {
        return Ok(Err(Problem::new("manifest-missing", "")));
    }
    let project = match Project::new(root, out) {
        Ok(p) => p,
        Err(problem) => return Ok(Err(problem)),
    };
    for dir in ["target", "build", "tmp"] {
        fs::create_dir_all(out.join("work").join(dir))?;
    }
    let staged = out.join("work/index.scip");
    let indexed = prepare(&project, &staged, tools, before_index);
    if indexed.is_ok() {
        fs::rename(staged, out.join("index.scip"))?;
    }
    Ok(indexed)
}
fn prepare(
    project: &Project<'_>,
    staged: &Path,
    tools: &mut Vec<Tool>,
    before_index: &mut BeforeIndex<'_>,
) -> Result<Index, Problem> {
    for name in ["cargo", "rust-analyzer"] {
        let bytes = command(
            project,
            name,
            &[OsStr::new("--version")],
            "toolchain-missing",
        )?;
        let line = String::from_utf8_lossy(&bytes);
        let line = line.trim();
        let version = line
            .strip_prefix(name)
            .and_then(|s| s.strip_prefix(' '))
            .unwrap_or(line);
        tools.push(Tool {
            name: name.into(),
            version: version.into(),
        });
    }
    let unloaded = metadata(project, "--no-deps", "manifest-invalid")?;
    let workspace = unloaded["workspace_root"]
        .as_str()
        .and_then(|s| PathBuf::from(s).canonicalize().ok());
    if workspace.as_deref() != Some(project.root) {
        return Err(Problem::new("not-workspace-root", ""));
    }
    if !project.root.join("Cargo.lock").is_file() {
        return Err(Problem::new("lockfile-missing", ""));
    }
    let meta = metadata(project, "--locked", "locked-metadata-failed")?;
    before_index(project, &meta);
    let work = staged
        .parent()
        .expect("the staged SCIP file has a work directory");
    let mut graph = None;
    let config_value = match capture::collect(project, work, &meta)
        .map_err(|error| Problem::new("indexer-failed", error.to_string()))?
    {
        capture::Build::Native => None,
        capture::Build::Replay(path) => {
            let adapter = std::env::current_exe()
                .map_err(|error| Problem::new("indexer-failed", error.to_string()))?;
            Some(
                serde_json::json!({"cargo":{"buildScripts":{"overrideCommand":[
                    adapter, "cargo-build-messages", path
                ]}}}),
            )
        }
        capture::Build::Split(capture) => {
            graph = Some(
                graph::write(project, work, &meta, &capture)
                    .map_err(|error| Problem::new("indexer-failed", error.to_string()))?,
            );
            Some(serde_json::json!({"cfg":{"setTest":false}}))
        }
    };
    let index_root = graph
        .as_ref()
        .map_or(OsStr::new("."), |graph| graph.root.as_os_str());
    let mut args = vec![
        OsStr::new("scip"),
        index_root,
        OsStr::new("--output"),
        staged.as_os_str(),
    ];
    let config = work.join("rust-analyzer-config.json");
    if let Some(value) = config_value {
        let write_config = || -> io::Result<()> {
            serde_json::to_writer(fs::File::create(&config)?, &value)?;
            Ok(())
        };
        write_config().map_err(|error| Problem::new("indexer-failed", error.to_string()))?;
        args.extend([OsStr::new("--config-path"), config.as_os_str()]);
    }
    command(project, "rust-analyzer", &args, "indexer-failed")?;
    if !staged.is_file() {
        return Err(Problem::new("indexer-failed", "index file missing"));
    }
    let bytes = fs::read(staged).map_err(|e| Problem::new("index-unreadable", e.to_string()))?;
    let mut index = Index::parse_from_bytes(&bytes)
        .map_err(|e| Problem::new("index-unreadable", e.to_string()))?;
    if let Some(graph) = graph {
        remap::restore(&mut index, project.root, &graph)
            .and_then(|()| {
                let bytes = index.write_to_bytes().map_err(io::Error::other)?;
                fs::write(staged, bytes)
            })
            .map_err(|error| Problem::new("indexer-failed", error.to_string()))?;
    }
    check_documents(&index, &meta, project.root)?;
    Ok(index)
}
