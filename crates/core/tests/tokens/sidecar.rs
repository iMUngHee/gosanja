use gosanja_schema::position::Site;
use gosanja_schema::snapshot::RefRole;
use scip::types::SymbolRole;
use support::{DEF, MemSource, REF, baseline_token_snapshot, doc, occ, token_snapshot};

fn token_src() -> MemSource {
    MemSource::with(&[("x.rs", &"abcdefghijklmnopqrst\n".repeat(16))])
}

#[test]
fn token_imports_enrich_only_exact_references() {
    const OWNER: &str = "s cargo p 1 owner().";
    const TARGET: &str = "s cargo p 1 target().";
    const OTHER: &str = "s cargo p 1 other().";
    let read = SymbolRole::ReadAccess as i32;
    let import = SymbolRole::Import as i32;
    let source = token_src();
    let mut document = doc(
        "x.rs",
        vec![
            occ(OWNER, DEF, &[0, 0, 5], &[0, 0, 4, 0]),
            occ(TARGET, DEF, &[5, 0, 6], &[]),
            occ(OTHER, DEF, &[5, 7, 12], &[]),
            occ(TARGET, read, &[1, 1, 2], &[]),
            occ(TARGET, read, &[1, 3, 4], &[]),
            occ(OTHER, read, &[1, 3, 4], &[]),
            occ(TARGET, read, &[1, 5, 6], &[]),
            occ(TARGET, read | import, &[1, 5, 6], &[]),
            occ(TARGET, read, &[1, 7, 9], &[]),
            occ("s cargo p 1 alias().", DEF, &[1, 9, 10], &[]),
            occ("local 1", REF, &[1, 10, 11], &[]),
            occ(TARGET, read, &[4, 2, 3], &[]),
        ],
    );
    let wire = [
        json!({"row":"imports","file":"x.rs","sites":[[1,3,1,4],[1,5,1,6],
        [1,7,1,8],[1,9,1,10],[1,10,1,11],[3,0,3,1],[4,2,4,3]]}),
    ];
    let baseline = token_snapshot(&source, vec![document.clone()], &wire, "base", &[]);
    let native = baseline
        .relations
        .iter()
        .find(|row| {
            row.to == "cargo:p:target()."
                && row.roles.as_deref() == Some(&[RefRole::Import, RefRole::Read])
        })
        .unwrap();
    let site = |col, end| Site::new("x.rs", Range::new(1, col, 1, end));
    assert_eq!(native.sites.as_deref(), Some([site(5, 6)].as_slice()));
    assert!(native.syntax_import_sites.is_none());
    let snapshot = token_snapshot(&source, vec![document.clone()], &wire, "base,syntax", &[]);
    document.occurrences.reverse();
    let reordered = token_snapshot(&source, vec![document], &wire, "base,syntax", &[]);
    assert_eq!(serialise(&snapshot), serialise(&reordered));
    assert_eq!(snapshot.syntax, []);
    assert_eq!(snapshot.relations.len(), 3, "{:?}", snapshot.relations);
    let rows = |target: &str, roles: &[RefRole]| {
        snapshot
            .relations
            .iter()
            .find(|r| r.to == target && r.roles.as_deref() == Some(roles))
            .unwrap()
    };
    let ordinary = rows("cargo:p:target().", &[RefRole::Read]);
    assert_eq!(ordinary.from, "cargo:p:owner().");
    assert_eq!(
        ordinary.sites.as_deref(),
        Some([site(1, 2), site(5, 6), site(7, 9)].as_slice())
    );
    assert!(ordinary.syntax_import_sites.is_none());
    let marked = rows("cargo:p:target().", &[RefRole::Import, RefRole::Read]);
    assert_eq!(
        marked.sites.as_deref(),
        Some([site(3, 4), site(5, 6)].as_slice())
    );
    assert_eq!(
        marked.syntax_import_sites.as_deref(),
        Some([site(3, 4)].as_slice())
    );
    assert_eq!(marked.src, "core:reference-in-body");
    let other = rows("cargo:p:other().", &[RefRole::Import, RefRole::Read]);
    assert_eq!(
        other.syntax_import_sites.as_deref(),
        Some([site(3, 4)].as_slice())
    );
    assert_eq!(snapshot.dropped_local_references, 1);
    assert_eq!(snapshot.gaps.len(), 1, "{:?}", snapshot.gaps);
    assert_eq!(snapshot.gaps[0].gap, GapKind::UnattributedReferences);
    assert_eq!(snapshot.gaps[0].count, Some(1));
    assert_eq!(
        gosanja_core::validate::validate(&serialise(&snapshot), Some(&source)).unwrap(),
        []
    );
}

