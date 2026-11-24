//! In-memory comparison of two snapshots.
mod candidates;
mod entities;
mod externals;
mod pair;
mod relations;
mod similarity;
#[cfg(test)]
mod tests;
use crate::validate::{Diagnostic, ReadSnapshot};
use gosanja_schema::{
    diff::{
        self, CandidateRow, Change, Counts, EntityRow, Excluded, ExternalRow, Manifest,
        RelationRow, Side,
    },
    snapshot::{LayerState, Scope, Tool},
};
use std::collections::BTreeMap;

/// Structural differences and the cross-snapshot key mapping used by candidate ranking.
#[derive(Debug)]
pub struct Comparison {
    /// Input descriptions, coverage, weights and output counts.
    pub manifest: Manifest,
    /// Added, removed, rekeyed and changed entity rows.
    pub entities: Vec<EntityRow>,
    /// Changes to external symbol rows.
    pub externals: Vec<ExternalRow>,
    /// Added and removed relation identities.
    pub relations: Vec<RelationRow>,
    /// Ranked alternatives for unpaired definitions.
    pub candidates: Vec<CandidateRow>,
    /// Paired old and new keys across both symbol tables.
    pub pairs: BTreeMap<String, String>,
}

/// Why a snapshot pair could not be compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareError {
    /// The snapshot schema is incompatible with this reader.
    SchemaVersion(String),
    /// A snapshot contains a number of scopes other than one.
    ScopeCount(usize),
    /// The first diagnostic from snapshot validation.
    Invalid(Diagnostic),
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemaVersion(version) => write!(f, "unsupported snapshot schema {version}"),
            Self::ScopeCount(count) => write!(f, "expected one scope, found {count}"),
            Self::Invalid(diagnostic) => diagnostic.fmt(f),
        }
    }
}

impl std::error::Error for CompareError {}

fn side(snapshot: &ReadSnapshot) -> Result<(Side, &Scope), CompareError> {
    let manifest = snapshot
        .manifest
        .as_ref()
        .expect("validated snapshot has a manifest");
    let [scope] = manifest.scopes.as_slice() else {
        return Err(CompareError::ScopeCount(manifest.scopes.len()));
    };
    let mut gaps = BTreeMap::new();
    for (_, gap) in &snapshot.gaps {
        let name = serde_json::to_value(gap.gap).expect("gap kind is a string");
        *gaps
            .entry(
                name.as_str()
                    .expect("serialized gap kind is a JSON string")
                    .to_owned(),
            )
            .or_default() += 1;
    }
    Ok((
        Side {
            schema: manifest.schema.clone(),
            project: manifest.project.clone(),
            adapter: scope.adapter.clone(),
            layers: scope.layers,
            builds: scope.builds.clone(),
            counts: manifest.counts,
            gaps,
        },
        scope,
    ))
}

/// Validate and compare two snapshots without reading source files.
///
/// # Errors
/// Returns a version or scope refusal, or a diagnostic for an invalid input.
pub fn compare(
    old: &crate::write::SnapshotFiles,
    new: &crate::write::SnapshotFiles,
) -> Result<Comparison, CompareError> {
    let read = |files: &crate::write::SnapshotFiles| {
        use gosanja_schema::compat::Refusal;
        crate::validate::read(files, gosanja_schema::snapshot::SCHEMA_VERSION).map_err(|err| {
            let (Refusal::Major { writer, .. } | Refusal::UnstableMinor { writer, .. }) = err;
            CompareError::SchemaVersion(writer.to_string())
        })
    };
    let old_rows = read(old)?;
    let new_rows = read(new)?;
    let validate = |rows, files: &crate::write::SnapshotFiles| {
        let (rows, index) = crate::validate::checked(rows, files, None);
        if let Some(diagnostic) = rows.diagnostics.first() {
            return Err(CompareError::Invalid(diagnostic.clone()));
        }
        Ok((rows, index))
    };
    let (left, old_symbols) = validate(old_rows, old)?;
    let (right, new_symbols) = validate(new_rows, new)?;
    compare_rows(&left, &right, &old_symbols, &new_symbols)
}

fn compare_rows(
    old_snapshot: &ReadSnapshot,
    new_snapshot: &ReadSnapshot,
    old_symbols: &crate::validate::SymbolIndex,
    new_symbols: &crate::validate::SymbolIndex,
) -> Result<Comparison, CompareError> {
    let (old, old_scope) = side(old_snapshot)?;
    let (new, new_scope) = side(new_snapshot)?;
    let present = |state| state == LayerState::Present;
    let mut excluded = Vec::new();
    let old_calls = present(old_scope.layers.calls);
    let new_calls = present(new_scope.layers.calls);
    if old_calls != new_calls
        || (old_calls
            && new_calls
            && (!old_scope
                .builds
                .iter()
                .all(|build| new_scope.builds.contains(build))
                || !new_scope
                    .builds
                    .iter()
                    .all(|build| old_scope.builds.contains(build))))
    {
        excluded.push(Excluded::Calls);
    }
    if present(old_scope.layers.syntax) != present(new_scope.layers.syntax) {
        excluded.push(Excluded::Syntax);
    }
    let pairs = pair::pairs(old_snapshot, new_snapshot, old_symbols, new_symbols);
    let reverse = pairs.iter().map(|(&old, &new)| (new, old)).collect();
    let entities = entities::changes(
        old_snapshot,
        new_snapshot,
        &pairs,
        !excluded.contains(&Excluded::Syntax),
    );
    let externals = externals::changes(old_snapshot, new_snapshot, &pairs);
    let old_relations = relations::SideRelations::new(old_snapshot, &pairs, true, &excluded);
    let new_relations = relations::SideRelations::new(new_snapshot, &reverse, false, &excluded);
    let mut relations: Vec<_> = old_relations
        .differences(&new_relations, Change::Removed)
        .chain(new_relations.differences(&old_relations, Change::Added))
        .collect();
    relations::order(&mut relations);
    let weights = diff::WEIGHTS;
    let candidates = candidates::rank(
        old_snapshot,
        new_snapshot,
        &pairs,
        &reverse,
        &excluded,
        weights,
    );
    let counts = Counts {
        entities: entities::counts(&entities),
        externals: externals::counts(&externals),
        relations: relations::counts(&relations),
        candidates: candidates.len(),
    };
    let manifest = Manifest {
        format: diff::FORMAT.into(),
        schema: diff::SCHEMA_VERSION.to_string(),
        tool: Tool {
            name: "gosanja".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        weights,
        old,
        new,
        counts,
        packages_added: new_relations
            .packages
            .difference(&old_relations.packages)
            .cloned()
            .collect(),
        packages_removed: old_relations
            .packages
            .difference(&new_relations.packages)
            .cloned()
            .collect(),
        not_compared: excluded,
    };
    Ok(Comparison {
        manifest,
        entities,
        externals,
        relations,
        candidates,
        pairs: pairs
            .into_iter()
            .map(|(old_key, new_key)| (old_key.into(), new_key.into()))
            .collect(),
    })
}
