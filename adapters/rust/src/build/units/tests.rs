use super::*;
use serde_json::json;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use tempfile::TempDir;

struct Replay {
    _work: TempDir,
    root: PathBuf,
    source: PathBuf,
    out: PathBuf,
    compiler: PathBuf,
}

impl Replay {
    fn new() -> Self {
        let work = tempfile::tempdir().unwrap();
        let root = work.path().canonicalize().unwrap();
        let source = root.join("src.rs");
        let out = root.join("build");
        let compiler = root.join("bin/rustc");
        fs::write(&source, "fn item() {}").unwrap();
        fs::create_dir(&out).unwrap();
        Self {
            _work: work,
            root,
            source,
            out,
            compiler,
        }
    }

    fn artifact(&self, kind: &str, filename: &str, test: bool) -> Value {
        json!({
            "reason": "compiler-artifact",
            "package_id": "test",
            "target": { "name": "demo", "kind": [kind], "src_path": &self.source },
            "profile": { "test": test },
            "filenames": [self.out.join(filename)],
        })
    }

    fn log(&self, source: &str, extra: &str) -> String {
        format!(
            "     Running `{} --crate-name demo --out-dir {} --emit dep-info,metadata {extra} '{source}'`\n",
            self.compiler.display(),
            self.out.display()
        )
    }

    fn depfile(&self, path: &str, suffix: &str, output: &str) {
        let depfile = self.out.join(format!("demo{suffix}.d"));
        let output = self.out.join(output);
        fs::write(
            &depfile,
            format!(
                "{}: {path}\n\n{}: {path}\n\n{path}:\n",
                depfile.display(),
                output.display()
            ),
        )
        .unwrap();
    }

    fn run(&self, artifacts: &[Value], log: String) -> Run {
        let mut stdout = Vec::new();
        for artifact in artifacts {
            serde_json::to_writer(&mut stdout, artifact).unwrap();
            stdout.push(b'\n');
        }
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout,
            stderr: log.into_bytes(),
        };
        read(
            &output,
            &self.compiler,
            "test-host",
            &self.root,
            &self.root.join("target"),
            &self.out,
        )
    }
}

#[test]
fn build_units_pair_invocations_and_artifacts() {
    let replay = Replay::new();
    replay.depfile("src.rs", "-hash", "libdemo-hash.rmeta");
    let log = replay.log("src.rs", "-Cextra-filename=-hash --cfg 'feature=\"x\"'");
    let artifact = replay.artifact("lib", "libdemo-hash.rmeta", false);
    let run = replay.run(&[artifact], log);
    assert!(run.complete);
    let [unit] = run.units.as_slice() else {
        panic!("{run:?}")
    };
    assert!(!unit.test);
    assert!(!unit.opaque);
    assert_eq!(unit.files[0].canonical, replay.source);
}

#[test]
fn build_units_link_library_options_are_known() {
    let replay = Replay::new();
    replay.depfile("src.rs", "-hash", "libdemo-hash.rmeta");
    for extra in [
        "-l static=demo",
        "-l static:+whole-archive=demo",
        "-lm",
        "-L native=/lib -l m",
    ] {
        let log = replay.log("src.rs", &format!("-Cextra-filename=-hash {extra}"));
        let artifact = replay.artifact("lib", "libdemo-hash.rmeta", false);
        let run = replay.run(&[artifact], log);
        assert!(run.complete, "{extra}: {run:?}");
        assert!(!run.units[0].opaque, "{extra}");
    }
}

#[test]
fn build_units_working_directory_verification() {
    let replay = Replay::new();
    let artifact = replay.artifact("lib", "libdemo-hash.rmeta", true);
    let absolute = replay.source.to_str().unwrap();
    replay.depfile(absolute, "-hash", "libdemo-hash.rmeta");
    let run = replay.run(
        std::slice::from_ref(&artifact),
        replay.log(absolute, "-C extra-filename=-hash --test"),
    );
    assert!(run.complete);
    assert_eq!(run.units[0].files[0].canonical, replay.source);
    replay.depfile("src.rs", "-hash", "libdemo-hash.rmeta");
    let run = replay.run(
        std::slice::from_ref(&artifact),
        replay.log(absolute, "-C extra-filename=-hash --test"),
    );
    assert!(!run.complete);
    assert!(run.units[0].files.is_empty());
    let run = replay.run(
        &[artifact],
        replay.log("../src.rs", "-C extra-filename=-hash --test"),
    );
    assert!(!run.complete);
    assert!(run.units[0].files.is_empty());
}