fn impl_block(line: i32, trait_: bool) -> Value {
    let mut row = json!({"row":"impl","file":"x.rs","extent":[line,0,line+2,12],
        "members":[[line+1,2,line+1,3]],"self_type":[line,4,line,5]});
    if trait_ {
        row["trait_type"] = json!([line, 8, line, 9]);
    }
    row
}

#[test]
fn token_impl_links_use_exact_members_and_headers() {
    const SELF: &str = "s cargo p 1 Concrete#";
    const TRAIT: &str = "s cargo dep 1 Trait#";
    const STRING: &str = "s cargo alloc 1 string/String#";
    let source = token_src();
    let documents = vec![doc(
        "x.rs",
        vec![
            occ(SELF, DEF, &[0, 0, 4], &[]),
            occ(SELF, DEF, &[1, 0, 4], &[]),
            occ(SELF, REF, &[2, 4, 5], &[]),
            occ(SELF, REF, &[2, 4, 5], &[]),
            occ(TRAIT, REF, &[2, 8, 9], &[]),
            occ("s cargo p 1 impl_a#a().", DEF, &[3, 2, 3], &[3, 0, 3, 16]),
            occ("s cargo p 1 impl_a#a().nested().", DEF, &[3, 8, 9], &[]),
            occ(SELF, REF, &[5, 4, 5], &[]),
            occ(TRAIT, REF, &[5, 8, 9], &[]),
            occ("s cargo p 1 impl_a#b().", DEF, &[6, 2, 3], &[]),
            occ(SELF, REF, &[8, 3, 5], &[]),
            occ(TRAIT, REF, &[8, 8, 9], &[]),
            occ("local 2", REF, &[8, 4, 5], &[]),
            occ("s cargo p 1 impl_b#c().", DEF, &[9, 2, 3], &[]),
            occ(STRING, REF, &[11, 4, 5], &[]),
            occ("s cargo p 1 LocalTrait#", DEF, &[11, 0, 1], &[]),
            occ("s cargo p 1 LocalTrait#", REF, &[11, 8, 9], &[]),
            occ("s cargo p 1 impl_c#d().", DEF, &[12, 2, 3], &[]),
        ],
    )];
    let mut first = impl_block(2, true);
    first["members"] = json!([[3, 2, 3, 3], [3, 6, 3, 7]]);
    let snapshot = token_snapshot(
        &source,
        documents,
        &[
            first,
            impl_block(5, true),
            impl_block(8, true),
            impl_block(11, true),
        ],
        "base,syntax",
        &[],
    );
    let link = |parent: &str, rel| {
        snapshot
            .relations
            .iter()
            .find(|r| r.from == parent && r.rel == rel)
    };
    let sites = |lines: &[u32], col| {
        lines
            .iter()
            .map(|&line| Site::new("x.rs", Range::new(line, col, line, col + 1)))
            .collect::<Vec<_>>()
    };
    let self_link = link("cargo:p:impl_a#", Rel::ImplSelfType).unwrap();
    assert_eq!(self_link.to, "cargo:p:Concrete#");
    assert!(
        snapshot
            .entities
            .iter()
            .any(|e| e.key == self_link.to && e.group)
    );
    assert_eq!(self_link.sites, Some(sites(&[2, 5], 4)));
    let trait_link = link("cargo:p:impl_a#", Rel::ImplTrait).unwrap();
    assert_eq!(trait_link.to, "cargo:dep:Trait#");
    assert_eq!(trait_link.sites, Some(sites(&[2, 5], 8)));
    assert!(link("cargo:p:impl_b#", Rel::ImplSelfType).is_none());
    assert_eq!(
        link("cargo:p:impl_b#", Rel::ImplTrait).unwrap().to,
        "cargo:dep:Trait#"
    );
    assert_eq!(
        link("cargo:p:impl_c#", Rel::ImplSelfType).unwrap().to,
        "cargo:alloc:string/String#"
    );
    assert_eq!(
        link("cargo:p:impl_c#", Rel::ImplTrait).unwrap().to,
        "cargo:p:LocalTrait#"
    );
    assert!(
        !snapshot
            .gaps
            .iter()
            .any(|g| g.gap == GapKind::AmbiguousImpl)
    );
    assert_eq!(
        gosanja_core::validate::validate(&serialise(&snapshot), Some(&source)).unwrap(),
        []
    );
}

