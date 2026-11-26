use gosanja_core::graph::Snapshot as TokenSnapshot;
use gosanja_schema::{
    position::Site as EvidenceSite,
    snapshot::{AdapterInfo, LayerState as SyntaxState, RefRole, Rel as Relation, Scope},
};

fn scoped_tokens(snapshot: &TokenSnapshot, scopes: &[Scope]) -> SnapshotFiles {
    let mut files = write::serialise(snapshot);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(files.manifest.as_ref().unwrap()).unwrap();
    manifest["scopes"] = serde_json::to_value(scopes).unwrap();
    files.manifest = Some(serde_json::to_vec(&manifest).unwrap());
    files
}

fn token_scope(snapshot: &TokenSnapshot, path: &str, adapter: &str, syntax: SyntaxState) -> Scope {
    let mut scope = snapshot.scope.clone();
    scope.path = path.into();
    scope.adapter = Some(AdapterInfo {
        name: adapter.into(),
        version: "1".into(),
        contract: "0.6".into(),
    });
    scope.layers.syntax = syntax;
    scope.layers.build = SyntaxState::NotRequested;
    scope.layers.calls = SyntaxState::NotRequested;
    scope
}

fn token_validation_inputs() -> (TokenSnapshot, MemSource) {
    use serde_json::json;
    const TEXT: &str = "abcdefghijklmnopqrst\n";
    let source = MemSource::with(&[
        ("pkg/lib.rs", TEXT),
        ("pkg/mod.rs", TEXT),
        ("pkg/generated.rs", TEXT),
        ("pkg-other/lib.rs", TEXT),
    ]);
    let bytes = index(
        "test",
        vec![
            doc(
                "pkg/lib.rs",
                vec![
                    occ("s cargo p 1 owner().", DEF, &[0, 0, 1], &[]),
                    occ("s cargo p 1 Thing#", DEF, &[0, 2, 3], &[]),
                    occ("s cargo p 1 impl_parent#member().", DEF, &[0, 6, 7], &[]),
                ],
            ),
            doc(
                "pkg/mod.rs",
                vec![occ("s cargo p 1 Trait#", DEF, &[0, 0, 1], &[])],
            ),
            doc("pkg/generated.rs", vec![]),
            doc("pkg-other/lib.rs", vec![]),
        ],
    );
    let mut snapshot = build(&bytes, &source);
    snapshot.relations = serde_json::from_value(json!([
        {"from":"cargo:p:owner().","to":"cargo:p:Thing#","rel":"ref","roles":["import"],
         "sites":[["pkg/lib.rs",0,8,0,9]],"syntax_import_sites":[["pkg/lib.rs",0,8,0,9]],
         "src":"core:reference-in-body"},
        {"from":"cargo:p:impl_parent#","to":"cargo:p:Thing#","rel":"impl-self-type",
         "sites":[["pkg/lib.rs",0,10,0,11]],"src":"adapter:rust/syntax"},
        {"from":"cargo:p:impl_parent#","to":"cargo:p:Trait#","rel":"impl-trait",
         "sites":[["pkg/mod.rs",0,10,0,11]],"src":"adapter:rust/syntax"}
    ]))
    .unwrap();
    snapshot.gaps.extend(serde_json::from_value::<Vec<_>>(json!([
        {"gap":"ambiguous-impl","file":"pkg/mod.rs","range":[0,12,0,15],"layer":"syntax",
         "src":"adapter:rust/syntax"},
        {"gap":"unknown-file","file":"pkg/generated.rs","layer":"syntax","src":"adapter:rust/syntax"}
    ])).unwrap());
    (snapshot, source)
}

