use gosanja_core::{
    graph::Snapshot,
    validate,
    write::{SnapshotFiles, serialise},
};
use gosanja_schema::{
    position::{Range as TokenRange, Site as TokenSite},
    snapshot::{EntityRow, EntityTest, GapRow, Layer, RefRole, Rel as TokenRel, RelationRow},
};
use serde_json::json;
use support::{DEF, MemSource, REF, baseline_token_snapshot, doc, occ, token_snapshot};

#[test]
fn token_sites_follow_syntax_admission_and_terminal_order() {
    token_terminal_precedence();
    token_source_admission();
}

fn terminal_token_ref<'a>(
    relations: impl Iterator<Item = &'a RelationRow>,
    site: &TokenSite,
) -> &'a RelationRow {
    let mut matching = relations.filter(|row| {
        row.rel == TokenRel::Ref
            && row.from == "cargo:p:use_owner/"
            && row.to == "cargo:p:target()."
            && row.sites.as_ref().is_some_and(|sites| sites.contains(site))
    });
    let reference = matching.next().expect("a reference at the exact site");
    assert!(matching.next().is_none(), "repeated reference at {site:?}");
    reference
}

fn assert_terminal_baseline(baseline: &SnapshotFiles, site: &TokenSite) {
    let baseline = validate::read(baseline, gosanja_schema::compat::Version::new(0, 5)).unwrap();
    assert_eq!(
        baseline.manifest.unwrap().scopes[0].layers.syntax,
        LayerState::Present
    );
    let entity = &baseline
        .entities
        .iter()
        .find(|(_, row)| row.name == "f")
        .unwrap()
        .1;
    assert_eq!(
        entity.def.as_ref().unwrap().body,
        Some(TokenRange::new(0, 0, 1, 15))
    );
    assert_eq!(entity.test, Some(EntityTest::Cfg));
    for kind in [GapKind::Unanchored, GapKind::SkippedFile] {
        assert!(baseline.gaps.iter().any(|(_, row)| row.gap == kind));
    }
    let reference = terminal_token_ref(baseline.relations.iter().map(|(_, row)| row), site);
    assert_eq!(reference.roles.as_deref(), Some([].as_slice()));
    assert!(reference.syntax_import_sites.is_none());
}

fn token_terminal_precedence() {
    let sources = MemSource::with(&[(
        "src/lib.rs",
        "#[cfg(test)]\nfn f() { f(); } \nfn target() {}\nmod use_owner { use super::target; }\n",
    )]);
    let document = doc(
        "src/lib.rs",
        vec![
            occ("s cargo p 1 f().", DEF, &[1, 3, 4], &[]),
            occ("s cargo p 1 target().", DEF, &[2, 3, 9], &[]),
            occ("s cargo p 1 use_owner/", DEF, &[3, 4, 13], &[3, 0, 3, 36]),
            occ("s cargo p 1 target().", REF, &[3, 27, 33], &[]),
        ],
    );
    let valid_rows = vec![
        json!({"anchor":{"file":"src/lib.rs","line":1,"col":3},"body":[0,0,1,15]}),
        json!({"row":"test","anchor":{"file":"src/lib.rs","line":1,"col":3},"test":"cfg"}),
        json!({"anchor":{"file":"missing.rs","line":0,"col":0}}),
        json!({"row":"imports","file":"src/lib.rs","sites":[[3,27,3,33]]}),
    ];
    let site = TokenSite::new("src/lib.rs", TokenRange::new(3, 27, 3, 33));
    let options = &["--mode", "token-skipped"];
    if let Some(baseline) = baseline_token_snapshot(
        &sources,
        vec![document.clone()],
        &valid_rows[..3],
        "base,syntax",
        options,
    ) {
        assert_terminal_baseline(&baseline, &site);
    }
    for invalid in [
        None,
        Some("shape"),
        Some("duplicate"),
        Some("impl-duplicate"),
        Some("impl-boundary"),
        Some("extent"),
    ] {
        let mut rows = valid_rows.clone();
        match invalid {
            Some("shape") => rows.push(json!({"row":"imports","file":"missing.rs","sites":[]})),
            Some("duplicate") => {
                rows.push(json!({"row":"imports","file":"src/lib.rs","sites":[[3,1,3,2]]}));
            }
            Some("impl-duplicate") => {
                let row = json!({"row":"impl","file":"missing.rs","extent":[0,0,2,0],
                    "members":[[1,1,1,2]],"self_type":[0,5,0,6]});
                rows.extend([row.clone(), row]);
            }
            Some("impl-boundary") => rows
                .push(json!({"row":"impl","file":"missing.rs","extent":[0,0,1,2],
                    "members":[[1,1,1,2]],"self_type":[0,5,0,6]})),
            Some("extent") => {
                rows[0]["outline"] = json!([{"k":"opaque","range":[3,0,3,1]}]);
            }
            None => {}
            _ => unreachable!(),
        }
        let snapshot = token_snapshot(
            &sources,
            vec![document.clone()],
            &rows,
            "base,syntax",
            options,
        );
        let layer_gaps: Vec<_> = snapshot
            .gaps
            .iter()
            .filter(|g| g.layer == Some(Layer::Syntax))
            .collect();
        let row = snapshot.entities.iter().find(|r| r.name == "f").unwrap();
        let reference = terminal_token_ref(snapshot.relations.iter(), &site);
        if let Some(case) = invalid {
            assert_discarded_token_syntax(case, &snapshot, row, reference, &layer_gaps);
        } else {
            assert_eq!(
                snapshot.scope.layers.syntax,
                LayerState::Present,
                "{layer_gaps:?}"
            );
            assert_eq!(
                row.def.as_ref().unwrap().body,
                Some(TokenRange::new(0, 0, 1, 15))
            );
            assert_eq!(row.test, Some(EntityTest::Cfg));
            assert_eq!(
                reference.roles.as_deref(),
                Some([RefRole::Import].as_slice())
            );
            assert_eq!(
                reference.syntax_import_sites.as_deref(),
                Some(std::slice::from_ref(&site))
            );
            assert!(layer_gaps.iter().any(|g| g.gap == GapKind::Unanchored));
            assert!(layer_gaps.iter().any(|g| g.gap == GapKind::SkippedFile));
        }
        let diagnostics = validate::validate(&serialise(&snapshot), Some(&sources)).unwrap();
        assert!(diagnostics.is_empty(), "{invalid:?}: {diagnostics:?}");
    }
}

