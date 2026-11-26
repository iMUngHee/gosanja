use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::result::Problem;

fn unresolvable(e: &io::Error) -> Problem {
    Problem::new("tool-home-unresolvable", e.to_string())
}

/// Resolve links even when Cargo will create the final directory later.
fn destination(path: &Path) -> Result<PathBuf, Problem> {
    for prefix in path.ancestors() {
        match fs::symlink_metadata(prefix) {
            Ok(_) => {
                let existing = prefix.canonicalize().map_err(|e| unresolvable(&e))?;
                if prefix == path {
                    return Ok(existing);
                }
                let tail = &path.as_os_str().as_encoded_bytes()
                    [prefix.as_os_str().as_encoded_bytes().len()..];
                if tail
                    .split(|b| std::path::is_separator(char::from(*b)))
                    .any(|s| s == b"." || s == b"..")
                {
                    return Err(Problem::new(
                        "tool-home-inside-root",
                        "unresolved dot segment in tool home",
                    ));
                }
                let suffix = path
                    .strip_prefix(prefix)
                    .map_err(|e| Problem::new("tool-home-unresolvable", e.to_string()))?;
                return Ok(existing.join(suffix));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(unresolvable(&e)),
        }
    }
    Err(Problem::new(
        "tool-home-unresolvable",
        "tool home has no existing prefix",
    ))
}
pub struct Project<'a> {
    pub root: &'a Path,
    vars: BTreeMap<OsString, OsString>,
}
impl<'a> Project<'a> {
    pub fn new(root: &'a Path, out: &Path) -> Result<Self, Problem> {
        let mut vars = BTreeMap::new();
        let mut allow = vec![
            "PATH",
            "HOME",
            "RUSTUP_HOME",
            "RUSTUP_TOOLCHAIN",
            "RUSTUP_DIST_SERVER",
            "RUSTUP_UPDATE_ROOT",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
        ];
        if cfg!(windows) {
            allow.extend(["SystemRoot", "USERPROFILE"]);
        }
        for key in allow {
            if let Some(v) = env::var_os(key) {
                vars.insert(key.into(), v);
            }
        }
        let mut home_path = |key: &str, default: &str| -> Result<PathBuf, Problem> {
            let path = env::var_os(key)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| env::home_dir().map(|p| p.join(default)))
                .ok_or_else(|| Problem::new("tool-home-unresolvable", "missing home directory"))?;
            let path = destination(&root.join(path))?;
            if path.starts_with(root) {
                return Err(Problem::new(
                    "tool-home-inside-root",
                    "tool home inside root",
                ));
            }
            vars.insert(key.into(), path.as_os_str().to_owned());
            Ok(path)
        };
        let cargo = home_path("CARGO_HOME", ".cargo")?;
        home_path("RUSTUP_HOME", ".rustup")?;
        for subdir in ["registry", "git"] {
            if destination(&cargo.join(subdir))?.starts_with(root) {
                return Err(Problem::new(
                    "tool-home-inside-root",
                    "Cargo cache inside root",
                ));
            }
        }
        vars.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
        let work = out.join("work");
        for (key, dir) in [
            ("CARGO_TARGET_DIR", "target"),
            ("CARGO_BUILD_BUILD_DIR", "build"),
            ("TMPDIR", "tmp"),
        ] {
            vars.insert(key.into(), work.join(dir).into_os_string());
        }
        if cfg!(windows) {
            for key in ["TEMP", "TMP"] {
                vars.insert(key.into(), work.join("tmp").into_os_string());
            }
        }
        Ok(Self { root, vars })
    }

    pub fn run(&self, tool: &str, args: &[impl AsRef<OsStr>]) -> io::Result<Output> {
        self.run_with(tool, args, &[])
    }

    pub fn run_with(
        &self,
        tool: &str,
        args: &[impl AsRef<OsStr>],
        vars: &[(&str, &OsStr)],
    ) -> io::Result<Output> {
        Command::new(tool)
            .args(args)
            .current_dir(self.root)
            .env_clear()
            .envs(&self.vars)
            .envs(vars.iter().copied())
            .output()
    }
}
