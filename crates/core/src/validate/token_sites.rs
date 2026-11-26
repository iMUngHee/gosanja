//! Snapshot invariants for token-backed relations and syntax gaps.
use std::collections::{BTreeMap, BTreeSet, HashSet};

use gosanja_schema::{
    position::{Range, Site},
    snapshot::{EntityRow, GapKind, GapRow, Layer, LayerState, RefRole, Rel, RelationRow, Scope},
};

use super::{Checks, ReadSnapshot};
use crate::{graph::is_valid_path, token_sites::is_token, write::enum_name};

struct View<'a> {
    scopes: &'a [Scope],
    files: BTreeSet<&'a str>,
    entities: BTreeMap<&'a str, &'a EntityRow>,
}

impl View<'_> {
    fn scope(&self, file: &str) -> Option<&Scope> {
        if !is_valid_path(file) {
            return None;
        }
        let mut deepest = None;
        let mut depth = 0;
        let mut tied = false;
        for scope in self.scopes {
            let path = scope.path.as_str();
            let candidate = if path == "." {
                0
            } else if file == path
                || file
                    .strip_prefix(path)
                    .is_some_and(|rest| rest.starts_with('/'))
            {
                path.split('/').count()
            } else {
                continue;
            };
            if deepest.is_none() || candidate > depth {
                deepest = Some(scope);
                depth = candidate;
                tied = false;
            } else if candidate == depth {
                tied = true;
            }
        }
        deepest.filter(|_| !tied)
    }

    fn site_scope(&self, sites: &[Site]) -> Option<&Scope> {
        let scope = self.scope(sites.first()?.file())?;
        if scope.layers.syntax != LayerState::Present {
            return None;
        }
        sites
            .iter()
            .all(|site| {
                self.files.contains(site.file())
                    && self
                        .scope(site.file())
                        .is_some_and(|other| std::ptr::eq(scope, other))
            })
            .then_some(scope)
    }
}

fn provenance(scope: &Scope, src: &str) -> bool {
    scope.layers.syntax == LayerState::Present
        && scope
            .adapter
            .as_ref()
            .is_some_and(|adapter| src == format!("adapter:{}/syntax", adapter.name))
}

fn ordered_tokens(sites: &[Site]) -> bool {
    !sites.is_empty()
        && sites.iter().all(|site| is_token(site.range()))
        && sites.windows(2).all(|pair| pair[0] < pair[1])
}

pub(super) fn check(checks: &mut Checks, snap: &ReadSnapshot) {
    let duplicate_lines: BTreeSet<_> = checks
        .diagnostics
        .iter()
        .filter(|diag| diag.code == "duplicate-row")
        .filter_map(|diag| Some((diag.file, diag.line?)))
        .collect();
    let duplicate = |checks: &mut Checks, table, line, identity| {
        if !duplicate_lines.contains(&(table, line)) {
            checks.diag(table, line, "duplicate-row", format!("repeated {identity}"));
        }
    };
    let view = View {
        scopes: snap.manifest.as_ref().map_or(&[], |m| m.scopes.as_slice()),
        files: snap.files.iter().map(|(_, f)| f.path.as_str()).collect(),
        entities: snap
            .entities
            .iter()
            .map(|(_, e)| (e.key.as_str(), e))
            .collect(),
    };
    let mut refs = BTreeSet::new();
    let mut links = BTreeSet::new();
    for (line, row) in &snap.relations {
        if row.rel == Rel::Ref
            && let Some(roles) = &row.roles
        {
            if !roles
                .windows(2)
                .all(|p| enum_name(&p[0]) < enum_name(&p[1]))
            {
                checks.diag(
                    "relations.jsonl",
                    *line,
                    "ref-roles-unsorted",
                    "`roles` must be strictly sorted by name".into(),
                );
            } else if !refs.insert((row.from.as_str(), row.to.as_str(), roles.as_slice())) {
                duplicate(checks, "relations.jsonl", *line, "reference identity");
            }
        }
        if let Some(marks) = &row.syntax_import_sites {
            import_marks(checks, &view, *line, row, marks);
        }
        if matches!(row.rel, Rel::ImplSelfType | Rel::ImplTrait) {
            if !links.insert((row.from.as_str(), row.rel)) {
                duplicate(checks, "relations.jsonl", *line, "impl identity");
            }
            impl_link(checks, &view, *line, row);
        }
    }
    let mut ambiguous = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    for (line, gap) in &snap.gaps {
        match gap.gap {
            GapKind::AmbiguousImpl => {
                syntax_gap(checks, &view, *line, gap, true);
                if !ambiguous.insert((gap.file.as_deref(), gap.range)) {
                    duplicate(checks, "gaps.jsonl", *line, "ambiguous impl block");
                }
            }
            GapKind::UnknownFile if gap.layer == Some(Layer::Syntax) => {
                syntax_gap(checks, &view, *line, gap, false);
                if !unknown.insert(gap.file.as_deref()) {
                    duplicate(checks, "gaps.jsonl", *line, "syntax unknown file");
                }
            }
            GapKind::PositionOutOfRange
                if gap.layer == Some(Layer::Syntax)
                    && gap
                        .file
                        .as_deref()
                        .and_then(|file| view.scope(file))
                        .is_none_or(|scope| !provenance(scope, &gap.src)) =>
            {
                checks.diag(
                    "gaps.jsonl",
                    *line,
                    "syntax-gap-scope",
                    "a syntax position diagnostic requires the owning scope and its provenance"
                        .into(),
                );
            }
            _ => {}
        }
    }
}

