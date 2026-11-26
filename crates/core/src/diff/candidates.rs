//! Rank alternatives for each removed definition independently.
use super::{
    entities::lines,
    relations::EffectiveRelations,
    similarity::{multiset, sequence},
};
use crate::validate::ReadSnapshot;
use gosanja_schema::{
    diff::{CandidateRow, RelationKind, Weights},
    snapshot::{EntityRow, Rel},
};
use std::collections::BTreeMap;

type Neighbors<'a> = BTreeMap<(Rel, &'a str), u64>;

struct Definition<'a> {
    key: &'a str,
    kind: &'a str,
    name: Vec<char>,
    parents: Vec<&'a str>,
    file: &'a str,
    body: Option<(&'a str, u64)>,
    outgoing: Neighbors<'a>,
    incoming: Neighbors<'a>,
}

fn definitions<'a>(
    snapshot: &'a ReadSnapshot,
    mapped: &BTreeMap<&'a str, &'a str>,
    old_side: bool,
    effective: &EffectiveRelations<'a>,
) -> Vec<Definition<'a>> {
    let entities: BTreeMap<_, _> = snapshot
        .entities
        .iter()
        .map(|(_, entity)| (entity.key.as_str(), entity))
        .collect();
    let mut definitions = BTreeMap::new();
    for (&key, entity) in &entities {
        if mapped.contains_key(key) || entity.member_of.is_some() {
            continue;
        }
        let (Some(def), Some(kind)) = (&entity.def, entity.kind.as_deref()) else {
            continue;
        };
        let mut parents = vec![];
        let mut current: &EntityRow = entity;
        while let Some(parent) = current.parent.as_deref().and_then(|key| entities.get(key)) {
            parents.push(parent.name.as_str());
            current = parent;
        }
        parents.reverse();
        definitions.insert(
            key,
            Definition {
                key,
                kind,
                name: entity.name.chars().collect(),
                parents,
                file: &def.file,
                body: def.body_src.as_deref().zip(lines(def)),
                outgoing: BTreeMap::new(),
                incoming: BTreeMap::new(),
            },
        );
    }
    let canonical = |key: &'a str| {
        mapped
            .get(key)
            .map(|&partner| if old_side { partner } else { key })
    };
    for (&(from, to, ref kind), &count) in effective {
        let rel = match kind {
            RelationKind::Ref { .. } => Rel::Ref,
            RelationKind::Call { .. } => Rel::Call,
            RelationKind::Implements => Rel::Implements,
            RelationKind::ImplSelfType => Rel::ImplSelfType,
            RelationKind::ImplTrait => Rel::ImplTrait,
        };
        if let Some(target) = canonical(to)
            && let Some(from) = definitions.get_mut(from)
        {
            *from.outgoing.entry((rel, target)).or_default() += count;
        }
        if let Some(source) = canonical(from)
            && let Some(to) = definitions.get_mut(to)
        {
            *to.incoming.entry((rel, source)).or_default() += count;
        }
    }
    definitions.into_values().collect()
}

pub(super) fn rank<'a>(
    old: &'a ReadSnapshot,
    new: &'a ReadSnapshot,
    pairs: &BTreeMap<&'a str, &'a str>,
    reverse: &BTreeMap<&'a str, &'a str>,
    effective: (&EffectiveRelations<'a>, &EffectiveRelations<'a>),
    weights: Weights,
) -> Vec<CandidateRow> {
    let mut by_kind = BTreeMap::<_, Vec<_>>::new();
    for row in definitions(new, reverse, false, effective.1) {
        by_kind.entry(row.kind).or_default().push(row);
    }
    let mut output = Vec::new();
    for left in definitions(old, pairs, true, effective.0) {
        let Some(added) = by_kind.get(left.kind) else {
            continue;
        };
        let mut best: Vec<CandidateRow> = Vec::with_capacity(3);
        for right in added {
            let name = sequence(&left.name, &right.name);
            let file = u32::from(left.file == right.file) * 1_000_000;
            let parents = sequence(&left.parents, &right.parents);
            let size = match (left.body, right.body) {
                (Some((old_source, old_lines)), Some((new_source, new_lines)))
                    if old_source == new_source =>
                {
                    u32::try_from(old_lines.min(new_lines) * 1_000_000 / old_lines.max(new_lines))
                        .expect("ratio at most one million")
                }
                _ => 0,
            };
            let outgoing = multiset(&left.outgoing, &right.outgoing);
            let incoming = multiset(&left.incoming, &right.incoming);
            let score = (weights.name * name
                + weights.file * file
                + weights.parents * parents
                + weights.size * size
                + weights.outgoing * outgoing
                + weights.incoming * incoming)
                / 100;
            if score < weights.threshold {
                continue;
            }
            // The added rows are in key order, so earlier equal scores win the tie.
            let at = best.partition_point(|candidate| candidate.score >= score);
            if at < 3 {
                if best.len() == 3 {
                    best.pop();
                }
                best.insert(
                    at,
                    CandidateRow {
                        old: left.key.into(),
                        new: right.key.into(),
                        rank: 0,
                        score,
                        name,
                        file,
                        parents,
                        size,
                        outgoing,
                        incoming,
                    },
                );
            }
        }
        for (rank, mut row) in (1..).zip(best) {
            row.rank = rank;
            output.push(row);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::tests::{empty, entity};

    #[test]
    fn candidate_features_use_the_outermost_parent_first_and_unicode_scalars() {
        let mut old = empty();
        let mut outer = entity("outer", "ra m p 1 outer/");
        outer.name = "library".into();
        let mut inner = entity("inner", "ra m p 1 inner/");
        inner.name = "reader".into();
        inner.parent = Some("outer".into());
        let mut leaf = entity("leaf", "ra m p 1 leaf.");
        leaf.name = "café".into();
        leaf.parent = Some("inner".into());
        old.entities = vec![(1, outer), (2, inner), (3, leaf)];
        let mut new = old.clone();
        new.entities[0].1.name = "application".into();
        new.entities[2].1.key = "renamed".into();
        new.entities[2].1.symbol = Some("ra m p 1 renamed.".into());
        new.entities[2].1.name = "cafè".into();
        let paired = BTreeMap::from([("outer", "outer"), ("inner", "inner")]);
        let effective = EffectiveRelations::new();
        let removed = definitions(&old, &paired, true, &effective);
        assert_eq!(removed[0].parents, ["library", "reader"]);
        let rows = rank(
            &old,
            &new,
            &paired,
            &paired,
            (&effective, &effective),
            gosanja_schema::diff::WEIGHTS,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].parents, rows[0].name), (500_000, 750_000));
    }
}
