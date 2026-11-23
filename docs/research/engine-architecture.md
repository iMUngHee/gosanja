# Indexing-engine architecture and data models — research record (2026-11-23)

How existing code-indexing systems separate language-specific extraction from a
language-agnostic core, carry facts that do not fit a common schema, name entities, and
record precision. Covers Kythe, Glean, CodeQL, Joern/CPG, SCIP, Sourcetrail, SemanticDB,
LSIF, Moose/FAMIX, srcML, Semgrep, ast-grep and several Rust codebases, with a paper list.
The addendum answers three design questions: adapters as external commands, tree-sitter
versus native parsers, and templated languages through a transform plus source map.

Collected by a web-research agent on 2026-11-23. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The text is kept as the researcher wrote it.

## Report

Unreachable sources: web.archive.org (the fetch tool refused it), dl.acm.org and cacm.acm.org (HTTP 403), dblp.org and hal.science (bot protection), and Springer (redirects to a login page, which I did not follow). Every statement I could not check on a page I opened is prefixed "Unverified:".

### 1. Per system

1) KYTHE
- Language boundary: a language-specific extractor writes a hermetic CompilationUnit into a .kzip archive (units/ and files/ directories, each file named by its SHA-256). The archive holds "enough information to enable us to replay the compilation to the front-end of the compiler". A language-specific indexer reads the kzip and emits a stream of entries; for tests it can take source text directly.
- Interface: each entry is (source VName, edge kind, target VName, fact name, fact value), serialised as protobuf or JSON. The storage doc lists schema validation as a non-goal: keys and values are plain strings.
- Language-specific facts: node kinds, subkinds and edge kinds can be extended "without recourse to a central authority". Edges take qualifiers (extends/public, extends/private/virtual) and ordinals (param.N). The schema doc has per-language rule sections (C++, Go, Java, Objective-C and others).
- Identity: a VName has five fields (corpus, root, path, language, signature). Signatures must be consistent for identical input and across compilation units, but the schema "does not necessarily need to be stable across different versions of the input". Indexers should avoid source locations in VNames, except for anchors, which are location-based. Long signatures may be hashed.
- Precision and provenance are encoded in the edge kind itself:
  - ref/call versus ref/call/direct (the latter ignores indirect dispatch);
  - ref/implicit and ref/call/implicit (references produced inside template instantiations);
  - instantiates/speculative and specializes/speculative;
  - completedby, and the `complete` fact;
  - semantic/generated, generates and imputes for generated code;
  - `lookup` nodes for names that cannot be resolved;
  - some edges are marked "experimental".
- Storage and query: the entry store (LevelDB, a single file, or SQL) is for storage only, and query efficiency is a stated non-goal. Data is converted into separate serving tables. Indexers emit only forward edges; reverse edges are built in post-processing. Stores compose by set union, and the interface has no delete.
- Testing: a verifier checks goals written as source comments; the goals do not name VNames, so changing the VName scheme does not break tests.
- Regret: the Rust indexer was dropped (see code-graph-engines.md).
- Sources: https://kythe.io/docs/kythe-overview.html https://kythe.io/docs/schema/ https://kythe.io/docs/kythe-storage.html https://kythe.io/docs/kythe-kzip.html https://kythe.io/docs/schema/writing-an-indexer.html

2) META GLEAN
- Language boundary: each language has its own indexer and its own versioned schema (for example scip.1). "Glean doesn't force all the data into a single schema."
- Language-neutral layer: codemarkup.angle (schema codemarkup.31, "Public language-agnostic API"). It is a set of derived predicates over a `code.Entity` union with one branch per language: hack, python, flow, cxx, java, kotlin, csharp, lsif, scip, buck and others.
- SCIP languages: Rust is indexed with "rust-analyzer in SCIP mode" and converted by scip-to-glean into the scip.1 schema. In that schema, scip.Entity has rust, go and typescript variants, and every one of them just wraps a scip.Definition. So all SCIP-derived languages share one thin schema. Codemarkup reaches it for kinds, locations, cross-references, references and IsImplementation.
- Facts and queries: facts are immutable, deduplicated and form a DAG. Each has a 64-bit fact ID, and each key has exactly one value. The query language, Angle, is Datalog-like, and the intro page says it is "limited to non-recursive queries only".
- Derived predicates are either stored (computed with `glean derive` before `glean finish`) or computed on demand at query time. They are also used for schema migration, by defining an old predicate in terms of a new one.
- Schema versioning:
  - Compatible changes (keep the version): adding or removing a field with a defaultable type; adding or removing a sum-type alternative; adding or removing a predicate.
  - Incompatible changes require a version bump.
  - Each schema has a SchemaId hash, and each database records the schema it was written with.
  - `evolves` only applies when the database has no facts of the old schema.
- Incremental indexing:
  - The indexer labels facts with units (strings, typically file names).
  - A new database stacks on an older immutable one and hides units with `--exclude`.
  - Ownership is propagated through the fact graph so that hiding a unit leaves no dangling references.
  - A derived fact's ownership is an expression over its sources' ownership.
  - Claimed costs: 2-3% extra indexing time for Python, +7% database size, under 10% query overhead for typical queries and about 3x for search-heavy queries.
  - Incremental derivation in the general case is "hard" and has been deferred.
  - For C++, the best achievable cost is O(fanout).
- Identity: fact IDs are internal ("like memory addresses"). The blog claims Glass symbol IDs give "a URL for every symbol that doesn't change even if the symbol's definition moves". In the SCIP schema, local symbols are made unique by prefixing the document path.
- Precision: I saw no general precision model.
- Change summaries: "diff sketches" are machine-readable summaries of a changeset, which is directly relevant to your item (d).
- Sources: https://glean.software/docs/introduction/ https://engineering.fb.com/2024/12/19/developer-tools/glean-open-source-code-indexing/ https://glean.software/docs/derived/ https://glean.software/docs/schema/basic/ https://glean.software/docs/schema/changing/ https://glean.software/blog/incremental/ https://glean.software/docs/angle/guide/ https://glean.software/docs/databases/ (does not cover stacking) https://github.com/facebookincubator/Glean/blob/main/glean/schema/source/codemarkup.angle https://github.com/facebookincubator/Glean/blob/main/glean/schema/source/scip.angle https://glean.software/docs/indexer/lsif-rust/
- I found no peer-reviewed Glean paper.

