//! Comparison semantics on small constructed sets of statements.
use super::{CompareError, Comparison};
use crate::validate::{self, ReadSnapshot, read};
use crate::write::SnapshotFiles;
use gosanja_schema::snapshot::{EntityRow, RelationRow, SCHEMA_VERSION};
use serde_json::{json, to_value};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

fn compare(old: &ReadSnapshot, new: &ReadSnapshot) -> Result<Comparison, CompareError> {
    let mut diagnostics = Vec::new();
    let old_symbols = validate::symbol_parts(old, &mut diagnostics);
    let new_symbols = validate::symbol_parts(new, &mut diagnostics);
    assert_eq!(diagnostics, []);
    super::compare_rows(old, new, &old_symbols, &new_symbols)
}

fn empty() -> ReadSnapshot {
    let mut s = read(
        &SnapshotFiles::read(&fixture("rust-basic").join("expected")),
        SCHEMA_VERSION,
    )
    .unwrap();
    s.entities.clear();
    s.externals.clear();
    s.relations.clear();
    s.syntax.clear();
    s.gaps.clear();
    s
}

fn entity(key: &str, symbol: &str) -> EntityRow {
    serde_json::from_value(
        json!({"key":key,"symbol":symbol,"name":"f","kind":"rust:function",
      "role":"callable","src":"scip:ra",
      "def":{"file":"src/lib.rs","name":[0,0,0,1],"body":[0,0,1,0],"body_src":"scip:ra"}}),
    )
    .unwrap()
}

fn reference(from: &str, to: &str) -> RelationRow {
    serde_json::from_value(json!({"from":from,"to":to,"rel":"ref","roles":[],"sites":[],"src":"core:reference-in-body"})).unwrap()
}

fn external(key: &str, version: &str) -> gosanja_schema::snapshot::ExternalRow {
    serde_json::from_value(json!({
      "key":key,"symbol":format!("ra m p {version} x."),
      "package":{"manager":"m","name":"p","version":version},"src":"scip:ra"
    }))
    .unwrap()
}

fn ref_change(
    change: &str,
    from: serde_json::Value,
    to: serde_json::Value,
    endpoint: &str,
) -> serde_json::Value {
    let mut row = json!({"change":change,"endpoint":endpoint,"rel":"ref","roles":[]});
    row["from"] = from;
    row["to"] = to;
    row
}

fn described(change: &str, key: &str, member: Option<&str>, group: bool) -> serde_json::Value {
    let mut row = json!({"change":change,"name":"f","role":"callable"});
    row[if change == "added" { "new" } else { "old" }] = key.into();
    if group {
        row["group"] = true.into();
    } else {
        row["kind"] = "rust:function".into();
        row["file"] = "src/lib.rs".into();
    }
    if let Some(group) = member {
        row["member_of"] = group.into();
    }
    row
}

#[test]
fn versions_seen_only_in_old_externals_or_members_block_fallback() {
    for extra_is_external in [true, false] {
        let mut old = empty();
        old.entities.push((1, entity("f", "ra m p 1 f.")));
        let mut new = old.clone();
        new.entities[0].1.symbol = Some("ra m p 2 f.".into());
        if extra_is_external {
            old.externals.push((1, external("x", "2")));
        } else {
            let mut member = entity("member", "ra m p 2 something.");
            member.member_of = Some("g".into());
            old.entities.push((2, member));
        }
        let result = compare(&old, &new).unwrap();
        assert!(!result.pairs.contains_key("f"));
    }
}

