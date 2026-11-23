//! Process-level checks for the adapter contract and snapshot publication.
mod support;
use gosanja_core::{adapter, publish};
use gosanja_schema::snapshot::{FailedDetail, GapKind, LayerState};
use std::{fs, io, path::Path};
use tempfile::tempdir;

fn run(
    mode: &str,
    root: &Path,
    parent: &Path,
) -> Result<adapter::AdapterOutput, adapter::AdapterError> {
    support::fake(mode)?.run(root, parent)
}

#[test]
fn fatal_outcomes_preserve_previous_output() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("src");
    fs::create_dir(&root).unwrap();
    let out = tmp.path().join("snapshot");
    fs::create_dir(&out).unwrap();
    let marker = br#"{"format":"gosanja-snapshot"}"#;
    fs::write(out.join("manifest.json"), marker).unwrap();
    fs::write(out.join("old"), "old contents").unwrap();
    for (mode, code) in [
        ("incompatible", 2),
        ("nonzero", 3),
        ("no-result", 3),
        ("not-json", 3),
        ("invalid-result", 3),
        ("failed-base", 3),
        ("absent-base", 3),
        ("no-index", 3),
        ("path-build", 3),
        ("duplicate-builds", 3),
    ] {
        publish::publish(&root, &out, |_| {
            let err = run(mode, &root, tmp.path()).unwrap_err();
            assert_eq!(err.exit_code(), code, "{mode}");
            assert!(
                !err.to_string().is_empty(),
                "CLI needs a diagnostic for {mode}"
            );
            Err(io::Error::other(err))
        })
        .unwrap_err();
        assert_eq!(fs::read(out.join("old")).unwrap(), b"old contents");
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), marker);
    }
}

#[test]
fn optional_outcomes_and_forward_fields() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("src");
    fs::create_dir(&root).unwrap();
    let good = run("good", &root, tmp.path()).unwrap();
    let extra = run("extra", &root, tmp.path()).unwrap();
    assert_eq!(good.manifest, extra.manifest);
    assert_eq!(good.layers, extra.layers);
    assert_eq!(good.syntax, extra.syntax);
    assert_eq!(good.calls, extra.calls);
    assert_eq!(good.builds, extra.builds);
    assert_eq!(good.gaps, extra.gaps);
    assert_eq!(good.syntax.len(), 1);
    assert_eq!(good.layers.syntax, LayerState::Present);
    for (mode, detail) in [
        ("not-offered", None),
        ("reported-a", Some(FailedDetail::ReportedFailed)),
        ("no-status", Some(FailedDetail::NoStatus)),
        ("no-file", Some(FailedDetail::FileMissing)),
        ("bad-row", Some(FailedDetail::RowInvalid)),
        ("no-lf", Some(FailedDetail::RowInvalid)),
        ("crlf", Some(FailedDetail::RowInvalid)),
    ] {
        let output = run(mode, &root, tmp.path()).unwrap();
        assert_eq!(output.syntax.len(), 0);
        assert_eq!(output.layers.calls, LayerState::Present);
        let gap = &output.gaps[0];
        assert_eq!(output.gaps.len(), 1);
        assert_eq!(gap.detail, detail);
        assert_eq!(gap.src, "adapter:fake/syntax");
        assert_eq!(
            gap.gap,
            if detail.is_some() {
                GapKind::FailedLayer
            } else {
                GapKind::MissingLayer
            }
        );
        assert_eq!(
            output.layers.syntax,
            if detail.is_some() {
                LayerState::Failed
            } else {
                LayerState::Absent
            }
        );
    }
    let a = run("reported-a", &root, tmp.path()).unwrap();
    let b = run("reported-b", &root, tmp.path()).unwrap();
    assert_eq!(a.gaps[0].code.as_deref(), Some("parser-failed"));
    assert_eq!(
        serde_json::to_vec(&a.gaps).unwrap(),
        serde_json::to_vec(&b.gaps).unwrap()
    );
}

