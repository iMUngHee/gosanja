use super::graph::Graph;
use protobuf::Message;
use scip::types::{Document, Index, SymbolInformation};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

fn symbol(value: &mut String, graph: &Graph, unit: Option<&str>) {
    if let (Some(local), Some(unit)) = (value.strip_prefix("local "), unit) {
        *value = format!("local {unit}_{local}");
        return;
    }
    let mut fields = value.splitn(4, ' ');
    if let (Some("rust-analyzer"), Some("cargo"), Some(name), Some(tail)) =
        (fields.next(), fields.next(), fields.next(), fields.next())
        && let Some(original) = graph.package_names.get(name)
    {
        *value = format!("rust-analyzer cargo {original} {tail}");
    }
}

fn info(value: &mut SymbolInformation, graph: &Graph, unit: Option<&str>) {
    symbol(&mut value.symbol, graph, unit);
    symbol(&mut value.enclosing_symbol, graph, unit);
    for relationship in &mut value.relationships {
        symbol(&mut relationship.symbol, graph, unit);
    }
    if let Some(signature) = value.signature_documentation.as_mut() {
        for occurrence in &mut signature.occurrences {
            symbol(&mut occurrence.symbol, graph, unit);
        }
    }
}

fn deduplicate<T: Message>(values: &mut Vec<T>) -> io::Result<()> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::with_capacity(values.len());
    for value in std::mem::take(values) {
        if seen.insert(value.write_to_bytes().map_err(io::Error::other)?) {
            unique.push(value);
        }
    }
    *values = unique;
    Ok(())
}

pub(super) fn restore(index: &mut Index, root: &Path, graph: &Graph) -> io::Result<()> {
    let mut documents = BTreeMap::<String, Document>::new();
    index
        .documents
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    for mut document in std::mem::take(&mut index.documents) {
        let Some((unit, original)) = document.relative_path.split_once('/') else {
            return Err(io::Error::other(
                "missing unit in rust-analyzer document path",
            ));
        };
        if !graph.documents.contains(unit) {
            return Err(io::Error::other("unmapped rust-analyzer document"));
        }
        let original = original.to_owned();
        for occurrence in &mut document.occurrences {
            symbol(&mut occurrence.symbol, graph, Some(unit));
        }
        for entry in &mut document.symbols {
            info(entry, graph, Some(unit));
        }
        document.relative_path.clone_from(&original);
        match documents.entry(original) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(document);
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                let existing = slot.get_mut();
                if existing.position_encoding != document.position_encoding {
                    return Err(io::Error::other(
                        "one source file has multiple SCIP encodings",
                    ));
                }
                existing.occurrences.extend(document.occurrences);
                existing.symbols.extend(document.symbols);
            }
        }
    }
    for document in documents.values_mut() {
        deduplicate(&mut document.occurrences)?;
        deduplicate(&mut document.symbols)?;
    }
    index.documents = documents.into_values().collect();
    for entry in &mut index.external_symbols {
        info(entry, graph, None);
    }
    deduplicate(&mut index.external_symbols)?;
    if let Some(metadata) = index.metadata.as_mut() {
        metadata.project_root = format!("file://{}", root.display());
    }
    Ok(())
}
