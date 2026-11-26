//! Process entry point for the Rust adapter.
mod base;
mod project;
mod result;

use gosanja_adapter_rust::{
    build,
    calls::{self, Outcome, build::Failure, debug::RootFiles},
    syntax,
};
use gosanja_schema::contract::{LayerName, LayerRequest};
use result::Layers;
use std::{
    collections::BTreeSet,
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

fn index_options(args: &[OsString]) -> Result<(PathBuf, PathBuf, LayerRequest), String> {
    let mut root = None;
    let mut out = None;
    let mut request = None;
    let mut options = args.iter();
    while let Some(name) = options.next() {
        let value = options
            .next()
            .ok_or_else(|| format!("missing value for {}", name.to_string_lossy()))?;
        match name.to_str() {
            Some("--root") if root.is_none() => root = Some(value.into()),
            Some("--out") if out.is_none() => out = Some(value.into()),
            Some("--layers") if request.is_none() => {
                let list = value
                    .to_str()
                    .ok_or_else(|| format!("invalid layer list {}", value.to_string_lossy()))?;
                request = Some(list.parse()?);
            }
            _ => {
                return Err(format!(
                    "unexpected index option {}",
                    name.to_string_lossy()
                ));
            }
        }
    }
    Ok((
        root.ok_or("missing --root")?,
        out.ok_or("missing --out")?,
        request.unwrap_or(LayerRequest::ALL),
    ))
}

fn index(root: &Path, out: &Path, request: LayerRequest) -> Result<ExitCode, io::Error> {
    if !root.is_dir() {
        eprintln!("expected a project directory at {}", root.display());
        return Ok(ExitCode::from(2));
    }
    let root = root.canonicalize()?;
    let Ok(resolved_out) = out.canonicalize() else {
        eprintln!("expected an existing output directory at {}", out.display());
        return Ok(ExitCode::from(2));
    };
    if !resolved_out.is_dir()
        || resolved_out.starts_with(&root)
        || fs::read_dir(&resolved_out)?.next().is_some()
    {
        eprintln!(
            "expected an empty output directory outside the project at {}",
            out.display()
        );
        return Ok(ExitCode::from(2));
    }
    let out = resolved_out;
    let mut tools = Vec::new();
    let want_calls = request.contains(LayerName::Calls);
    let want_syntax = request.contains(LayerName::Syntax);
    let want_build = request.contains(LayerName::Build);
    let mut metadata = None;
    let mut compiled_calls = None;
    let mut crates = BTreeSet::new();
    let indexed = base::build(&root, &out, &mut tools, &mut |project, meta| {
        if want_build || want_syntax {
            metadata = Some(meta.clone());
        }
        if want_calls {
            crates = calls::member_crates(meta);
            compiled_calls = Some(calls::compile(&out, meta, &mut |args| {
                project.run("cargo", args)
            }));
        }
    })?;
    let compiled_calls = compiled_calls.transpose()?;
    let layers = match indexed {
        Ok(index) => {
            let mut syntax = want_syntax.then(|| syntax::layer(&root, &index));
            let build_result = if let Some(meta) = metadata {
                let project =
                    project::Project::new(&root, &out).map_err(|p| io::Error::other(p.detail))?;
                let evidence = build::collect(
                    &root,
                    &out,
                    &meta,
                    &index,
                    syntax.as_ref().map(|layer| &layer.hashes),
                    &mut |tool, args, vars| project.run_with(tool, args, vars),
                );
                if let Some(syntax) = &mut syntax {
                    syntax
                        .tests
                        .retain(|row| evidence.eligible.contains(&row.anchor.file));
                    syntax.skipped.extend(evidence.withheld.iter().map(|file| {
                        gosanja_schema::contract::SkippedFile {
                            file: file.clone(),
                            code: "test-unknown".into(),
                        }
                    }));
                    syntax.skipped.sort();
                    syntax.skipped.dedup();
                }
                want_build.then(|| evidence.publish(&out)).transpose()?
            } else {
                None
            };
            let syntax = syntax
                .map(|layer| syntax::publish(layer, &out))
                .transpose()?;
            let calls = compiled_calls
                .map(|built| {
                    let mut files = RootFiles::new(&root);
                    calls::finish(&out, built, |modules| {
                        let rows = calls::rows::rows(modules, &index, &mut files, &crates);
                        (rows.evidence, rows.skipped)
                    })
                })
                .transpose()?;
            Layers {
                base: Ok(()),
                syntax,
                calls,
                build: build_result,
            }
        }
        Err(problem) => Layers {
            base: Err(problem),
            syntax: want_syntax.then(|| result::failed("base-failed")),
            calls: want_calls.then(|| Outcome::failed(&Failure::new("base-failed", ""), None)),
            build: want_build.then(|| result::failed("base-failed")),
        },
    };
    result::publish(&out, layers, tools)?;
    Ok(ExitCode::SUCCESS)
}
fn run() -> io::Result<ExitCode> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "describe" => {
            serde_json::to_writer(io::stdout().lock(), &result::manifest())?;
            Ok(ExitCode::SUCCESS)
        }
        [cmd, opts @ ..] if cmd == "index" => match index_options(opts) {
            Ok((root, out, request)) => index(&root, &out, request),
            Err(error) => {
                eprintln!("{error}");
                Ok(usage())
            }
        },
        _ => Ok(usage()),
    }
}
fn usage() -> ExitCode {
    eprintln!(
        "usage: gosanja-adapter-rust describe | index --root <dir> --out <dir> [--layers <names>]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    run().unwrap_or_else(|e| {
        eprintln!("{e}");
        ExitCode::FAILURE
    })
}
