//! Joins an adapter's build layer to the snapshot.
//!
//! Targets are joined to their root entities, file facts to the admitted documents, and test
//! evidence to entities. The rules are in `spec/adapter-contract.md` (build.jsonl, How a layer
//! is read) and `spec/snapshot.md`.

use std::collections::{BTreeMap, BTreeSet};

use gosanja_schema::{
    contract::{Anchor, BuildRow, FileFact, ItemTest, SkippedFile, TargetFact},
    snapshot::{
        EntityRow, EntityTest, FailedDetail, GapKind, GapRow, Layer, LayerState, TargetRoot,
        TargetRow,
    },
};

use crate::{anchor::Column, graph::Snapshot};

/// What the adapter's build layer gave, as the runner read it.
#[derive(Debug, Default)]
pub struct BuildInput {
    /// Rows of `build.jsonl`, each of which passed its schema.
    pub rows: Vec<BuildRow>,
    /// The build entry's `skipped`.
    pub skipped: Vec<SkippedFile>,
}

/// A key as `spec/adapter-contract.md` defines it: 1 to 1024 bytes of UTF-8, no whitespace or
/// control character, `%` only before two upper-case hex digits.
#[must_use]
pub fn is_target_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 1024 {
        return false;
    }
    let bytes = key.as_bytes();
    let mut i = 0;
    for c in key.chars() {
        let at = i;
        i += c.len_utf8();
        if c.is_whitespace() || c.is_control() {
            return false;
        }
        if c == '%'
            && !(bytes.len() >= at + 3
                && bytes[at + 1..at + 3]
                    .iter()
                    .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(b)))
        {
            return false;
        }
    }
    true
}

/// Applies the build layer to `snapshot` after the syntax layer and the fold, before calls add
/// files: its phases when `state` is `Present`, then every entity's `test`.
///
/// A discarded layer turns `state` into `Failed` and leaves only its `failed-layer` gap.
pub fn apply(
    snapshot: &mut Snapshot,
    state: &mut LayerState,
    input: BuildInput,
    tests: &BTreeMap<String, ItemTest>,
    adapter: &str,
    column: &Column<'_>,
) {
    let src = format!("adapter:{adapter}/build");
    if *state == LayerState::Present {
        let mut gaps = Vec::new();
        if read(snapshot, input, &src, column, &mut gaps).is_ok() {
            snapshot.gaps.extend(gaps);
        } else {
            *state = LayerState::Failed;
            snapshot.gaps.retain(|g| g.layer != Some(Layer::Build));
            let mut gap = GapRow::new(GapKind::FailedLayer, &src);
            gap.layer = Some(Layer::Build);
            gap.scope = Some(crate::graph::SCOPE.to_owned());
            gap.detail = Some(FailedDetail::RowInvalid);
            snapshot.gaps.push(gap);
        }
    }
    entity_tests(snapshot, tests);
}

struct Discard;

fn read(
    snapshot: &mut Snapshot,
    input: BuildInput,
    src: &str,
    column: &Column<'_>,
    gaps: &mut Vec<GapRow>,
) -> Result<(), Discard> {
    let mut targets = Vec::new();
    let mut files = Vec::new();
    for row in input.rows {
        match row {
            BuildRow::Target(target) => targets.push(target),
            BuildRow::File(file) => files.push(file),
        }
    }
    shape(&targets, &files)?;
    keys(&targets, &files, &input.skipped)?;
    let rows = roots(snapshot, targets, src, column, gaps);
    attach(snapshot, files, &input.skipped, src, gaps);
    snapshot.targets = rows;
    Ok(())
}

/// Phase 1, after each row passed its schema: sorted `targets` without repeats, global root
/// symbols, keys of the grammar.
fn shape(targets: &[TargetFact], files: &[FileFact]) -> Result<(), Discard> {
    let sorted = files.iter().all(|f| {
        f.targets
            .as_deref()
            .is_none_or(|t| t.windows(2).all(|p| p[0] < p[1]) && t.iter().all(|k| is_target_key(k)))
    });
    let roots = targets.iter().all(|t| {
        t.root.as_ref().is_none_or(|root| {
            matches!(
                crate::identity::parse_strict(&root.symbol),
                Ok(crate::identity::Parsed::Global(_))
            )
        })
    });
    let grammar = targets.iter().all(|t| is_target_key(&t.key));
    (sorted && roots && grammar).then_some(()).ok_or(Discard)
}

