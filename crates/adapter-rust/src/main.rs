//! Process entry point for the Rust adapter.
mod base;
mod project;
mod result;

use gosanja_adapter_rust::calls::{self, Outcome, build::Failure, debug::RootFiles};
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
    let mut built = None;
    let mut crates = BTreeSet::new();
    let indexed = base::build(&root, &out, &mut tools, &mut |project, meta| {
        if want_calls {
            crates = calls::member_crates(meta);
            built = Some(calls::compile(&out, meta, &mut |args| {
                project.run("cargo", args)
            }));
        }
    })?;
    let built = built.transpose()?;
    let layers = match indexed {
        Ok(index) => {
            let syntax = want_syntax
                .then(|| gosanja_adapter_rust::syntax::extract(&root, &out, &index))
                .transpose()?;
            let calls = built
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
            }
        }
        Err(problem) => Layers {
            base: Err(problem),
            syntax: want_syntax.then(|| result::failed("base-failed")),
            calls: want_calls.then(|| Outcome::failed(&Failure::new("base-failed", ""), None)),
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
