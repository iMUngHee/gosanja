//! Installed adapters: what counts as one, where they are looked for, which one a project root
//! selects, and how their values are shown.
//!
//! An adapter executable is a file named `gosanja-adapter-<name>` (with `.exe` on Windows) in a
//! search directory. Discovery reads every search directory before any adapter is started, so a
//! file-system error means that nothing was run.

use std::ffi::{OsStr, OsString};
use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use gosanja_core::adapter::{self, AdapterError, Described, Invocation};
use gosanja_schema::contract::AdapterManifest;

/// The file-name prefix of an adapter executable.
pub const PREFIX: &str = "gosanja-adapter-";

/// Whether `name` is an adapter name: lower-case letters, digits and `-`.
pub fn is_adapter_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether `name` is one portable file name: letters, digits, `.`, `_` and `-`, and not `.`
/// or `..`. It cannot hold a separator, a drive prefix, a control character or a NUL.
pub fn is_portable_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The adapter name a file name gives under the rule of the platform: on Windows the file name
/// ends in `.exe`, which is not part of the name.
pub fn name_from_file(file_name: &OsStr, windows: bool) -> Option<&str> {
    let text = file_name.to_str()?;
    let text = if windows {
        text.strip_suffix(".exe")?
    } else {
        text
    };
    text.strip_prefix(PREFIX).filter(|n| is_adapter_name(n))
}

/// Whether an argument of `--adapter` or `adapter describe` is a program path rather than an
/// adapter name: it is absolute or holds a separator of the platform (`/`; on Windows also `\`).
pub fn is_path_argument(argument: &OsStr, windows: bool) -> bool {
    let text = argument.to_string_lossy();
    Path::new(argument).is_absolute() || text.contains('/') || (windows && text.contains('\\'))
}

/// `value` on one line: a control character, line breaks included, and the Unicode line and
/// paragraph separators as `\u{…}` with their code in hexadecimal, and a backslash as `\\`.
pub fn one_line(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

/// A path on one line, after lossy conversion to UTF-8.
pub fn path_line(path: &Path) -> String {
    one_line(&path.to_string_lossy())
}

/// A file-system error that leaves discovery undecided. Nothing has been started when one is
/// returned.
#[derive(Debug)]
pub enum DiscoveryError {
    /// A search directory exists and cannot be read.
    Directory(PathBuf, io::Error),
    /// A file with an adapter's name cannot be inspected.
    Candidate(PathBuf, io::Error),
    /// A marker file cannot be inspected.
    Marker(PathBuf, io::Error),
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Directory(path, error) => write!(
                f,
                "cannot read the adapter search directory {}: {error}",
                path_line(path)
            ),
            Self::Candidate(path, error) => {
                write!(f, "cannot inspect the adapter {}: {error}", path_line(path))
            }
            Self::Marker(path, error) => write!(
                f,
                "cannot inspect the marker file {}: {error}",
                path_line(path)
            ),
        }
    }
}

/// The directories adapters are looked for in, in priority order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    directories: Vec<PathBuf>,
}

impl Search {
    /// Each entry of `GOSANJA_ADAPTER_PATH`, the directory of the running executable, each entry
    /// of `PATH`.
    pub fn from_environment() -> Self {
        let executable = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        Self::new(
            std::env::var_os("GOSANJA_ADAPTER_PATH").as_deref(),
            executable,
            std::env::var_os("PATH").as_deref(),
        )
    }

    /// The search order from the two variables and the executable's directory. Empty and
    /// relative entries are ignored.
    pub fn new(
        adapter_path: Option<&OsStr>,
        executable_directory: Option<PathBuf>,
        path: Option<&OsStr>,
    ) -> Self {
        let split = |value: Option<&OsStr>| -> Vec<PathBuf> {
            value
                .map(|v| std::env::split_paths(v).collect())
                .unwrap_or_default()
        };
        let directories = split(adapter_path)
            .into_iter()
            .chain(executable_directory)
            .chain(split(path))
            .filter(|d| d.is_absolute())
            .collect();
        Self { directories }
    }

    /// The directories, in priority order.
    pub fn directories(&self) -> &[PathBuf] {
        &self.directories
    }