fn import_marks(
    checks: &mut Checks,
    view: &View<'_>,
    line: usize,
    row: &RelationRow,
    marks: &[Site],
) {
    let sites = row.sites.as_deref().unwrap_or_default();
    if row.rel != Rel::Ref
        || row
            .roles
            .as_ref()
            .is_none_or(|roles| !roles.contains(&RefRole::Import))
        || row.src != "core:reference-in-body"
    {
        checks.diag(
            "relations.jsonl",
            line,
            "syntax-import-role",
            "`syntax_import_sites` requires an import reference with core reference provenance"
                .into(),
        );
    }
    let reference_sites: HashSet<_> = sites.iter().collect();
    if !ordered_tokens(marks) || !marks.iter().all(|site| reference_sites.contains(site)) {
        checks.diag("relations.jsonl", line, "syntax-import-subset",
            "`syntax_import_sites` must be a sorted nonempty subset of single-line tokens in `sites`".into());
    }
    if view.site_scope(sites).is_none() {
        checks.diag(
            "relations.jsonl",
            line,
            "syntax-relation-scope",
            "import evidence requires one owning scope with syntax present".into(),
        );
    }
}

fn impl_link(checks: &mut Checks, view: &View<'_>, line: usize, row: &RelationRow) {
    if view
        .entities
        .get(row.from.as_str())
        .is_none_or(|from| !from.r#virtual || from.group)
    {
        checks.diag(
            "relations.jsonl",
            line,
            "impl-source",
            "an impl link requires a virtual non-group source".into(),
        );
    }
    let sites = row.sites.as_deref().unwrap_or_default();
    if !ordered_tokens(sites) {
        checks.diag(
            "relations.jsonl",
            line,
            "impl-sites",
            "impl header sites must be nonempty single-line tokens in strictly increasing order"
                .into(),
        );
    }
    if view
        .site_scope(sites)
        .is_none_or(|scope| !provenance(scope, &row.src))
    {
        checks.diag(
            "relations.jsonl",
            line,
            "syntax-relation-scope",
            "impl evidence requires one owning syntax scope and its adapter provenance".into(),
        );
    }
}

fn syntax_gap(checks: &mut Checks, view: &View<'_>, line: usize, gap: &GapRow, ambiguous: bool) {
    let shape = gap.layer == Some(Layer::Syntax)
        && gap.keys.is_none()
        && gap.count.is_none()
        && gap.anchor.is_none()
        && gap.code.is_none()
        && gap.detail.is_none()
        && gap.target.is_none()
        && gap.scope.is_none();
    let extent = if ambiguous {
        gap.file
            .as_deref()
            .is_some_and(|file| view.files.contains(file))
            && gap.range.is_some_and(|r: Range| r.start() < r.end())
    } else {
        gap.range.is_none()
    };
    if !shape || !extent {
        checks.diag(
            "gaps.jsonl",
            line,
            "syntax-gap-shape",
            "invalid fields or block extent on a syntax token gap".into(),
        );
    }
    if gap
        .file
        .as_deref()
        .and_then(|file| view.scope(file))
        .is_none_or(|scope| !provenance(scope, &gap.src))
    {
        checks.diag(
            "gaps.jsonl",
            line,
            "syntax-gap-scope",
            "syntax token gap requires the owning syntax scope and its adapter provenance".into(),
        );
    }
}
