//! Guards the output path and replaces a completed snapshot or diff.
use gosanja_schema::snapshot::FORMAT;
use serde_json::Value;
use std::{fs, io, path::Path};

/// An unsafe output choice or a filesystem failure during publication.
#[derive(Debug)]
pub enum PublishError {
    Refused(String),
    Io(io::Error),
}
impl PublishError {
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Refused(_) => 2,
            Self::Io(_) => 3,
        }
    }
}
impl From<io::Error> for PublishError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(reason) => f.write_str(reason),
            Self::Io(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for PublishError {}

/// Whether `out` may be used: it is missing, an empty directory, or an earlier output whose
/// manifest has `format` equal to `marker`. `Ok(true)` means there is something to replace.
fn replaceable(out: &Path, marker: &str) -> Result<bool, PublishError> {
    let meta = match fs::symlink_metadata(out) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        other => other?,
    };
    if meta.is_dir() {
        if fs::read_dir(out)?.next().transpose()?.is_none() {
            return Ok(true);
        }
        let manifest = out.join("manifest.json");
        if fs::symlink_metadata(&manifest).is_ok_and(|m| m.is_file())
            && fs::File::open(manifest)
                .ok()
                .and_then(|f| serde_json::from_reader::<_, Value>(f).ok())
                .is_some_and(|v| v["format"] == marker)
        {
            return Ok(true);
        }
    }
    Err(PublishError::Refused(format!(
        "{} is not an empty directory or an earlier {marker}",
        out.display()
    )))
}

/// Writes a snapshot of `root` to a private sibling directory and installs it after the writer
/// succeeds.
///
/// # Errors
/// Refuses overlapping paths and unrelated output. Returns filesystem and writer errors.
pub fn publish(
    root: &Path,
    out: &Path,
    write: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<(), PublishError> {
    publish_guarded(&[root], FORMAT, out, write)
}

/// Writes to a private sibling directory of `out` and installs it after the writer succeeds.
///
/// `out` may not be, contain or lie inside any of `protected`, after links are resolved, and an
/// existing non-empty `out` is replaced only when its manifest's `format` is `marker`.
///
/// # Errors
/// Refuses overlapping paths and unrelated output. Returns filesystem and writer errors.
pub fn publish_guarded(
    protected: &[&Path],
    marker: &str,
    out: &Path,
    write: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<(), PublishError> {
    publish_with(protected, marker, out, write, |a, b| fs::rename(a, b))
}

fn publish_with(
    protected: &[&Path],
    marker: &str,
    out: &Path,
    write: impl FnOnce(&Path) -> io::Result<()>,
    install: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<(), PublishError> {
    let protected = protected
        .iter()
        .map(|dir| dir.canonicalize())
        .collect::<io::Result<Vec<_>>>()?;
    let absolute = std::path::absolute(out)?;
    let name = absolute
        .file_name()
        .ok_or_else(|| PublishError::Refused("output must name a directory".into()))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| PublishError::Refused("output needs a parent".into()))?;
    let parent = parent.canonicalize().map_err(|err| match err.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => PublishError::Refused(format!(
            "output parent {} must be an existing directory",
            parent.display()
        )),
        _ => err.into(),
    })?;
    if !parent.is_dir() {
        return Err(PublishError::Refused(
            "output parent must be a directory".into(),
        ));
    }
    let out = parent.join(name);
    if let Some(dir) = protected
        .iter()
        .find(|dir| out.starts_with(dir) || dir.starts_with(&out))
    {
        return Err(PublishError::Refused(format!(
            "output overlaps {}",
            dir.display()
        )));
    }
    replaceable(&out, marker)?;
    let stage = tempfile::tempdir_in(&parent)?;
    write(stage.path())?;
    // Serialize competing publishers across the check and both renames.
    let lock = directory_handle(&parent)?;
    lock.lock()?;
    if !replaceable(&out, marker)? {
        install(stage.path(), &out)?;
        return Ok(());
    }
    let backup = tempfile::tempdir_in(&parent)?;
    let old = backup.path().join("previous");
    fs::rename(&out, &old)?;
    if let Err(failure) = install(stage.path(), &out) {
        if let Err(restore) = fs::rename(&old, &out) {
            let preserved = backup.keep();
            return Err(io::Error::other(format!(
                "install failed ({failure}); rollback failed ({restore}); previous output at {}",
                preserved.join("previous").display()
            ))
            .into());
        }
        return Err(failure.into());
    }
    Ok(())
}

fn directory_handle(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        // Windows requires FILE_FLAG_BACKUP_SEMANTICS to open a directory.
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0200_0000);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_rename_failure_restores_previous_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("src");
        let out = temp.path().join("out");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&out).unwrap();
        let bytes = br#"{"format":"gosanja-snapshot","other":42}"#;
        fs::write(out.join("manifest.json"), bytes).unwrap();
        fs::write(out.join("data"), [0, 255]).unwrap();
        let err = publish_with(
            &[&root],
            FORMAT,
            &out,
            |stage| fs::write(stage.join("new"), "new"),
            |_, destination| {
                assert!(!destination.exists());
                Err(io::Error::other("injected install failure"))
            },
        )
        .unwrap_err();
        assert_eq!(err.exit_code(), 3);
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), bytes);
        assert_eq!(fs::read(out.join("data")).unwrap(), [0, 255]);
        assert_eq!(fs::read_dir(out).unwrap().count(), 2);
    }

    #[test]
    fn final_rename_failure_restores_an_earlier_diff() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        let out = temp.path().join("out");
        for dir in [&old, &new, &out] {
            fs::create_dir(dir).unwrap();
        }
        let bytes = br#"{"format":"gosanja-diff","schema":"0.1"}"#;
        fs::write(out.join("manifest.json"), bytes).unwrap();
        fs::write(out.join("entities.jsonl"), [0, 255, b'\n']).unwrap();
        let err = publish_with(
            &[&old, &new],
            "gosanja-diff",
            &out,
            |stage| fs::write(stage.join("manifest.json"), "replacement"),
            |_, destination| {
                assert!(!destination.exists(), "the earlier diff was moved aside");
                Err(io::Error::other("injected install failure"))
            },
        )
        .unwrap_err();
        assert_eq!(err.exit_code(), 3);
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), bytes);
        assert_eq!(
            fs::read(out.join("entities.jsonl")).unwrap(),
            [0, 255, b'\n']
        );
        assert_eq!(fs::read_dir(&out).unwrap().count(), 2);
        assert_eq!(
            fs::read_dir(temp.path()).unwrap().count(),
            3,
            "no staging or backup directory is left beside the output"
        );
    }
}
