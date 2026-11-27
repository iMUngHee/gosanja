//! Build scripts that run again in a directory an earlier check used.
//!
//! A script that Cargo runs a second time finds the `OUT_DIR` its first run left, which a
//! check in directories of its own would not. Its second run is evidence only when it printed
//! the same directives and left the same files.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// What a script's first run in the shared directories left.
struct Seen {
    /// Cargo's saved copy of everything the script printed, and when Cargo wrote it.
    output: Option<(Vec<u8>, SystemTime)>,
    /// A digest of every file under `OUT_DIR`, absent when one cannot be read or is a link.
    files: Option<[u8; 32]>,
}

/// The build scripts the checks in one pair of directories have run, by `OUT_DIR`.
#[derive(Default)]
pub(super) struct Scripts {
    first: BTreeMap<PathBuf, Seen>,
}

impl Scripts {
    /// Record the scripts a check reports.
    ///
    /// # Errors
    /// The reason, when a script ran again and its result differs from its first run's or
    /// cannot be compared with it.
    pub(super) fn verify(&mut self, stdout: &[u8]) -> Result<(), &'static str> {
        for message in stdout
            .split(|byte| *byte == b'\n')
            .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
            .filter(|message| message["reason"] == "build-script-executed")
        {
            let Some(out_dir) = message["out_dir"].as_str().map(PathBuf::from) else {
                continue;
            };
            let output = saved_output(&out_dir);
            match self.first.entry(out_dir) {
                Entry::Vacant(slot) => {
                    let files = digest(slot.key());
                    slot.insert(Seen { output, files });
                }
                Entry::Occupied(mut slot) => {
                    if output.is_some() && output == slot.get().output {
                        continue;
                    }
                    rerun(slot.key(), slot.get(), output.as_ref())?;
                    slot.get_mut().output = output;
                }
            }
        }
        Ok(())
    }
}

/// Compare a script's second run with its first.
fn rerun(
    out_dir: &Path,
    seen: &Seen,
    output: Option<&(Vec<u8>, SystemTime)>,
) -> Result<(), &'static str> {
    let (Some((printed, _)), Some((before, _))) = (output, &seen.output) else {
        return Err("a build script ran again and its output cannot be read");
    };
    if printed != before {
        return Err("a build script ran again and printed different directives");
    }
    if seen.files.is_none() || digest(out_dir) != seen.files {
        return Err("a build script ran again and its OUT_DIR differs or cannot be compared");
    }
    Ok(())
}

/// Cargo saves what a script printed beside its `OUT_DIR`: in `output` up to 1.99, in
/// `run/stdout` from 1.100. With both there, which one Cargo wrote is unknown, so neither is
/// read.
fn saved_output(out_dir: &Path) -> Option<(Vec<u8>, SystemTime)> {
    let beside = out_dir.parent()?;
    let mut saved = [beside.join("output"), beside.join("run/stdout")]
        .into_iter()
        .filter(|path| path.symlink_metadata().is_ok());
    let (path, None) = (saved.next()?, saved.next()) else {
        return None;
    };
    let written = fs::metadata(&path).ok()?.modified().ok()?;
    Some((fs::read(path).ok()?, written))
}

fn digest(dir: &Path) -> Option<[u8; 32]> {
    let mut hasher = Sha256::new();
    visit(dir, Path::new(""), &mut hasher)?;
    Some(hasher.finalize().into())
}

fn visit(dir: &Path, relative: &Path, hasher: &mut Sha256) -> Option<()> {
    let mut entries = fs::read_dir(dir)
        .ok()?
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let relative = relative.join(entry.file_name());
        let kind = entry.file_type().ok()?;
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update([0, u8::from(kind.is_dir())]);
        if kind.is_dir() {
            visit(&entry.path(), &relative, hasher)?;
        } else if kind.is_file() {
            let mut file = fs::File::open(entry.path()).ok()?;
            hasher.update(file.metadata().ok()?.len().to_le_bytes());
            let mut chunk = [0; 1 << 13];
            loop {
                match file.read(&mut chunk).ok()? {
                    0 => break,
                    read => hasher.update(&chunk[..read]),
                }
            }
        } else {
            return None;
        }
    }
    Some(())
}

#[cfg(all(test, unix))]
mod tests;