#[test]
fn member_kind_and_unchanged_file_discriminate_occurrences() {
    let mut old = empty();
    let mut group = entity("g", "ra m p 1 g.");
    group.group = true;
    group.def = None;
    group.kind = None;
    let mut member = entity("g~1", "ra m p 1 g.");
    member.member_of = Some("g".into());
    old.entities = vec![(1, group), (2, member)];
    old.relations.push((1, reference("g~1", "g")));
    let mut new = old.clone();
    new.entities[1].1.kind = Some("rust:struct".into());
    assert!(!compare(&old, &new).unwrap().pairs.contains_key("g~1"));
    new = old.clone();
    let previous = EntityRow {
        key: "g~2".into(),
        ..new.entities[1].1.clone()
    };
    new.entities[1].1.def.as_mut().unwrap().file = "src/views.rs".into();
    new.entities.push((3, previous));
    new.relations[0].1.from = "g~2".into();
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(result.entities).unwrap(),
        json!([
 {"change":"rekeyed","old":"g~1","new":"g~2"},
 {"change":"added","new":"g~1","kind":"rust:function","role":"callable","name":"f","member_of":"g","file":"src/views.rs"}])
    );
    assert_eq!(result.relations, []);
    for whole in [true, false] {
        let mut new = old.clone();
        let file = new
            .files
            .iter_mut()
            .find(|(_, file)| file.path == "src/lib.rs")
            .unwrap();
        file.1.sha256 = "0".repeat(64);
        let end = u32::try_from(file.1.lines).unwrap();
        let span = if whole { [0, 0, end, 0] } else { [0, 0, 0, 1] };
        old.entities[1].1.def.as_mut().unwrap().name = gosanja_schema::position::Range(span);
        new.entities[1].1.def.as_mut().unwrap().name =
            gosanja_schema::position::Range(if whole { span } else { [1, 0, 1, 1] });
        let result = compare(&old, &new).unwrap();
        assert_eq!(
            to_value(result.entities).unwrap(),
            json!([
                described("removed", "g~1", Some("g"), false),
                described("added", "g~1", Some("g"), false)
            ])
        );
        let target = json!({"old":"g","new":"g"});
        assert_eq!(
            to_value(result.relations).unwrap(),
            json!([
                ref_change("removed", json!({"old":"g~1"}), target.clone(), "from"),
                ref_change("added", json!({"new":"g~1"}), target, "from")
            ])
        );
    }
}

#[test]
fn external_rekeys_and_reached_packages_follow_relations() {
    let mut old = empty();
    old.entities.push((1, entity("f", "ra m p 1 f.")));
    old.externals.push((1, external("K", "1")));
    old.relations.push((1, reference("f", "K")));
    let mut new = old.clone();
    new.externals = vec![(1, external("K@1", "1")), (2, external("K@2", "2"))];
    new.relations = vec![(1, reference("f", "K@1")), (2, reference("f", "K@2"))];
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(&result.externals).unwrap(),
        json!([
 {"change":"rekeyed","old":"K","new":"K@1"},{"change":"added","new":"K@2","package":"m:p"}])
    );
    assert_eq!(result.manifest.packages_added, Vec::<String>::new());
    new.relations.clear();
    new.externals.clear();
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(result.externals).unwrap(),
        json!([{"change":"removed","old":"K","package":"m:p"}])
    );
    assert_eq!(result.manifest.packages_removed, ["m:p"]);
}

#[test]
fn a_named_virtual_and_definition_with_the_same_symbol_do_not_pair() {
    let mut old = empty();
    old.entities.push((1, entity("f", "ra m p 1 f.")));
    let mut new = old.clone();
    new.entities[0].1.r#virtual = true;
    new.entities[0].1.def = None;
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.pairs.len(), 0);
    assert_eq!(
        to_value(result.entities).unwrap(),
        json!([described("removed","f",None,false),
 {"change":"added","new":"f","name":"f","kind":"rust:function","role":"callable","virtual":true}])
    );
}

#[test]
fn a_parent_rekey_preserves_the_child_parent_property() {
    let mut old = empty();
    let mut child = entity("child", "ra m p 1 child.");
    child.parent = Some("old".into());
    old.entities = vec![(1, entity("old", "ra m p 1 parent/")), (2, child)];
    let mut new = old.clone();
    new.entities[0].1.key = "new".into();
    new.entities[1].1.parent = Some("new".into());
    assert_eq!(
        to_value(compare(&old, &new).unwrap().entities).unwrap(),
        json!([{"change":"rekeyed","old":"old","new":"new"}])
    );
}

