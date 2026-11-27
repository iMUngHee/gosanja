use super::*;
use serde_json::json;

struct Script {
    _work: tempfile::TempDir,
    out_dir: PathBuf,
    runs: std::cell::Cell<u64>,
}

impl Script {
    fn new() -> Self {
        let work = tempfile::tempdir().unwrap();
        let out_dir = work.path().join("pkg-0123/out");
        fs::create_dir_all(&out_dir).unwrap();
        let script = Self {
            _work: work,
            out_dir,
            runs: std::cell::Cell::new(0),
        };
        script.print("cargo::rustc-cfg=a\n");
        script
    }

    /// The script runs: Cargo saves what it printed, at a time no earlier run had.
    fn print(&self, directives: &str) {
        let path = self.out_dir.parent().unwrap().join("output");
        fs::write(&path, directives).unwrap();
        self.runs.set(self.runs.get() + 1);
        let written = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(self.runs.get());
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(written)
            .unwrap();
    }

    fn message(&self) -> Vec<u8> {
        json!({"reason": "build-script-executed", "out_dir": self.out_dir})
            .to_string()
            .into_bytes()
    }
}

#[test]
fn a_script_that_did_not_run_again_passes_whatever_its_out_dir_holds() {
    let script = Script::new();
    std::os::unix::fs::symlink("/dev/null", script.out_dir.join("link")).unwrap();
    let mut scripts = Scripts::default();
    scripts.verify(&script.message()).unwrap();
    scripts.verify(&script.message()).unwrap();
}

#[test]
fn a_rerun_with_the_same_directives_and_files_passes() {
    let script = Script::new();
    fs::create_dir(script.out_dir.join("gen")).unwrap();
    fs::write(script.out_dir.join("gen/a.rs"), "pub fn a() {}").unwrap();
    let mut scripts = Scripts::default();
    scripts.verify(&script.message()).unwrap();
    for _ in 0..2 {
        script.print("cargo::rustc-cfg=a\n");
        fs::write(script.out_dir.join("gen/a.rs"), "pub fn a() {}").unwrap();
        scripts.verify(&script.message()).unwrap();
    }
}

#[test]
fn a_rerun_that_prints_other_directives_fails() {
    let script = Script::new();
    let mut scripts = Scripts::default();
    scripts.verify(&script.message()).unwrap();
    script.print("cargo::rustc-cfg=a\ncargo::rustc-check-cfg=cfg(a)\n");
    assert_eq!(
        scripts.verify(&script.message()),
        Err("a build script ran again and printed different directives")
    );
}

#[test]
fn a_rerun_that_changes_renames_or_adds_a_file_fails() {
    for change in [
        |dir: &Path| fs::write(dir.join("a.rs"), "pub fn b() {}").unwrap(),
        |dir: &Path| fs::rename(dir.join("a.rs"), dir.join("b.rs")).unwrap(),
        |dir: &Path| fs::write(dir.join("b.rs"), "").unwrap(),
        |dir: &Path| fs::create_dir(dir.join("empty")).unwrap(),
    ] {
        let script = Script::new();
        fs::write(script.out_dir.join("a.rs"), "pub fn a() {}").unwrap();
        let mut scripts = Scripts::default();
        scripts.verify(&script.message()).unwrap();
        script.print("cargo::rustc-cfg=a\n");
        change(&script.out_dir);
        assert_eq!(
            scripts.verify(&script.message()),
            Err("a build script ran again and its OUT_DIR differs or cannot be compared")
        );
    }
}

#[test]
fn a_rerun_with_a_link_in_either_run_fails() {
    for before in [true, false] {
        let script = Script::new();
        fs::write(script.out_dir.join("a.rs"), "pub fn a() {}").unwrap();
        let link = || {
            std::os::unix::fs::symlink(script.out_dir.join("a.rs"), script.out_dir.join("l"))
                .unwrap();
        };
        if before {
            link();
        }
        let mut scripts = Scripts::default();
        scripts.verify(&script.message()).unwrap();
        script.print("cargo::rustc-cfg=a\n");
        if !before {
            link();
        }
        assert_eq!(
            scripts.verify(&script.message()),
            Err("a build script ran again and its OUT_DIR differs or cannot be compared")
        );
    }
}

#[test]
fn a_rerun_whose_saved_output_is_gone_fails() {
    let script = Script::new();
    let mut scripts = Scripts::default();
    scripts.verify(&script.message()).unwrap();
    fs::remove_file(script.out_dir.parent().unwrap().join("output")).unwrap();
    assert_eq!(
        scripts.verify(&script.message()),
        Err("a build script ran again and its output cannot be read")
    );
}

#[test]
fn a_script_first_seen_in_a_later_check_is_a_first_run() {
    let early = Script::new();
    let late = Script::new();
    let mut scripts = Scripts::default();
    scripts.verify(&early.message()).unwrap();
    let both = [early.message(), late.message()].join(&b'\n');
    scripts.verify(&both).unwrap();
    late.print("cargo::rustc-cfg=b\n");
    assert_eq!(
        scripts.verify(&both),
        Err("a build script ran again and printed different directives")
    );
}

/// Cargo 1.100 keeps what a script printed in `run/stdout` beside `OUT_DIR`.
#[test]
fn a_rerun_is_compared_where_cargo_saves_the_output_in_a_run_directory() {
    let work = tempfile::tempdir().unwrap();
    let out_dir = work.path().join("pkg/0123/out");
    let saved = work.path().join("pkg/0123/run/stdout");
    fs::create_dir_all(&out_dir).unwrap();
    fs::create_dir_all(saved.parent().unwrap()).unwrap();
    let print = |directives: &str, at: u64| {
        fs::write(&saved, directives).unwrap();
        let written = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(at);
        fs::File::options()
            .write(true)
            .open(&saved)
            .unwrap()
            .set_modified(written)
            .unwrap();
    };
    let message = json!({"reason": "build-script-executed", "out_dir": out_dir})
        .to_string()
        .into_bytes();
    let mut scripts = Scripts::default();
    print("cargo::rustc-cfg=a\n", 1);
    scripts.verify(&message).unwrap();
    scripts.verify(&message).unwrap();
    print("cargo::rustc-cfg=a\n", 2);
    scripts.verify(&message).unwrap();
    print("cargo::rustc-cfg=b\n", 3);
    assert_eq!(
        scripts.verify(&message),
        Err("a build script ran again and printed different directives")
    );
}

#[test]
fn a_rerun_with_a_saved_output_in_both_places_fails() {
    let script = Script::new();
    let run = script.out_dir.parent().unwrap().join("run");
    fs::create_dir(&run).unwrap();
    fs::write(run.join("stdout"), "cargo::rustc-cfg=a\n").unwrap();
    let mut scripts = Scripts::default();
    scripts.verify(&script.message()).unwrap();
    assert_eq!(
        scripts.verify(&script.message()),
        Err("a build script ran again and its output cannot be read")
    );
}
