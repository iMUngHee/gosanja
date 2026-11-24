//! Report added, removed and rekeyed external symbols and their packages.
use crate::identity::escape_component;
use crate::validate::ReadSnapshot;
use gosanja_schema::diff::ExternalRow;
use std::collections::BTreeMap;

pub(super) fn changes(
    old_snapshot: &ReadSnapshot,
    new_snapshot: &ReadSnapshot,
    pairs: &BTreeMap<&str, &str>,
) -> Vec<ExternalRow> {
    let mut rows = Vec::new();
    let mut new: BTreeMap<_, _> = new_snapshot
        .externals
        .iter()
        .map(|(_, external)| (external.key.as_str(), external))
        .collect();
    let old: BTreeMap<_, _> = old_snapshot
        .externals
        .iter()
        .map(|(_, external)| (external.key.as_str(), external))
        .collect();
    for (key, external) in old {
        match pairs.get(key).and_then(|new_key| new.remove(new_key)) {
            None => rows.push(ExternalRow::Removed {
                old: key.into(),
                package: package(external),
            }),
            Some(paired) if paired.key != key => rows.push(ExternalRow::Rekeyed {
                old: key.into(),
                new: paired.key.clone(),
            }),
            Some(_) => {}
        }
    }
    rows.extend(new.into_iter().map(|(key, external)| ExternalRow::Added {
        new: key.into(),
        package: package(external),
    }));
    rows
}

pub(super) fn counts(rows: &[ExternalRow]) -> gosanja_schema::diff::ExternalCounts {
    let mut counts = gosanja_schema::diff::ExternalCounts::default();
    for external in rows {
        *match external {
            ExternalRow::Added { .. } => &mut counts.added,
            ExternalRow::Removed { .. } => &mut counts.removed,
            ExternalRow::Rekeyed { .. } => &mut counts.rekeyed,
        } += 1;
    }
    counts
}

pub(super) fn package(external: &gosanja_schema::snapshot::ExternalRow) -> String {
    format!(
        "{}:{}",
        escape_component(&external.package.manager),
        escape_component(&external.package.name)
    )
}