#[test]
fn build_units_unpaired_or_missing_depfile_make_coverage_incomplete() {
    let replay = Replay::new();
    let log = || replay.log("src.rs", "-Cextra-filename=-hash");
    let artifact = replay.artifact("lib", "libdemo-hash.rmeta", false);
    let run = replay.run(std::slice::from_ref(&artifact), log());
    assert!(!run.complete, "missing .d");
    assert!(run.units[0].files.is_empty());
    replay.depfile("src.rs", "-hash", "libdemo-hash.rmeta");
    let run = replay.run(std::slice::from_ref(&artifact), String::new());
    assert!(!run.complete, "missing invocation");
    assert!(run.units.is_empty());
    let run = replay.run(&[], log());
    assert!(!run.complete, "missing artifact");
    assert!(run.units.is_empty());
    let run = replay.run(std::slice::from_ref(&artifact), log().repeat(2));
    assert!(!run.complete, "two invocations sharing a depfile");
    assert!(run.units.is_empty());
    let run = replay.run(&[artifact.clone(), artifact], log());
    assert!(!run.complete, "two artifacts sharing an invocation");
    assert!(run.units.is_empty());
}

#[test]
fn build_units_orphan_depfile_makes_coverage_incomplete() {
    let replay = Replay::new();
    let log = || replay.log("src.rs", "-Cextra-filename=-hash");
    let artifact = replay.artifact("lib", "libdemo-hash.rmeta", false);
    replay.depfile("src.rs", "-hash", "libdemo-hash.rmeta");
    assert!(replay.run(std::slice::from_ref(&artifact), log()).complete);
    fs::write(replay.out.join("orphan.d"), "").unwrap();
    let run = replay.run(&[artifact], log());
    assert!(!run.complete);
    assert_eq!(run.units[0].files[0].canonical, replay.source);
}

#[test]
fn build_units_opaque_production_makes_the_run_incomplete() {
    let replay = Replay::new();
    let log = || replay.log("src.rs", "-Cextra-filename=-hash @flags");
    replay.depfile("src.rs", "-hash", "libdemo-hash.rmeta");
    for (test, complete) in [(true, true), (false, false)] {
        let artifact = replay.artifact("lib", "libdemo-hash.rmeta", test);
        let run = replay.run(&[artifact], log());
        assert_eq!(run.complete, complete, "{run:?}");
        assert!(run.units[0].opaque);
    }
}

#[test]
fn build_units_first_word_is_the_set_compiler() {
    let replay = Replay::new();
    replay.depfile("src.rs", "-hash", "libdemo-hash.rmeta");
    let artifact = replay.artifact("lib", "libdemo-hash.rmeta", false);
    let log = replay.log("src.rs", "-Cextra-filename=-hash");
    assert!(
        replay
            .run(std::slice::from_ref(&artifact), log.clone())
            .complete
    );
    let log = log.replace(replay.compiler.to_str().unwrap(), "/different/rustc");
    let run = replay.run(&[artifact], log);
    assert!(!run.complete);
    assert!(run.units[0].opaque);
}

#[test]
fn build_units_non_host_target_makes_the_run_incomplete() {
    let replay = Replay::new();
    let artifact = replay.artifact("lib", "libdemo-hash.rmeta", false);
    replay.depfile("src.rs", "-hash", "libdemo-hash.rmeta");
    for (arg, complete) in [
        ("--target=test-host", true),
        ("--target test-host.json", false),
    ] {
        let log = replay.log("src.rs", &format!("-Cextra-filename=-hash {arg}"));
        let run = replay.run(std::slice::from_ref(&artifact), log);
        assert_eq!(run.complete, complete);
        assert!(!run.units[0].opaque);
    }
}

#[test]
fn build_units_pinned_build_script_pairs() {
    let replay = Replay::new();
    for suffix in ["", "-hash"] {
        replay.depfile("src.rs", suffix, &format!("demo{suffix}"));
        let artifact = replay.artifact("custom-build", "build-script-build", false);
        let log = replay.log("src.rs", &format!("-Cextra-filename={suffix}"));
        let run = replay.run(&[artifact], log);
        assert!(run.complete, "{run:?}");
        assert_eq!(run.units[0].files[0].canonical, replay.source);
        fs::remove_file(replay.out.join(format!("demo{suffix}.d"))).unwrap();
    }
}
