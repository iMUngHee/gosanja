//! The `gosanja` command line.
//!
//! Exit codes: 0 success, 1 validation failure, 2 usage or compatibility refusal, 3 adapter or
//! input failure.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use gosanja_core::publish;
use gosanja_core::validate;
use gosanja_core::write::{self, FsSource, SnapshotFiles, SourceReader};
use gosanja_core::{adapter, graph, sidecar};

const USAGE: &str = "usage:
  gosanja index --scip <file> --root <dir> --out <dir>
  gosanja index --adapter <program> --root <dir> --out <dir>
  gosanja adapter describe <program>
  gosanja validate <snapshot> [--root <dir>]";

const VALIDATION_FAILED: u8 = 1;
const REFUSED: u8 = 2;
const INPUT_FAILED: u8 = 3;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure { code, message }) => {
            eprintln!("gosanja: {message}");
            ExitCode::from(code)
        }
    }
}

/// A run that ended without success: the exit code and what to tell the user.
#[derive(Debug)]
struct Failure {
    code: u8,
    message: String,
}

fn fail(code: u8, message: impl Into<String>) -> Failure {
    Failure {
        code,
        message: message.into(),
    }
}

fn usage(problem: &str) -> Failure {
    fail(REFUSED, format!("{problem}\n{USAGE}"))
}

fn run(args: &[OsString]) -> Result<(), Failure> {
    let (command, rest) = args
        .split_first()
        .ok_or_else(|| usage("no command given"))?;
    match command.to_str() {
        Some("index") => index(&Options::parse(
            rest,
            &["--scip", "--adapter", "--root", "--out"],
        )?),
        Some("adapter") => {
            let [subcommand, program] = rest else {
                return Err(usage("expected adapter describe <program>"));
            };
            if subcommand != "describe" {
                return Err(usage("expected adapter describe <program>"));
            }
            let (manifest, _) = adapter::describe(Command::new(program))
                .map_err(|e| fail(e.exit_code(), e.to_string()))?;
            let mut stdout = io::stdout().lock();
            serde_json::to_writer(&mut stdout, &manifest)
                .map_err(|e| fail(INPUT_FAILED, e.to_string()))?;
            writeln!(stdout).map_err(|e| fail(INPUT_FAILED, e.to_string()))
        }
        Some("validate") => validate(&Options::parse(rest, &["--root"])?),
        _ => Err(usage(&format!(
            "unknown command {}",
            command.to_string_lossy()
        ))),
    }
}

/// `--name value` pairs and positional arguments.
#[derive(Debug, Default)]
struct Options {
    named: Vec<(&'static str, PathBuf)>,
    positional: Vec<PathBuf>,
}

impl Options {
    fn parse(args: &[OsString], known: &[&'static str]) -> Result<Self, Failure> {
        let mut out = Self::default();
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let text = arg.to_string_lossy();
            if !text.starts_with("--") {
                out.positional.push(PathBuf::from(arg));
                continue;
            }
            let name = known
                .iter()
                .find(|k| **k == text)
                .ok_or_else(|| usage(&format!("unknown option {text}")))?;
            if out.get(name).is_some() {
                return Err(usage(&format!("{name} given twice")));
            }
            let value = args
                .next()
                .ok_or_else(|| usage(&format!("{name} needs a value")))?;
            out.named.push((name, PathBuf::from(value)));
        }
        Ok(out)
    }

    fn get(&self, name: &str) -> Option<&Path> {
        self.named
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_path())
    }

    fn require(&self, name: &str) -> Result<&Path, Failure> {
        self.get(name)
            .ok_or_else(|| usage(&format!("{name} is required")))
    }
}

fn index(options: &Options) -> Result<(), Failure> {
    if !options.positional.is_empty() {
        return Err(usage("index takes no positional argument"));
    }
    let root = options.require("--root")?;
    let out = options.require("--out")?;
    if let Some(program) = options.get("--adapter") {
        if options.get("--scip").is_some() {
            return Err(usage("give exactly one of --scip and --adapter"));
        }
        return publish::publish(root, out, |stage| {
            let sources = FsSource::new(root)?;
            let cache = adapter_cache(root)?;
            let output =
                adapter::run(|| Command::new(program), root, &cache).map_err(io::Error::other)?;
            let snapshot =
                sidecar::build(output, &sources, &project_name(root)).map_err(io::Error::other)?;
            write::write_snapshot(stage, &snapshot)
        })
        .map_err(|e| {
            let code = match &e {
                publish::PublishError::Io(error) => error
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<adapter::AdapterError>())
                    .map_or(INPUT_FAILED, adapter::AdapterError::exit_code),
                publish::PublishError::Refused(_) => REFUSED,
            };
            fail(code, e.to_string())
        });
    }
    let scip = options.require("--scip")?;
    let sources = FsSource::new(root).map_err(|e| {
        fail(
            INPUT_FAILED,
            format!("cannot read the root {}: {e}", root.display()),
        )
    })?;
    let mut file = File::open(scip)
        .map_err(|e| fail(INPUT_FAILED, format!("cannot open {}: {e}", scip.display())))?;
    let snapshot = graph::build(&mut file, &sources, &project_name(root))
        .map_err(|e| fail(INPUT_FAILED, format!("{}: {e}", scip.display())))?;
    publish::publish(root, out, |dir| write::write_snapshot(dir, &snapshot))
        .map_err(|e| fail(e.exit_code(), e.to_string()))
}

/// Resolves the cache before creating it so even a missing cache cannot modify the source.
fn adapter_cache(root: &Path) -> io::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let cache = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let cache = std::env::home_dir().map(|home| home.join("Library/Caches"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".cache")));
    let cache = cache
        .filter(|p| {
            p.is_absolute() && !p.components().any(|c| c == std::path::Component::ParentDir)
        })
        .ok_or_else(|| io::Error::other("cannot resolve user cache directory"))?;
    let cache = cache.join("gosanja");
    let root = root.canonicalize()?;
    for prefix in cache.ancestors() {
        match prefix.canonicalize() {
            Ok(path) if path.starts_with(&root) => {
                return Err(io::Error::other("user cache is inside the indexed root"));
            }
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    fs::create_dir_all(&cache)?;
    Ok(cache)
}

/// The display name of a project: the last component of its root.
fn project_name(root: &Path) -> String {
    root.canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

fn validate(options: &Options) -> Result<(), Failure> {
    let [snapshot] = options.positional.as_slice() else {
        return Err(usage("validate takes one snapshot directory"));
    };
    // The version is decided from the manifest alone, before any table is opened and before
    // the root is resolved; unreadable tables become diagnostics, reported after that.
    let mut files = SnapshotFiles::read_manifest(snapshot);
    validate::check_version(&files)
        .map_err(|refusal| fail(REFUSED, format!("{}: {refusal}", snapshot.display())))?;
    files.read_tables(snapshot);
    let sources = match options.get("--root") {
        Some(root) => Some(FsSource::new(root).map_err(|e| {
            fail(
                INPUT_FAILED,
                format!("cannot read the root {}: {e}", root.display()),
            )
        })?),
        None => None,
    };
    let diagnostics = validate::validate(&files, sources.as_ref().map(|s| s as &dyn SourceReader))
        .map_err(|refusal| fail(REFUSED, format!("{}: {refusal}", snapshot.display())))?;
    if diagnostics.is_empty() {
        return Ok(());
    }
    for diagnostic in &diagnostics {
        eprintln!("{}: {diagnostic}", snapshot.display());
    }
    Err(fail(
        VALIDATION_FAILED,
        format!("{} problem(s) in {}", diagnostics.len(), snapshot.display()),
    ))
}