3) CODEQL (design only)
- Extraction: each language has an extractor that writes into a "language-specific database schema" (the dbscheme), and there is one database per language. For compiled languages, extraction hooks into each compiler invocation. The database holds the AST, CFG and data-flow graph.
- Per-language query libraries wrap the tables as classes.
- Language-agnostic algorithms are shared through parameterised modules. In shared/dataflow, `signature module InputSig<LocationSig Location>` is the per-language interface. A language must supply Node, DataFlowCall, DataFlowCallable, viableCallable, nodeGetEnclosingCallable, readStep/storeStep and similar members, and engines such as DataFlowMake\<Location, Lang> are instantiated per language. In short, the schema is per language and only the algorithms are generic.
- Tree-sitter extractor (Ruby): the dbscheme is generated mechanically from tree-sitter's node-types.json; "Our schema-generator and extractor know nothing about Ruby". The blog calls this a trade-off: automation moves work from extraction into the QL library ("has to do more work"). Because files are extracted independently, Rayon parallelism made extraction 8x faster on 8 cores.
- Rust extractor: built directly on rust-analyzer library crates (ra_ap_hir, hir_def, hir_ty, ide_db, load-cargo and others, all at 0.0.358) plus the shared extractor crate. rust/ql/lib/upgrades/ contains hash-named schema-upgrade entries.
- Incremental "overlay" databases: a base database is built from one version and an overlay from the changes. Predicates are annotated local, global or caller; `overlay[discard_entity]` predicates filter stale base entities out of cached results.
- Unverified: TRAP labels are database-local IDs and are not stable across databases.
- Precision: provenance is not a first-class schema concept in what I opened.
- Sources: https://codeql.github.com/docs/codeql-overview/about-codeql/ https://github.blog/security/web-application-security/code-scanning-and-ruby-turning-source-code-into-a-queryable-database/ https://raw.githubusercontent.com/github/codeql/main/shared/dataflow/codeql/dataflow/DataFlow.qll https://codeql.github.com/docs/ql-language-reference/annotations/ https://raw.githubusercontent.com/github/codeql/main/rust/extractor/Cargo.toml https://github.com/github/codeql/tree/main/rust/ql/lib/upgrades https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ECOOP.2016.2

4) JOERN / CODE PROPERTY GRAPH
- Origin (Yamaguchi 2014): AST, CFG and PDG merged into one property graph, stored in a graph database and queried by traversals; the paper reports 18 new Linux kernel vulnerabilities.
- Spec 1.1: a "directed, edge-labeled, attributed multigraph" organised in layers (MetaData, FileSystem, Namespace, Method, Type, Ast, CallGraph, Cfg, Dominators, Pdg, Finding, Shortcuts, and others).
- Language boundary (the key point):
  - Frontends MUST create META_DATA and the AST layer.
  - Frontends MUST NOT create FILE nodes or shortcut edges; NAMESPACE nodes and inheritance edges are derived on load.
  - CALL edges are normally derived from METHOD_FULL_NAME.
  - The CFG "can be automatically generated from the syntax tree layer" when the frontend only uses supported control structures.
  - Overlays are layers added later; META_DATA.OVERLAYS records them in order of application.
- Identity: one METHOD node per METHOD_FULL_NAME and one TYPE_DECL per full name; hashes are optional ("can help with incremental analysis").
- Precision: no formal model. Unknown types are an empty string; a dynamic call may leave METHOD_FULL_NAME blank; for aliasing, frontends must "make a reasonable guess".
- x2cpg: generic passes are abstract classes with an X prefix (for example XTypeRecovery); frontends subclass them.
- Pain points (one developer's notes): `joern-parse` and a frontend run directly produce different CPGs; the pass lists are registered in two places and drift apart; non-deterministic crashes.
- The frontends page lists c, csharp, ghidra, gosrc, javasrc, jimple, jssrc, kotlin, php, pysrc, rubysrc and swiftsrc. Rust is not listed.
- Unverified: type-recovery fields (dynamicTypeHintFullName, possibleTypes) carry candidate types for dynamic languages; I saw them only in search snippets.
- Sources: https://www.ieee-security.org/TC/SP2014/papers/ModelingandDiscoveringVulnerabilitieswithCodePropertyGraphs.pdf https://cpg.joern.io/ https://docs.joern.io/frontends/ https://www.wunused.com/posts/joern-developer-notes/

5) SCIP
- DESIGN.md says: "SCIP is meant to be a transmission format", not a storage or query format.
  - Goals: IDE-level navigation, easy indexer authoring (file-level incrementality, parallelism), robustness ("limited blast radius" for bad data), and easy debugging.
  - Non-goals: code modification, ease of writing consumers, uncompressed compactness, efficient navigation on its own.
  - Deliberate choices: no direct graph encoding, no integer IDs or symbol tables, string symbol IDs, and protobuf length-prefixed framing so indexes can be streamed and merged by concatenation.
- Why it replaced LSIF:
  - opaque numeric IDs that were hard to debug;
  - moniker bookkeeping that could "silently break navigation";
  - global IDs that impose an emit-order constraint;
  - no way to update a subset of documents;
  - no machine-readable schema;
  - compressed LSIF was about 4x larger;
  - off-by-one bugs in indexers broke navigation repo-wide.
- Symbol grammar: scheme, then package (manager, name, version), then descriptors with suffixes (Namespace, Type, Term, Method, TypeParameter, Parameter, Meta, Local, Macro). Local symbols are document-scoped. SymbolInformation.Kind is finer-grained than the suffix.
- Relationships carry only is_reference, is_implementation, is_type_definition and is_definition. SymbolRole bits are Definition, Import, WriteAccess, ReadAccess, Generated, Test and ForwardDefinition. There is no call relation. enclosing_range is optional. New producers SHOULD set typed_range.
- Extending SCIP: there is no official guidance beyond "open an issue" and vendoring the protobuf (at the cost of harder updates). Per-occurrence diagnostics and override_documentation are the only sanctioned extra payloads.
- The scip.proto page now shows the repository under the scip-code organisation.
- Testing: a `scip snapshot` command does golden testing, and the docs recommend deterministic output.
- Inference from the grammar: symbols embed the package version, so symbol strings are not stable identity across releases unless you strip the version.
- Sources: https://github.com/sourcegraph/scip/blob/main/docs/DESIGN.md https://sourcegraph.com/blog/announcing-scip https://github.com/sourcegraph/scip/blob/main/scip.proto https://sourcegraph.com/docs/code-navigation/writing-an-indexer

