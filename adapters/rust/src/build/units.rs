use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Output;

use serde_json::Value;

use super::dep_info;

#[derive(Debug)]
pub(super) struct Unit {
    pub package: String,
    pub target: Value,
    pub test: bool,
    pub opaque: bool,
    pub enables_test: bool,
    pub files: Vec<Listing>,
}

#[derive(Debug)]
pub(super) struct Listing {
    pub canonical: PathBuf,
    pub aliased: bool,
}

#[derive(Debug)]
pub(super) struct Run {
    pub complete: bool,
    pub units: Vec<Unit>,
    /// Why the invocations of earlier checks could not explain this one, when they could not.
    pub stale: Option<Stale>,
}

/// A check in a shared directory that the accumulated invocations do not explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stale {
    /// A fresh artifact that no earlier invocation produced.
    Unmatched,
    /// A fresh artifact that several earlier invocations could have produced.
    Ambiguous,
    /// A dep-info file that no invocation wrote.
    Unexplained,
}

impl Stale {
    pub(super) const fn reason(self) -> &'static str {
        match self {
            Self::Unmatched => "a fresh artifact matches no earlier compiler invocation",
            Self::Ambiguous => "a fresh artifact matches several earlier compiler invocations",
            Self::Unexplained => "a dep-info file has no compiler invocation",
        }
    }
}

/// One Cargo check and the directories it wrote.
pub(super) struct Check<'a> {
    pub output: &'a Output,
    pub compiler: &'a Path,
    pub host: &'a str,
    pub root: &'a Path,
    pub target: &'a Path,
    pub build: &'a Path,
}

/// Compiler invocations by the dep-info file each one writes. Checks that share a directory
/// accumulate them, because Cargo prints no invocation for a unit it finds fresh.
pub(super) type Known = BTreeMap<PathBuf, Invocation>;

#[derive(Debug, Default)]
pub(super) struct Invocation {
    name: String,
    dir: PathBuf,
    root: Option<PathBuf>,
    suffix: String,
    target: Option<String>,
    dep_info: bool,
    opaque: bool,
    enables_test: bool,
}

/// Cargo renders arguments using single quotes and apostrophe escapes. This parses its
/// diagnostic, without handing the string to a command interpreter.
fn words(line: &str) -> Option<Vec<String>> {
    let mut chars = line.chars();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(q), ch) if q == ch => quote = None,
            (Some(_), ch) => word.push(ch),
            (None, '\'' | '"') => {
                quote = Some(ch);
                started = true;
            }
            (None, '\\') => {
                word.push(chars.next()?);
                started = true;
            }
            (None, ' ') if started => {
                words.push(std::mem::take(&mut word));
                started = false;
            }
            (None, ' ') => (),
            (None, ch) => {
                word.push(ch);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        words.push(word);
    }
    Some(words)
}

fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}

fn cfg(arg: &str) -> bool {
    arg.split_once('=').map_or_else(
        || identifier(arg),
        |(key, value)| identifier(key) && syn::parse_str::<syn::LitStr>(value).is_ok(),
    )
}

fn codegen(unit: &mut Invocation, arg: &str) -> bool {
    if arg == "prefer-dynamic" {
        return true;
    }
    let Some((key, value)) = arg.split_once('=') else {
        return false;
    };
    if key == "extra-filename" {
        value.clone_into(&mut unit.suffix);
    }
    matches!(
        key,
        "metadata"
            | "extra-filename"
            | "embed-bitcode"
            | "debuginfo"
            | "opt-level"
            | "debug-assertions"
            | "overflow-checks"
            | "panic"
            | "incremental"
            | "codegen-units"
            | "strip"
            | "split-debuginfo"
            | "lto"
            | "force-frame-pointers"
            | "linker"
    )
}

fn option(unit: &mut Invocation, name: &str, arg: &str) -> bool {
    match name {
        "--crate-name" => {
            arg.clone_into(&mut unit.name);
            identifier(arg)
        }
        "--out-dir" => {
            unit.dir = arg.into();
            true
        }
        "--emit" => {
            unit.dep_info = arg.split(',').any(|name| name == "dep-info");
            arg.split(',')
                .all(|name| matches!(name, "dep-info" | "metadata" | "link"))
        }
        "--cfg" => {
            unit.enables_test |= arg == "test";
            cfg(arg)
        }
        "--target" => {
            unit.target = Some(arg.to_owned());
            true
        }
        "-C" | "--codegen" => codegen(unit, arg),
        "-L" | "-l" | "-A" | "-W" | "-D" | "-F" | "--allow" | "--warn" | "--deny" | "--forbid"
        | "--force-warn" | "--edition" | "--crate-type" | "--error-format" | "--json"
        | "--diagnostic-width" | "--check-cfg" | "--extern" | "--cap-lints" => true,
        _ => false,
    }
}