fn assert_discarded_token_syntax(
    case: &str,
    snapshot: &Snapshot,
    row: &EntityRow,
    reference: &RelationRow,
    layer_gaps: &[&GapRow],
) {
    assert_eq!(snapshot.scope.layers.syntax, LayerState::Failed, "{case}");
    assert_eq!(layer_gaps.len(), 1, "{case}: {layer_gaps:?}");
    assert_eq!(layer_gaps[0].gap, GapKind::FailedLayer);
    assert_eq!(layer_gaps[0].detail, Some(FailedDetail::RowInvalid));
    assert!(row.def.as_ref().unwrap().body.is_none() && row.test.is_none());
    assert_eq!(reference.roles.as_deref(), Some([].as_slice()));
    assert!(reference.syntax_import_sites.is_none());
    assert_eq!(snapshot.syntax, []);
}

fn assert_token_admission_baseline(baseline: &SnapshotFiles) {
    let baseline = validate::read(baseline, gosanja_schema::compat::Version::new(0, 5)).unwrap();
    assert!(
        baseline
            .files
            .iter()
            .any(|(_, row)| row.path == "generated.rs")
    );
    assert!(
        baseline
            .gaps
            .iter()
            .all(|(_, row)| row.layer != Some(Layer::Syntax))
    );
    assert!(
        baseline
            .relations
            .iter()
            .all(|(_, row)| row.syntax_import_sites.is_none())
    );
}