6) SOURCETRAIL
- Boundary: indexers write through the SourcetrailDB writer API (C++, with SWIG bindings for Python, Java, C# and Perl). Calls include recordSymbol(NameHierarchy) returning an id, recordSymbolKind, recordSymbolDefinitionKind, and recordReference(context, referenced, ReferenceKind such as CALL). The API also records locations, scope locations, local symbols, errors and transactions.
- The C/C++ indexer uses Clang; the Java indexer is called through JNI; the Python indexer is a separate prebuilt program; Qt starts the indexer processes.
- Identity: a NameHierarchy is a delimiter plus elements (prefix, name, postfix); the prefix and postfix are display-only. The same hierarchy always maps to the same id, so identity is the qualified name, and overload disambiguation has to live inside the name element.
- Precision: symbols that are used but never defined appear as "non-indexed" (hatched) nodes; DEFINITION_EXPLICIT clears that state; errors are recorded. There are no confidence levels.
- Node and edge kinds: include, type use, variable use, call, inheritance, override, template specialisation, and bundled edges.
- Refresh mode re-indexes changed files and the files that depend on them; another mode re-indexes files that had errors.
- Unverified: .srctrldb is a SQLite file. A search snippet attributes this to Sourcetrail's docs, but the part of the docs I read did not show it.
- Discontinuation: archived at the end of 2021. The blog post with the reasons was unreachable (web.archive.org was refused). Unverified (from a search snippet of the project's X post): "We haven't been active on the project for quite some time".
- Sources: https://github.com/CoatiSoftware/SourcetrailDB https://github.com/CoatiSoftware/Sourcetrail https://raw.githubusercontent.com/CoatiSoftware/Sourcetrail/master/DOCUMENTATION.md

7) SEMANTICDB and LSIF
- SemanticDB:
  - Organised around TextDocument; sections are optional and partial documents are legal.
  - Has a `schema` version field.
  - `build_target` distinguishes the same file compiled for different targets.
  - Symbol format is language-dependent and "not guaranteed to be globally unique".
  - The Synthetics section stores code "added by compilers that do not appear in the original source" (inferred type arguments, implicit parameters, desugarings), each mapped to a source range. This is the closest precedent for carrying compiler-only facts next to occurrences.
  - Its type model is a superset modelled on Scala, and it covers only Scala and Java.
  - SCIP was "heavily inspired by" SemanticDB.
- LSIF:
  - Stores LSP request results, not symbol information.
  - Graph of vertices and edges with unique IDs, plus result sets, monikers with uniqueness levels, and strict emit-order rules.
  - It modelled the consumer's requests rather than facts, and its IDs are what SCIP's authors blame for the problems listed above.
- Sources: https://scalameta.org/docs/semanticdb/specification.html https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/

8) MOOSE / FAMIX and srcML
- The paper documents the old Famix's regret:
  - Famix was one single-inheritance metamodel with "typical features" in the core and language extensions on top. In practice it was tailored to Java and Smalltalk.
  - It was "too permissive": for example, PrimitiveType inherits the implements relation, and an Interface may implement another Interface, both wrong for Java.
  - Unused properties wasted memory across hundreds of thousands of instances.
  - The single-owner containment assumption breaks on C# partial classes and on extension methods.
  - Visibility semantics differ across languages.
  - Conclusion: "experience ... for more than 15 years showed that we need dedicated meta-models for each language."
- FamixNG instead composes each metamodel from stateful traits:
  - 46 Core traits;
  - 38 Terminal traits, which are "minimal definitions (intersection of languages) rather than maximal ones (union)";
  - 16 Technical traits, for example TSourceAnchor;
  - association traits (inheritance, invocation, access, reference, plus DereferencedInvocation, FileInclude and TraitUsage).
  - Example: JavaClass = TClass + TWithModifiers. Generic tools work against the traits.
- srcML: lossless XML markup over the original text, round-trippable and needing no preprocessing. Tags mix shared names (function, decl, expr) with language-prefixed ones (cpp:...), but the page does not state the rule. Languages: C, C++, C# and Java.
- Sources: https://arxiv.org/abs/2011.10975 https://arxiv.org/pdf/2011.10975 https://www.srcml.org/about.html

9) SEMGREP generic AST, ast-grep
- AST_generic.ml describes itself as "mostly the factorized union" of the supported languages.
  - Aim: stay precise, but not become "too generic" (the way a raw tree is).
  - Escape hatches: OtherXxx constructors with todo_kind, and RawExpr/RawStmt for untranslated tree-sitter subtrees.
  - Warning in the source: OtherStmt "should _not_ contain any statement" or the CFG breaks.
  - Rule: important semantics must be supported directly, not through an other_xxx.
  - Name resolution: Naming_AST.resolve must be run or ids keep a fake value. id_resolved kinds include Global, LocalVar, Parameter and ImportedEntity; id_resolved_alternatives is filled when there are several candidates, which is a candidate-edge model.
  - Contributor docs: "Extend sparingly", "Keep the generic AST as simple as possible", and build the language-specific AST first.
- ast-grep: matches on a CST, not an AST; "Tree-sitter parsers do not encode all semantics with named nodes". Its Rust `Language` trait needs only kind_to_id, field_to_id and build_pattern; meta_var_char (PHP uses '#'), expando_char and from_path are overridable.
- Sources: https://raw.githubusercontent.com/semgrep/semgrep/develop/src/ast_generic/AST_generic.ml https://docs.semgrep.dev/contributing/cst-to-ast-tips https://ast-grep.github.io/advanced/core-concepts.html https://docs.rs/ast-grep-core/latest/ast_grep_core/language/trait.Language.html

