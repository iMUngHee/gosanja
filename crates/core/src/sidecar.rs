//! Joins adapter facts to definitions and derives calls backed by concrete observations.
use crate::{
    anchor::{Anchors, Column},
    graph::Snapshot,
};
use gosanja_schema::{
    contract::{Anchor, CallEvidence, SyntaxFact},
    position::{Range, Site},
    snapshot::{
        EntityRow, Evidence, GapKind, GapRow, Layer, Mechanism, OutlineKind, OutlineNode, Rel,
        RelationRow, Resolution, SyntaxRow,
    },
};
use std::collections::{BTreeMap, BTreeSet};

fn unanchored(anchor: &Anchor, layer: Layer, src: &str) -> GapRow {
    let mut gap = GapRow::new(GapKind::Unanchored, src);
    gap.anchor = Some(anchor.clone());
    gap.layer = Some(layer);
    gap
}

fn position_gaps(
    counts: BTreeMap<String, u64>,
    layer: Layer,
    src: &str,
) -> impl Iterator<Item = GapRow> {
    counts.into_iter().map(move |(file, count)| {
        let mut gap = GapRow::new(GapKind::PositionOutOfRange, src);
        gap.file = Some(file);
        gap.count = Some(count);
        gap.layer = Some(layer);
        gap
    })
}

fn convert(range: Range, file: &str, col: &Column<'_>) -> Option<Range> {
    let [sl, sc, el, ec] = range.0;
    let r = Range::new(sl, col(file, sl, sc)?, el, col(file, el, ec)?);
    (r.start() <= r.end()).then_some(r)
}

fn convert_outline(nodes: &mut [OutlineNode], file: &str, col: &Column<'_>) -> Option<()> {
    for node in nodes {
        node.range = convert(node.range, file, col)?;
        if let Some(token) = &mut node.callee {
            *token = convert(*token, file, col)?;
        }
        if let Some(children) = &mut node.children {
            convert_outline(children, file, col)?;
        }
    }
    Some(())
}

/// Attaches syntax and fills missing bodies before the graph assigns reference ownership.
pub fn apply_syntax(
    entities: &mut [EntityRow],
    facts: &[SyntaxFact],
    adapter: &str,
    column: &Column<'_>,
    gaps: &mut Vec<GapRow>,
) -> Vec<SyntaxRow> {
    let src = format!("adapter:{adapter}/syntax");
    let index = Anchors::new(entities);
    let mut rows = Vec::new();
    let mut bad = BTreeMap::new();
    for fact in facts {
        let Some(i) = index.resolve(&fact.anchor, column) else {
            gaps.push(unanchored(&fact.anchor, Layer::Syntax, &src));
            continue;
        };
        let file = &fact.anchor.file;
        let body = fact.body.and_then(|r| convert(r, file, column));
        let mut outline = fact.outline.clone();
        if (fact.body.is_some() && body.is_none())
            || outline
                .as_mut()
                .is_some_and(|nodes| convert_outline(nodes, file, column).is_none())
        {
            *bad.entry(file.clone()).or_default() += 1;
            continue;
        }
        let entity = &mut entities[i];
        if let (Some(def), Some(body)) = (&mut entity.def, body)
            && def.body.is_none()
        {
            def.body = Some(body);
            def.body_src = Some(src.clone());
        }
        rows.push(SyntaxRow {
            key: entity.key.clone(),
            visibility: fact.visibility.clone(),
            qualifiers: fact.qualifiers.clone(),
            attributes: fact.attributes.clone(),
            outline,
            src: src.clone(),
        });
    }
    gaps.extend(position_gaps(bad, Layer::Syntax, &src));
    rows
}

type Calls = BTreeMap<(String, String, Mechanism, Resolution, String), Vec<Evidence>>;

