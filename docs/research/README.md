# Research records

Background collected before the engine was designed: papers, existing products and
engineering write-ups on building a code-indexing engine, plus one local trial. These are
records, not decisions. Decisions belong in the architecture document once it exists.

## How to read these

- Each file is a report as its researcher wrote it, with raw source URLs. Six of them were
  written in Korean and are given here in English translation. Only process notes and names
  from a private codebase were removed.
- A statement prefixed `Unverified:` was not confirmed on a page the researcher opened.
  Treat it as a lead, not a fact.
- Several publisher sites (ACM, Springer, ScienceDirect, HAL) refused access, so some
  papers are cited from bibliographic records or search summaries only. Each report names
  what it could not open.
- Tool versions, licences and maintenance dates are as observed on the report's date.

## Files

| File | Date | What it answers |
| --- | --- | --- |
| [scip-indexer-differences.md](scip-indexer-differences.md) | 2026-11-23 | What rust-analyzer, scip-go and scip-typescript write for three small fixtures: position encodings, range forms, enclosing ranges, symbol shapes, message counts, absolute paths and repeatability, and whether the pinned `scip` 0.10.0 bindings decode typed ranges |
| [engine-architecture.md](engine-architecture.md) | 2026-11-23 | How Kythe, Glean, CodeQL, Joern, SCIP, Sourcetrail and others split language-specific extraction from a common core; adapter boundaries; tree-sitter versus native parsers; templated languages through transform plus source map; 15 papers |
| [call-relations.md](call-relations.md) | 2026-11-23 | Call-graph literature (CHA, RTA, VTA, soundness studies), routes for Rust, Go and TypeScript, and a vocabulary for certain versus possible call edges |
| [structural-diff-identity.md](structural-diff-identity.md) | 2026-11-23 | Matching entities across versions, stability of symbol-naming schemes, API-surface diff tools, architecture drift, incremental indexing |
| [indexer-experience-scip.md](indexer-experience-scip.md) | 2026-11-23 | What teams that ran indexers reported, practical SCIP (Rust crate, interoperability traps, scip-typescript coverage, merging), testing practice |
| [code-graph-engines.md](code-graph-engines.md) | 2026-11-18 | Which open-source engine can serve Rust, Go and TypeScript together, and why SCIP plus per-language indexers ranked first |
| [ai-era-code-graph-tools.md](ai-era-code-graph-tools.md) | 2026-11-18 | Code-graph tools of 2024-2026, evidence on humans losing track of agent-written code, and what no existing tool does |
| [visualization-history-conformance.md](visualization-history-conformance.md) | 2026-11-18 | History analysis, architecture-conformance checks, graph rendering libraries, studies on which visualisations help |
| [rust-call-evidence-trial.md](rust-call-evidence-trial.md) | 2026-11-17 | Local trial of four rustc-based routes to call evidence on a 75k-line workspace |

## What the records say, in short

The starting idea was an engine built on SCIP indexes, with per-language indexers adding
the facts SCIP cannot carry, and language differences hidden behind adapter traits. The
records support the direction and change its shape.

| Starting idea | What prior art suggests | Where |
| --- | --- | --- |
| SCIP is the base of the core | The core owns its schema and identity; SCIP is one input format. SCIP's design document calls it a transmission format, and Glean keeps it as one thin schema behind its own model | engine-architecture.md, indexer-experience-scip.md |
| One common model for all languages | A small intersection core (entity, containment, occurrence, reference, call, implements) plus per-language fact tables. Famix reports 15 years of regret over a single metamodel | engine-architecture.md |
| One trait per capability per language | A per-language capability table in which a missing entry is explicit, as in biome | engine-architecture.md |
| Language indexers inside the engine | External commands with a declared, versioned file contract, as in protoc plugins and CodeQL extractor packs | engine-architecture.md, addendum |
| Identity is the path of names | A version-independent key (symbol path without the package version) with collision handling; renames and moves found by a matching step afterwards, as in RefDiff | structural-diff-identity.md |
| "Maybe" and "exact" call edges | Two closed fields per edge, mechanism and resolution, with tiers that map to CHA and RTA; unresolved call sites are marked rather than dropped | call-relations.md |
| Integer ids in the output | No integer ids across the serialised boundary; this is what LSIF got wrong | engine-architecture.md, indexer-experience-scip.md |

Other points that recur across the records:

- Indexer output is thinner than it looks. One consumer's comparison put reference recall
  for `rust-analyzer scip` at 0.36 to 0.51 on two open-source crates, mostly because of
  macro-generated items. That figure is the comparison author's own and has to be measured
  again on a known corpus.
- Indexers disagree on position units (UTF-8 for rust-analyzer, UTF-16 for
  scip-typescript), on whether enclosing ranges are present, and on symbol conventions.
  The core has to normalise all three.
- Silent degradation is the commonest failure: an index ignored, zero call edges, a
  component that failed to transform, each without an error. Every fact needs a recorded
  origin, and a gap needs to be a fact too.
- Output must be deterministic, and golden tests should not depend on internal ids.
- Incremental indexing can wait, provided every fact carries the unit (file) that owns it
  from the first version.
- Textual LLVM IR for Rust call evidence is contested. The call-relations report prefers a
  compiler driver on `rustc_public`, which still needs a nightly toolchain and has no
  release date; the trial record shows what the IR route delivers on a pinned stable
  toolchain. The IR route's known gaps are `#[inline(always)]`, `async fn` bodies and
  shared generic instances, none of them yet confirmed by experiment.

## Licences

For a project released under MIT OR Apache-2.0, nothing recommended as a dependency or as
an external command is a problem: SCIP and its indexers and TypeScript are Apache-2.0,
tree-sitter and its grammars are MIT, the rust-analyzer crates are MIT OR Apache-2.0, and
the Go tooling is BSD-3-Clause. Invoking a tool as a separate process carries no
obligation; vendoring `scip.proto` means keeping its licence notice.

Use the published technique only, never the code, for GumTree (LGPL-3.0), Rupta (GPL-3.0)
and srcML (GPL according to its site). The CodeQL command-line tool may not be used on
private code without a paid licence. Each report ends with its own licence notes, and
what was read versus assumed is marked there.

## To check by experiment before relying on it

1. Reference recall of `rust-analyzer scip` on a corpus with known answers.
2. Whether scip-go and scip-typescript give enclosing ranges on definitions. Measured on
   small fixtures in scip-indexer-differences.md: both do for functions.
3. Cross-package symbol linking in a pnpm monorepo with per-package `paths`, and whether
   SvelteKit's `$lib` resolves after `svelte-kit sync`.
4. The three gaps of the LLVM IR route listed above.
5. Parse-error rate of the tree-sitter grammars on real repositories.

## What was not found

- No user study of which structural diff view helps a reviewer, and none of per-function
  control-flow outlines.
- No academic evaluation of Go call-graph precision, and no paper on type-based call
  graphs for TypeScript.
- No peer-reviewed paper on Kythe or Glean; their documentation and blog posts are the
  sources.
- No official convention for merging SCIP indexes of several languages, and nothing in
  SCIP for positions in generated or virtual documents.