10) RUST CODEBASES
- rust-analyzer architecture.md:
  - Crates are layered syntax, then base-db, then hir-expand/hir-def/hir-ty, then hir, then ide, then rust-analyzer.
  - hir-def and hir-ty are "not, and will never be, an api boundary"; hir is a facade over them.
  - The ide API uses POD types and "LSP _does not_ influence its API design"; only the top crate knows LSP and JSON.
  - The core "never does any IO".
  - Inputs and derived data are split with salsa; "typing inside a function's body never invalidates global derived data"; the ItemTree stays stable across body edits.
  - Long computations are cancelled by unwinding; each request runs under catch_unwind; proc macros run in a separate process.
  - Core types are deliberately not serialisable: "serializable types impose significant backwards compatibility constraints".
  - Tests are data-driven and do not test the API.
- CodeQL proves that a Rust fact extractor can be built directly on the ra_ap_* crates (see 3). The cost is tracking an explicitly unstable API.
- biome:
  - The `ExtensionHandler` trait has a single method, capabilities(), returning a `Capabilities` struct. Its groups (parser, debug, analyzer, formatter, search, enabled_for_path, editors) are made of `Option<fn ...>` pointers, so a language can be partially supported.
  - A `Features` registry holds per-language handlers gated by cfg features, with an unknown-language fallback.
  - biome_rowan provides generic Language and SyntaxKind traits over one lossless tree.
- stack-graphs: a core crate, tree-sitter-stack-graphs, and per-language rules written in a DSL. Each file is analysed "completely in isolation" and the per-file graphs are merged at query time. The repository was archived on 2025-09-09 with no reason given.
- ty: "The entire ty architecture is built around incrementality". Unverified (from a talk summary): it builds a per-file semantic index with use-def chains on salsa.
- Not verified: rust-code-analysis's per-language traits (I opened only the README). I did not research oxc.
- Sources: https://github.com/rust-lang/rust-analyzer/blob/master/docs/book/src/contributing/architecture.md https://raw.githubusercontent.com/biomejs/biome/main/crates/biome_service/src/file_handlers/mod.rs https://github.blog/open-source/introducing-stack-graphs/ https://github.com/github/stack-graphs https://astral.sh/blog/ty https://github.com/mozilla/rust-code-analysis

