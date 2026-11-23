//! The `gosanja` command line.
//!
//! Exit codes: 0 success, 1 validation failure, 2 usage or compatibility refusal, 3 adapter or
//! input failure.

use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gosanja_core::publish;
use gosanja_core::validate;
use gosanja_core::write::{self, FsSource, SnapshotFiles, SourceReader};
use gosanja_core::{adapter, graph, sidecar};

mod discover;

use discover::Search;

const USAGE: &str = "usage:
  gosanja index --root <dir> --out <dir>
  gosanja index --adapter <name or path> --root <dir> --out <dir>
  gosanja index --scip <file> --root <dir> --out <dir>
  gosanja adapter list
  gosanja adapter describe <name or path>
  gosanja validate <snapshot> [--root <dir>]
An adapter argument that is absolute or holds a path separator is a program; any other is the
name of an installed adapter. To run a program found on PATH, give its path.";

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
        Some("adapter") => match rest {
            [subcommand] if subcommand == "list" => list(),
            [subcommand, which] if subcommand == "describe" => {
                let described = named(which)?;
                let mut stdout = io::stdout().lock();
                serde_json::to_writer(&mut stdout, described.manifest())
                    .map_err(|e| fail(INPUT_FAILED, e.to_string()))?;
                writeln!(stdout).map_err(|e| fail(INPUT_FAILED, e.to_string()))
            }
            _ => Err(usage(
                "expected adapter list or adapter describe <name or path>",
            )),
        },
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

fn discovery_failure(error: &discover::DiscoveryError) -> Failure {
    fail(INPUT_FAILED, error.to_string())
}

/// The adapter an `--adapter` or `adapter describe` argument names, described once: a program
/// path starts that program, a name starts the installed adapter of that name.
fn named(which: &OsStr) -> Result<adapter::Described, Failure> {
    if discover::is_path_argument(which, cfg!(windows)) {
        let path = Path::new(which);
        let invocation = std::path::absolute(path)
            .and_then(|program| adapter::Invocation::new(program, Vec::new()))
            .map_err(|e| fail(INPUT_FAILED, format!("{}: {e}", path.display())))?;
        return adapter::describe(invocation).map_err(|e| fail(e.exit_code(), e.to_string()));
    }
    let search = Search::from_environment();
    let Some(name) = which.to_str().filter(|n| discover::is_adapter_name(n)) else {
        return Err(fail(
            REFUSED,
            format!(
                "{} is neither a path nor an adapter name (lower-case letters, digits and -); \
                 searched {}",
                discover::one_line(&which.to_string_lossy()),
                search.describe_directories()
            ),
        ));
    };
    let installed = search.installed().map_err(|e| discovery_failure(&e))?;
    let Some(found) = installed.into_iter().find(|i| i.name == name) else {
        return Err(fail(
            REFUSED,
            format!(
                "no adapter named {name} is installed; searched {}",
                search.describe_directories()
            ),
        ));
    };
    found.describe().map_err(|reason| {
        fail(
            reason.exit_code(),
            format!("{}: {reason}", discover::path_line(&found.path)),
        )
    })
}

/// The adapter whose marker file is in `root`, among the usable installed ones.
fn automatic(root: &Path) -> Result<adapter::Described, Failure> {
    const ALTERNATIVES: &str =
        "name one with --adapter <name or path>, or index a SCIP file with --scip <file>";
    let search = Search::from_environment();
    let installed = search.installed().map_err(|e| discovery_failure(&e))?;
    let mut usable = Vec::new();
    for adapter in installed {
        match adapter.describe() {
            Ok(described) => usable.push((adapter, described)),
            Err(reason) => eprintln!(
                "gosanja: the adapter {} at {} is unusable: {reason}",
                adapter.name,
                discover::path_line(&adapter.path)
            ),
        }
    }
    // Every marker of every usable adapter is inspected before anything is decided.
    let mut candidates = Vec::new();
    for (index, (_, described)) in usable.iter().enumerate() {
        if discover::is_candidate(root, described.manifest()).map_err(|e| discovery_failure(&e))? {
            candidates.push(index);
        }
    }
    let root_line = discover::path_line(root);
    match candidates.as_slice() {
        [chosen] => Ok(usable.swap_remove(*chosen).1),
        [] if usable.is_empty() => Err(fail(
            REFUSED,
            format!(
                "no usable adapter is installed; searched {}\n{ALTERNATIVES}",
                search.describe_directories()
            ),
        )),
        [] => {
            let mut message = format!("no installed adapter has a marker file in {root_line}\n");
            for (adapter, described) in &usable {
                let markers: Vec<_> = described
                    .manifest()
                    .markers
                    .iter()
                    .flatten()
                    .map(|m| discover::one_line(m))
                    .collect();
                if markers.is_empty() {
                    let _ = writeln!(message, "  {}: no markers", adapter.name);
                } else {
                    let _ = writeln!(
                        message,
                        "  {}: markers {}",
                        adapter.name,
                        markers.join(", ")
                    );
                }
            }
            message.push_str(ALTERNATIVES);
            Err(fail(REFUSED, message))
        }
        several => {
            let names: Vec<_> = several.iter().map(|i| usable[*i].0.name.as_str()).collect();
            Err(fail(
                REFUSED,
                format!(
                    "several installed adapters have a marker file in {root_line}: {}; \
                     name one with --adapter <name>",
                    names.join(", ")
                ),
            ))
        }
    }
}

