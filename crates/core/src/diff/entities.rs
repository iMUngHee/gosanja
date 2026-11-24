use crate::validate::ReadSnapshot;
use gosanja_schema::{
    diff::{Description, EntityRow, Properties, Sides},
    snapshot::{self, Def, OutlineNode, SyntaxRow},
};
use std::collections::BTreeMap;

pub(super) fn lines(definition: &Def) -> Option<u64> {
    let [sl, _, el, ec] = definition.body?.0;
    u64::from(el)
        .checked_sub(u64::from(sl))?
        .checked_add(1)?
        .checked_sub(u64::from(ec == 0 && el > sl))
}

fn outline(nodes: &[OutlineNode]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    let mut stack: Vec<_> = nodes.iter().collect();
    while let Some(node) = stack.pop() {
        let name = serde_json::to_value(node.k).expect("outline kind is a string");
        *counts
            .entry(
                name.as_str()
                    .expect("serialized outline kind is a JSON string")
                    .to_owned(),
            )
            .or_default() += 1;
        stack.extend(node.children.iter().flatten());
    }
    counts
}

fn difference<T: PartialEq>(old: Option<T>, new: Option<T>) -> Option<Sides<T>> {
    (old != new).then_some(Sides { old, new })
}

fn properties(
    old: &snapshot::EntityRow,
    new: &snapshot::EntityRow,
    syntax: Option<(&SyntaxRow, &SyntaxRow)>,
    pairs: &BTreeMap<&str, &str>,
) -> Properties {
    let parents_equal = match (&old.parent, &new.parent) {
        (None, None) => true,
        (Some(old_key), Some(new_key)) => pairs.get(old_key.as_str()) == Some(&new_key.as_str()),
        _ => false,
    };
    let mut properties = Properties {
        parent: (!parents_equal).then(|| Sides {
            old: old.parent.clone(),
            new: new.parent.clone(),
        }),
        kind: difference(old.kind.clone(), new.kind.clone()),
        role: difference(old.role, new.role),
        file: difference(
            old.def.as_ref().map(|definition| definition.file.clone()),
            new.def.as_ref().map(|definition| definition.file.clone()),
        ),
        ..Properties::default()
    };
    if let (Some(left), Some(right)) = (&old.def, &new.def)
        && left.body_src == right.body_src
        && let Some((old_lines, new_lines)) = lines(left).zip(lines(right))
    {
        properties.lines = difference(Some(old_lines), Some(new_lines));
    }
    if let Some((left, right)) = syntax {
        properties.visibility = difference(left.visibility.clone(), right.visibility.clone());
        properties.qualifiers = difference(left.qualifiers.clone(), right.qualifiers.clone());
        properties.attributes = difference(left.attributes.clone(), right.attributes.clone());
        if let Some((old, new)) = left.outline.as_deref().zip(right.outline.as_deref()) {
            properties.outline = difference(Some(outline(old)), Some(outline(new)));
        }
    }
    properties
}

fn describe(entity: &snapshot::EntityRow) -> Description {
    Description {
        kind: entity.kind.clone(),
        role: entity.role,
        name: entity.name.clone(),
        parent: entity.parent.clone(),
        r#virtual: entity.r#virtual.then_some(true),
        group: entity.group.then_some(true),
        member_of: entity.member_of.clone(),
        file: entity
            .def
            .as_ref()
            .map(|definition| definition.file.clone()),
    }
}

pub(super) fn changes(
    old_snapshot: &ReadSnapshot,
    new_snapshot: &ReadSnapshot,
    pairs: &BTreeMap<&str, &str>,
    syntax: bool,
) -> Vec<EntityRow> {
    fn syntax_map(snapshot: &ReadSnapshot) -> BTreeMap<&str, &SyntaxRow> {
        snapshot
            .syntax
            .iter()
            .map(|(_, snapshot)| (snapshot.key.as_str(), snapshot))
            .collect()
    }
    let (old_syntax, new_syntax) = (syntax_map(old_snapshot), syntax_map(new_snapshot));
    let mut new: BTreeMap<_, _> = new_snapshot
        .entities
        .iter()
        .map(|(_, entity)| (entity.key.as_str(), entity))
        .collect();
    let old: BTreeMap<_, _> = old_snapshot
        .entities
        .iter()
        .map(|(_, entity)| (entity.key.as_str(), entity))
        .collect();
    let mut rows = Vec::new();
    for (key, entity) in old {
        let Some(other) = pairs.get(key).and_then(|new_key| new.remove(new_key)) else {
            rows.push(EntityRow::Removed {
                old: key.into(),
                description: describe(entity),
            });
            continue;
        };
        let new_key = other.key.as_str();
        let syntax_pair = syntax
            .then(|| {
                old_syntax
                    .get(key)
                    .copied()
                    .zip(new_syntax.get(new_key).copied())
            })
            .flatten();
        let changes = properties(entity, other, syntax_pair, pairs);
        if changes != Properties::default() {
            rows.push(EntityRow::Changed {
                old: key.into(),
                new: new_key.into(),
                properties: Box::new(changes),
            });
        } else if key != new_key {
            rows.push(EntityRow::Rekeyed {
                old: key.into(),
                new: new_key.into(),
            });
        }
    }
    rows.extend(new.into_iter().map(|(key, entity)| EntityRow::Added {
        new: key.into(),
        description: describe(entity),
    }));
    rows
}

pub(super) fn counts(rows: &[EntityRow]) -> gosanja_schema::diff::EntityCounts {
    let mut counts = gosanja_schema::diff::EntityCounts::default();
    for entity in rows {
        *match entity {
            EntityRow::Added { .. } => &mut counts.added,
            EntityRow::Removed { .. } => &mut counts.removed,
            EntityRow::Changed { .. } => &mut counts.changed,
            EntityRow::Rekeyed { .. } => &mut counts.rekeyed,
        } += 1;
    }
    counts
}
