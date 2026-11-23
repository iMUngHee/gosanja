# Open-source code-graph engines for Rust, Go and TypeScript — research record (2026-11-18)

Survey of engines that extract symbols, references, call graphs and dependency graphs and
could sit under a separate viewer: maintenance state, licence, precision, output format
and limits, ending with a ranking for covering the three languages together.

Collected by a web-research agent on 2026-11-18. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The report was written in Korean and is given here in English translation.

## Report

These are the results of the code-graph engine survey. Conclusion: SCIP (per-language indexer) plus LSP enrichment is the only fit as a base that covers Rust+Go+TS together. Kythe does not support Rust, CodeQL has a licence problem, and stack-graphs is archived.
Checked on 2026-11-18. Unless stated otherwise, maintenance dates are the pushed_at of https://api.github.com/repos/\<owner>/\<repo> and /releases/latest, read with gh api (authenticated).

### SCIP family
- SCIP format and the scip CLI: https://github.com/scip-code/scip (moved from sourcegraph/scip to the scip-code org; the old URL redirects). Apache-2.0. v0.10.0 (2026-09-03), push 2026-11-11, active.
  - What it can express: Occurrence(range, symbol, symbol_roles = Definition/Import/WriteAccess/ReadAccess/Generated/Test/ForwardDefinition), enclosing_range, SymbolInformation.relationships(is_reference/is_implementation/is_type_definition/is_definition), enclosing_symbol, signature_documentation.
  - There is no call edge type. The proto comment tells call hierarchy to "find the references inside a function body with enclosing_range", so telling a call from a plain reference is the consumer's job. SyntaxKind.IdentifierFunction is for highlighting, and its comment reads "Function references, including calls". There are no fields for dynamic dispatch, macro expansion or generics instantiation either. Source: https://github.com/scip-code/scip/blob/main/scip.proto
  - In v0.9.0 (2026-06-29) ranges became a typed oneof (single_line_range/multi_line_range), and the old repeated int32 range/enclosing_range were deprecated. scip-typescript and scip-go use the typed form, rust-analyzer the deprecated form (checked in each source). A reader therefore has to handle both forms.
  - CLI subcommands: lint, print --json, snapshot, stats, test, expt-convert (EXPERIMENTAL, conversion to SQLite). Source: https://github.com/scip-code/scip/blob/main/docs/CLI.md
  - No incremental support. Unverified: an index is always regenerated in full, and merging per document or package appears to be the consumer's job.
- rust-analyzer scip: https://github.com/rust-lang/rust-analyzer, Apache-2.0/MIT. Release 2026-11-16, last change to scip.rs 2026-10-16.
  - From the source (crates/rust-analyzer/src/cli/scip.rs): relationships is always emitted as Vec::new(), so there are no trait impls and no "implements" relations. symbol_roles sets only Definition; Import/Read/Write are absent. enclosing_range is present only on definition bodies.
  - It uses the proc-macro server (ProcMacroServerChoice::Sysroot). Unverified: how far references inside macro expansions are mapped back to the original token positions.
  - Batch only. The LSP mode updates incrementally on salsa.
  - LSP capabilities: callHierarchy, implementation and references are supported, and type_hierarchy_provider is None (crates/rust-analyzer/src/lsp/capabilities.rs).
  - The same CLI still has an lsif command.
- scip-go: https://github.com/scip-code/scip-go, Apache-2.0. v0.2.9 (2026-10-22), push 2026-11-10, active.
  - Built on go/packages; internal/implementations emits interface implementation relations. Bazel/Buck are handled through GOPACKAGESDRIVER, but cross-repo navigation does not work in that case (README).
  - No incremental support.