#[test]
fn publication_checks_the_source_and_destination_before_writing() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("src");
    fs::create_dir(&root).unwrap();
    for dest in [&root, &root.join("nested"), &tmp.path().to_owned()] {
        assert_eq!(
            publish::publish(&root, dest, |_| panic!("overlap"))
                .unwrap_err()
                .exit_code(),
            2
        );
    }
    let dest = tmp.path().join("dest");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("manifest.json"), "{}").unwrap();
    assert_eq!(
        publish::publish(&root, &dest, |_| panic!("unrelated destination"))
            .unwrap_err()
            .exit_code(),
        2
    );
    assert_eq!(fs::read(dest.join("manifest.json")).unwrap(), b"{}");
    fs::remove_file(dest.join("manifest.json")).unwrap();
    for contents in ["first", "replacement"] {
        publish::publish(&root, &dest, |stage| {
            fs::write(
                stage.join("manifest.json"),
                br#"{"format":"gosanja-snapshot"}"#,
            )?;
            fs::write(stage.join("data"), contents)
        })
        .unwrap();
        assert_eq!(fs::read_to_string(dest.join("data")).unwrap(), contents);
    }
}

#[test]
fn fresh_output_and_inspection_errors() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("src");
    fs::create_dir(&root).unwrap();
    let out = tmp.path().join("fresh");
    publish::publish(&root, &out, |stage| fs::write(stage.join("data"), b"fresh")).unwrap();
    assert_eq!(fs::read(out.join("data")).unwrap(), b"fresh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&out).unwrap().permissions().mode(),
            fs::metadata(&root).unwrap().permissions().mode()
        );
        let long = tmp.path().join("x".repeat(4096));
        let err = publish::publish(&root, &long, |_| panic!("cannot inspect output")).unwrap_err();
        assert_eq!(err.exit_code(), 3);
        assert_ne!(err.to_string(), "");
        let alias = tmp.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        let err =
            publish::publish(&root, &alias.join("out"), |_| panic!("source alias")).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("overlap"));
    }
}

#[test]
fn resolved_sibling_output_is_publishable() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("src");
    fs::create_dir(&root).unwrap();
    publish::publish(&root, &root.join("../out"), |stage| {
        fs::write(stage.join("data"), "published")
    })
    .unwrap();
    assert_eq!(
        fs::read(temp.path().join("out/data")).unwrap(),
        b"published"
    );
    #[cfg(unix)]
    {
        let inner = root.join("inner");
        fs::create_dir(&inner).unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(inner, &link).unwrap();
        let err = publish::publish(&root, &link.join("../inside"), |_| panic!("source overlap"))
            .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }
}

#[test]
fn missing_output_parent_is_a_refusal_and_creates_nothing() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("src");
    fs::create_dir(&root).unwrap();
    let missing = temp.path().join("missing");
    let err = publish::publish(&root, &missing.join("nested/out"), |_| {
        panic!("missing parent")
    })
    .unwrap_err();
    assert_eq!(err.exit_code(), 2);
    assert!(!missing.exists());
}

#[test]
fn invocation_rejects_programs_that_are_not_absolute() {
    for program in ["sh", "./adapter.sh"] {
        assert_eq!(
            adapter::Invocation::new(program.into(), vec![])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[test]
fn reported_skips_follow_layer_availability_and_nameless_calls_keep_their_range() {
    let temp = tempdir().unwrap();
    let root = support::fixture("rust-basic");
    let skipped = run("skipped", &root, temp.path()).unwrap();
    assert_eq!(
        skipped
            .gaps
            .iter()
            .filter(|g| g.gap == GapKind::SkippedFile)
            .count(),
        4
    );
    // Even an ok result entry cannot supply syntax that the manifest never offered.
    let absent = run("skip-not-offered", &root, temp.path()).unwrap();
    assert_eq!(absent.layers.syntax, LayerState::Absent);
    assert_eq!(absent.gaps.len(), 1);
    assert_eq!(absent.gaps[0].gap, GapKind::MissingLayer);
    let unnamed = run("nameless-call", &root, temp.path()).unwrap();
    let node = &unnamed.syntax[0].outline.as_ref().unwrap()[0];
    assert_eq!(node.k, gosanja_schema::snapshot::OutlineKind::Call);
    assert_eq!(node.callee, None);
    assert_eq!(
        node.range,
        gosanja_schema::position::Range::new(4, 8, 4, 15)
    );
}