#[test]
fn token_relation_evidence_is_checked() {
    let (snapshot, source) = token_validation_inputs();
    let outer = token_scope(&snapshot, ".", "outer", SyntaxState::Failed);
    let inner = token_scope(&snapshot, "pkg", "rust", SyntaxState::Present);
    let adjacent = token_scope(&snapshot, "pkg-other", "rust", SyntaxState::Present);
    let scopes = [inner.clone(), outer.clone()];
    let files = scoped_tokens(&snapshot, &scopes);
    for source in [None, Some(&source as &dyn write::SourceReader)] {
        let diagnostics = validate::validate(&files, source).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    let expect = |copy: &TokenSnapshot, scopes: &[Scope], table, code| {
        let files = scoped_tokens(copy, scopes);
        for source in [None, Some(&source as &dyn write::SourceReader)] {
            let diagnostics = validate::validate(&files, source).unwrap();
            assert!(
                codes(&diagnostics).contains(&(table, code)),
                "{diagnostics:?}"
            );
        }
    };
    let site: EvidenceSite = serde_json::from_str(r#"["pkg/lib.rs",0,12,0,13]"#).unwrap();
    let sibling: EvidenceSite = serde_json::from_str(r#"["pkg-other/lib.rs",0,8,0,9]"#).unwrap();
    let mut copy = snapshot.clone();
    copy.relations[0].syntax_import_sites = Some(vec![site]);
    expect(&copy, &scopes, "relations.jsonl", "syntax-import-subset");
    let mut copy = snapshot.clone();
    copy.relations[0].roles = Some(vec![RefRole::Read, RefRole::Import]);
    expect(&copy, &scopes, "relations.jsonl", "ref-roles-unsorted");
    let mut native = copy.clone();
    native.relations[0].syntax_import_sites = None;
    expect(&native, &scopes, "relations.jsonl", "ref-roles-unsorted");

    let mut copy = snapshot.clone();
    copy.relations[0].syntax_import_sites = Some(vec![sibling.clone()]);
    copy.relations[0].sites = Some(vec![sibling.clone()]);
    expect(&copy, &scopes, "relations.jsonl", "syntax-relation-scope");
    expect(
        &copy,
        std::slice::from_ref(&inner),
        "relations.jsonl",
        "syntax-relation-scope",
    );
    for i in 0..3 {
        let mut copy = snapshot.clone();
        let sites = copy.relations[i].sites.as_mut().unwrap();
        sites.push(sibling.clone());
        sites.sort();
        expect(
            &copy,
            &[inner.clone(), adjacent.clone()],
            "relations.jsonl",
            "syntax-relation-scope",
        );
    }

    let mut copy = snapshot.clone();
    copy.relations[1].src = "adapter:outer/syntax".into();
    expect(&copy, &scopes, "relations.jsonl", "syntax-relation-scope");
    let mut copy = snapshot.clone();
    let position = copy
        .gaps
        .iter()
        .position(|row| row.gap == gosanja_schema::snapshot::GapKind::AmbiguousImpl)
        .unwrap();
    copy.gaps[position].src = "adapter:outer/syntax".into();
    expect(&copy, &scopes, "gaps.jsonl", "syntax-gap-scope");
    copy.gaps[position] = snapshot.gaps[position].clone();
    copy.gaps.push(copy.gaps[position].clone());
    expect(&copy, &scopes, "gaps.jsonl", "duplicate-row");
    let mut copy = snapshot.clone();
    copy.relations[1].from = "cargo:p:owner().".into();
    expect(&copy, &scopes, "relations.jsonl", "impl-source");

    let scopes = [outer, inner, adjacent];
    for index in 0..3 {
        let mut duplicate = snapshot.relations[index].clone();
        duplicate.sites = Some(vec![sibling.clone()]);
        if duplicate.rel == Relation::Ref {
            duplicate.syntax_import_sites = Some(vec![sibling.clone()]);
        } else if duplicate.rel == Relation::ImplTrait {
            duplicate.to = "cargo:p:Thing#".into();
        } else {
            duplicate.to = "cargo:p:Trait#".into();
        }
        let mut alone = snapshot.clone();
        alone.relations[index] = duplicate.clone();
        assert_eq!(check(&scoped_tokens(&alone, &scopes)), []);
        let mut copy = snapshot.clone();
        copy.relations.push(duplicate);
        expect(&copy, &scopes, "relations.jsonl", "duplicate-row");
    }
}