### 2. Comparison table
| system | core/language boundary | extra language-specific facts | identity | provenance/precision | storage |
| --- | --- | --- | --- | --- | --- |
| Kythe | extractor writes .kzip; indexer emits entry stream | extensible kinds, subkinds, qualified edges, free-form facts | VName (5 fields); not stable across versions | edge-kind variants (direct, implicit, speculative, lookup) | entry store, converted to serving tables |
| Glean | per-language indexer into a per-language schema | per-language schema; neutral view via derived predicates (codemarkup) | internal fact IDs; Glass symbol IDs; SCIP symbol strings | none general | immutable stacked databases with unit ownership; Angle queries |
| CodeQL | per-language extractor into a per-language dbscheme | each language has its whole own schema; generic algorithms via InputSig modules | database-local labels (Unverified) | none in schema | relational database per language; overlay databases for increments |
| Joern CPG | frontend emits AST + META_DATA; generic passes derive the rest | layered overlays; language passes subclass generic X passes | full names (METHOD_FULL_NAME) | blank or empty names; type hints (Unverified) | in-memory graph (Unverified: flatgraph) |
| SCIP | indexer emits a protobuf index (transmission only) | none sanctioned beyond diagnostics and docs; vendor the proto | symbol string including package version | roles only; no call edges | none (consumer's job) |
| Sourcetrail | writer API (SourcetrailDB) | fixed node and edge kinds | NameHierarchy (qualified name) | non-indexed vs explicit definitions; errors | .srctrldb (Unverified: SQLite) |
| SemanticDB | compiler plugin emits TextDocument | Synthetics; language-dependent symbols | symbol string, "not guaranteed globally unique" | role only | per-document files |
| LSIF | emitter writes a vertex/edge graph | LSP-shaped results only | numeric IDs plus monikers | none | JSON lines |
| FamixNG | importer builds a model | per-language metamodel composed from traits | object model | none | in-image model |
| Semgrep | per-language AST converted to the generic AST | OtherXxx, Raw subtrees | resolved sid | id_resolved_alternatives | in-memory |

### 3. Papers
1. F. Yamaguchi, N. Golde, D. Arp, K. Rieck. Modeling and Discovering Vulnerabilities with Code Property Graphs. IEEE S&P 2014. https://www.ieee-security.org/TC/SP2014/papers/ModelingandDiscoveringVulnerabilitieswithCodePropertyGraphs.pdf
   - For your design: one layered multigraph where frontends emit only a base layer and generic passes derive control flow. That argues for deriving your control-flow outline (a) generically rather than per language.
2. P. Avgustinov, O. de Moor, M. Peyton Jones, M. Schäfer. QL: Object-oriented Queries on Relational Data. ECOOP 2016, LIPIcs 56, 2:1-2:25, doi:10.4230/LIPIcs.ECOOP.2016.2. https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ECOOP.2016.2
   - Relational facts with a class layer per language. Store facts as relations, not a bespoke graph.
3. E. Hajiyev, M. Verbaere, O. de Moor. codeQuest: Scalable Source Code Queries with Datalog. ECOOP 2006, LNCS 4067, 2-27. Unverified (search result only; Springer requires login). https://link.springer.com/chapter/10.1007/11785477_2
   - Recursive queries such as transitive callers fit Datalog over an RDBMS, so no custom graph engine is needed.
4. Y. Smaragdakis, M. Bravenboer. Using Datalog for Fast and Easy Program Analysis. Datalog 2.0 workshop (Unverified venue and year; the PDF shows none). https://yanniss.github.io/doop-datalog2.0.pdf
   - Separate fact extraction (input relations) from declarative rules. Doop is reported as more than 15x faster than Paddle for the same logic.
5. H. Jordan, B. Scholz, P. Subotić. Soufflé: On Synthesis of Program Analyzers. CAV 2016, LNCS 9780, 422-430 (citation from the Soufflé README; the Springer page needs login). https://github.com/souffle-lang/souffle
   - If derived edges (dispatch candidates, transitive containment) are rules, a Datalog engine keeps the core small.
6. T. Szabó, G. Bergmann, S. Erdweg, M. Voelter. Incrementalizing Lattice-Based Program Analyses in Datalog. PACMPL (OOPSLA 2018). https://voelter.de/data/pub/inca-oopsla2018.pdf
   - Derived relations can be maintained incrementally, with millisecond updates. This shows a path to incrementality without rerunning indexers.
7. T. Szabó, S. Erdweg, M. Voelter. IncA: A DSL for the Definition of Incremental Program Analyses. ASE 2016, 320-331. https://dl.acm.org/doi/10.1145/2970276.2970298 (403; citation from search)
   - The earlier, non-lattice version of 6.
8. P. Néron, A. Tolmach, E. Visser, G. Wachsmuth. A Theory of Name Resolution. ESOP 2015, LNCS 9032, 205-231. https://pdxscholar.library.pdx.edu/compsci_fac/146/
   - A language-independent model of name binding (scope graphs): the principled core if you ever need resolution where SCIP is missing.
9. D. Creager, H. van Antwerpen. Stack Graphs: Name Resolution at Scale. EVCS 2023, OASIcs 109; arXiv:2211.01224. https://arxiv.org/abs/2211.01224
   - File-incremental per-file partial graphs, merged at query time: a template for per-file re-indexing. Caution: the project is now archived.
10. N. Anquetil et al. Modular Moose: A New Generation of Software Reverse Engineering Platform. ICSR 2020, LNCS 12541; arXiv:2011.10975. https://arxiv.org/pdf/2011.10975
    - Compose per-language data models from capability traits that are the intersection of languages, not the union. This is the data-model counterpart of your adapter traits.
11. M. Kim, D. Notkin, D. Grossman. Automatic Inference of Structural Changes for Matching Across Program Versions. ICSE 2007, 333-343 (pages from search). https://web.cs.ucla.edu/~miryung/Publications/icse07-apirule.pdf
    - For version-to-version changes (d), entity matching must be inferred (renames, moves), not assumed from names.
12. M. Kim, D. Notkin. Program Element Matching for Multi-Version Program Analyses. MSR 2006, 58-64. Unverified (dblp blocked; citation from search).
    - A taxonomy of matching techniques to choose from for (d).
13. J.-R. Falleri, F. Morandat, X. Blanc, M. Martinez, M. Monperrus. Fine-grained and Accurate Source Code Differencing (GumTree). ASE 2014, 313-324, doi:10.1145/2642937.2642982. Slides opened: https://www.labri.fr/perso/falleri/img/slides/ase14.pdf ; HAL blocked by bot protection.
    - AST diff with move detection, the standard algorithm for structural diffs.
14. L. Sui, J. Dietrich, A. Tahir, G. Fourtounis. On the Recall of Static Call Graph Construction in Practice. ICSE 2020, 1049-1060, doi:10.1145/3377811.3380441. Unverified (ACM and dblp blocked; citation from search).
    - Static call graphs miss edges from dynamic features. Label edge provenance and avoid calling any static set "exact".
15. D. Helm, S. Keidel, A. Kampkötter, J. Düsing, T. Roth, B. Hermann, M. Mezini. Total Recall? How Good Are Static Call Graphs Really? ISSTA 2024, 112-123, doi:10.1145/3650212.3652114. https://www.opal-project.de/articles/TotalRecall@ISSTA24.pdf
    - Validate call edges against dynamic baselines built from fixed entry points and input corpora. This gives a method to measure your LLVM-IR and SCIP edges.
I found no papers for Kythe or Glean; both are documented only in docs and blog posts.

### 4. Implications for the planned design
1. The plan treats SCIP as the base of the core, but SCIP is a transmission format by design (no call edges, no incremental updates, and rust-analyzer emits no relationships). Glean uses SCIP only as one thin schema behind its own model. Make the core's schema and identity your own, and make SCIP one input adapter.
2. Prior art splits the language boundary at the schema, not only at the code (Glean per-language schemas plus codemarkup; CodeQL dbscheme plus InputSig; FamixNG). Famix's single core metamodel is a documented 15-year regret. Keep a minimal intersection core (entity, contains, occurrence, ref, call, implements) plus namespaced per-language fact tables. Do not grow a union type.
3. Biome's Capabilities pattern (a table of optional function pointers per language) is a lighter alternative to one trait per capability per language, and it makes "language X lacks capability Y" explicit. The viewer must render that absence.
4. There is a simpler alternative for Rust: CodeQL builds its Rust extractor on the ra_ap_* crates (hir). One rust-analyzer-library extractor could produce definitions, references, impls, attributes, visibility and body outlines in one pass. That removes the SCIP-plus-niche join for (a), (b) and part of (c). The cost is chasing an unstable API that releases weekly. Decide this before writing adapters.
5. Lock the join key between producers early. Joining on (file, span) breaks under macro expansion; joining on symbol string forces every niche indexer to reproduce SCIP's grammar exactly. This is the LSIF moniker failure ("silently break navigation") in a new form.
6. Lock identity early, and separate:
   - a version-independent entity key (SCIP symbol minus package version; Kythe and SCIP give no cross-version stability);
   - per-snapshot occurrence keys;
   - a matching step (Kim et al., GumTree) for renames and moves in (d).
   Keep integer IDs out of the serialised boundary.
7. Lock provenance early as a closed enum on every edge: producer (scip / ra / llvm-ir / syntax), resolution (resolved / candidates / unresolved), and evidence site. Prior art: Kythe ref/call/direct, ref/implicit and speculative edges; Semgrep's resolution alternatives.
8. Do not label LLVM-IR edges "exact". They cover only instantiations that were code-generated for one build (cfg, features, target). Record that build context, and check recall against dynamic baselines (papers 14 and 15).
9. Lock schema versioning early. Use Glean's rules (adding or removing defaultable fields is compatible; anything else bumps the version; a schema hash in each database). Keep exactly one serialised engine-to-viewer boundary; rust-analyzer warns that every serialised type becomes a compatibility obligation.
10. The control-flow outline (a) can be a generic pass over a small per-language mapping of control nodes, as the CPG derives its CFG from the AST. Heed Semgrep's warning that statements hidden in escape-hatch nodes break the CFG.
11. Defer incrementality: Glean's units and stacked databases and CodeQL's overlays were added later on top of immutable snapshots. But tag every fact with an owning unit (file) from day one, so exclusion stays possible later.
12. Adopt from the start: deterministic output with golden tests that do not depend on IDs (Kythe verifier, scip snapshot); forward edges only, with reverse edges derived at load time.

## Addendum: adapter boundary, parser choice, virtual documents

### 5. Q1: Adapters as external commands (subprocess or file contract)

Prior art, with the lessons each one documents:

- protoc plugins
  - Discovery is by name: an executable called protoc-gen-NAME anywhere on PATH runs for --NAME_out.
  - Input is a CodeGeneratorRequest on stdin; output is a CodeGeneratorResponse on stdout.
  - Error reporting is split. Input problems go in `error`, and "The plugin process should exit with status code zero even if it reports an error in this way". Protocol or crash problems go to stderr with a non-zero exit.
  - Versioning is negotiated: the request carries compiler_version; the plugin declares a supported_features bitmask and minimum_edition / maximum_edition.
  - Plugins run in command-line order, and earlier plugins can expose insertion points for later ones.
  - Source: https://raw.githubusercontent.com/protocolbuffers/protobuf/main/src/google/protobuf/compiler/plugin.proto

- CodeQL extractor packs
  - Each extractor is a separate program that writes TRAP files.
  - Its pack declares itself in codeql-extractor.yml. Ruby's declares name, version, column_kind "utf8", build_modes, file_types (extension to type), overlay_support_version 20250626, and typed options.
  - Options can be set by flag or file and are validated against each option's regex pattern; unknown options are an error unless the file sets __allow_unknown_properties.
  - So the contract carries its own capability and version fields, including the position unit (column_kind).
  - Sources: https://raw.githubusercontent.com/github/codeql/main/ruby/codeql-extractor.yml and https://docs.github.com/en/code-security/codeql-cli/using-the-advanced-functionality-of-the-codeql-cli/extractor-options

- Kythe
  - The extractor writes a .kzip file and a separate indexer binary reads it (see section 1 of the first report).
  - The file contract makes indexing replayable and lets indexers run anywhere, but every language needs a build-integrated extractor.

- LSP
  - Out-of-process JSON-RPC.
  - Versioning is by capability flags, not by version number: "There is no version handshake in version 3.0x".
  - Unknown properties are ignored, and a missing property means the capability is absent.
  - Lesson on position units: offsets were UTF-16-only until 3.17 added positionEncoding negotiation. UTF-16 "is the default and must always be supported by servers".
  - Source: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/

- Bazel persistent workers
  - Spawning one process per action was costly enough that Bazel added long-lived workers.
  - Protocol: WorkRequest / WorkResponse as newline-delimited JSON or protobuf over stdin/stdout.
  - Claimed speed-ups: 2-4x for Java builds, about 2.5x for compiling Bazel itself.
  - Caveats: state leaks between requests, sandboxing is off by default, and each distinct start-up flag set gets its own worker pool, which costs memory.
  - Source: https://bazel.build/remote/persistent

- Semgrep
  - The documented design is "a Python wrapper (semgrep-cli) around an OCaml engine (semgrep-core)", which "invokes the semgrep-core binary as a subprocess, with a flag to request JSON output".
  - Source: https://docs.semgrep.dev/contributing/contributing-code
  - Unverified (secondary sources only): the "osemgrep" project re-implemented the CLI in OCaml as a single binary to remove the subprocess boundary and the Python runtime. This is the clearest case of moving back in-process.

- tree-sitter grammars as loadable libraries
  - The library checks an ABI version (LANGUAGE_VERSION = 15). It is "generally backward-compatible with languages from older CLI versions but not forward-compatible".
  - Source: https://docs.rs/tree-sitter/latest/tree_sitter/constant.LANGUAGE_VERSION.html

- Zed extensions
  - Extensions are WebAssembly components built against versioned WIT API directories (wit/since_v0.2.0). The manifest records wasm_api_version, and the host picks matching bindings.
  - Tree-sitter grammars ship as .wasm files.
  - Language servers stay external processes: the extension only returns the command, arguments and environment.
  - Source: https://zed.dev/blog/zed-decoded-extensions

- SWC WebAssembly plugins (the cautionary case)
  - Host and plugin both compiled in the serialisation code, using rkyv, which needs an exact memory-layout match. Any AST change broke every plugin.
  - SWC switched to self-describing CBOR plus `Unknown` variants on every AST enum, compatible from @swc/core 1.15.0.
  - It still "does not resolve all compatibility issues": deleting a field or changing its type still breaks plugins.
  - Source: https://swc.rs/docs/plugin/ecmascript/compatibility
  - Unverified (issue reports seen only in search results): breakage across host upgrades continued into 2026.

- dprint
  - Offers both WebAssembly plugins (sandboxed) and process plugins (not sandboxed). Process plugins exist because "many languages can't compile to a single .wasm file".
  - Source: https://dprint.dev/plugins/

- rust-analyzer moved in the opposite direction: proc-macro expansion runs out of process because "Bad proc macros may panic or segfault" (architecture.md, see the first report).

Implications for your adapter boundary:
- The external-command boundary is well supported by prior art, but SWC shows a binary-layout contract fails. Use a self-describing, versioned format: SCIP protobuf plus a side-car file in protobuf or JSON with an explicit schema-version field.
- Put a capability manifest beside each adapter, modelled on codeql-extractor.yml. Declare name, version, contract_version, file_types, position_encoding and the side-car kinds it can produce.
- Copy the protoc error model: diagnostics are data in the output and the process exits 0; a non-zero exit means the adapter itself failed.
- Per-file process spawning is the known performance trap (Bazel). Make one adapter invocation handle a whole scope, or support a long-lived worker mode later.
- Adopting the LSP rule (ignore unknown fields; a missing field means unsupported) gives forward compatibility cheaply.

### 6. Q2: One parser for all languages vs native parsers, for the POST-adapter

What the evidence shows:

- Rust (tree-sitter-rust grammar.js)
  - macro_invocation's body is `alias($.delim_token_tree, $.token_tree)`: raw tokens, never expressions.
  - Attribute arguments are also token_tree, except the `= expr` form.
  - Consequence: control flow and calls inside macros (println!, assert!, select!, async macros) are invisible to a tree-sitter outline. syn sees the same token streams, so syn shares this limit. Only rust-analyzer, which expands macros, does not.
  - Source: https://raw.githubusercontent.com/tree-sitter/tree-sitter-rust/master/grammar.js
  - Unverified (several third-party issues seen only in search): tools built on tree-sitter-rust miss call edges inside macros.

- TypeScript
  - tree-sitter-typescript issue #370 (opened 2026-09-04, still open): TS 4.7 variance annotations (in / out) are not in the grammar. The parse error spreads to the enclosing declaration, so the whole interface, class or type alias yields no usable tree.
  - The reporter found that about 20 other modern constructs (satisfies, accessor, using, const type parameters) parse correctly.
  - Source: https://github.com/tree-sitter/tree-sitter-typescript/issues/370
  - So grammar lag is real but concentrated in edge cases, and when it hits, it loses a whole declaration silently unless you check `has_error`.

- Who uses what
  - Semgrep keeps legacy Menhir parsers and tree-sitter parsers "for historical reasons", but says "in general new languages should use Tree-sitter". Source: https://docs.semgrep.dev/contributing/semgrep-core-contributing
  - GitHub code navigation "uses the open source tree-sitter library" for its search-based navigation across about 20 languages. Its precise navigation goes through stack graphs built on tree-sitter, not native parsers. Source: https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github
  - Difftastic patches inconsistencies between grammars: some treat strings as one token, others don't, and it needs a per-language list of open-delimiter tokens. Source: https://difftastic.wilfred.me.uk/parsing.html

- Leaving generated parsers for hand-written ones
  - Ruff replaced its generated (LALRPOP) parser with a hand-written one, citing ">2x faster" and "fighting LALRPOP to support the latest Python syntax" (from search results; I did not open the blog). Source: https://astral.sh/blog/ruff-v0.4.0
  - Jake Zimmerman (2022) says tree-sitter "places a ceiling on possibilities for future improvement" and that its error recovery misleads autocompletion. Source: https://blog.jez.io/tree-sitter-limitations/
  - Unverified: tree-sitter's author has said its error recovery cannot be customised per domain.
  - Error recovery matters little for you, because your input is code that compiles.

- Position units differ across every parser:
  - tree-sitter: byte offsets, and Point.column "represents the number of bytes" from line start. Source: https://tree-sitter.github.io/tree-sitter/using-parsers/2-basic-parsing.html
  - syn / proc-macro2: LineColumn.column is "0-indexed column (in UTF-8 characters)", and only with the `span-locations` feature. Source: https://docs.rs/proc-macro2/latest/proc_macro2/struct.LineColumn.html
  - go/token: Column is a "byte count". Source: https://pkg.go.dev/go/token#Position
  - SCIP: Document.position_encoding is UTF-8 for Go, Rust and C++ indexers, UTF-16 for JavaScript/TypeScript indexers, UTF-32 for Python. Source: https://github.com/sourcegraph/scip/blob/main/scip.proto
  - Joining SCIP (UTF-16 for scip-typescript) with tree-sitter (bytes) therefore needs one conversion layer in the core.

Recommendation for the POST-adapter:
- Tree-sitter is sound for purely syntactic facts: outline, attributes, visibility, qualifiers.
- It gives one position unit (bytes), one walking API, and grammars for Svelte and other template languages that no native compiler parser covers.
- Conditions:
  (a) Treat token_tree nodes, ERROR nodes and nodes flagged `has_error` as an explicit "opaque or unparsed" outline node. Never report them as "no calls" or "no branches".
  (b) Pin grammar versions per adapter and run a parse-error rate check in CI against the real repositories you have.
  (c) For Rust, macro bodies need rust-analyzer-level expansion. Neither tree-sitter nor syn gives that, so do not switch to syn for this reason.
  (d) Use native parsers only where a grammar is shown to lag on code you actually index.

### 7. Q3: Embedded and templated languages via transform + source map

- Volar (used by Vue language tools; Astro's language server is "powered by Volar"; MDX ships utilities for it)
  - VirtualCode = {id, languageId, snapshot, mappings: CodeMapping[], embeddedCodes?, linkedCodeMappings?}.
  - Mapping = {sourceOffsets[], generatedOffsets[], lengths[], generatedLengths?, data}.
  - CodeInformation flags on each mapping: verification (with a shouldReport callback), completion, semantic, navigation (with shouldRename), structure, format. They exist because features "shouldn't be triggerable on certain 'in-between' regions of generated code".
  - Generated scaffolding is filtered by leaving it unmapped or by switching those flags off on its mapping.
  - Several mappings may match one position: toSourceRange yields every match. fallbackToAnyMatch lets start and end come from different mappings, which is how non-round-trip spans are handled.
  - Volar issue #316 (opened 2026-08-20, still open) proposes deprecating linkedCodeMappings (generated-to-generated edges) in v3 as "an in-process concept that does not map to an external-process transform model".
  - Sources: https://raw.githubusercontent.com/volarjs/volar.js/master/packages/language-core/lib/types.ts https://raw.githubusercontent.com/volarjs/volar.js/master/packages/source-map/lib/sourceMap.ts https://raw.githubusercontent.com/volarjs/volar.js/master/packages/source-map/README.md https://github.com/volarjs/volar.js/issues/316 https://github.com/withastro/language-tools https://github.com/mdx-js/mdx-analyzer

- TypeScript-Go "content mappers" (PR #4712, merged 2026-08-19)
  - An external process speaks JSON-RPC over stdio with initialize, openProject, transform and closeProject. It is enabled by `contentMappers` in tsconfig, and tsc must run with --runExternalCode.
  - A transform returns virtual text, a virtual extension, span mappings and diagnostic directives.
  - Each span mapping is a tuple: (virtual start, virtual length, original start, original length, kind, feature mask).
  - Kinds: Verbatim (same text), Atom (text may differ), Alias (differs, but diagnostics show the original text).
  - Virtual spans may not overlap, but several virtual spans may map to the same original range.
  - "Gaps in the span map are treated as fully synthesized content". Diagnostics there are shown as generated, not dropped, and unused-declaration diagnostics are suppressed.
  - The PR page shows the typescript-go repository archived on 2026-09-01; npm `typescript` is now 7.0.2.
  - Unverified: content mappers ship in TypeScript 7.
  - This is the closest thing to an emerging standard data model, and it already matches your PRE-adapter contract (external process, transform, mappings).
  - Source: https://github.com/microsoft/typescript-go/pull/4712

- svelte2tsx
  - Returns {code, map}, where the map is a version-3 source map (magic-string type). The output is "ugly but type checkable".
  - Source: https://raw.githubusercontent.com/sveltejs/language-tools/master/packages/svelte2tsx/README.md
  - Issue #1077 (2021) records that the MagicString approach limits mapping precision "especially for ranges", which are "often off by one", and led to a rewrite of the transform. Source: https://github.com/sveltejs/language-tools/issues/1077
  - Unverified (seen in search results, not in an opened source file): svelte2tsx wraps generated scaffolding in /*Ωignore_startΩ*/ … /*Ωignore_endΩ*/ comments, and the language server drops diagnostics that start inside those markers.

- Angular type-check blocks
  - The generated TypeScript carries in-band comment trivia: parse-span comments matching /^(\d+),(\d+)$/, and a "D:ignore" marker read by hasIgnoreForDiagnosticsMarker ("diagnostics errors should be ignored").
  - Source: https://raw.githubusercontent.com/angular/angular/main/packages/compiler-cli/src/ngtsc/typecheck/src/comments.ts

- Source map format ECMA-426
  - Its README lists "Range Mappings" as a Stage 2 proposal and "Scopes" as Stage 3. My inference: the current format maps points, not ranges.
  - Source: https://raw.githubusercontent.com/tc39/ecma426/main/README.md

Answers to your three sub-questions:
- Filtering generated scaffolding: either leave it unmapped (TypeScript-Go gaps; Volar code with no mapping) or put a feature mask on each mapping (Volar CodeInformation; TypeScript-Go feature bitmask). In-band comment markers (Svelte, Angular) are the older, more fragile pattern.
- De-duplication: both models allow several generated spans to map to one original range. De-duplicate in the core by (original range, symbol, role) after mapping back, and keep the occurrence with the highest-provenance mapping kind.
- Non-round-trip mappings: TypeScript-Go makes this explicit with Atom and Alias kinds, and Volar's fallbackToAnyMatch handles start and end coming from different mappings. Drop or mark as "approximate" any occurrence whose start and end do not land in one mapping.
- Data model to reuse: do not invent one, and do not use plain version-3 source maps (point-only; svelte2tsx documented off-by-one ranges). Adopt TypeScript-Go's span tuple and kinds as the side-car schema; it is the newest design and was built for an external-process boundary like yours. Use Volar's six feature flags as the meaning of the mask.

### 8. Licence notes (only for things I recommend adopting)

Fine for an MIT or MIT/Apache-2.0 engine:
- SCIP schema (scip.proto) and the `scip` crate 0.10.0: Apache-2.0. Vendoring the .proto means keeping the licence text and any NOTICE; Apache-2.0 grants a patent licence that terminates if you start patent litigation. Repository: scip-code/scip.
- rust-analyzer ra_ap_* crates (0.0.363, 2026-11-16), salsa 0.28.6, syn 3.0.10: MIT OR Apache-2.0. Keep the copyright and licence text.
- tree-sitter 0.27.2, tree-sitter-rust 0.24.2, tree-sitter-typescript 0.23.2, tree-sitter-go 0.25.0: MIT. Attribution only; generated parser.c is MIT too.
- scip-typescript 0.6.2 and scip-go: Apache-2.0. If you only invoke them as external commands, nothing is linked; if you redistribute their binaries, ship licence and NOTICE.
- TypeScript 7.0.2 and typescript-go (content-mapper protocol): Apache-2.0. Using the protocol's data model as a design inside your own schema carries no copyleft.
- svelte2tsx 0.7.63 and @volar/source-map 2.4.31: MIT. Volar's flags as a design reference carry no obligation; copying code means keeping the MIT notice.
- Soufflé (if you ever adopt Datalog): UPL-1.0, which is OSI-approved, permissive and includes a patent grant. It is C++ code generation, so it is a build-time tool.

Flags:
- GumTree: LGPL-3.0 (from its LICENSE file). Do not vendor or statically link it into an MIT binary. Reimplementing the published algorithm (ASE 2014 paper) is fine; running it as a separate external command keeps the LGPL out of your binary.
- CodeQL: the CLI and engine are proprietary. Only the github/codeql repository's code (libraries, extractors) is open. Unverified: that repository is MIT. Its CLI licence forbids analysing private code without a paid licence (see code-graph-engines.md). Do not depend on the CLI; borrowing ideas is fine.
- srcML (the site says GPL), Sourcetrail and Glean/Kythe code: I am not recommending adopting any of them; their designs only.

## Licence notes, second pass

Sent later by the same researcher; it adds items the first notes did not cover. Licence
texts read in full: SCIP, scip-go, typescript-go, Kythe, Glean, Joern, GumTree, Soufflé.
The other licences are from the crates.io or npm metadata field, or the repository README
where noted.

- biome crates (biome_rowan 0.7.3): MIT OR Apache-2.0 (crates.io). The Capabilities
  pattern as an idea carries no obligation. Copied code carries the MIT notice, or the
  Apache-2.0 licence plus NOTICE.
- Datalog engine, only if one is adopted; not needed for the first version: crepe 0.2.0
  (MIT OR Apache-2.0), ascent 0.8.1 (MIT), datafrog 2.0.1 (Apache-2.0/MIT), all from
  crates.io. Dependency, notice only. Soufflé is UPL-1.0 (permissive, OSI-approved, with a
  patent grant) and is a C++ code-generation tool, so prefer the Rust crates.
- Schemas whose text might be adapted, recommended as ideas only so that no schema text
  needs copying:
  - Kythe: Apache-2.0, plus an NCSA section copied from LLVM (LICENSE file). Copying schema
    text requires the licence and NOTICE.
  - Glean: BSD (LICENSE file). Copying Angle text requires keeping the copyright notice and
    licence.
  - CPG specification: Joern is Apache-2.0 (LICENSE file). Unverified: the specification's
    own licence, because the codepropertygraph repository returned 404.
- The github/codeql repository is MIT according to its README. The CodeQL command-line tool
  and engine are licensed separately, and analysing closed-source code requires a
  commercial licence. Use its patterns only as ideas.
- srcML is GPL according to https://www.srcml.org/about.html and is not recommended for
  adoption.