/// `adapter list`: one block per installed adapter, in name order.
fn list() -> Result<(), Failure> {
    let search = Search::from_environment();
    let installed = search.installed().map_err(|e| discovery_failure(&e))?;
    let mut text = String::new();
    if installed.is_empty() {
        text.push_str("no adapters installed\n");
        for directory in search.directories() {
            let _ = writeln!(text, "  searched {}", discover::path_line(directory));
        }
    }
    let path = std::env::var_os("PATH");
    for adapter in &installed {
        let outcome = adapter.describe();
        text.push_str(&discover::list_block(adapter, &outcome, path.as_deref()));
    }
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|e| fail(INPUT_FAILED, e.to_string()))
}

/// The root of an adapter run must be a directory whose entries can be looked up.
fn check_root(root: &Path) -> Result<(), Failure> {
    let shown = discover::path_line(root);
    match fs::metadata(root) {
        Ok(meta) if meta.is_dir() => fs::metadata(root.join("."))
            .map(drop)
            .map_err(|e| fail(INPUT_FAILED, format!("cannot search the root {shown}: {e}"))),
        Ok(_) => Err(fail(
            INPUT_FAILED,
            format!("the root {shown} is not a directory"),
        )),
        Err(e) => Err(fail(
            INPUT_FAILED,
            format!("cannot use the root {shown}: {e}"),
        )),
    }
}

fn index(options: &Options) -> Result<(), Failure> {
    if !options.positional.is_empty() {
        return Err(usage("index takes no positional argument"));
    }
    let root = options.require("--root")?;
    let out = options.require("--out")?;
    let adapter_argument = options.get("--adapter");
    if let Some(scip) = options.get("--scip") {
        if adapter_argument.is_some() {
            return Err(usage("give at most one of --scip and --adapter"));
        }
        return index_scip(scip, root, out);
    }
    check_root(root)?;
    // The output is judged first, as for every source; then nothing is described before the
    // cache is known to lie outside the root and the output.
    // A refusal decided inside the publication step keeps its own code and message.
    let mut decided = None;
    let mut decide = |failure: Failure| {
        decided = Some(failure);
        io::Error::other("decided")
    };
    let published = publish::publish(root, out, |stage| {
        let cache =
            adapter_cache(root, out).map_err(|e| decide(fail(INPUT_FAILED, e.to_string())))?;
        let described = match adapter_argument {
            Some(which) => named(which.as_os_str()),
            None => automatic(root),
        }
        .map_err(&mut decide)?;
        let sources = FsSource::new(root)?;
        fs::create_dir_all(&cache)?;
        let output = described.run(root, &cache).map_err(io::Error::other)?;
        let snapshot =
            sidecar::build(output, &sources, &project_name(root)).map_err(io::Error::other)?;
        write::write_snapshot(stage, &snapshot)
    });
    published.map_err(|e| {
        if let Some(failure) = decided {
            return failure;
        }
        let code = match &e {
            publish::PublishError::Io(error) => error
                .get_ref()
                .and_then(|cause| cause.downcast_ref::<adapter::AdapterError>())
                .map_or(INPUT_FAILED, adapter::AdapterError::exit_code),
            publish::PublishError::Refused(_) => REFUSED,
        };
        fail(code, e.to_string())
    })
}

fn index_scip(scip: &Path, root: &Path, out: &Path) -> Result<(), Failure> {
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

/// `path` with links resolved as far as it exists, and the rest appended.
fn resolved(path: &Path) -> io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let mut missing = Vec::new();
    let mut existing = path.as_path();
    loop {
        match existing.canonicalize() {
            Ok(base) => return Ok(missing.iter().rev().fold(base, |acc, part| acc.join(part))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let (Some(part), Some(parent)) = (existing.file_name(), existing.parent()) else {
                    return Err(e);
                };
                missing.push(part);
                existing = parent;
            }
            Err(e) => return Err(e),
        }
    }
}

/// The adapter cache directory, resolved but not created: it must lie neither inside the
/// indexed root nor at or beneath the output, so that a failed run leaves both as they were.
fn adapter_cache(root: &Path, out: &Path) -> io::Result<PathBuf> {
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
    let where_it_is = resolved(&cache)?;
    if where_it_is.starts_with(root.canonicalize()?) {
        return Err(io::Error::other("user cache is inside the indexed root"));
    }
    if where_it_is.starts_with(resolved(out)?) {
        return Err(io::Error::other(format!(
            "the adapter cache {} lies in the output {}",
            discover::path_line(&cache),
            discover::path_line(out)
        )));
    }
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