#[test]
fn multiple_versions_and_exact_symbols_in_different_tables_cannot_fall_back() {
    let mut old = empty();
    for (key, sym) in [
        ("f", "ra m p 1 f."),
        ("g", "ra m p 1 g."),
        ("g2", "ra m p 2 g."),
    ] {
        old.entities.push((1, entity(key, sym)));
    }
    let mut new = old.clone();
    new.entities[0].1.symbol = Some("ra m p 2 f.".into());
    old.relations.push((1, reference("g", "f")));
    new.relations = old.relations.clone();
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(result.entities).unwrap(),
        json!([
            described("removed", "f", None, false),
            described("added", "f", None, false)
        ])
    );
    let source = json!({"old":"g","new":"g"});
    assert_eq!(
        to_value(result.relations).unwrap(),
        json!([
            ref_change("removed", source.clone(), json!({"old":"f"}), "to"),
            ref_change("added", source, json!({"new":"f"}), "to")
        ])
    );
    new.entities.remove(0);
    new.externals.push((1,serde_json::from_value(json!({"key":"f","symbol":"ra m p 1 f.","package":{"name":"p","manager":"m","version":"1"},"src":"scip:ra"})).unwrap()));
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.entities.len(), 1);
    assert_eq!(result.externals.len(), 1);
    assert_eq!(result.relations.len(), 2);
    assert_eq!(result.manifest.packages_added, ["m:p"]);
}

#[test]
fn schemes_and_unnamed_ancestors_follow_paired_children() {
    let mut old = empty();
    let mut container = entity("container", "ra m p 1 c/");
    container.r#virtual = true;
    container.def = None;
    container.symbol = None;
    let mut leaf = entity("K", "b m p a f.");
    leaf.parent = Some("container".into());
    old.entities = vec![(1, container), (2, leaf)];
    let mut new = old.clone();
    new.entities[1].1.key = "K@a".into();
    new.entities.push((3, entity("K", "a m p a f.")));
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.pairs.get("K").unwrap(), "K@a");
    assert_eq!(result.pairs.get("container").unwrap(), "container");
    assert_eq!(result.entities.len(), 2);
    new.entities.remove(1);
    new.entities[1].1.parent = Some("container".into());
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.entities.len(), 4);
    assert_eq!(result.pairs.len(), 0);
    // A paired child under two named virtual parents does not override the parents' symbols.
    new.entities[1].1.symbol = old.entities[1].1.symbol.clone();
    old.entities[0].1.symbol = Some("a m p a c/".into());
    new.entities[0].1.symbol = Some("b m p a c/".into());
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.pairs.len(), 1);
    assert_eq!(
        to_value(&result.entities)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn member_pairing_requires_equal_file_digest_kind_and_range() {
    let mut old = empty();
    let mut group = entity("group", "ra m p 1 group.");
    group.group = true;
    group.def = None;
    group.kind = None;
    old.entities.push((1, group));
    let member = |key: &str, line| {
        let mut e = entity(key, "ra m p 1 group.");
        e.member_of = Some("group".into());
        e.def.as_mut().unwrap().name = gosanja_schema::position::Range([line, 0, line, 1]);
        e
    };
    old.entities.push((2, member("one", 0)));
    old.entities.push((3, member("two", 1)));
    old.relations.push((1, reference("two", "group")));
    old.externals.push((1, external("external", "1")));
    let unchanged = compare(&old, &old).unwrap();
    assert_eq!(unchanged.entities, []);
    assert_eq!(unchanged.relations, []);
    assert_eq!(unchanged.externals, []);
    assert_eq!(unchanged.candidates, []);
    let mut new = old.clone();
    new.entities[1].1 = member("two", 1);
    new.entities[2].1 = member("three", 2);
    for (_, f) in &mut new.files {
        f.sha256.make_ascii_uppercase();
    }
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.entities.len(), 2);
    assert_eq!(result.pairs.get("two").unwrap(), "two");
    assert_eq!(result.relations, []);
    let file = &mut new
        .files
        .iter_mut()
        .find(|(_, f)| f.path == "src/lib.rs")
        .unwrap()
        .1;
    file.sha256 = "0".repeat(64);
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.entities.len(), 4);
    assert_eq!(result.relations.len(), 2);
}

