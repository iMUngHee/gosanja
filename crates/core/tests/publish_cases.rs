//! The publication guard with several protected inputs and a format marker, as a diff uses it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gosanja_core::publish;
use tempfile::{TempDir, tempdir};

const SNAPSHOT: &str = "gosanja-snapshot";
const DIFF: &str = "gosanja-diff";

/// Two input directories, `old` and `new`, each holding a snapshot manifest and a data file.
struct Inputs {
    tmp: TempDir,
    old: PathBuf,
    new: PathBuf,
}

impl Inputs {
    fn new() -> Self {
        let tmp = tempdir().unwrap();
        let old = tmp.path().join("old");
        let new = tmp.path().join("new");
        for (dir, data) in [(&old, "old data"), (&new, "new data")] {
            fs::create_dir(dir).unwrap();
            output(dir, SNAPSHOT, data).unwrap();
        }
        Self { tmp, old, new }
    }

    fn guarded(&self, out: &Path) -> Result<(), publish::PublishError> {
        publish::publish_guarded(&[&self.old, &self.new], DIFF, out, |stage| {
            output(stage, DIFF, "written")
        })
    }
}

/// Fills `dir` with a manifest of `format` and one data file.
fn output(dir: &Path, format: &str, data: &str) -> std::io::Result<()> {
    fs::write(
        dir.join("manifest.json"),
        format!(r#"{{"format":"{format}"}}"#),
    )?;
    fs::write(dir.join("data"), data)
}

/// Every path under `dir` with the bytes of each file, links recorded by their target.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        let meta = fs::symlink_metadata(&path).unwrap();
        let name = path.strip_prefix(dir).unwrap().to_path_buf();
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&path).unwrap();
            found.insert(name, target.as_os_str().as_encoded_bytes().to_vec());
        } else if meta.is_dir() {
            found.insert(name, Vec::new());
            for entry in fs::read_dir(&path).unwrap() {
                pending.push(entry.unwrap().path());
            }
        } else {
            found.insert(name, fs::read(&path).unwrap());
        }
    }
    found
}

#[test]
fn an_output_overlapping_either_input_is_refused_and_nothing_changes() {
    let inputs = Inputs::new();
    let before = tree(inputs.tmp.path());
    for out in [
        inputs.old.clone(),
        inputs.new.clone(),
        inputs.old.join("inside"),
        inputs.new.join("inside"),
        inputs.tmp.path().to_path_buf(),
    ] {
        let err = inputs.guarded(&out).unwrap_err();
        assert_eq!(err.exit_code(), 2, "{}: {err}", out.display());
    }
    assert_eq!(tree(inputs.tmp.path()), before);
}

#[cfg(unix)]
#[test]
fn an_overlap_through_a_link_is_refused_and_nothing_changes() {
    let inputs = Inputs::new();
    let links = inputs.tmp.path().join("links");
    fs::create_dir(&links).unwrap();
    std::os::unix::fs::symlink(&inputs.old, links.join("to-old")).unwrap();
    std::os::unix::fs::symlink(&inputs.new, links.join("to-new")).unwrap();
    std::os::unix::fs::symlink(inputs.tmp.path(), links.join("to-both")).unwrap();
    let before = tree(inputs.tmp.path());
    for out in [
        links.join("to-old"),
        links.join("to-new"),
        links.join("to-old").join("inside"),
        links.join("to-new").join("inside"),
        links.join("to-both").join("links").join("to-both"),
        links.join("to-both").join("old").join(".."),
    ] {
        let err = inputs.guarded(&out).unwrap_err();
        assert_eq!(err.exit_code(), 2, "{}: {err}", out.display());
    }
    assert_eq!(tree(inputs.tmp.path()), before);
}

#[test]
fn a_diff_never_replaces_a_snapshot_and_a_snapshot_never_replaces_a_diff() {
    let inputs = Inputs::new();
    let out = inputs.tmp.path().join("out");
    fs::create_dir(&out).unwrap();
    output(&out, SNAPSHOT, "a snapshot").unwrap();
    assert_eq!(inputs.guarded(&out).unwrap_err().exit_code(), 2);
    assert_eq!(fs::read(out.join("data")).unwrap(), b"a snapshot");

    output(&out, DIFF, "a diff").unwrap();
    let err = publish::publish(&inputs.old, &out, |_| {
        panic!("a diff is not replaced by a snapshot")
    })
    .unwrap_err();
    assert_eq!(err.exit_code(), 2);
    assert_eq!(fs::read(out.join("data")).unwrap(), b"a diff");
}

#[test]
fn an_earlier_diff_is_replaced_and_an_empty_or_missing_output_is_used() {
    let inputs = Inputs::new();
    let out = inputs.tmp.path().join("out");
    inputs.guarded(&out).unwrap();
    assert_eq!(fs::read(out.join("data")).unwrap(), b"written");

    fs::write(out.join("data"), "earlier").unwrap();
    fs::write(out.join("stale"), "left over").unwrap();
    inputs.guarded(&out).unwrap();
    assert_eq!(fs::read(out.join("data")).unwrap(), b"written");
    assert!(
        !out.join("stale").exists(),
        "the earlier diff is replaced whole"
    );

    let empty = inputs.tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    inputs.guarded(&empty).unwrap();
    assert_eq!(fs::read(empty.join("data")).unwrap(), b"written");
}

#[test]
fn any_other_non_empty_output_is_refused_and_left_alone() {
    let inputs = Inputs::new();
    for manifest in [None, Some("{}"), Some("not json")] {
        let out = inputs.tmp.path().join("other");
        let _ = fs::remove_dir_all(&out);
        fs::create_dir(&out).unwrap();
        fs::write(out.join("data"), "kept").unwrap();
        if let Some(text) = manifest {
            fs::write(out.join("manifest.json"), text).unwrap();
        }
        let before = tree(&out);
        assert_eq!(inputs.guarded(&out).unwrap_err().exit_code(), 2);
        assert_eq!(tree(&out), before);
    }
}