/// Adds syntax- and compiler-supported calls after reference ownership is known.
/// The scope's build contexts come from the adapter result.
pub fn derive_calls(
    snapshot: &mut Snapshot,
    observations: &[CallEvidence],
    adapter: &str,
    column: &Column<'_>,
) {
    let mut calls = Calls::new();
    syntax_calls(snapshot, adapter, &mut calls);
    compiler_calls(snapshot, observations, adapter, column, &mut calls);
    snapshot.relations.extend(calls.into_iter().map(
        |((from, to, mechanism, resolution, src), mut evidence)| {
            evidence.sort();
            RelationRow {
                from,
                to,
                rel: Rel::Call,
                roles: None,
                mechanism: Some(mechanism),
                resolution: Some(resolution),
                sites: None,
                evidence: Some(evidence),
                src,
            }
        },
    ));
}

fn syntax_calls(snapshot: &mut Snapshot, adapter: &str, calls: &mut Calls) {
    let src = format!("adapter:{adapter}/syntax");
    let mut refs = BTreeMap::new();
    for row in snapshot.relations.iter().filter(|r| r.rel == Rel::Ref) {
        for site in row.sites.iter().flatten() {
            refs.entry((row.from.as_str(), site))
                .or_insert_with(BTreeSet::new)
                .insert(&row.to);
        }
    }
    let files: BTreeMap<_, _> = snapshot
        .entities
        .iter()
        .filter_map(|e| Some((&e.key, &e.def.as_ref()?.file)))
        .collect();
    for row in &snapshot.syntax {
        let Some(&file) = files.get(&row.key) else {
            continue;
        };
        let mut stack: Vec<_> = row.outline.iter().flatten().collect();
        while let Some(node) = stack.pop() {
            stack.extend(node.children.iter().flatten());
            if node.k == OutlineKind::Opaque {
                let mut gap = GapRow::new(GapKind::OpaqueRegion, &src);
                gap.keys = Some(vec![row.key.clone()]);
                gap.file = Some(file.clone());
                gap.range = Some(node.range);
                snapshot.gaps.push(gap);
            } else if let (OutlineKind::Call, Some(token)) = (node.k, node.callee) {
                let site = Site::new(file, token);
                if let Some(targets) = refs.get(&(row.key.as_str(), &site)) {
                    let [sl, sc, el, ec] = token.0;
                    for &target in targets {
                        calls
                            .entry((
                                row.key.clone(),
                                target.clone(),
                                Mechanism::Unknown,
                                Resolution::Declared,
                                src.clone(),
                            ))
                            .or_default()
                            .push(Evidence {
                                file: Some(file.clone()),
                                line: Some(sl),
                                col: Some(sc),
                                end: Some([el, ec]),
                                build: None,
                            });
                    }
                } else {
                    let mut gap = GapRow::new(GapKind::OpenCall, &src);
                    gap.file = Some(file.clone());
                    gap.range = Some(token);
                    snapshot.gaps.push(gap);
                }
            }
        }
    }
}

fn compiler_calls(
    snapshot: &mut Snapshot,
    observations: &[CallEvidence],
    adapter: &str,
    column: &Column<'_>,
    calls: &mut Calls,
) {
    let src = format!("adapter:{adapter}/calls");
    let index = Anchors::new(&snapshot.entities);
    let builds: BTreeSet<_> = snapshot.scope.builds.iter().map(|b| &b.id).collect();
    let mut unknown = 0;
    let mut bad = BTreeMap::new();
    for obs in observations {
        if !builds.contains(&obs.build) {
            unknown += 1;
            continue;
        }
        let Some(from) = index.resolve(&obs.caller, column) else {
            snapshot
                .gaps
                .push(unanchored(&obs.caller, Layer::Calls, &src));
            continue;
        };
        let Some(to) = index.resolve(&obs.callee, column) else {
            snapshot
                .gaps
                .push(unanchored(&obs.callee, Layer::Calls, &src));
            continue;
        };
        let mut evidence = Evidence {
            file: None,
            line: None,
            col: None,
            end: None,
            build: Some(obs.build.clone()),
        };
        if let Some(site) = &obs.site {
            let Some(byte) = column(&site.file, site.line, site.col.unwrap_or(0)) else {
                *bad.entry(site.file.clone()).or_default() += 1;
                continue;
            };
            evidence.file = Some(site.file.clone());
            evidence.line = Some(site.line);
            evidence.col = site.col.map(|_| byte);
        }
        calls
            .entry((
                snapshot.entities[from].key.clone(),
                snapshot.entities[to].key.clone(),
                obs.mechanism,
                obs.resolution,
                src.clone(),
            ))
            .or_default()
            .push(evidence);
    }
    if unknown > 0 {
        let mut gap = GapRow::new(GapKind::UnknownBuildContext, &src);
        gap.count = Some(unknown);
        gap.layer = Some(Layer::Calls);
        snapshot.gaps.push(gap);
    }
    snapshot.gaps.extend(position_gaps(bad, Layer::Calls, &src));
}

