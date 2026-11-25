//! Joins adapter facts to definitions and derives calls backed by concrete observations.
use crate::{
    anchor::{Anchors, Column},
    graph::{self, LineIndex, Snapshot},
    write::{Location, SourceReader},
};
use gosanja_schema::{
    contract::{Anchor, CallEvidence, SyntaxFact},
    position::{PositionEncoding, Range, Site},
    snapshot::{
        EntityRow, Evidence, FailedDetail, GapKind, GapRow, Layer, LayerState, Mechanism,
        OutlineKind, OutlineNode, Rel, RelationRow, Resolution, SyntaxRow,
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

pub(crate) fn valid_ranges(fact: &SyntaxFact) -> bool {
    fn nodes_valid(nodes: &[OutlineNode], parent: Option<Range>) -> bool {
        nodes.iter().all(|n| {
            n.range.start() <= n.range.end()
                && parent.is_none_or(|p| p.contains(n.range))
                && n.callee
                    .is_none_or(|c| c.start() <= c.end() && n.range.contains(c))
                && n.children
                    .as_deref()
                    .is_none_or(|children| nodes_valid(children, Some(n.range)))
        })
    }
    fact.body.is_none_or(|r| r.start() <= r.end())
        && fact
            .outline
            .as_deref()
            .is_none_or(|nodes| nodes_valid(nodes, None))
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
    let resolved: Vec<_> = facts
        .iter()
        .map(|fact| index.resolve(&fact.anchor, column))
        .collect();
    let discard = |gaps: &mut Vec<GapRow>| {
        let mut gap = GapRow::new(GapKind::FailedLayer, &src);
        gap.detail = Some(FailedDetail::RowInvalid);
        gap.layer = Some(Layer::Syntax);
        gaps.push(gap);
        Vec::new()
    };
    let mut seen = BTreeSet::new();
    if !facts.iter().all(valid_ranges) || !resolved.iter().flatten().all(|i| seen.insert(*i)) {
        return discard(gaps);
    }
    let mut bodies = Vec::new();
    let mut unmatched = Vec::new();
    let mut rows = Vec::new();
    let mut bad = BTreeMap::new();
    for (fact, resolved) in facts.iter().zip(resolved) {
        let Some(i) = resolved else {
            unmatched.push(unanchored(&fact.anchor, Layer::Syntax, &src));
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
        let entity = &entities[i];
        let base_body = entity.def.as_ref().and_then(|def| def.body);
        // Child ranges already lie within their parents, so checking roots covers the tree.
        if let Some(extent) = base_body.or(body)
            && outline
                .iter()
                .flatten()
                .any(|node| !extent.contains(node.range))
        {
            return discard(gaps);
        }
        if let Some(body) = body
            && base_body.is_none()
        {
            bodies.push((i, body));
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
    for (i, body) in bodies {
        if let Some(def) = &mut entities[i].def {
            def.body = Some(body);
            def.body_src = Some(src.clone());
        }
    }
    gaps.extend(unmatched);
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
    derive_with_sources(snapshot, observations, adapter, column, None);
}

fn derive_with_sources(
    snapshot: &mut Snapshot,
    observations: &[CallEvidence],
    adapter: &str,
    column: &Column<'_>,
    source: Option<(&dyn SourceReader, PositionEncoding)>,
) {
    let mut calls = Calls::new();
    syntax_calls(snapshot, adapter, &mut calls);
    compiler_calls(snapshot, observations, adapter, column, source, &mut calls);
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
            } else if node.k == OutlineKind::Call {
                let token = node.callee.unwrap_or(node.range);
                let site = Site::new(file, token);
                if node.callee.is_some()
                    && let Some(targets) = refs.get(&(row.key.as_str(), &site))
                {
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
    source: Option<(&dyn SourceReader, PositionEncoding)>,
    calls: &mut Calls,
) {
    let src = format!("adapter:{adapter}/calls");
    let index = Anchors::new(&snapshot.entities);
    let builds: BTreeSet<_> = snapshot.scope.builds.iter().map(|b| &b.id).collect();
    let mut unknown = 0;
    let mut sites = BTreeMap::new();
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
        sites
            .entry(obs.site.as_ref().map(|s| s.file.as_str()))
            .or_insert_with(Vec::new)
            .push((from, to, obs));
    }
    let indexed: BTreeSet<_> = snapshot.files.iter().map(|f| f.path.clone()).collect();
    let mut bad = BTreeMap::new();
    for (file, observations) in sites {
        // Only read a site after its build and both endpoints have been accepted.
        let extra = if let Some((p, (reader, encoding))) =
            file.filter(|p| !indexed.contains(*p)).zip(source)
        {
            let bytes = (graph::is_valid_path(p) && reader.locate(p) == Location::Inside)
                .then(|| reader.read(p).ok())
                .flatten();
            let Some(bytes) = bytes else {
                *bad.entry(p.to_owned()).or_default() += observations.len() as u64;
                continue;
            };
            Some((p, bytes, encoding))
        } else {
            None
        };
        let lines = extra
            .as_ref()
            .map(|(_, bytes, encoding)| (LineIndex::new(bytes), *encoding));
        let mut used = false;
        for (from, to, obs) in observations {
            let mut evidence = Evidence {
                file: None,
                line: None,
                col: None,
                end: None,
                build: Some(obs.build.clone()),
            };
            if let Some(site) = &obs.site {
                let col = site.col.unwrap_or(0);
                let byte = match &lines {
                    Some((lines, encoding)) => lines.byte_column(site.line, col, *encoding),
                    None => column(&site.file, site.line, col),
                };
                let Some(byte) = byte else {
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
            used = true;
        }
        if used && let Some((file, bytes, _)) = extra {
            snapshot.files.push(graph::file_row(file, None, &bytes));
        }
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
        mut layers,
        syntax,
        calls,
        merged,
        mut gaps,
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
    let merged: BTreeSet<String> = merged.into_iter().collect();
    let mut snapshot =
        crate::graph::build_with(&mut index, &reader, project_name, &merged, |snapshot| {
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
    if snapshot
        .gaps
        .iter()
        .any(|g| g.gap == GapKind::FailedLayer && g.layer == Some(Layer::Syntax))
    {
        layers.syntax = LayerState::Failed;
        gaps.retain(|g| g.layer != Some(Layer::Syntax));
    }
    snapshot.scope.layers = layers;
    snapshot.scope.builds = builds;
    snapshot.gaps.extend(gaps);
    reader.with_columns(manifest.position_encoding, |column| {
        derive_with_sources(
            &mut snapshot,
            &calls,
            &manifest.name,
            column,
            Some((sources, manifest.position_encoding)),
        );
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