fn token_source_admission() {
    use support::{Recording, write_token_rows};
    let source = Recording {
        inner: MemSource::with(&[
            ("src/lib.rs", "é🦀f\n"),
            ("src/bad.rs", "é🦀f\n"),
            ("generated.rs", "generated\ncontent\n"),
        ]),
        requested: std::cell::RefCell::default(),
    };
    let calls = tempfile::NamedTempFile::new().unwrap();
    write_token_rows(
        calls.path(),
        &[json!({
            "caller":{"file":"src/lib.rs","line":0,"col":0},
            "callee":{"file":"src/bad.rs","line":0,"col":3},
            "resolution":"direct","mechanism":"static","build":"dev",
            "site":{"file":"generated.rs","line":0,"col":1}
        })],
    );
    let documents = vec![
        doc(
            "src/lib.rs",
            vec![
                occ("s cargo p 1 owner().", DEF, &[0, 0, 2], &[0, 0, 0, 7]),
                occ("s cargo p 1 f().", REF, &[0, 6, 7], &[]),
            ],
        ),
        doc(
            "src/bad.rs",
            vec![occ("s cargo p 1 f().", DEF, &[0, 6, 7], &[])],
        ),
    ];
    let options = &[
        "--encoding",
        "utf-16",
        "--calls-file",
        calls.path().to_str().unwrap(),
    ];
    if let Some(baseline) = baseline_token_snapshot(
        &source.inner,
        documents.clone(),
        &[],
        "base,syntax,calls",
        options,
    ) {
        assert_token_admission_baseline(&baseline);
    }
    let snapshot = token_snapshot(
        &source,
        documents,
        &[
            json!({"row":"imports","file":"src/lib.rs","sites":[[0,3,0,4]]}),
            json!({"row":"imports","file":"src/bad.rs","sites":[[0,2,0,4]]}),
            json!({"row":"imports","file":"generated.rs","sites":[[0,0,0,1]]}),
            json!({"row":"impl","file":"generated.rs","extent":[0,0,1,7],
                "members":[[1,1,1,2]],"self_type":[0,2,0,3]}),
        ],
        "base,syntax,calls",
        options,
    );
    assert_eq!(snapshot.scope.layers.syntax, LayerState::Present);
    assert!(snapshot.files.iter().any(|row| row.path == "generated.rs"));
    assert_eq!(
        source
            .requested
            .borrow()
            .iter()
            .filter(|file| *file == "generated.rs")
            .count(),
        1
    );
    let layer: Vec<_> = snapshot
        .gaps
        .iter()
        .filter(|g| g.layer == Some(Layer::Syntax))
        .collect();
    assert_eq!(layer.len(), 2, "{layer:?}");
    assert!(
        layer
            .iter()
            .any(|g| g.gap == GapKind::UnknownFile && g.file.as_deref() == Some("generated.rs"))
    );
    assert!(layer.iter().any(|g| g.gap == GapKind::PositionOutOfRange
        && g.file.as_deref() == Some("src/bad.rs")
        && g.count == Some(1)));
    let marks: Vec<_> = snapshot
        .relations
        .iter()
        .flat_map(|row| row.syntax_import_sites.iter().flatten().cloned())
        .collect();
    assert_eq!(
        marks,
        [TokenSite::new("src/lib.rs", TokenRange::new(0, 6, 0, 7))]
    );
    let diagnostics = validate::validate(&serialise(&snapshot), Some(&source)).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn token_sites_preserve_legacy_syntax_outcomes() {
    use gosanja_core::{
        sidecar, validate,
        write::{FsSource, serialise},
    };
    let root = support::fixture("rust-basic");
    let source = FsSource::new(&root).unwrap();
    let scratch = tempdir().unwrap();
    for (mode, request, want, detail) in [
        ("bad-row", "base", LayerState::NotRequested, None),
        ("not-offered", "base,syntax", LayerState::Absent, None),
        (
            "no-status",
            "base,syntax",
            LayerState::Failed,
            Some(FailedDetail::NoStatus),
        ),
        (
            "reported-a",
            "base,syntax",
            LayerState::Failed,
            Some(FailedDetail::ReportedFailed),
        ),
        (
            "no-file",
            "base,syntax",
            LayerState::Failed,
            Some(FailedDetail::FileMissing),
        ),
        ("empty-syntax", "base,syntax", LayerState::Present, None),
    ] {
        let output = support::fake(mode)
            .unwrap()
            .run_with_layers(&root, scratch.path(), request.parse().unwrap())
            .unwrap();
        let snapshot = sidecar::build(output, &source, "rust-basic").unwrap();
        assert_eq!(snapshot.scope.layers.syntax, want, "{mode}");
        let layer: Vec<_> = snapshot
            .gaps
            .iter()
            .filter(|g| g.layer == Some(gosanja_schema::snapshot::Layer::Syntax))
            .collect();
        match want {
            LayerState::Absent => {
                assert_eq!(layer.len(), 1, "{mode}: {layer:?}");
                assert_eq!(layer[0].gap, GapKind::MissingLayer);
            }
            LayerState::Failed => {
                assert_eq!(layer.len(), 1, "{mode}: {layer:?}");
                assert_eq!(layer[0].gap, GapKind::FailedLayer);
                assert_eq!(layer[0].detail, detail);
            }
            _ => assert!(layer.is_empty(), "{mode}: {layer:?}"),
        }
        assert!(snapshot.syntax.is_empty(), "{mode}");
        assert!(
            snapshot
                .relations
                .iter()
                .all(|row| row.syntax_import_sites.is_none())
        );
        let diagnostics = validate::validate(&serialise(&snapshot), Some(&source)).unwrap();
        assert!(diagnostics.is_empty(), "{mode}: {diagnostics:?}");
    }
}