#[test]
fn asymmetric_layers_filter_their_own_properties_relations_and_package_targets() {
    use gosanja_schema::snapshot::LayerState::{Absent, Present};
    let mut old = empty();
    old.entities.push((1, entity("a", "ra cargo p 1 a.")));
    let mut new = old.clone();
    new.externals.push((1,serde_json::from_value(json!({"key":"ext","symbol":"ra cargo dep 1 x.","package":{"name":"dep","manager":"cargo","version":"1"},"src":"scip:ra"})).unwrap()));
    let mut r = reference("a", "ext");
    r.src = "adapter:rust/syntax".into();
    new.relations.push((1, r.clone()));
    r.rel = gosanja_schema::snapshot::Rel::Call;
    r.roles = None;
    r.mechanism = Some(gosanja_schema::snapshot::Mechanism::Static);
    r.resolution = Some(gosanja_schema::snapshot::Resolution::Direct);
    r.src = "adapter:rust/calls".into();
    new.relations.push((2, r));
    let scope = &mut old.manifest.as_mut().unwrap().scopes[0];
    scope.layers.syntax = Absent;
    scope.layers.calls = Present;
    let scope = &mut new.manifest.as_mut().unwrap().scopes[0];
    scope.layers.syntax = Present;
    scope.layers.calls = Present;
    scope
        .builds
        .push(serde_json::from_value(json!({"id":"debug"})).unwrap());
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(result.manifest.not_compared).unwrap(),
        json!(["calls", "syntax"])
    );
    assert_eq!(result.relations, []);
    assert_eq!(result.manifest.packages_added.len(), 0);
    old.manifest.as_mut().unwrap().scopes[0].layers.calls = Absent;
    new.manifest.as_mut().unwrap().scopes[0].builds.clear();
    assert_eq!(compare(&old, &new).unwrap().relations, []);
    old.manifest.as_mut().unwrap().scopes = new.manifest.as_ref().unwrap().scopes.clone();
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.manifest.not_compared, []);
    let source = json!({"old":"a","new":"a"});
    let target = json!({"new":"ext"});
    assert_eq!(
        to_value(result.relations).unwrap(),
        json!([
            {"change":"added","from":source,"to":target,"endpoint":"to","rel":"call",
             "mechanism":"static","resolution":"direct","src":"adapter:rust/calls"},
            ref_change("added",source,target,"to")
        ])
    );
    assert_eq!(result.manifest.packages_added, ["cargo:dep"]);
}

#[test]
fn exact_symbols_pair_before_version_fallback() {
    let mut old = empty();
    old.entities.push((1, entity("f", "ra cargo p 1 f().")));
    old.relations.push((1, reference("f", "f")));
    let mut new = old.clone();
    new.entities[0].1.key = "f@1".into();
    new.entities.push((2, entity("f@2", "ra cargo p 2 f().")));
    new.relations[0].1 = reference("f@1", "f@1");
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.entities.len(), 2);
    assert_eq!(
        to_value(&result.entities[0]).unwrap(),
        json!({"change":"rekeyed","old":"f","new":"f@1"})
    );
    assert_eq!(result.relations, []);
    new.entities.pop();
    new.entities[0].1.symbol = Some("ra cargo p 3 f().".into());
    assert_eq!(
        to_value(compare(&old, &new).unwrap().entities).unwrap(),
        json!([{"change":"rekeyed","old":"f","new":"f@1"}])
    );
}

