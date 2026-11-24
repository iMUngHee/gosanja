use super::externals::package;
use crate::validate::ReadSnapshot;
use gosanja_schema::diff::{Change, Endpoint, Excluded, RelationKind, RelationRow, Sides};
use gosanja_schema::snapshot::Rel;
use std::collections::{BTreeMap, BTreeSet};

/// Whether a snapshot relation's source layer can contribute to the comparison.
#[must_use]
pub fn compared(src: &str, excluded: &[Excluded]) -> bool {
    !src.strip_prefix("adapter:")
        .and_then(|s| s.rsplit_once('/'))
        .is_some_and(|(_, layer)| match layer {
            "syntax" => excluded.contains(&Excluded::Syntax),
            "calls" => excluded.contains(&Excluded::Calls),
            _ => false,
        })
}

type Identity = (Sides<String>, Sides<String>, RelationKind);
pub(super) struct SideRelations {
    rows: BTreeSet<Identity>,
    pub packages: BTreeSet<String>,
}

impl SideRelations {
    pub fn new(
        s: &ReadSnapshot,
        pairs: &BTreeMap<&str, &str>,
        old: bool,
        excluded: &[Excluded],
    ) -> Self {
        let endpoint = |key: &str| {
            let partner = pairs.get(key).map(|s| (*s).to_owned());
            if old {
                Sides {
                    old: Some(key.into()),
                    new: partner,
                }
            } else {
                Sides {
                    old: partner,
                    new: Some(key.into()),
                }
            }
        };
        let externals: BTreeMap<_, _> = s
            .externals
            .iter()
            .map(|(_, external)| (external.key.as_str(), external))
            .collect();
        let mut rows = BTreeSet::new();
        let mut packages = BTreeSet::new();
        for (_, relation) in &s.relations {
            if !compared(&relation.src, excluded) {
                continue;
            }
            let kind = match relation.rel {
                Rel::Ref => RelationKind::Ref {
                    roles: relation
                        .roles
                        .clone()
                        .expect("validated reference has roles"),
                },
                Rel::Call => RelationKind::Call {
                    mechanism: relation.mechanism.expect("validated call has mechanism"),
                    resolution: relation.resolution.expect("validated call has resolution"),
                    src: relation.src.clone(),
                },
                Rel::Implements => RelationKind::Implements,
                Rel::Unknown => unreachable!("validated snapshot has known relation kinds"),
            };
            rows.insert((endpoint(&relation.from), endpoint(&relation.to), kind));
            if let Some(external) = externals.get(relation.to.as_str()) {
                packages.insert(package(external));
            }
        }
        Self { rows, packages }
    }

    pub fn differences<'a>(
        &'a self,
        other: &'a Self,
        change: Change,
    ) -> impl Iterator<Item = RelationRow> + 'a {
        self.rows
            .difference(&other.rows)
            .map(move |(from, to, kind)| {
                let unpaired = |key: &Sides<String>| key.old.is_none() || key.new.is_none();
                let endpoint = match (unpaired(from), unpaired(to)) {
                    (true, true) => Some(Endpoint::Both),
                    (true, false) => Some(Endpoint::From),
                    (false, true) => Some(Endpoint::To),
                    (false, false) => None,
                };
                RelationRow {
                    change,
                    from: from.clone(),
                    to: to.clone(),
                    endpoint,
                    kind: kind.clone(),
                }
            })
    }
}

pub(super) fn order(rows: &mut [RelationRow]) {
    rows.sort_by_cached_key(|relation| {
        let last = |key: &Option<String>| (key.is_none(), key.clone());
        let rel = match relation.kind {
            RelationKind::Call { .. } => "call",
            RelationKind::Ref { .. } => "ref",
            RelationKind::Implements => "implements",
        };
        (
            last(&relation.from.old),
            last(&relation.from.new),
            last(&relation.to.old),
            last(&relation.to.new),
            rel,
            serde_json::to_vec(relation).expect("serializable relation"),
        )
    });
}

pub(super) fn counts(rows: &[RelationRow]) -> gosanja_schema::diff::RelationCounts {
    let mut counts = gosanja_schema::diff::RelationCounts::default();
    for relation in rows {
        *match relation.change {
            Change::Added => &mut counts.added,
            Change::Removed => &mut counts.removed,
        } += 1;
    }
    counts
}