#[test]
fn token_impl_group_members_keep_their_parent_after_fold() {
    const MEMBER: &str = "s cargo p 1 impl_a#method().";
    let source = token_src();
    let docs = vec![doc(
        "x.rs",
        vec![
            occ("s cargo p 1 Type#", REF, &[2, 4, 5], &[]),
            occ(MEMBER, DEF, &[3, 2, 3], &[]),
            occ("s cargo p 1 Type#", REF, &[5, 4, 5], &[]),
            occ(MEMBER, DEF, &[6, 2, 3], &[]),
        ],
    )];
    for options in [
        &[][..],
        &["--merged", r#"["s cargo p 1 impl_a#method()."]"#],
    ] {
        let snapshot = token_snapshot(
            &source,
            docs.clone(),
            &[impl_block(2, false), impl_block(5, false)],
            "base,syntax",
            options,
        );
        let method = &support::entity(&snapshot, "cargo:p:impl_a#method().");
        if options.is_empty() {
            assert!(method.group);
        } else {
            assert_eq!(method.decls.as_ref().unwrap().len(), 2);
        }
        let [link] = snapshot.relations.as_slice() else {
            panic!("expected one combined link: {:?}", snapshot.relations);
        };
        assert_eq!(link.rel, Rel::ImplSelfType);
        assert_eq!(link.from, "cargo:p:impl_a#");
        assert_eq!(link.to, "cargo:p:Type#");
        assert_eq!(link.sites.as_ref().unwrap().len(), 2);
        assert_eq!(
            gosanja_core::validate::validate(&serialise(&snapshot), Some(&source)).unwrap(),
            []
        );
    }
}

#[test]
fn token_impl_shared_parent_ambiguity_vetoes_only_that_kind() {
    const X: &str = "s cargo p 1 X#";
    const Y: &str = "s cargo p 1 Y#";
    const Z: &str = "s cargo p 1 Z#";
    const TRAIT: &str = "s cargo dep 1 Trait#";
    for (targets, second) in [([X, Y], Z), ([X, Y], X), ([X, X], Z)] {
        let source = token_src();
        let snapshot = token_snapshot(
            &source,
            vec![doc(
                "x.rs",
                vec![
                    occ(targets[0], REF, &[2, 4, 5], &[]),
                    occ(targets[1], REF, &[2, 4, 5], &[]),
                    occ(TRAIT, REF, &[2, 8, 9], &[]),
                    occ("s cargo p 1 same#a().", DEF, &[3, 2, 3], &[]),
                    occ(second, REF, &[5, 4, 5], &[]),
                    occ(TRAIT, REF, &[5, 8, 9], &[]),
                    occ("s cargo p 1 same#b().", DEF, &[6, 2, 3], &[]),
                    occ("s cargo p 1 same#silent().", DEF, &[9, 2, 3], &[]),
                    occ(Z, REF, &[11, 4, 5], &[]),
                    occ("s cargo p 1 same#c().", DEF, &[12, 2, 3], &[]),
                    occ("s cargo p 1 elsewhere#d().", DEF, &[12, 4, 5], &[]),
                ],
            )],
            &[
                impl_block(2, true),
                impl_block(5, true),
                impl_block(8, true),
                json!({"row":"impl","file":"x.rs","extent":[11,0,13,12],
                "members":[[12,2,12,3],[12,4,12,5]],"self_type":[11,4,11,5]}),
            ],
            "base,syntax",
            &[],
        );
        assert_eq!(snapshot.relations.len(), 1, "{:?}", snapshot.relations);
        let link = &snapshot.relations[0];
        assert_eq!(link.rel, Rel::ImplTrait);
        assert_eq!(link.from, "cargo:p:same#");
        assert_eq!(
            link.sites.as_deref(),
            Some(
                [
                    Site::new("x.rs", Range::new(2, 8, 2, 9)),
                    Site::new("x.rs", Range::new(5, 8, 5, 9)),
                ]
                .as_slice()
            )
        );
        assert_eq!(
            snapshot
                .gaps
                .iter()
                .filter(|g| g.gap == GapKind::AmbiguousImpl)
                .map(|g| g.range.unwrap().0)
                .collect::<Vec<_>>(),
            [[2, 0, 4, 12], [5, 0, 7, 12], [11, 0, 13, 12]]
        );
        assert_eq!(
            gosanja_core::validate::validate(&serialise(&snapshot), Some(&source)).unwrap(),
            []
        );
    }
}

#[test]
fn token_wire_rows_keep_fold_and_published_syntax() {
    let source = MemSource::with(&[
        ("a.rs", "pkg\n"),
        ("b.rs", "pkg\n"),
        ("use.rs", "owner\npkg\n"),
    ]);
    let package = "s cargo p 1 pkg/";
    let docs = vec![
        doc("a.rs", vec![occ(package, DEF, &[0, 0, 3], &[])]),
        doc("b.rs", vec![occ(package, DEF, &[0, 0, 3], &[])]),
        doc(
            "use.rs",
            vec![
                occ("s cargo p 1 owner().", DEF, &[0, 0, 5], &[0, 0, 1, 3]),
                occ(package, REF, &[1, 0, 3], &[]),
            ],
        ),
    ];
    let mut rows = vec![json!({"anchor":{"file":"use.rs","line":0,"col":0}})];
    let merged = "[\"s cargo p 1 pkg/\"]";
    if let Some(reference) = baseline_token_snapshot(
        &source,
        docs.clone(),
        &rows,
        "base,syntax",
        &["--merged", merged],
    ) {
        assert_eq!(
            reference,
            baseline_token_snapshot(
                &source,
                docs.clone(),
                &rows,
                "base,syntax",
                &["--merged", merged]
            )
            .unwrap()
        );
    }
    let base = token_snapshot(
        &source,
        docs.clone(),
        &rows,
        "base,syntax",
        &["--merged", merged],
    );
    rows.extend([
        json!({"row":"imports","file":"a.rs","sites":[[0,0,0,3]]}),
        json!({"row":"imports","file":"use.rs","sites":[[1,0,1,3]]}),
    ]);
    let with = token_snapshot(&source, docs, &rows, "base,syntax", &["--merged", merged]);
    assert_eq!(with.entities, base.entities);
    assert_eq!(with.files, base.files);
    assert_eq!(with.syntax.len(), 1);
    let before = serialise(&base);
    let after = serialise(&with);
    assert_eq!(before.syntax, after.syntax);
    let counts = |files: &gosanja_core::write::SnapshotFiles| {
        let manifest: Value = serde_json::from_slice(files.manifest.as_ref().unwrap()).unwrap();
        manifest["counts"]["syntax"].as_u64().unwrap()
    };
    assert_eq!((counts(&before), counts(&after)), (1, 1));
    let package = with
        .entities
        .iter()
        .find(|entity| entity.name == "pkg")
        .unwrap();
    assert_eq!(package.decls.as_ref().unwrap().len(), 2);
    assert_eq!(with.relations.len(), 1);
    assert_eq!(with.relations[0].to, package.key);
    assert!(with.relations[0].syntax_import_sites.is_some());
    assert_eq!(
        gosanja_core::validate::validate(&after, Some(&source)).unwrap(),
        []
    );
}
