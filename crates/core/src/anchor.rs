//! Matches an adapter anchor to one declaration site at an exact position or name.
use gosanja_schema::{contract::Anchor, snapshot::EntityRow};
use std::collections::HashMap;

/// Converts a producer column to a UTF-8 byte column in the given file and line.
pub type Column<'a> = dyn Fn(&str, u32, u32) -> Option<u32> + 'a;

struct Candidate {
    index: usize,
    column: u32,
    name: String,
}
#[derive(Default)]
pub(crate) struct Anchors {
    lines: HashMap<String, HashMap<u32, Vec<Candidate>>>,
}

impl Anchors {
    pub(crate) fn new(entities: &[EntityRow]) -> Self {
        let mut result = Self::default();
        // One candidate per declaration site: a definition's, or each site of an entity
        // declared at several sites, so that uniqueness is counted by site, not by entity.
        for (index, entity) in entities.iter().enumerate() {
            let sites = entity
                .def
                .iter()
                .map(|def| (&def.file, def.name))
                .chain(entity.decls.iter().flatten().map(|d| (&d.file, d.name)));
            for (file, name) in sites {
                let (line, column) = name.start();
                result
                    .lines
                    .entry(file.clone())
                    .or_default()
                    .entry(line)
                    .or_default()
                    .push(Candidate {
                        index,
                        column,
                        name: entity.name.clone(),
                    });
            }
        }
        result
    }
    pub(crate) fn resolve(&self, anchor: &Anchor, column: &Column<'_>) -> Option<usize> {
        let line = self.lines.get(&anchor.file)?.get(&anchor.line)?;
        let col = match anchor.col {
            Some(c) => Some(column(&anchor.file, anchor.line, c)?),
            None => None,
        };
        let mut matches = line.iter().filter(|candidate| {
            col.map_or_else(
                || anchor.name.as_ref() == Some(&candidate.name),
                |v| candidate.column == v,
            )
        });
        let first = matches.next()?.index;
        matches.next().is_none().then_some(first)
    }
}