/// Phase 2: unique keys, known targets, one row per file, and membership consistent with the
/// `target-unknown` and `targets-incomplete` skips.
fn keys(
    targets: &[TargetFact],
    files: &[FileFact],
    skipped: &[SkippedFile],
) -> Result<(), Discard> {
    let mut known = BTreeSet::new();
    if !targets.iter().all(|t| known.insert(t.key.as_str())) {
        return Err(Discard);
    }
    let mut rows = BTreeSet::new();
    for file in files {
        if !rows.insert(file.file.as_str())
            || !file
                .targets
                .iter()
                .flatten()
                .all(|k| known.contains(k.as_str()))
        {
            return Err(Discard);
        }
    }
    let skips = |code: &str| -> BTreeSet<&str> {
        skipped
            .iter()
            .filter(|s| s.code == code)
            .map(|s| s.file.as_str())
            .collect()
    };
    let unknown = skips("target-unknown");
    let incomplete = skips("targets-incomplete");
    let with_targets: BTreeSet<_> = files
        .iter()
        .filter(|f| f.targets.is_some())
        .map(|f| f.file.as_str())
        .collect();
    let consistent = files
        .iter()
        .all(|f| f.targets.is_some() || unknown.contains(f.file.as_str()))
        && unknown.is_disjoint(&with_targets)
        && incomplete.is_subset(&with_targets);
    consistent.then_some(()).ok_or(Discard)
}

/// Phase 3: each target's root, its anchor converted and resolved against the kept
/// declarations of its symbol.
fn roots(
    snapshot: &Snapshot,
    targets: Vec<TargetFact>,
    src: &str,
    column: &Column<'_>,
    gaps: &mut Vec<GapRow>,
) -> Vec<TargetRow> {
    let admitted: BTreeSet<_> = snapshot.files.iter().map(|f| f.path.as_str()).collect();
    targets
        .into_iter()
        .map(|target| {
            let mut row = TargetRow {
                scope: crate::graph::SCOPE.to_owned(),
                key: target.key,
                kind: target.kind,
                name: target.name,
                root: None,
                entity: None,
                required_features: target.required_features,
                test: target.test,
                src: src.to_owned(),
            };
            let Some(root) = target.root else {
                return row;
            };
            let (anchor, converted) = match root.anchor {
                None => (None, true),
                Some(anchor) => match convert(&anchor, &admitted, column) {
                    Some(anchor) => (Some(anchor), true),
                    None => (None, false),
                },
            };
            let entity = converted
                .then(|| resolve(&snapshot.entities, &root.symbol, anchor.as_ref()))
                .flatten();
            if entity.is_none() {
                let mut gap = GapRow::new(GapKind::UnresolvedRoot, src);
                gap.layer = Some(Layer::Build);
                gap.target = Some(row.key.clone());
                gap.anchor.clone_from(&anchor);
                gaps.push(gap);
            }
            row.entity = entity;
            row.root = Some(TargetRoot {
                symbol: root.symbol,
                anchor,
            });
            row
        })
        .collect()
}

/// The anchor with its column in UTF-8 bytes, when its file is admitted and the column falls
/// on a character boundary of its line.
fn convert(anchor: &Anchor, admitted: &BTreeSet<&str>, column: &Column<'_>) -> Option<Anchor> {
    if !admitted.contains(anchor.file.as_str()) {
        return None;
    }
    let mut converted = anchor.clone();
    if let Some(col) = anchor.col {
        converted.col = Some(column(&anchor.file, anchor.line, col)?);
    } else if column(&anchor.file, anchor.line, 0).is_none() {
        return None;
    }
    Some(converted)
}

