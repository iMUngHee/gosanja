//! Pair rows only when the snapshots establish one partner.
use crate::validate::ReadSnapshot;
use crate::validate::SymbolIndex;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Variant {
    Definition,
    Virtual,
    Group,
    External,
}

type Rows<'a> = Vec<(&'a str, Variant)>;
type Identity = (String, String, Variant);

#[derive(Default)]
struct Symbols<'a> {
    exact: BTreeMap<&'a str, Rows<'a>>,
    versions: BTreeMap<(String, String), BTreeSet<String>>,
}

impl<'a> Symbols<'a> {
    fn of(snapshot: &'a ReadSnapshot, parsed: &SymbolIndex) -> Self {
        let mut index = Self::default();
        let entities = snapshot.entities.iter().filter_map(|(_, entity)| {
            Some((
                entity.symbol.as_deref()?,
                entity.key.as_str(),
                if entity.group {
                    Variant::Group
                } else if entity.r#virtual {
                    Variant::Virtual
                } else {
                    Variant::Definition
                },
                entity.member_of.is_some(),
            ))
        });
        for (raw, key, variant, member) in
            entities.chain(snapshot.externals.iter().map(|(_, external)| {
                (
                    external.symbol.as_str(),
                    external.key.as_str(),
                    Variant::External,
                    false,
                )
            }))
        {
            let parts = parsed
                .get(raw)
                .expect("validation indexed every non-local symbol");
            index
                .versions
                .entry(parts.package.clone())
                .or_default()
                .insert(parts.version.clone());
            if !member {
                index.exact.entry(raw).or_default().push((key, variant));
            }
        }
        index
    }

    fn moved(&self, other: &Self, parsed: &SymbolIndex) -> BTreeMap<Identity, Vec<&'a str>> {
        let mut buckets = BTreeMap::<_, Vec<_>>::new();
        for (&raw, rows) in &self.exact {
            if other.exact.contains_key(raw) {
                continue;
            }
            let parts = parsed
                .get(raw)
                .expect("validation indexed every non-local symbol");
            if ![self, other].iter().all(|index| {
                index
                    .versions
                    .get(&parts.package)
                    .is_some_and(|versions| versions.len() == 1)
            }) {
                continue;
            }
            for &(key, variant) in rows {
                buckets
                    .entry((parts.scheme.clone(), parts.base_key.clone(), variant))
                    .or_default()
                    .push(key);
            }
        }
        buckets
    }
}

pub(super) fn pairs<'snapshot>(
    old_snapshot: &'snapshot ReadSnapshot,
    new_snapshot: &'snapshot ReadSnapshot,
    old_index: &SymbolIndex,
    new_index: &SymbolIndex,
) -> BTreeMap<&'snapshot str, &'snapshot str> {
    let (old_symbols, new_symbols) = (
        Symbols::of(old_snapshot, old_index),
        Symbols::of(new_snapshot, new_index),
    );
    let mut pairs = BTreeMap::new();
    for (raw, left) in &old_symbols.exact {
        if let ([(old_key, variant)], Some([(new_key, other_variant)])) = (
            left.as_slice(),
            new_symbols.exact.get(raw).map(Vec::as_slice),
        ) && variant == other_variant
        {
            pairs.insert(*old_key, *new_key);
        }
    }
    let right = new_symbols.moved(&old_symbols, new_index);
    for (identity, left) in old_symbols.moved(&new_symbols, old_index) {
        if let ([old_key], Some([new_key])) =
            (left.as_slice(), right.get(&identity).map(Vec::as_slice))
        {
            pairs.insert(*old_key, *new_key);
        }
    }
    let files = |snapshot: &'snapshot ReadSnapshot| {
        snapshot
            .files
            .iter()
            .map(|(_, file)| (file.path.as_str(), file.sha256.as_str()))
            .collect::<BTreeMap<_, _>>()
    };
    let (old_files, new_files) = (files(old_snapshot), files(new_snapshot));
    let unchanged: BTreeSet<_> = old_files
        .iter()
        .filter(|(path, sha)| {
            new_files
                .get(**path)
                .is_some_and(|digest| sha.eq_ignore_ascii_case(digest))
        })
        .map(|(&path, _)| path)
        .collect();
    let member_buckets = |snapshot: &'snapshot ReadSnapshot, old_side: bool| {
        let mut buckets = BTreeMap::<_, Vec<_>>::new();
        for (_, entity) in &snapshot.entities {
            if let (Some(group), Some(def)) = (entity.member_of.as_deref(), entity.def.as_ref())
                && unchanged.contains(def.file.as_str())
            {
                let paired_group = if old_side {
                    pairs.get(group).copied()
                } else {
                    Some(group)
                };
                if let Some(paired_group) = paired_group {
                    buckets
                        .entry((
                            paired_group,
                            def.file.as_str(),
                            entity.kind.as_deref(),
                            def.name.0,
                        ))
                        .or_default()
                        .push(entity.key.as_str());
                }
            }
        }
        buckets
    };
    let right = member_buckets(new_snapshot, false);
    for (id, left) in member_buckets(old_snapshot, true) {
        if let ([old_key], Some([new_key])) = (left.as_slice(), right.get(&id).map(Vec::as_slice)) {
            pairs.insert(*old_key, *new_key);
        }
    }
    let entities = |snapshot: &'snapshot ReadSnapshot| {
        snapshot
            .entities
            .iter()
            .map(|(_, entity)| (entity.key.as_str(), entity))
            .collect::<BTreeMap<_, _>>()
    };
    let (old, new) = (entities(old_snapshot), entities(new_snapshot));
    let mut work: Vec<_> = pairs
        .iter()
        .map(|(&old_key, &new_key)| (old_key, new_key))
        .collect();
    while let Some((old_key, new_key)) = work.pop() {
        if let (Some(parent), Some(other)) = (
            old.get(old_key).and_then(|entity| entity.parent.as_deref()),
            new.get(new_key).and_then(|entity| entity.parent.as_deref()),
        ) && parent == other
            && !pairs.contains_key(parent)
            && [old.get(parent), new.get(other)]
                .iter()
                .all(|row| row.is_some_and(|entity| entity.r#virtual && entity.symbol.is_none()))
        {
            pairs.insert(parent, other);
            work.push((parent, other));
        }
    }
    pairs
}