#[test]
fn differing_properties_preserve_positions_and_text_as_specified() {
    let mut old = empty();
    old.entities.push((1, entity("f", "ra cargo p 1 f().")));
    old.syntax.push((1,serde_json::from_value(json!({"key":"f","src":"adapter:rust/syntax","attributes":[r#"#[path = "/a/one.rs"]"#]})).unwrap()));
    let mut new = old.clone();
    new.entities[0].1.def.as_mut().unwrap().body =
        Some(gosanja_schema::position::Range([3, 0, 4, 0]));
    new.entities[0].1.def.as_mut().unwrap().name = gosanja_schema::position::Range([3, 0, 3, 1]);
    assert_eq!(compare(&old, &new).unwrap().entities, []);
    new.entities[0].1.def.as_mut().unwrap().body =
        Some(gosanja_schema::position::Range([3, 0, 5, 0]));
    new.syntax[0].1.attributes = Some(vec![r#"#[path = "/a/two.rs"]"#.into()]);
    let rows = to_value(compare(&old, &new).unwrap().entities).unwrap();
    assert_eq!(
        rows[0]["properties"],
        json!({"lines":{"old":1,"new":2},"attributes":{"old":[r#"#[path = "/a/one.rs"]"#],"new":[r#"#[path = "/a/two.rs"]"#]}})
    );
}

#[test]
fn relation_identity_includes_roles_and_call_discriminators() {
    let mut old = empty();
    old.entities.push((1, entity("f", "ra cargo p 1 f().")));
    old.relations.push((1, reference("f", "f")));
    let mut write = reference("f", "f");
    write.roles = Some(vec![gosanja_schema::snapshot::RefRole::Write]);
    old.relations.push((2, write));
    old.relations.push((3,serde_json::from_value(json!({"from":"f","to":"f","rel":"call","mechanism":"static","resolution":"direct","src":"adapter:rust/calls","evidence":[]})).unwrap()));
    let mut new = old.clone();
    new.relations.remove(1);
    new.relations[1].1.resolution = Some(gosanja_schema::snapshot::Resolution::Declared);
    let rows = to_value(compare(&old, &new).unwrap().relations).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for (rel, change, fields) in [
        ("ref", "removed", json!({"roles":["write"]})),
        (
            "call",
            "removed",
            json!({"mechanism":"static","resolution":"direct","src":"adapter:rust/calls"}),
        ),
        (
            "call",
            "added",
            json!({"mechanism":"static","resolution":"declared","src":"adapter:rust/calls"}),
        ),
    ] {
        let r = rows
            .iter()
            .find(|r| r["rel"] == rel && r["change"] == change)
            .unwrap();
        for (k, v) in fields.as_object().unwrap() {
            assert_eq!(&r[k], v);
        }
    }
}

#[test]
fn every_property_is_compared_only_when_observed_on_both_sides_as_required() {
    let mut old = empty();
    let mut f = entity("f", "ra m p 1 f().");
    f.parent = Some("module".into());
    old.entities.push((1, f));
    old.entities.push((2, entity("module", "ra m p 1 m/")));
    old.syntax.push((
        1,
        serde_json::from_value(
            json!({"key":"f","src":"adapter:rust/syntax","visibility":"pub","qualifiers":["async"],
        "outline":[{"k":"branch","range":[0,0,1,0],"children":[{"k":"arm","range":[0,0,1,0]}]}]}),
        )
        .unwrap(),
    ));
    let mut new = old.clone();
    let f = &mut new.entities[0].1;
    f.kind = Some("rust:method".into());
    f.role = Some(gosanja_schema::snapshot::Role::Value);
    f.parent = None;
    let def = f.def.as_mut().unwrap();
    def.file = "src/new.rs".into();
    def.body_src = Some("adapter:rust/syntax".into());
    def.body = Some(gosanja_schema::position::Range([0, 0, 4, 1]));
    let syntax = &mut new.syntax[0].1;
    syntax.visibility = None;
    syntax.qualifiers = Some(vec!["const".into()]);
    syntax.outline = Some(vec![]);
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(&result.entities[0]).unwrap()["properties"],
        json!({
          "kind":{"old":"rust:function","new":"rust:method"},"role":{"old":"callable","new":"value"},
          "parent":{"old":"module"},"file":{"old":"src/lib.rs","new":"src/new.rs"},
          "qualifiers":{"old":["async"],"new":["const"]},"visibility":{"old":"pub"},
          "outline":{"old":{"arm":1,"branch":1},"new":{}}
        })
    );
    new.syntax.clear();
    let row = to_value(&compare(&old, &new).unwrap().entities[0]).unwrap();
    assert_eq!(row["properties"].as_object().unwrap().len(), 4);
}

#[test]
fn overlapping_versions_with_swapped_variants_never_pair() {
    let mut old = empty();
    let mut group = entity("g", "ra m p 2 f.");
    group.group = true;
    group.def = None;
    group.kind = None;
    let mut member = entity("g1", "ra m p 2 f.");
    member.member_of = Some("g".into());
    let second = EntityRow {
        key: "g2".into(),
        ..member.clone()
    };
    old.entities = vec![
        (1, entity("f", "ra m p 1 f.")),
        (2, group),
        (3, member),
        (4, second),
    ];
    let mut new = old.clone();
    new.entities[0].1.symbol = Some("ra m p 2 f.".into());
    new.entities[1].1.symbol = Some("ra m p 1 f.".into());
    new.entities[2].1.symbol = new.entities[1].1.symbol.clone();
    new.entities[3].1.symbol = new.entities[1].1.symbol.clone();
    old.relations.push((1, reference("f", "g")));
    new.relations = old.relations.clone();
    let result = compare(&old, &new).unwrap();
    let mut expected = vec![];
    for change in ["removed", "added"] {
        expected.extend([
            described(change, "f", None, false),
            described(change, "g", None, true),
            described(change, "g1", Some("g"), false),
            described(change, "g2", Some("g"), false),
        ]);
    }
    assert_eq!(to_value(result.entities).unwrap(), json!(expected));
    assert_eq!(
        to_value(result.relations).unwrap(),
        json!([
            ref_change("removed", json!({"old":"f"}), json!({"old":"g"}), "both"),
            ref_change("added", json!({"new":"f"}), json!({"new":"g"}), "both")
        ])
    );
}

#[test]
fn a_definition_that_becomes_a_group_keeps_no_reference_endpoint() {
    let mut old = empty();
    old.entities = vec![
        (1, entity("f", "ra m p 1 f.")),
        (2, entity("other", "ra m p 1 x.")),
    ];
    old.relations = vec![(1, reference("other", "f")), (2, reference("f", "other"))];
    let mut new = old.clone();
    new.entities[0].1.group = true;
    new.entities[0].1.def = None;
    new.entities[0].1.kind = None;
    for key in ["f~1", "f~2"] {
        let mut member = entity(key, "ra m p 1 f.");
        member.member_of = Some("f".into());
        new.entities.push((3, member));
    }
    new.relations[1].1.from = "f~2".into();
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(result.entities).unwrap(),
        json!([
            described("removed", "f", None, false),
            described("added", "f", None, true),
            described("added", "f~1", Some("f"), false),
            described("added", "f~2", Some("f"), false)
        ])
    );
    let common = json!({"old":"other","new":"other"});
    assert_eq!(
        to_value(result.relations).unwrap(),
        json!([
            ref_change("removed", json!({"old":"f"}), common.clone(), "from"),
            ref_change("removed", common.clone(), json!({"old":"f"}), "to"),
            ref_change("added", common.clone(), json!({"new":"f"}), "to"),
            ref_change("added", json!({"new":"f~2"}), common, "from")
        ])
    );
}

#[test]
fn failed_and_absent_layers_have_the_same_coverage_and_build_sets_ignore_order() {
    use gosanja_schema::snapshot::LayerState::{Absent, Failed, Present};
    let mut old = empty();
    let mut new = old.clone();
    let old_scope = &mut old.manifest.as_mut().unwrap().scopes[0];
    old_scope.layers.syntax = Failed;
    old_scope.layers.calls = Absent;
    let new_scope = &mut new.manifest.as_mut().unwrap().scopes[0];
    new_scope.layers.syntax = Absent;
    new_scope.layers.calls = Failed;
    assert_eq!(compare(&old, &new).unwrap().manifest.not_compared, []);
    new.manifest.as_mut().unwrap().scopes[0].layers.calls = Present;
    assert_eq!(
        to_value(compare(&old, &new).unwrap().manifest.not_compared).unwrap(),
        json!(["calls"])
    );
    let a_scope = &mut old.manifest.as_mut().unwrap().scopes[0];
    a_scope.layers.calls = Present;
    let row = |name, profile| serde_json::from_value(json!({"id":name,"profile":profile})).unwrap();
    a_scope.builds = vec![row("a", "debug"), row("b", "release")];
    new.manifest.as_mut().unwrap().scopes[0].builds = vec![row("b", "release"), row("a", "debug")];
    assert_eq!(compare(&old, &new).unwrap().manifest.not_compared, []);
    new.manifest.as_mut().unwrap().scopes[0].builds.pop();
    assert_eq!(
        to_value(compare(&old, &new).unwrap().manifest.not_compared).unwrap(),
        json!(["calls"])
    );
}

#[test]
fn missing_file_observations_leave_rows_and_gap_counts() {
    let mut old = empty();
    old.entities.push((1, entity("f", "ra m p 1 f.")));
    let mut new = old.clone();
    new.entities.clear();
    new.gaps.push((1,serde_json::from_value(json!({"gap":"skipped-file","layer":"syntax","file":"src/lib.rs","code":"parse-failed","src":"adapter:rust/syntax"})).unwrap()));
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.manifest.counts.entities.removed, 1);
    assert_eq!(result.manifest.not_compared, []);
    assert_eq!(result.manifest.new.gaps["skipped-file"], 1);
}

#[test]
fn empty_span_on_a_single_line_still_covers_one_line() {
    let mut old = empty();
    let mut e = entity("f", "ra m p 1 f.");
    e.def.as_mut().unwrap().body = Some(gosanja_schema::position::Range([0, 0, 0, 0]));
    old.entities.push((1, e));
    let mut new = old.clone();
    new.entities[0].1.def.as_mut().unwrap().body =
        Some(gosanja_schema::position::Range([1, 0, 3, 0]));
    let result = to_value(compare(&old, &new).unwrap().entities).unwrap();
    assert_eq!(result[0]["properties"]["lines"], json!({"old":1,"new":2}));
}

#[test]
fn candidate_threshold_ties_and_missing_evidence() {
    use gosanja_schema::position::Range;
    let mut old = empty();
    let mut source = entity("old", "ra m p 1 old.");
    source.name = "x".into();
    source.def.as_mut().unwrap().body = Some(Range([0, 0, 1_000_000, 0]));
    old.entities.push((1, source));
    let mut new = empty();
    for (key, lines) in [
        ("d", 666_667),
        ("b", 666_667),
        ("a", 666_667),
        ("c", 666_667),
        ("below", 666_666),
    ] {
        let mut row = entity(key, &format!("ra m p 1 {key}."));
        row.name = "y".into();
        row.def.as_mut().unwrap().body = Some(Range([0, 0, lines, 0]));
        new.entities.push((1, row));
    }
    let result = compare(&old, &new).unwrap();
    let got: Vec<_> = result
        .candidates
        .iter()
        .map(|r| {
            (
                r.new.as_str(),
                r.rank,
                r.score,
                r.parents,
                r.outgoing,
                r.incoming,
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("a", 1, 440_000, 1_000_000, 0, 0),
            ("b", 2, 440_000, 1_000_000, 0, 0),
            ("c", 3, 440_000, 1_000_000, 0, 0)
        ]
    );
    new.entities.retain(|(_, e)| {
        e.name == "y" && e.def.as_ref().unwrap().body == Some(Range([0, 0, 666_666, 0]))
    });
    assert_eq!(compare(&old, &new).unwrap().candidates, []);
    new.entities[0].1.name = "x".into();
    new.entities[0].1.def.as_mut().unwrap().body_src = Some("adapter:rust/syntax".into());
    assert_eq!(compare(&old, &new).unwrap().candidates[0].size, 0);
}

#[test]
fn only_unpaired_definitions_with_the_same_known_kind_compete() {
    let mut old = empty();
    old.entities.push((1, entity("gone", "ra m p 1 gone.")));
    let mut new = empty();
    for variant in [
        "ok",
        "group",
        "member",
        "virtual",
        "unknown-kind",
        "other-kind",
    ] {
        let mut row = entity(variant, &format!("ra m p 1 `{variant}`."));
        match variant {
            "group" => {
                row.group = true;
                row.def = None;
            }
            "member" => row.member_of = Some("group".into()),
            "virtual" => {
                row.r#virtual = true;
                row.def = None;
            }
            "unknown-kind" => row.kind = None,
            "other-kind" => row.kind = Some("rust:struct".into()),
            _ => {}
        }
        new.entities.push((1, row));
    }
    let forward = compare(&old, &new).unwrap();
    assert_eq!(forward.candidates.len(), 1);
    assert_eq!(forward.candidates[0].new, "ok");
    let backward = compare(&new, &old).unwrap();
    assert_eq!(backward.candidates.len(), 1);
    assert_eq!(backward.candidates[0].old, "ok");
}

#[test]
fn candidate_neighbors_count_rows_once_and_use_only_compared_paired_endpoints() {
    let mut old = empty();
    old.entities.push((1, entity("old", "ra m p 1 old.")));
    old.entities.push((2, entity("paired", "ra m p 1 common.")));
    let mut new = empty();
    new.entities.push((1, entity("new", "ra m p 1 new.")));
    new.entities
        .push((2, entity("paired-new", "ra m p 1 common.")));
    let mut one = reference("old", "paired");
    one.sites = Some(vec![
        gosanja_schema::position::Site(
            "src/lib.rs".into(),
            0,
            0,
            0,
            1
        );
        5
    ]);
    let mut two = one.clone();
    two.roles = Some(vec![gosanja_schema::snapshot::RefRole::Write]);
    old.relations = vec![
        (1, one),
        (2, two),
        (3, reference("paired", "old")),
        (4, reference("old", "old")),
    ];
    new.relations = vec![
        (1, reference("new", "paired-new")),
        (2, reference("paired-new", "new")),
    ];
    let mut call = reference("paired", "old");
    call.src = "adapter:rust/syntax".into();
    old.relations.push((5, call));
    old.manifest.as_mut().unwrap().scopes[0].layers.syntax =
        gosanja_schema::snapshot::LayerState::Present;
    new.manifest.as_mut().unwrap().scopes[0].layers.syntax =
        gosanja_schema::snapshot::LayerState::Absent;
    let result = compare(&old, &new).unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(
        (result.candidates[0].outgoing, result.candidates[0].incoming),
        (500_000, 1_000_000)
    );
}

#[test]
fn absent_calls_ignore_build_metadata() {
    let mut old = empty();
    old.manifest.as_mut().unwrap().scopes[0].layers.calls =
        gosanja_schema::snapshot::LayerState::Absent;
    let mut new = old.clone();
    new.manifest.as_mut().unwrap().scopes[0]
        .builds
        .push(serde_json::from_value(json!({"id":"different"})).unwrap());
    assert_eq!(compare(&old, &new).unwrap().manifest.not_compared, []);
}

#[test]
fn member_children_establish_their_unnamed_ancestor_chain() {
    let mut old = empty();
    let mut outer = entity("outer", "ra m p 1 o/");
    outer.symbol = None;
    outer.def = None;
    outer.r#virtual = true;
    let inner = EntityRow {
        key: "inner".into(),
        parent: Some("outer".into()),
        ..outer.clone()
    };
    let mut group = entity("group", "ra m p 1 g.");
    group.group = true;
    group.def = None;
    group.kind = None;
    let mut member = entity("member", "ra m p 1 g.");
    member.member_of = Some("group".into());
    member.parent = Some("inner".into());
    old.entities = vec![(1, outer), (2, inner), (3, group), (4, member)];
    let result = compare(&old, &old).unwrap();
    assert_eq!(result.entities, []);
    assert_eq!(result.externals, []);
    assert_eq!(result.relations, []);
    assert_eq!(result.candidates, []);
}

#[test]
fn syntax_excluded_even_when_both_sides_have_syntax_rows() {
    let mut old = empty();
    old.entities.push((1, entity("f", "ra m p 1 f.")));
    old.syntax.push((
        1,
        serde_json::from_value(json!({"key":"f","src":"adapter:rust/syntax","visibility":"pub"}))
            .unwrap(),
    ));
    old.manifest.as_mut().unwrap().scopes[0].layers.syntax =
        gosanja_schema::snapshot::LayerState::Present;
    let mut new = old.clone();
    new.syntax[0].1.visibility = None;
    new.manifest.as_mut().unwrap().scopes[0].layers.syntax =
        gosanja_schema::snapshot::LayerState::Failed;
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(result.manifest.not_compared).unwrap(),
        json!(["syntax"])
    );
    assert_eq!(result.entities, []);
}

#[test]
fn scheme_suffix_and_version_suffix_do_not_establish_identity() {
    let mut old = empty();
    old.entities = vec![
        (1, entity("K", "b m p a f.")),
        (2, entity("K@b", "b m p b f.")),
    ];
    let mut new = empty();
    new.entities = vec![
        (1, entity("K", "a m p a f.")),
        (2, entity("K@b", "b m p a f.")),
    ];
    let result = compare(&old, &new).unwrap();
    assert_eq!(
        to_value(result.entities).unwrap(),
        json!([
 {"change":"rekeyed","old":"K","new":"K@b"},described("removed","K@b",None,false),described("added","K",None,false)])
    );
}