    /// The directories on one line, for messages.
    pub fn describe_directories(&self) -> String {
        if self.directories.is_empty() {
            return "no directory".to_owned();
        }
        self.directories
            .iter()
            .map(|d| path_line(d))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Every installed adapter, in name order: for each name the first file found. A directory
    /// that is missing or not a directory is ignored; one that cannot be read, and a file with an
    /// adapter's name that cannot be inspected, are errors.
    ///
    /// # Errors
    /// The first directory or file that could not be read or inspected.
    pub fn installed(&self) -> Result<Vec<Installed>, DiscoveryError> {
        let mut found: Vec<Installed> = Vec::new();
        for directory in &self.directories {
            match fs::metadata(directory) {
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => continue,
                Err(e) if is_absent(&e) => continue,
                Err(e) => return Err(DiscoveryError::Directory(directory.clone(), e)),
            }
            let entries = fs::read_dir(directory)
                .map_err(|e| DiscoveryError::Directory(directory.clone(), e))?;
            let mut here = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| DiscoveryError::Directory(directory.clone(), e))?;
                let file_name = entry.file_name();
                let Some(name) = name_from_file(&file_name, cfg!(windows)) else {
                    continue;
                };
                if found.iter().any(|i| i.name == name) {
                    continue;
                }
                let path = entry.path();
                match fs::metadata(&path) {
                    Ok(meta) if is_executable_file(&meta) => here.push(Installed {
                        name: name.to_owned(),
                        path,
                    }),
                    Ok(_) => {}
                    Err(e) if is_absent(&e) => {}
                    Err(e) => return Err(DiscoveryError::Candidate(path, e)),
                }
            }
            found.extend(here);
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }
}

/// An error that says a path does not exist: a component is missing or is not a directory, as
/// for a link whose target runs through a regular file.
fn is_absent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

#[cfg(unix)]
fn is_executable_file(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable_file(meta: &fs::Metadata) -> bool {
    meta.is_file()
}

/// An adapter executable found in a search directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The name its file gives.
    pub name: String,
    /// The file, as found in its search directory.
    pub path: PathBuf,
}

/// Why an installed adapter cannot be used.
#[derive(Debug)]
pub enum Unusable {
    /// `describe` failed, printed an invalid manifest or declared a refused contract.
    Describe(AdapterError),
    /// The manifest names another adapter than the file.
    Name(String),
}

impl Unusable {
    /// 2 for a refused contract or a mismatched name, 3 for a failed `describe`.
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Describe(error) => error.exit_code(),
            Self::Name(_) => 2,
        }
    }
}

impl fmt::Display for Unusable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Describe(error) => f.write_str(&one_line(&error.to_string())),
            Self::Name(name) => write!(f, "its manifest names the adapter {}", one_line(name)),
        }
    }
}

impl Installed {
    /// Runs `describe` once; the adapter is usable when that succeeds and its manifest names it.
    ///
    /// # Errors
    /// Why the adapter is unusable.
    pub fn describe(&self) -> Result<Described, Unusable> {
        let invocation = Invocation::new(self.path.clone(), Vec::<OsString>::new())
            .map_err(|e| Unusable::Describe(e.into()))?;
        let described = adapter::describe(invocation).map_err(Unusable::Describe)?;
        if described.manifest().name != self.name {
            return Err(Unusable::Name(described.manifest().name.clone()));
        }
        Ok(described)
    }
}

/// Whether one marker is present in `root`: `<root>/<marker>`, with links followed, is a regular
/// file.
///
/// # Errors
/// The marker cannot be inspected for a reason other than not existing.
pub fn marker_present(root: &Path, marker: &str) -> Result<bool, DiscoveryError> {
    let path = root.join(marker);
    match fs::metadata(&path) {
        Ok(meta) => Ok(meta.is_file()),
        Err(e) if is_absent(&e) => Ok(false),
        Err(e) => Err(DiscoveryError::Marker(path, e)),
    }
}

/// Whether an adapter's manifest makes it a candidate for `root`. Every marker is inspected, so
/// the order of the markers never changes the outcome.
///
/// # Errors
/// A marker that cannot be inspected.
pub fn is_candidate(root: &Path, manifest: &AdapterManifest) -> Result<bool, DiscoveryError> {
    let mut present = false;
    for marker in manifest.markers.iter().flatten() {
        present |= marker_present(root, marker)?;
    }
    Ok(present)
}

/// What `adapter list` says about a tool an adapter requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tool {
    /// A regular, executable file of that name in an absolute entry of `PATH`.
    Found(PathBuf),
    /// None was found.
    Missing,
    /// The name is not one portable file name, so it was not looked up.
    NotChecked,
}

/// Looks a required tool up in the absolute entries of `path`.
pub fn find_tool(tool: &str, path: Option<&OsStr>) -> Tool {
    if !is_portable_file_name(tool) {
        return Tool::NotChecked;
    }
    let file = if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_owned()
    };
    path.map(std::env::split_paths)
        .into_iter()
        .flatten()
        .filter(|d| d.is_absolute())
        .map(|d| d.join(&file))
        .find(|p| fs::metadata(p).is_ok_and(|m| is_executable_file(&m)))
        .map_or(Tool::Missing, Tool::Found)
}