fn invocation(text: &str, compiler: &Path) -> Option<Invocation> {
    let (program, args) = text.split_once(' ')?;
    let mut unit = Invocation {
        opaque: Path::new(program) != compiler,
        ..Invocation::default()
    };
    let args = words(args)?;
    let mut args = args.iter().map(String::as_str);
    while let Some(arg) = args.next() {
        match arg {
            "--test" => unit.enables_test = true,
            arg if arg.starts_with("--") => {
                let (name, value) = arg
                    .split_once('=')
                    .unwrap_or_else(|| (arg, args.next().unwrap_or_default()));
                unit.opaque |= !option(&mut unit, name, value);
            }
            arg if arg.starts_with('-') => {
                let (name, value) = arg.split_at(arg.floor_char_boundary(2));
                let value = if value.is_empty() {
                    args.next().unwrap_or_default()
                } else {
                    value
                };
                unit.opaque |= !option(&mut unit, name, value);
            }
            arg if arg.starts_with('@') => unit.opaque = true,
            arg => {
                unit.opaque |= unit.root.replace(arg.into()).is_some();
            }
        }
    }
    unit.opaque |= unit.root.is_none();
    (unit.dep_info && !unit.name.is_empty() && unit.dir.is_absolute()).then_some(unit)
}

pub(super) fn lexical(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir if clean.file_name().is_some() => {
                clean.pop();
            }
            Component::CurDir | Component::ParentDir => (),
            other => clean.push(other.as_os_str()),
        }
    }
    clean
}

fn volatile(path: &Path) -> bool {
    path.starts_with("/dev") || path.starts_with("/proc")
}

impl Invocation {
    fn depfile(&self) -> PathBuf {
        self.dir.join(format!("{}{}.d", self.name, self.suffix))
    }
}

fn known_outputs(invocation: &Invocation, artifact: &Value) -> BTreeSet<PathBuf> {
    let is_build = artifact["target"]["kind"][0] == "custom-build";
    let stem = format!("{}{}", invocation.name, invocation.suffix);
    artifact["filenames"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .filter(|path| {
            path.parent() == Some(invocation.dir.as_path())
                && (is_build
                    || path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(|name| {
                            name == stem || name.strip_prefix("lib") == Some(&stem)
                        }))
        })
        .collect()
}

fn inventory(dir: &Path, listed: &BTreeSet<PathBuf>) -> bool {
    if !dir.exists() {
        return true;
    }
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.into_iter().all(|entry| {
            entry.is_ok_and(|e| match e.file_type() {
                Ok(kind) if kind.is_dir() => inventory(&e.path(), listed),
                Ok(_) => {
                    e.path().extension().is_none_or(|ext| ext != "d") || listed.contains(&e.path())
                }
                Err(_) => false,
            })
        })
    })
}

fn listed_paths(
    invocation: &Invocation,
    artifact: &Value,
    root: &Path,
    depfile: &Path,
    outputs: BTreeSet<PathBuf>,
) -> Option<Vec<PathBuf>> {
    let src = Path::new(artifact["target"]["src_path"].as_str()?);
    let arg = invocation.root.as_ref()?;
    let with_directory = !arg.is_absolute();
    if lexical(&root.join(arg)) != lexical(src) {
        return None;
    }
    let outputs: Vec<_> = std::iter::once(depfile.to_owned())
        .chain(outputs.into_iter().map(|output| {
            if artifact["target"]["kind"][0] == "custom-build" {
                // Cargo's artifact name can differ from rustc's output name.
                invocation.dir.join(format!(
                    "{}{}{}",
                    invocation.name,
                    invocation.suffix,
                    std::env::consts::EXE_SUFFIX
                ))
            } else {
                output
            }
        }))
        .collect();
    dep_info(&fs::read_to_string(depfile).ok()?, &outputs)?
        .into_iter()
        .map(|path| (path.is_absolute() || with_directory).then(|| root.join(path)))
        .collect()
}