/// The key of the entity a root names: with an anchor, the one entity of that symbol with a
/// kept declaration at it; without, the one plain or `decls` entity of that symbol.
pub(crate) fn resolve(
    entities: &[EntityRow],
    symbol: &str,
    anchor: Option<&Anchor>,
) -> Option<String> {
    let of_symbol = entities
        .iter()
        .filter(|e| e.symbol.as_deref() == Some(symbol) && !e.group && !e.r#virtual);
    let mut found = match anchor {
        None => of_symbol
            .filter(|e| e.member_of.is_none())
            .map(|e| e.key.clone())
            .collect::<Vec<_>>(),
        Some(anchor) => of_symbol
            .filter(|e| {
                sites(e).any(|(file, line, col)| {
                    file == anchor.file
                        && line == anchor.line
                        && anchor.col.map_or_else(
                            || anchor.name.as_deref() == Some(e.name.as_str()),
                            |c| c == col,
                        )
                })
            })
            .map(|e| e.key.clone())
            .collect(),
    };
    (found.len() == 1).then(|| found.remove(0))
}

/// Each kept declaration site of an entity: its `def`, or every site of `decls`.
fn sites(entity: &EntityRow) -> impl Iterator<Item = (&str, u32, u32)> {
    entity
        .def
        .iter()
        .map(|def| (def.file.as_str(), def.name))
        .chain(
            entity
                .decls
                .iter()
                .flatten()
                .map(|d| (d.file.as_str(), d.name)),
        )
        .map(|(file, name)| {
            let (line, col) = name.start();
            (file, line, col)
        })
}

/// Phase 4: file facts and skips of admitted documents; the rest becomes `unknown-file`.
fn attach(
    snapshot: &mut Snapshot,
    files: Vec<FileFact>,
    skipped: &[SkippedFile],
    src: &str,
    gaps: &mut Vec<GapRow>,
) {
    let index: BTreeMap<String, usize> = snapshot
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.clone(), i))
        .collect();
    let mut unknown = BTreeSet::new();
    for fact in files {
        let Some(&i) = index.get(&fact.file) else {
            unknown.insert(fact.file);
            continue;
        };
        let row = &mut snapshot.files[i];
        row.targets = fact.targets;
        row.test = fact.test;
        row.constraint = fact.constraint;
    }
    let distinct: BTreeSet<_> = skipped.iter().collect();
    for skip in distinct {
        if !index.contains_key(&skip.file) {
            unknown.insert(skip.file.clone());
            continue;
        }
        let mut gap = GapRow::new(GapKind::SkippedFile, src);
        gap.layer = Some(Layer::Build);
        gap.file = Some(skip.file.clone());
        gap.code = Some(skip.code.clone());
        gaps.push(gap);
    }
    for file in unknown {
        let mut gap = GapRow::new(GapKind::UnknownFile, src);
        gap.layer = Some(Layer::Build);
        gap.file = Some(file);
        gaps.push(gap);
    }
}

/// Every entity's `test`: its own evidence first, then its file's; an entity declared at
/// several sites takes its files' common mark; a group and a virtual container none.
fn entity_tests(snapshot: &mut Snapshot, tests: &BTreeMap<String, ItemTest>) {
    let marks: BTreeMap<&str, Option<EntityTest>> = snapshot
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.test.map(EntityTest::from)))
        .collect();
    let mark = |file: &str| marks.get(file).copied().flatten();
    for entity in &mut snapshot.entities {
        entity.test = if entity.group || entity.r#virtual {
            None
        } else if let Some(decls) = &entity.decls {
            let mut common = decls.iter().map(|d| mark(&d.file));
            let first = common.next().flatten();
            common.all(|m| m == first).then_some(first).flatten()
        } else {
            tests
                .get(&entity.key)
                .map(|&t| EntityTest::from(t))
                .or_else(|| entity.def.as_ref().and_then(|d| mark(&d.file)))
        };
    }
}

#[cfg(test)]
mod tests {
    use super::is_target_key;

    #[test]
    fn target_keys_follow_the_grammar() {
        for key in [
            "cargo:demo:lib:demo",
            "packages/app/tsconfig.json",
            "a%20b",
            "é",
            &"k".repeat(1024),
        ] {
            assert!(is_target_key(key), "{key}");
        }
        for key in [
            "",
            "a b",
            "a\tb",
            "a%2",
            "a%2g",
            "a%zz",
            "%",
            &"k".repeat(1025),
            "é".repeat(513).as_str(),
        ] {
            assert!(!is_target_key(key), "{key}");
        }
    }
}
