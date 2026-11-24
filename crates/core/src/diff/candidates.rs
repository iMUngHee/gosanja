//! Rank alternatives for each removed definition independently.
use super::{
    compared,
    entities::lines,
    similarity::{multiset, sequence},
};
use crate::validate::ReadSnapshot;
use gosanja_schema::{
    diff::{CandidateRow, Excluded, Weights},
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
    excluded: &[Excluded],
) -> Vec<Definition<'a>> {
    let entities: BTreeMap<_, _> = snapshot
        .entities
        .iter()
        .map(|(_, entity)| (entity.key.as_str(), entity))
        .collect();
    let mut definitions = BTreeMap::new();
    for (&key, entity) in &entities {
        if mapped.contains_key(key)
            || entity.r#virtual
            || entity.group
            || entity.member_of.is_some()
        {
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
    for (_, relation) in &snapshot.relations {
        if !compared(&relation.src, excluded) {
            continue;
        }
        if let Some(target) = canonical(&relation.to)
            && let Some(from) = definitions.get_mut(relation.from.as_str())
        {
            *from.outgoing.entry((relation.rel, target)).or_default() += 1;
        }
        if let Some(source) = canonical(&relation.from)
            && let Some(to) = definitions.get_mut(relation.to.as_str())
        {
            *to.incoming.entry((relation.rel, source)).or_default() += 1;
        }
    }
    definitions.into_values().collect()
}

pub(super) fn rank<'a>(
    old: &'a ReadSnapshot,
    new: &'a ReadSnapshot,
    pairs: &BTreeMap<&'a str, &'a str>,
    reverse: &BTreeMap<&'a str, &'a str>,
    excluded: &[Excluded],
    weights: Weights,
) -> Vec<CandidateRow> {
    let mut by_kind = BTreeMap::<_, Vec<_>>::new();
    for row in definitions(new, reverse, false, excluded) {
        by_kind.entry(row.kind).or_default().push(row);
    }
    let mut output = Vec::new();
    for left in definitions(old, pairs, true, excluded) {
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
