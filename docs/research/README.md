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
| [rust-analyzer-reference-variance.md](rust-analyzer-reference-variance.md) | 2026-11-26 | Which references change between Rust indexes of unchanged source, and how Cargo build-script message order reproduces the change |
| [go-adapter-measurements.md](go-adapter-measurements.md) | 2026-11-24 to 2026-11-25 | What scip-go 0.2.10 needs, writes and reports for small Go modules: module root, build cache, exit status on broken modules, tests and package variants, repeatability, typed ranges, physical positions under `//line` and shared enclosing ranges, package collision groups, build constraints and cgo, references across generics and embedding, implementation relationships, absolute `replace` paths in symbols, and decoding concatenated or oversized index messages. What the first `go` command writes and reads (work directory, telemetry, env file, exported settings), VCS stamping, vendor mode, checksums, the error kinds of `go list` and go/packages, directories the package list does not reach, blank names, instantiation through selectors and the forms of compiler diagnostics. What go/parser, go/types and go/packages give a syntax layer, and the compiler's `-S` listing and SSA call graphs as sources of call evidence, with their costs on scip-go's own tree |
| [rust-call-evidence-measurements.md](rust-call-evidence-measurements.md) | 2026-11-24 to 2026-11-25 | How many Rust calls a scratch reader corroborates from compiler positions, where it loses calls, and which locations the engine resolves; the fixture comparison; which target emits a generic instance; two targets with one LLVM triple; the finished adapter's first run on ryu 1.0.23 with its counts, sampled rows, time, IR size, memory and disk; what a scratch probe gained on four corpora from calls into another target, dynamic dispatch, drop glue, attribute macros that return their input and crate inner attributes, and whether the indexes record trait relationships; and the adapter's eight added rows on regex-syntax 0.8.12 once crate inner attributes such as `no_std` are allowed |
| [snapshot-diff-prestudy.md](snapshot-diff-prestudy.md) | 2026-11-24 | Which renamed or moved definitions can be found in two public Rust release pairs, and where candidate rankings fail |
| [rust-adapter-measurements.md](rust-adapter-measurements.md) | 2026-11-24 | What Cargo and rust-analyzer write, which targets and definition spans SCIP includes, and how byte and character columns compare |
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