/// Add the invocations a check printed to `known`, and count them by dep-info file.
fn record(log: &str, compiler: &Path, known: &mut Known) -> BTreeMap<PathBuf, usize> {
    let mut live = BTreeMap::new();
    for call in log
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Running `"))
        .filter_map(|line| line.strip_suffix('`'))
        .filter_map(|line| invocation(line, compiler))
    {
        let depfile = call.depfile();
        *live.entry(depfile.clone()).or_insert(0) += 1;
        known.insert(depfile, call);
    }
    live
}

/// An artifact with every invocation that could have produced it.
type Candidates<'a> = Vec<(&'a PathBuf, &'a Invocation, BTreeSet<PathBuf>)>;

/// The invocations that could have produced `artifact`: those of this check, and for an
/// artifact Cargo found fresh those of the earlier checks too.
fn candidates<'a>(
    artifact: &Value,
    fresh: bool,
    live: &BTreeMap<PathBuf, usize>,
    known: &'a Known,
) -> Candidates<'a> {
    known
        .iter()
        .filter(|(depfile, _)| fresh || live.contains_key(*depfile))
        .filter_map(|(depfile, call)| {
            let files = known_outputs(call, artifact);
            (!files.is_empty()).then_some((depfile, call, files))
        })
        .collect()
}

/// Pair the check's artifacts with compiler invocations. `known` holds the invocations of the
/// earlier checks in the same directories and gains this check's; pass an empty one for a
/// check in directories of its own.
pub(super) fn read(check: &Check<'_>, known: &mut Known) -> Run {
    let Check {
        output,
        compiler,
        host,
        root,
        target,
        build,
    } = *check;
    let reusing = !known.is_empty();
    let mut complete = output.status.success() && !cfg!(windows);
    let artifacts: Vec<Value> = output
        .stdout
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .filter(|msg| msg["reason"] == "compiler-artifact")
        .collect();
    let compiler_log = std::str::from_utf8(&output.stderr);
    complete &= compiler_log.is_ok();
    let live = record(compiler_log.unwrap_or_default(), compiler, known);
    let mut stale = None;
    let mut matched = BTreeMap::<&Path, usize>::new();
    let mut pairs = Vec::new();
    for artifact in &artifacts {
        let fresh = reusing && artifact["fresh"] == true;
        let mut candidates = candidates(artifact, fresh, &live, known);
        for (depfile, _, _) in &candidates {
            *matched.entry(depfile.as_path()).or_insert(0) += 1;
        }
        match (candidates.pop(), candidates.is_empty()) {
            (Some(candidate), true) => pairs.push((artifact, candidate)),
            (found, _) => {
                complete = false;
                if fresh {
                    stale.get_or_insert(if found.is_some() {
                        Stale::Ambiguous
                    } else {
                        Stale::Unmatched
                    });
                }
            }
        }
    }
    complete &= live
        .iter()
        .all(|(depfile, count)| *count == 1 && matched.get(depfile.as_path()) == Some(&1));
    let mut units = Vec::new();
    for (artifact, (depfile, call, files)) in pairs {
        if matched[depfile.as_path()] != 1 || live.get(depfile).is_some_and(|count| *count != 1) {
            complete = false;
            continue;
        }
        let Some(test) = artifact["profile"]["test"].as_bool() else {
            complete = false;
            continue;
        };
        let Some(package) = artifact["package_id"].as_str() else {
            complete = false;
            continue;
        };
        let listed = listed_paths(call, artifact, root, depfile, files);
        complete &= listed.is_some();
        let files = listed
            .into_iter()
            .flatten()
            .filter_map(|path| {
                let canonical = path.canonicalize().ok().filter(|canonical| {
                    !volatile(canonical) && !volatile(&path) && !volatile(&lexical(&path))
                });
                complete &= canonical.is_some();
                canonical.map(|canonical| Listing {
                    aliased: lexical(&path) != canonical,
                    canonical,
                })
            })
            .collect();
        complete &= (!call.opaque || test)
            && (call.opaque || call.target.as_ref().is_none_or(|value| value == host));
        units.push(Unit {
            package: package.to_owned(),
            target: artifact["target"].clone(),
            test,
            opaque: call.opaque,
            enables_test: call.enables_test,
            files,
        });
    }
    let depfiles: BTreeSet<_> = known.keys().cloned().collect();
    if !(inventory(target, &depfiles) && inventory(build, &depfiles)) {
        complete = false;
        if reusing {
            stale.get_or_insert(Stale::Unexplained);
        }
    }
    Run {
        complete,
        units,
        stale,
    }
}

#[cfg(all(test, unix))]
mod tests;