/// The block `adapter list` prints for one installed adapter.
pub fn list_block(
    installed: &Installed,
    outcome: &Result<Described, Unusable>,
    path: Option<&OsStr>,
) -> String {
    let manifest = match outcome {
        Ok(described) => described.manifest(),
        Err(reason) => {
            return format!(
                "{} path={} unusable: {reason}\n",
                installed.name,
                path_line(&installed.path)
            );
        }
    };
    let join = |values: &mut dyn Iterator<Item = String>| {
        values.map(|v| one_line(&v)).collect::<Vec<_>>().join(",")
    };
    let layers = join(&mut manifest.layers.iter().map(|l| {
        serde_json::to_value(l)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }));
    let mut block = format!(
        "{} {} contract={} languages={} layers={} markers={} path={}\n",
        installed.name,
        one_line(&manifest.version),
        one_line(&manifest.contract),
        join(&mut manifest.languages.iter().cloned()),
        layers,
        join(&mut manifest.markers.iter().flatten().cloned()),
        path_line(&installed.path),
    );
    for requirement in manifest.requires.iter().flatten() {
        let version = requirement
            .version
            .as_deref()
            .map(|v| format!(" {}", one_line(v)))
            .unwrap_or_default();
        let status = match find_tool(&requirement.tool, path) {
            Tool::Found(found) => format!("found {}", path_line(&found)),
            Tool::Missing => "missing".to_owned(),
            Tool::NotChecked => "not checked".to_owned(),
        };
        let _ = writeln!(
            block,
            "  requires {}{version}: {status}",
            one_line(&requirement.tool)
        );
    }
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_adapter_name_is_lower_case_letters_digits_and_dashes() {
        for good in ["rust", "go", "type-script", "x1", "-"] {
            assert!(is_adapter_name(good), "{good}");
        }
        for bad in ["", "Rust", "rust.sh", "rust adapter", "rust_x", "ä", "a/b"] {
            assert!(!is_adapter_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_portable_file_name_holds_no_separator_or_drive() {
        for good in [
            "Cargo.toml",
            "go.mod",
            "package.json",
            ".tool-versions",
            "a_b-c",
        ] {
            assert!(is_portable_file_name(good), "{good}");
        }
        for bad in [
            "", ".", "..", "a/b", "a\\b", "C:x", "a\0b", "a b", "a\nb", "ä",
        ] {
            assert!(!is_portable_file_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_file_name_rule_differs_on_windows_only_by_the_extension() {
        fn name(file: &str, windows: bool) -> Option<&str> {
            name_from_file(OsStr::new(file), windows)
        }
        assert_eq!(name("gosanja-adapter-rust", false), Some("rust"));
        assert_eq!(name("gosanja-adapter-rust.exe", false), None);
        assert_eq!(name("gosanja-adapter-rust.sh", false), None);
        assert_eq!(name("gosanja-adapter-Rust", false), None);
        assert_eq!(name("gosanja-adapter-", false), None);
        assert_eq!(name("adapter-rust", false), None);
        assert_eq!(name("gosanja-adapter-rust.exe", true), Some("rust"));
        assert_eq!(name("gosanja-adapter-rust", true), None);
        assert_eq!(name("gosanja-adapter-rust.sh.exe", true), None);
        assert_eq!(name("gosanja-adapter-.exe", true), None);
    }

    #[test]
    fn an_argument_is_a_path_when_absolute_or_holding_a_separator() {
        let path = |arg: &str, windows| is_path_argument(OsStr::new(arg), windows);
        assert!(!path("rust", false));
        assert!(!path("rust", true));
        assert!(path("./adapter", false));
        assert!(path("bin/adapter", true));
        assert!(path("/usr/bin/adapter", false));
        assert!(!path("dir\\adapter", false));
        assert!(path("dir\\adapter", true));
        assert!(!path("adapter.sh", false));
    }

    #[test]
    fn values_are_shown_on_one_line() {
        assert_eq!(one_line("plain 1.0"), "plain 1.0");
        assert_eq!(one_line("a\nb"), "a\\u{a}b");
        assert_eq!(one_line("a\r\n\tb\u{7f}"), "a\\u{d}\\u{a}\\u{9}b\\u{7f}");
        assert_eq!(one_line("a\\b"), "a\\\\b");
        assert_eq!(one_line("\\u{a}"), "\\\\u{a}");
        assert_eq!(one_line("é\u{85}"), "é\\u{85}");
        assert_eq!(one_line("a\u{2028}b\u{2029}c"), "a\\u{2028}b\\u{2029}c");
    }

    #[test]
    fn empty_and_relative_entries_are_not_searched() {
        let joined = |dirs: &[&str]| std::env::join_paths(dirs).expect("joinable");
        let adapter_path = joined(&["/a", "", "rel", "/b c"]);
        let path = joined(&["/p", "./q"]);
        let search = Search::new(
            Some(&adapter_path),
            Some(PathBuf::from("/exe")),
            Some(&path),
        );
        assert_eq!(
            search.directories(),
            [
                PathBuf::from("/a"),
                PathBuf::from("/b c"),
                PathBuf::from("/exe"),
                PathBuf::from("/p")
            ]
        );
        assert_eq!(
            Search::new(None, None, None).directories(),
            [] as [PathBuf; 0]
        );
    }
}
