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

fn configured_adapter(mode: &str, offered: &str, record: &Path) -> adapter::Described {
    adapter::describe(
        adapter::Invocation::new(
            support::shell(),
            vec![
                support::repo("fixtures/fake-adapter/adapter.sh").into_os_string(),
                "--mode".into(),
                mode.into(),
                "--layers".into(),
                offered.into(),
                "--record-argv".into(),
                record.into(),
            ],
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn layer_requests_are_intersected_with_the_manifest() {
    use LayerState::{Absent, NotRequested, Present};
    use gosanja_schema::{contract::LayerRequest, snapshot::Layer};
    let temp = tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let record = temp.path().join("argv");
    for (offered, cases) in [
        (
            r#"["base","syntax","calls","build"]"#,
            [
                (None, "base,syntax,calls,build", Present, Present),
                (Some("base"), "base", NotRequested, NotRequested),
                (Some("base,syntax"), "base,syntax", Present, NotRequested),
                (Some("base,calls"), "base,calls", NotRequested, Present),
            ],
        ),
        (
            r#"["base"]"#,
            [
                (None, "base", Absent, Absent),
                (Some("base"), "base", NotRequested, NotRequested),
                (Some("base,syntax"), "base", Absent, NotRequested),
                (Some("base,calls"), "base", NotRequested, Absent),
            ],
        ),
    ] {
        for (request, argument, syntax, calls) in cases {
            let build = match request {
                Some(list) if !list.contains("build") => NotRequested,
                _ if offered.contains("build") => Present,
                _ => Absent,
            };
            fs::write(&record, "").unwrap();
            let described = configured_adapter("good", offered, &record);
            let output = match request {
                None => described.run(&root, temp.path()),
                Some(list) => described.run_with_layers(
                    &root,
                    temp.path(),
                    list.parse::<LayerRequest>().unwrap(),
                ),
            }
            .unwrap();
            assert_eq!(
                (
                    output.layers.base,
                    output.layers.syntax,
                    output.layers.calls,
                    output.layers.build
                ),
                (Present, syntax, calls, build)
            );
            let expected: Vec<_> = [
                (syntax, Layer::Syntax),
                (calls, Layer::Calls),
                (build, Layer::Build),
            ]
            .into_iter()
            .filter_map(|(state, layer)| {
                (state == Absent).then_some((GapKind::MissingLayer, Some(layer)))
            })
            .collect();
            assert_eq!(
                output
                    .gaps
                    .iter()
                    .map(|g| (g.gap, g.layer))
                    .collect::<Vec<_>>(),
                expected
            );
            let recorded = fs::read_to_string(&record).unwrap();
            assert_eq!(
                recorded.lines().rev().take(2).collect::<Vec<_>>(),
                [argument, "--layers"]
            );
        }
    }
}

#[test]
fn explicit_all_and_omission_have_equal_argv_and_snapshots() {
    use gosanja_core::{
        sidecar,
        write::{FsSource, serialise},
    };
    use gosanja_schema::contract::LayerRequest;
    let temp = tempdir().unwrap();
    let parent = temp.path().canonicalize().unwrap();
    let root = support::fixture("rust-basic");
    let sources = FsSource::new(&root).unwrap();
    let record = parent.join("argv");
    let all = "base,syntax,calls,build".parse::<LayerRequest>().unwrap();
    for offered in [r#"["base","syntax","calls","build"]"#, r#"["base"]"#] {
        let run = |request| {
            fs::write(&record, "").unwrap();
            let described = configured_adapter("good", offered, &record);
            let output = match request {
                None => described.run(&root, &parent),
                Some(request) => described.run_with_layers(&root, &parent, request),
            }
            .unwrap();
            let snapshot = sidecar::build(output, &sources, "rust-basic").unwrap();
            let mut argv: Vec<String> = fs::read_to_string(&record)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect();
            let output_path = argv
                .iter_mut()
                .skip_while(|arg| *arg != "--out")
                .nth(1)
                .unwrap();
            assert!(Path::new(output_path).starts_with(&parent));
            // The engine uses a fresh directory for each adapter invocation.
            output_path.clear();
            (argv, serialise(&snapshot))
        };
        let (implicit_argv, implicit_files) = run(None);
        let (explicit_argv, explicit_files) = run(Some(all));
        assert_eq!(explicit_argv, implicit_argv, "{offered}");
        assert_eq!(explicit_files.named(), implicit_files.named(), "{offered}");
    }
}

#[test]
fn calls_output_is_filtered_before_schema_and_build_checks() {
    use gosanja_schema::contract::LayerRequest;
    let temp = tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let record = temp.path().join("argv");
    let full = r#"["base","syntax","calls","build"]"#;
    let no_calls = "base,syntax".parse::<LayerRequest>().unwrap();
    let run = |mode, offered, request| {
        configured_adapter(mode, offered, &record).run_with_layers(&root, temp.path(), request)
    };
    let sources = gosanja_core::write::FsSource::new(&root).unwrap();
    let snapshot = |output| gosanja_core::sidecar::build(output, &sources, "root").unwrap();
    let expected = snapshot(run("good", full, no_calls).unwrap());
    for fault in ["malformed-calls", "path-build", "duplicate-builds"] {
        let actual = snapshot(run(fault, full, no_calls).unwrap());
        assert_eq!(actual, expected, "{fault}");
        let error = run(fault, full, LayerRequest::ALL).unwrap_err();
        assert_eq!(error.exit_code(), 3, "{fault}: {error}");
        if fault == "duplicate-builds" {
            assert_eq!(error.to_string(), "duplicate build context id");
        }
    }
    let old = run("wrong-result-contract", full, LayerRequest::ALL).unwrap_err();
    assert_eq!(
        (old.exit_code(), old.to_string().as_str()),
        (3, "result contract differs from describe")
    );

    for (mode, detail) in [
        ("failing-calls", FailedDetail::ReportedFailed),
        ("no-calls-status", FailedDetail::NoStatus),
    ] {
        let output = run(mode, full, LayerRequest::ALL).unwrap();
        assert_eq!(output.layers.calls, LayerState::Failed);
        assert_eq!(
            output
                .builds
                .iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>(),
            ["dev"]
        );
        assert_eq!(output.gaps.len(), 1);
        assert_eq!(output.gaps[0].detail, Some(detail));
    }
    let base = r#"["base"]"#;
    let expected = snapshot(run("good", base, LayerRequest::ALL).unwrap());
    let actual = snapshot(run("malformed-calls", base, LayerRequest::ALL).unwrap());
    assert_eq!(expected.scope.builds, []);
    assert_eq!(actual, expected);
}

// Symbols the base layer lists in `merged`: the fold, over the fake adapter.

mod merged {
    use super::support::{self, DEF, REF, doc, index, occ};
    use gosanja_core::{graph::Snapshot, sidecar, write::FsSource};
    use gosanja_schema::position::Range;
    use gosanja_schema::snapshot::{EntityRow, GapKind, LayerState, Rel};
    use scip::types::Document;
    use scip::types::symbol_information::Kind;
    use std::fs;

    pub const PACKAGE: &str = "s m p 1 pkg/";
    const USER: &str = "s m q 1 use/f().";

    /// Ten lines of forty characters.
    fn blank() -> String {
        format!("{}\n", "x".repeat(40)).repeat(10)
    }

    /// The package clause of `file` at `line`, without a body.
    fn clause(file: &str, line: i32) -> Document {
        doc(file, vec![occ(PACKAGE, DEF, &[line, 8, 11], &[])])
    }

    /// c.go: a function with a body, for references and call endpoints.
    fn user(extra: Vec<scip::types::Occurrence>) -> Document {
        let mut occurrences = vec![occ(USER, DEF, &[0, 5, 6], &[0, 0, 3, 0])];
        occurrences.extend(extra);
        doc("c.go", occurrences)
    }

    pub struct Run {
        pub documents: Vec<Document>,
        pub syntax: String,
        pub calls: String,
        pub merged: String,
    }

    impl Run {
        pub fn new(documents: Vec<Document>) -> Self {
            Self {
                documents,
                syntax: String::new(),
                calls: String::new(),
                merged: format!(r#"["{PACKAGE}"]"#),
            }
        }

        /// Runs the fake adapter on a root holding a.go, b.go and c.go, and builds the
        /// snapshot the engine publishes.
        pub fn snapshot(&self) -> Snapshot {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("root");
            fs::create_dir(&root).unwrap();
            for file in ["a.go", "b.go", "c.go"] {
                fs::write(root.join(file), blank()).unwrap();
            }
            let index_file = tmp.path().join("index.scip");
            fs::write(&index_file, index("t", self.documents.clone())).unwrap();
            let syntax_file = tmp.path().join("syntax.jsonl");
            fs::write(&syntax_file, &self.syntax).unwrap();
            let calls_file = tmp.path().join("calls.jsonl");
            fs::write(&calls_file, &self.calls).unwrap();
            let described = gosanja_core::adapter::describe(
                gosanja_core::adapter::Invocation::new(
                    support::shell(),
                    vec![
                        support::repo("fixtures/fake-adapter/adapter.sh").into_os_string(),
                        "--index-file".into(),
                        index_file.into_os_string(),
                        "--syntax-file".into(),
                        syntax_file.into_os_string(),
                        "--calls-file".into(),
                        calls_file.into_os_string(),
                        "--merged".into(),
                        self.merged.clone().into(),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
            let output = described.run(&root, tmp.path()).unwrap();
            let sources = FsSource::new(&root).unwrap();
            sidecar::build(output, &sources, "t").unwrap()
        }
    }

    pub fn entity<'a>(snapshot: &'a Snapshot, key: &str) -> &'a EntityRow {
        snapshot
            .entities
            .iter()
            .find(|e| e.key == key)
            .unwrap_or_else(|| panic!("no entity {key}"))
    }

    fn sites(e: &EntityRow) -> Vec<(&str, Range)> {
        e.decls
            .iter()
            .flatten()
            .map(|d| (d.file.as_str(), d.name))
            .collect()
    }

    fn collisions(snapshot: &Snapshot) -> usize {
        snapshot
            .gaps
            .iter()
            .filter(|g| g.gap == GapKind::SymbolCollision)
            .count()
    }

    fn is_group(snapshot: &Snapshot) -> bool {
        let package = entity(snapshot, "m:p:pkg/");
        package.group && package.decls.is_none() && collisions(snapshot) == 1
    }

    fn folded(snapshot: &Snapshot) -> bool {
        let package = entity(snapshot, "m:p:pkg/");
        package.decls.is_some() && !package.group && collisions(snapshot) == 0
    }

    /// The list is checked while `result.json` is read, before the base layer's status.
    #[test]
    fn merged_invalid_lists_are_refused_whatever_the_base_status() {
        let tmp = tempfile::tempdir().unwrap();
        let root = support::fixture("rust-basic");
        for (mode, expected) in [
            ("merged-not-global", "is not a global SCIP symbol"),
            ("merged-local-scheme", "is not a global SCIP symbol"),
            ("merged-bare-unicode", "is not a global SCIP symbol"),
            ("merged-needless-escape", "is not a global SCIP symbol"),
            ("merged-unsorted", "the list is not sorted"),
            ("merged-repeated", "is listed twice"),
            ("merged-failed-base", "is not a global SCIP symbol"),
        ] {
            let err = super::run(mode, &root, tmp.path()).unwrap_err();
            assert_eq!(err.exit_code(), 3, "{mode}");
            assert!(err.to_string().contains(expected), "{mode}: {err}");
        }
    }

    /// The list is checked before the build ids, so a result failing both reports the list.
    #[test]
    fn an_invalid_list_is_reported_before_duplicate_build_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let root = support::fixture("rust-basic");
        let err = super::run("merged-not-global-duplicate-builds", &root, tmp.path()).unwrap_err();
        assert_eq!(
            (err.exit_code(), err.to_string().as_str()),
            (3, "merged: \"local 1\" is not a global SCIP symbol")
        );
    }

    /// `merged` on an optional layer's entry is an unknown field there: an ill-typed value is
    /// ignored and the run stays usable.
    #[test]
    fn merged_on_an_optional_layer_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let root = support::fixture("rust-basic");
        let output = super::run("syntax-merged-false", &root, tmp.path()).unwrap();
        assert_eq!(output.layers.syntax, LayerState::Present);
        assert_eq!(output.merged, Vec::<String>::new());
    }

    #[test]
    fn merged_two_kept_sites_fold_and_an_excluded_document_leaves_no_fact() {
        let mut a = clause("a.go", 0);
        a.symbols = vec![support::info(PACKAGE, Kind::Namespace)];
        a.occurrences.push(occ("s m p 1 I#", DEF, &[5, 5, 6], &[]));
        // x.go is not in the root: source admission rejects the document, with its kind
        // (which would sort first) and its implementation fact.
        let mut excluded = clause("x.go", 0);
        let mut info = support::implementing(PACKAGE, &["s m p 1 I#"]);
        info.kind = protobuf::EnumOrUnknown::new(Kind::Interface);
        excluded.symbols = vec![info];
        let snapshot = Run::new(vec![a, clause("b.go", 0), excluded]).snapshot();
        assert!(folded(&snapshot));
        let package = entity(&snapshot, "m:p:pkg/");
        assert_eq!(
            sites(package),
            [
                ("a.go", Range::new(0, 8, 0, 11)),
                ("b.go", Range::new(0, 8, 0, 11))
            ]
        );
        assert_eq!(package.kind.as_deref(), Some("test:namespace"));
        assert!(!snapshot.relations.iter().any(|r| r.rel == Rel::Implements));
        assert!(
            snapshot
                .gaps
                .iter()
                .any(|g| g.gap == GapKind::SourceUnavailable && g.file.as_deref() == Some("x.go"))
        );
        assert!(!snapshot.files.iter().any(|f| f.path == "x.go"));
    }

    #[test]
    fn merged_one_kept_site_is_a_plain_definition() {
        let snapshot = Run::new(vec![clause("a.go", 0), clause("x.go", 0)]).snapshot();
        let package = entity(&snapshot, "m:p:pkg/");
        assert!(package.def.is_some() && package.decls.is_none() && !package.group);
        assert!(
            snapshot
                .gaps
                .iter()
                .any(|g| g.gap == GapKind::SourceUnavailable)
        );
    }

    #[test]
    fn merged_no_kept_site_changes_nothing() {
        let mut run = Run::new(vec![user(vec![
            occ("s m p 1 ext/", REF, &[1, 4, 7], &[]),
            occ("s m p 1 vir/F().", DEF, &[2, 5, 6], &[]),
        ])]);
        run.merged = r#"["s m p 1 ext/","s m p 1 vir/"]"#.to_owned();
        let snapshot = run.snapshot();
        assert!(
            snapshot
                .externals
                .iter()
                .any(|e| e.symbol == "s m p 1 ext/")
        );
        let vir = entity(&snapshot, "m:p:vir/");
        assert!(vir.r#virtual && vir.decls.is_none());
    }

    #[test]
    fn merged_a_site_with_an_index_body_stays_a_group() {
        let a = doc("a.go", vec![occ(PACKAGE, DEF, &[0, 8, 11], &[0, 0, 9, 0])]);
        let snapshot = Run::new(vec![a, clause("b.go", 0)]).snapshot();
        assert!(is_group(&snapshot));
    }

    #[test]
    fn merged_a_published_syntax_row_at_a_site_keeps_the_group_and_joins_the_others() {
        let mut run = Run::new(vec![clause("a.go", 0), clause("b.go", 0), user(vec![])]);
        run.syntax = concat!(
            r#"{"anchor":{"file":"a.go","line":0,"col":8},"attributes":["//go:build x"]}"#,
            "\n",
            r#"{"anchor":{"file":"c.go","line":0,"col":5},"visibility":"pub"}"#,
            "\n"
        )
        .to_owned();
        let snapshot = run.snapshot();
        assert!(is_group(&snapshot));
        let keys: Vec<_> = snapshot.syntax.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"m:q:use/f()."));
    }

    #[test]
    fn merged_a_row_dropped_in_phase_5_leaves_the_symbol_folded() {
        let mut run = Run::new(vec![clause("a.go", 0), clause("b.go", 0)]);
        run.syntax = concat!(
            r#"{"anchor":{"file":"a.go","line":0,"col":8},"outline":[{"k":"exit","range":[0,8,0,99]}]}"#,
            "\n"
        )
        .to_owned();
        let snapshot = run.snapshot();
        assert!(folded(&snapshot));
        assert_eq!(snapshot.syntax, []);
        assert!(
            snapshot
                .gaps
                .iter()
                .any(|g| g.gap == GapKind::PositionOutOfRange)
        );
    }

    #[test]
    fn merged_a_layer_discarded_in_phase_6_leaves_the_symbol_folded() {
        let mut run = Run::new(vec![clause("a.go", 0), clause("b.go", 0), user(vec![])]);
        run.syntax = concat!(
            r#"{"anchor":{"file":"a.go","line":0,"col":8},"attributes":["//go:build x"]}"#,
            "\n",
            r#"{"anchor":{"file":"c.go","line":0,"col":5},"outline":[{"k":"exit","range":[5,0,5,1]}]}"#,
            "\n"
        )
        .to_owned();
        let snapshot = run.snapshot();
        assert!(folded(&snapshot));
        assert_eq!(snapshot.scope.layers.syntax, LayerState::Failed);
        assert_eq!(snapshot.syntax, []);
    }

    #[test]
    fn merged_repeated_observations_at_one_site_are_one_site() {
        let a = doc(
            "a.go",
            vec![
                occ(PACKAGE, DEF, &[0, 8, 11], &[]),
                occ(PACKAGE, DEF, &[0, 8, 11], &[]),
            ],
        );
        let snapshot = Run::new(vec![a, clause("b.go", 0)]).snapshot();
        assert_eq!(sites(entity(&snapshot, "m:p:pkg/")).len(), 2);
    }

    fn call(callee: &str) -> String {
        format!(
            r#"{{"caller":{{"file":"c.go","line":0,"col":5}},"callee":{callee},"mechanism":"static","resolution":"direct","build":"dev"}}"#
        ) + "\n"
    }

    fn calls_to_package(snapshot: &Snapshot) -> usize {
        snapshot
            .relations
            .iter()
            .filter(|r| r.rel == Rel::Call && r.to == "m:p:pkg/")
            .count()
    }

    fn unanchored(snapshot: &Snapshot) -> usize {
        snapshot
            .gaps
            .iter()
            .filter(|g| g.gap == GapKind::Unanchored)
            .count()
    }

    #[test]
    fn merged_a_call_endpoint_at_one_folded_site_resolves_to_the_merged_key() {
        let mut run = Run::new(vec![clause("a.go", 0), clause("b.go", 0), user(vec![])]);
        run.calls = call(r#"{"file":"b.go","line":0,"col":8}"#);
        let snapshot = run.snapshot();
        assert!(folded(&snapshot));
        assert_eq!(calls_to_package(&snapshot), 1);
        assert_eq!(unanchored(&snapshot), 0);
    }

    #[test]
    fn merged_a_call_endpoint_at_a_position_another_entity_shares_stays_unanchored() {
        let a = doc(
            "a.go",
            vec![
                occ(PACKAGE, DEF, &[0, 8, 11], &[]),
                occ("s m p 1 other().", DEF, &[0, 8, 11], &[]),
            ],
        );
        let mut run = Run::new(vec![a, clause("b.go", 0), user(vec![])]);
        run.calls = call(r#"{"file":"a.go","line":0,"col":8}"#);
        let snapshot = run.snapshot();
        assert!(folded(&snapshot));
        assert_eq!((calls_to_package(&snapshot), unanchored(&snapshot)), (0, 1));
    }

    #[test]
    fn merged_a_call_endpoint_at_a_start_two_of_its_sites_share_stays_unanchored() {
        let a = doc(
            "a.go",
            vec![
                occ(PACKAGE, DEF, &[0, 8, 11], &[]),
                occ(PACKAGE, DEF, &[0, 8, 12], &[]),
            ],
        );
        let mut run = Run::new(vec![a, clause("b.go", 0), user(vec![])]);
        run.calls = call(r#"{"file":"a.go","line":0,"col":8}"#);
        let snapshot = run.snapshot();
        assert_eq!(sites(entity(&snapshot, "m:p:pkg/")).len(), 3);
        assert_eq!((calls_to_package(&snapshot), unanchored(&snapshot)), (0, 1));
    }

    #[test]
    fn merged_a_call_endpoint_whose_line_and_name_match_two_sites_stays_unanchored() {
        let a = doc(
            "a.go",
            vec![
                occ(PACKAGE, DEF, &[0, 8, 11], &[]),
                occ(PACKAGE, DEF, &[0, 20, 23], &[]),
            ],
        );
        let mut run = Run::new(vec![a, clause("b.go", 0), user(vec![])]);
        run.calls = call(r#"{"file":"a.go","line":0,"name":"pkg"}"#);
        let snapshot = run.snapshot();
        assert!(folded(&snapshot));
        assert_eq!((calls_to_package(&snapshot), unanchored(&snapshot)), (0, 1));
        // Control: the line and name of a site with no twin resolve.
        let mut run = Run::new(vec![clause("a.go", 0), clause("b.go", 0), user(vec![])]);
        run.calls = call(r#"{"file":"b.go","line":0,"name":"pkg"}"#);
        assert_eq!(calls_to_package(&run.snapshot()), 1);
    }
}

include!("build/protocol.rs");