/// Builds the base and both optional layers using the producer's position encoding.
///
/// # Errors
/// Returns the graph decoder's error if the base index cannot be read.
pub fn build(
    output: crate::adapter::AdapterOutput,
    sources: &dyn crate::write::SourceReader,
    project_name: &str,
) -> Result<Snapshot, crate::graph::BuildError> {
    use gosanja_schema::snapshot::AdapterInfo;
    let crate::adapter::AdapterOutput {
        mut index,
        manifest,
        builds,
        layers,
        syntax,
        calls,
        gaps,
        ..
    } = output;
    let mut needed = BTreeSet::new();
    for fact in &syntax {
        needed.insert(fact.anchor.file.as_str());
    }
    for row in &calls {
        needed.insert(row.caller.file.as_str());
        needed.insert(row.callee.file.as_str());
        if let Some(site) = &row.site {
            needed.insert(&site.file);
        }
    }
    let reader = IndexedSources {
        sources,
        needed,
        bytes: std::cell::RefCell::default(),
    };
    let mut snapshot = crate::graph::build_with(&mut index, &reader, project_name, |snapshot| {
        reader.with_columns(manifest.position_encoding, |column| {
            snapshot.syntax = apply_syntax(
                &mut snapshot.entities,
                &syntax,
                &manifest.name,
                column,
                &mut snapshot.gaps,
            );
        });
    })?;
    snapshot.scope.layers = layers;
    snapshot.scope.builds = builds;
    snapshot.gaps.extend(gaps);
    reader.with_columns(manifest.position_encoding, |column| {
        derive_calls(&mut snapshot, &calls, &manifest.name, column);
    });
    snapshot.scope.adapter = Some(AdapterInfo {
        name: manifest.name,
        version: manifest.version,
        contract: manifest.contract,
    });
    Ok(snapshot)
}

// Use exactly the bytes the graph indexed. Only retain files named by adapter positions.
struct IndexedSources<'a> {
    sources: &'a dyn crate::write::SourceReader,
    needed: BTreeSet<&'a str>,
    bytes: std::cell::RefCell<BTreeMap<String, Vec<u8>>>,
}
impl crate::write::SourceReader for IndexedSources<'_> {
    fn locate(&self, path: &str) -> crate::write::Location {
        self.sources.locate(path)
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let bytes = self.sources.read(path)?;
        if self.needed.contains(path) {
            self.bytes.borrow_mut().insert(path.into(), bytes.clone());
        }
        Ok(bytes)
    }
}
impl IndexedSources<'_> {
    fn with_columns(
        &self,
        encoding: gosanja_schema::position::PositionEncoding,
        apply: impl FnOnce(&Column<'_>),
    ) {
        let bytes = self.bytes.borrow();
        let indexes: BTreeMap<_, _> = bytes
            .iter()
            .map(|(file, bytes)| (file.as_str(), crate::graph::LineIndex::new(bytes)))
            .collect();
        apply(&|file, line, col| indexes.get(file)?.byte_column(line, col, encoding));
    }
}
