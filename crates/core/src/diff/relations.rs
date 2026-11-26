//! Compare relation identities and the packages reached by their targets.
use super::externals::package;
use crate::validate::{ReadSnapshot, SymbolIndex};
use gosanja_schema::diff::{Change, Endpoint, Excluded, RelationKind, RelationRow, Sides};
use gosanja_schema::snapshot::{RefRole, Rel};
use std::collections::{BTreeMap, BTreeSet};

/// Whether a snapshot relation's source layer can contribute to the comparison.
#[must_use]
fn compared(src: &str, excluded: &[Excluded]) -> bool {
    !src.strip_prefix("adapter:")
        .and_then(|adapter| adapter.rsplit_once('/'))
        .is_some_and(|(_, layer)| match layer {
            "syntax" => excluded.contains(&Excluded::Syntax),
            "calls" => excluded.contains(&Excluded::Calls),
            _ => false,
        })
}

pub(super) type EffectiveRelations<'a> = BTreeMap<(&'a str, &'a str, RelationKind), u64>;

/// Select relations and recover the base reference buckets when syntax is excluded.
pub(super) fn effective<'a>(
    snapshot: &'a ReadSnapshot,
    excluded: &[Excluded],
) -> EffectiveRelations<'a> {
    let mut rows = BTreeMap::new();
    let without_syntax = excluded.contains(&Excluded::Syntax);
    for (_, relation) in &snapshot.relations {
        if !compared(&relation.src, excluded) {
            continue;
        }
        let key = |kind| (relation.from.as_str(), relation.to.as_str(), kind);
        let kind = match relation.rel {
            Rel::Ref => {
                let roles = relation
                    .roles
                    .as_ref()
                    .expect("validated reference has roles");
                if without_syntax && let Some(derived) = &relation.syntax_import_sites {
                    let base_roles = roles
                        .iter()
                        .copied()
                        .filter(|role| *role != RefRole::Import);
                    rows.insert(
                        key(RelationKind::Ref {
                            roles: base_roles.collect(),
                        }),
                        1,
                    );
                    if relation
                        .sites
                        .as_ref()
                        .expect("validated reference has sites")
                        .len()
                        == derived.len()
                    {
                        continue;
                    }
                }
                rows.insert(
                    key(RelationKind::Ref {
                        roles: roles.clone(),
                    }),
                    1,
                );
                continue;
            }
            Rel::Call => RelationKind::Call {
                mechanism: relation.mechanism.expect("validated call has mechanism"),
                resolution: relation.resolution.expect("validated call has resolution"),
                src: relation.src.clone(),
            },
            Rel::Implements => RelationKind::Implements,
            Rel::ImplSelfType => {
                rows.insert(key(RelationKind::ImplSelfType), 1);
                continue;
            }
            Rel::ImplTrait => {
                rows.insert(key(RelationKind::ImplTrait), 1);
                continue;
            }
            Rel::Unknown => unreachable!("validated snapshot has known relation kinds"),
        };
        *rows.entry(key(kind)).or_default() += 1;
    }
    rows
}

type Identity = (Sides<String>, Sides<String>, RelationKind);

pub(super) struct SideRelations {
    rows: BTreeSet<Identity>,
    pub packages: BTreeSet<String>,
}

impl SideRelations {
    pub fn new(
        snapshot: &ReadSnapshot,
        pairs: &BTreeMap<&str, &str>,
        old: bool,
        effective: &EffectiveRelations<'_>,
        symbols: &SymbolIndex,
    ) -> Self {
        let endpoint = |key: &str| {
            let partner = pairs.get(key).copied().map(str::to_owned);
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
        let externals: BTreeMap<_, _> = snapshot
            .externals
            .iter()
            .map(|(_, external)| (external.key.as_str(), external))
            .collect();
        let mut rows = BTreeSet::new();
        let mut packages = BTreeSet::new();
        for (from, to, kind) in effective.keys() {
            rows.insert((endpoint(from), endpoint(to), kind.clone()));
            if let Some(external) = externals.get(to) {
                packages.insert(package(&external.symbol, symbols));
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
            RelationKind::ImplSelfType => "impl-self-type",
            RelationKind::ImplTrait => "impl-trait",
        };
        (
            last(&relation.from.old),
            last(&relation.from.new),
            last(&relation.to.old),
            last(&relation.to.new),
            rel,
            serde_json::to_vec(relation).expect("diff relation rows serialize to JSON"),
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