- scip-typescript: https://github.com/sourcegraph/scip-typescript, Apache-2.0. v0.6.2 (2026-11-10), active (v0.6.0 added Svelte).
  - It depends on typescript ^6.0.3. The API of TS7 (Go native) is "not ready" (https://github.com/microsoft/typescript-go README, archived 2026-08), so scip-typescript is tied to the TS6 compiler API.
  - It emits the isImplementation relationship (class implements) and uses typedEnclosingRange.
  - Options: --infer-tsconfig, --yarn-workspaces, --pnpm-workspaces. No incremental support. Watch for OOM on large repositories (README).
- LSIF: https://github.com/sourcegraph/lsif-go is archived (2023-09). https://github.com/microsoft/lsif-node is MIT with a push on 2026-10-22, but its recent commits are only action pins and dependency bumps, so it is in maintenance mode. Unverified: Sourcegraph replaced LSIF with SCIP. Not recommended as a format for a new consumer.

### General-purpose engines
- stack-graphs: https://github.com/github/stack-graphs, Apache-2.0. Archived (last push 2025-09-09, last release 2024-12-13). README: "no longer supported or updated by GitHub". Ruled out.
- tree-sitter-graph: https://github.com/tree-sitter/tree-sitter-graph, Apache-2.0. The last commit and v0.12.0 are from 2024-12-11; it is not archived but has stalled. It is a purely syntactic DSL, so it does not meet the precision requirement.
- Kythe: https://github.com/kythe/kythe, Apache-2.0. v0.0.76 (2026-07-16), push 2026-09-18.
  - The language directories are cxx, go, java and typescript. The Rust indexer was removed in 2023-08 (commit "fix(rust_common): finish removing rust (#5803)"). The TS indexer last changed in 2025-03.
  - Unverified: it is Bazel-centred and heavy, and its output is a protobuf entry stream and serving tables.
  - Ruled out for lacking Rust.
- Meta Glean: https://github.com/facebookincubator/Glean, BSD. No tagged releases; push 2026-11-17, active.
  - It indexes Rust, Go and TS through the SCIP indexers (rust-analyzer scip, scip-go, scip-typescript) and a scip-to-glean conversion (glean/website/docs/indexer/lsif-rust.md, lsif-go.md, lsif-typescript.md). Its precision is therefore the same as SCIP's, with no additional semantic information.
  - Its strengths are storage and querying (Angle), and it implements unit-based incrementality (docs/implementation/incrementality.md).
  - Unverified: being a Haskell build on RocksDB, it is heavy to bundle into a desktop application.
- Joern (CPG): https://github.com/joernio/joern, Apache-2.0. v4.0.685 (2026-11-17), very active.
  - Its frontends include rust2cpg, gosrc2cpg and jssrc2cpg. rust2cpg has commits up to 2026-11-17, and rust_ast_gen uses ra_ap_hir/ra_ap_ide 0.0.328, that is, the rust-analyzer libraries (https://github.com/joernio/rust_ast_gen Cargo.toml).
  - It is the only engine that can put all three languages into one graph model that includes the call graph.
  - Unverified: JS/TS call resolution is a type-recovery heuristic, and output goes through joern-export (dot, graphml, neo4j csv and others). It is JVM-based batch processing with no incremental support.
- CodeQL: https://github.com/github/codeql (MIT, queries); the CLI is https://github.com/github/codeql-cli-binaries v2.27.4 (2026-11-02).
  - It supports Rust (edition 2021/2024), Go (≤1.27) and TS (2.6–7.0) (docs/codeql/reusables/supported-versions-compilers.rst).
  - Licence (LICENSE.md): "may not be used ... in connection with any codebase that is not an Open Source Codebase (e.g., code in a private repo)". The only exception is a paid GHAS licence. It cannot be used on private code or in a distributed GUI, so it is ruled out.
- Sourcetrail:
  - The original, https://github.com/CoatiSoftware/Sourcetrail, is GPL-3.0 and was archived in 2021.
  - https://github.com/petermost/Sourcetrail is active with 2026.10 (2026-10-02) and a push on 2026-11-16. It supports only C/C++ and Java, though, and an announcement on 2026-08-01 slowed its development.
  - https://github.com/OpenSourceSourceTrail/Sourcetrail released 3.0.0 (including an MCP server) and was archived on 2026-09-05.
  - The Go indexer (TOMATOFQY/SourcetrailGolangIndexer, 2023-02) and the TS indexer (LouisStAmour, 2022-02) are abandoned, and no Rust indexer turned up in searches. Useful only as a GUI reference; ruled out as an engine.

### Rust only
- cargo-modules: https://github.com/regexident/cargo-modules, MPL-2.0. v0.28.0, commit 2026-11-15, active.
  - Built on ra_ap_hir =0.0.358, it produces the module tree and owns/uses dependency graphs with compiler-level resolution. There is no call graph.
  - Output is a text tree and DOT only. It works per crate and has no incremental support.
- crabviz: https://github.com/chanhx/crabviz, AGPL-3.0. Commit 2026-08-22, last release vscode_v0.5.0 (2025-08-24).
  - A VS Code extension that visualises LSP call hierarchy; its precision depends on each LSP. Being AGPL, reusing its code needs care.
- cargo-call-stack: https://github.com/japaric/cargo-call-stack, Apache-2.0/MIT. Push 2024-10.
  - "Last tested nightly: 2023-11-13", based on LLVM IR for embedded targets. Support for fn pointers and dyn Trait is "imperfect". Effectively stalled.
- cga (MIR call graph): https://github.com/OpenDevicePartnership/cga, MIT. Created 2026-06, push 2026-11-12.
  - It extracts a call graph from monomorphized MIR with rustc-driver and also resolves vtable candidates ("possible vtable resolution"). It needs a pinned nightly and only targets crates with an entry point. Its purpose is panic checking, so its output is not general-purpose.
  - Alternatives listed in its README: charon (https://github.com/AeneasVerif/charon, Apache-2.0, push 2026-11-17, MIR exporter) and cargo-callgraph (robinmoussu, stalled in 2022).
- rustdoc JSON: https://github.com/rust-lang/rustdoc-types, FORMAT_VERSION 61 (v0.61.0), push 2026-11-10.
  - Unverified: a nightly-only unstable format that holds only lists of items and impls. There are no references inside function bodies and no call graph.
- cargo-depgraph: https://github.com/jplatte/cargo-depgraph, archived (2026-05). A crate-level dependency graph, which cargo metadata can replace.
- rust-code-analysis: https://github.com/mozilla/rust-code-analysis. Last commit 2026-01-20, last release v0.0.25 (2023-01).
  - A tree-sitter-based metrics tool rather than a graph engine, so it is not relevant. Unverified: the licence is MPL-2.0 (the API gives no spdx information).

### Go only
- golang.org/x/tools: https://github.com/golang/tools, BSD-3. gopls/v0.24.0 (2026-10-15).
  - go/callgraph algorithms:
    - static: catches static calls only and is unsound.
    - cha: conservatively assumes every implements relation, so it is sound even for a partial program (a library), but it produces spurious edges.
    - rta: covers only what is reachable from main/root.
    - vta: type propagation, the most precise, and in "experimental phase".
  - Handling of generic function bodies is left as a TODO (callgraph.go).
  - go/pointer has been removed from x/tools/go (checked in the directory listing).
  - cmd/callgraph offers -algo=static|cha|rta|vta and a -format template.
- gopls: supports implementation (method set matching, signature matching for func types), type hierarchy and call hierarchy. Call hierarchy, however, says "Dynamic calls are not included" (gopls/doc/features/navigation.md). It also has an experimental built-in MCP server (mcp.md).
- go/packages + go/types: Unverified: compiler-level Defs/Uses can be obtained directly as a library (the basis of scip-go).
- go-callvis: https://github.com/ondrajz/go-callvis, MIT. v0.7.1 (2025-01), push 2026-03.
  - It supports -algo static|cha|rta|vta (default static), but its README still mentions go/pointer. Output is DOT and a web viewer; it is a visualisation tool rather than an engine.

### TS only
- dependency-cruiser: https://github.com/sverweij/dependency-cruiser, MIT. v18.8.0 (2026-11-16), active.
  - It builds only the module (import) graph. It supports JSON output and runs incrementally with --cache and --cache-strategy (doc/cli.md). There is no symbol or call information.
- madge: https://github.com/pahen/madge, MIT. Last commit 2024-08-05 (v8.0.0). It has stalled, so dependency-cruiser is the one to use.
- ts-morph: https://github.com/dsherret/ts-morph, MIT. 28.0.0 (2026-04-12), push 2026-11-16.
  - A wrapper library over the TS compiler API, so it gives compiler-level accuracy. The graph has to be built by hand. With no TS7 API it is tied to TS6 (same reason as the TS7 note above).
- knip: https://github.com/webpro-nl/knip, ISC. knip@6.45.1 (2026-11-11).
  - A detector of unused files, exports and dependencies that supports --reporter json. It does not expose the graph itself, so it can serve only as a secondary signal.

### LSP-as-engine
- rust-analyzer: callHierarchy O, implementation O, typeHierarchy X.
- gopls: call hierarchy, type hierarchy and implementation O. Call hierarchy excludes dynamic calls, however.
- tsserver: Unverified: supports call hierarchy (TS 3.8+). The TS7 LSP is "Nearly all features implemented".
- In all of them a query covers one position at a time, so dumping the whole graph is slow; in exchange they react incrementally to edits at once.
- multilspy: https://github.com/microsoft/multilspy, MIT. Commit 2026-10-13.
  - It supports rust-analyzer, gopls and typescript-language-server. The API its README lists is definition, references, completion, hover and documentSymbol; call hierarchy is not listed. Python only.
- serena: https://github.com/oraios/serena. v2.1.0 (2026-10-19). Unverified: an agent tool layer on top of LSP.

### Ranking (one data model for Rust+Go+TS)
1. SCIP with rust-analyzer scip, scip-go and scip-typescript: gives compiler-level def/ref for all three languages in one protobuf schema, is actively maintained, and has already been seen working locally. However, Rust trait impl relations are empty, call edges have to be derived from enclosing_range, and with no incremental support it has to be rerun per file or package and merged.
2. An LSP enrichment layer (rust-analyzer, gopls, tsserver and an LSP client of our own): the cheapest way to fill in the implementation and call hierarchy that SCIP lacks and to handle incremental updates between commits. No server resolves dynamic dispatch, however.
3. Glean (ingesting SCIP): a storage and query backend for when incremental units and a query language become necessary. It brings no precision gain and costs a lot to operate.
4. Joern CPG: gives the call graph and even data flow for the three languages in one graph, but TS is heuristic and it is a JVM batch engine, so it fits as an aid for security and flow analysis rather than as the main engine.

Aids where dynamic dispatch precision is needed: go/callgraph vta for Go, and MIR-based tools (cga, charon) for Rust.
