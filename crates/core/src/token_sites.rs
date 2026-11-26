//! Validates and joins exact syntax tokens against admitted base occurrences.
use std::collections::{BTreeMap, BTreeSet};

use gosanja_schema::{
    contract::{ImplFact, ImportFact, SyntaxLine},
    position::{Range, Site},
    snapshot::{EntityRow, GapKind, GapRow, Layer, Rel, RelationRow},
};

pub(crate) fn is_token(range: Range) -> bool {
    let [sl, sc, el, ec] = range.0;
    sl == el && sc < ec
}

fn ordered_tokens(ranges: &[Range]) -> bool {
    !ranges.is_empty()
        && ranges.iter().copied().all(is_token)
        && ranges.windows(2).all(|pair| pair[0] < pair[1])
}

pub(crate) fn valid_import(fact: &ImportFact) -> bool {
    ordered_tokens(&fact.sites)
}

pub(crate) fn valid_impl(fact: &ImplFact) -> bool {
    let extent = fact.extent;
    let headers = [fact.self_type, fact.trait_type];
    extent.start() < extent.end()
        && ordered_tokens(&fact.members)
        && fact
            .members
            .iter()
            .all(|member| extent.start() < member.start() && member.end() < extent.end())
        && headers.iter().any(Option::is_some)
        && headers.into_iter().flatten().all(|token| {
            is_token(token) && extent.contains(token) && token.end() < fact.members[0].start()
        })
        && fact
            .self_type
            .zip(fact.trait_type)
            .is_none_or(|(a, b)| a.end() <= b.start() || b.end() <= a.start())
}

pub(crate) fn unique_rows(lines: &[SyntaxLine]) -> bool {
    let mut imports = BTreeSet::new();
    let mut blocks = BTreeSet::new();
    lines.iter().all(|line| match line {
        SyntaxLine::Imports(fact) => imports.insert(fact.file.as_str()),
        SyntaxLine::Impl(fact) => blocks.insert((fact.file.as_str(), fact.extent)),
        SyntaxLine::Fact(_) | SyntaxLine::Test(_) => true,
    })
}

/// Syntax tokens after admission and conversion to byte columns.
#[derive(Default)]
pub(crate) struct Tokens {
    pub imports: BTreeSet<Site>,
    pub blocks: Vec<ImplFact>,
    pub provenance: String,
}

type MemberParents = BTreeMap<Site, Vec<Option<String>>>;

#[derive(Default)]
struct ImplLinks<'a> {
    targets: BTreeSet<&'a str>,
    sites: BTreeSet<Site>,
    blocks: BTreeSet<Site>,
}

impl Tokens {
    // Observe declarations before folding so each folded site retains its original parent.
    pub fn member_parents(&self, entities: &[EntityRow]) -> MemberParents {
        if self.blocks.is_empty() {
            return BTreeMap::new();
        }
        let sites: BTreeSet<_> = self
            .blocks
            .iter()
            .flat_map(|b| b.members.iter().map(|&r| Site::new(&b.file, r)))
            .collect();
        let containers: BTreeSet<_> = entities
            .iter()
            .filter(|e| e.r#virtual && !e.group)
            .map(|e| e.key.as_str())
            .collect();
        let mut members = MemberParents::new();
        for entity in entities {
            let Some(def) = &entity.def else {
                continue;
            };
            let site = Site::new(&def.file, def.name);
            if sites.contains(&site) {
                members.entry(site).or_default().push(
                    entity
                        .parent
                        .as_ref()
                        .filter(|p| containers.contains(p.as_str()))
                        .cloned(),
                );
            }
        }
        members
    }

    pub fn headers(&self) -> BTreeSet<Site> {
        self.blocks
            .iter()
            .flat_map(|b| {
                [b.self_type, b.trait_type]
                    .into_iter()
                    .flatten()
                    .map(|r| Site::new(&b.file, r))
            })
            .collect()
    }

    pub fn impl_relations(
        &self,
        members: &MemberParents,
        refs: &BTreeMap<Site, BTreeSet<&str>>,
        relations: &mut Vec<RelationRow>,
        gaps: &mut Vec<GapRow>,
    ) {
        let mut links = BTreeMap::<(&str, Rel), ImplLinks<'_>>::new();
        let mut ambiguous = BTreeSet::new();
        for block in &self.blocks {
            let extent = Site::new(&block.file, block.extent);
            let parent = match parent(block, members) {
                Ok(Some(parent)) => parent,
                Ok(None) => continue,
                Err(()) => {
                    ambiguous.insert(extent);
                    continue;
                }
            };
            for (kind, range) in [
                (Rel::ImplSelfType, block.self_type),
                (Rel::ImplTrait, block.trait_type),
            ] {
                let Some(site) = range.map(|r| Site::new(&block.file, r)) else {
                    continue;
                };
                let Some(targets) = refs.get(&site) else {
                    continue;
                };
                let link = links.entry((parent, kind)).or_default();
                link.targets.extend(targets);
                link.sites.insert(site);
                link.blocks.insert(extent.clone());
            }
        }
        for ((from, rel), link) in links {
            let Some(to) = link.targets.first().filter(|_| link.targets.len() == 1) else {
                ambiguous.extend(link.blocks);
                continue;
            };
            relations.push(RelationRow {
                from: from.into(),
                to: (*to).into(),
                rel,
                roles: None,
                mechanism: None,
                resolution: None,
                sites: Some(link.sites.into_iter().collect()),
                syntax_import_sites: None,
                evidence: None,
                src: self.provenance.clone(),
            });
        }
        gaps.extend(ambiguous.into_iter().map(|site| {
            let mut gap = GapRow::new(GapKind::AmbiguousImpl, &self.provenance);
            gap.layer = Some(Layer::Syntax);
            gap.range = Some(site.range());
            gap.file = Some(site.0);
            gap
        }));
    }
}

fn parent<'a>(block: &ImplFact, members: &'a MemberParents) -> Result<Option<&'a str>, ()> {
    let mut parent = None;
    for names in block
        .members
        .iter()
        .filter_map(|&range| members.get(&Site::new(&block.file, range)))
    {
        let [Some(current)] = names.as_slice() else {
            return Err(());
        };
        if let Some(old) = parent.replace(current.as_str())
            && old != current
        {
            return Err(());
        }
    }
    Ok(parent)
}
