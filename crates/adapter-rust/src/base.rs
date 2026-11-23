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

pub fn build(root: &Path, out: &Path, tools: &mut Vec<Tool>) -> io::Result<Result<Index, Problem>> {
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
    let indexed = prepare(&project, &staged, tools);
    if indexed.is_ok() {
        fs::rename(staged, out.join("index.scip"))?;
    }
    Ok(indexed)
}
fn prepare(project: &Project<'_>, staged: &Path, tools: &mut Vec<Tool>) -> Result<Index, Problem> {
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
    let args = [
        OsStr::new("scip"),
        OsStr::new("."),
        OsStr::new("--output"),
        staged.as_os_str(),
    ];
    command(project, "rust-analyzer", &args, "indexer-failed")?;
    if !staged.is_file() {
        return Err(Problem::new("indexer-failed", "index file missing"));
    }
    let bytes = fs::read(staged).map_err(|e| Problem::new("index-unreadable", e.to_string()))?;
    let index = Index::parse_from_bytes(&bytes)
        .map_err(|e| Problem::new("index-unreadable", e.to_string()))?;
    check_documents(&index, &meta, project.root)?;
    Ok(index)
}
