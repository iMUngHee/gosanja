//! Process entry point for the Rust adapter.
mod base;
mod project;
mod result;

use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

fn paths(args: &[OsString]) -> Option<(PathBuf, PathBuf)> {
    let [first, a, second, b] = args else {
        return None;
    };
    if first == "--root" && second == "--out" {
        Some((a.into(), b.into()))
    } else if first == "--out" && second == "--root" {
        Some((b.into(), a.into()))
    } else {
        None
    }
}

fn index(root: &Path, out: &Path) -> Result<ExitCode, io::Error> {
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
    let syntax = match base::build(&root, &out, &mut tools)? {
        Ok(index) => Ok(gosanja_adapter_rust::syntax::extract(&root, &out, &index)?),
        Err(problem) => Err(problem),
    };
    result::publish(&out, syntax, tools)?;
    Ok(ExitCode::SUCCESS)
}
fn run() -> io::Result<ExitCode> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "describe" => {
            serde_json::to_writer(io::stdout().lock(), &result::manifest())?;
            Ok(ExitCode::SUCCESS)
        }
        [cmd, opts @ ..]
            if cmd == "index"
                && let Some((root, out)) = paths(opts) =>
        {
            index(&root, &out)
        }
        _ => {
            eprintln!("usage: gosanja-adapter-rust describe | index --root <dir> --out <dir>");
            Ok(ExitCode::from(2))
        }
    }
}
fn main() -> ExitCode {
    run().unwrap_or_else(|e| {
        eprintln!("{e}");
        ExitCode::FAILURE
    })
}
