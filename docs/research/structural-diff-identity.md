# Structural diff, entity identity and incremental indexing — research record (2026-11-23)

Matching entities across versions (rename and move detection), symbol-naming schemes and
their stability, API-surface diff tools, architecture-level change and drift, and how
existing systems avoid re-indexing everything.

Collected by a web-research agent on 2026-11-23. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The report was written in Korean and is given here in English translation.

## Report

### A. Entity identity / matching

[Godfrey & Zou 2005, origin analysis] Source reviewed: abstract. Beagle detects file and function merges and splits in procedural code from changes in call relationships. The evaluation is a PostgreSQL case study. It gives no quantitative evidence of accuracy.
https://swag.uwaterloo.ca/publications/using-origin-analysis-to-detect-merging-and-splitting-of-source-code-entities.html
Unverified: The abstract does not say whether matching uses names, signatures or metrics.

[RefDiff 2.0, Silva et al. TSE 2020] Source reviewed: paper PDF (p.4-9, 12-14). This design is the most portable.
- Input: RefDiff parses only changed files to build a Code Structure Tree (CST). Each node has an identifier (name and parameter signature to distinguish overloads), namespace, node type, token list and body tokens. The tree also carries simple call graph and type hierarchy edges (uses, subtype).
- Algorithm (Fig.3): (1) ById recursively matches nodes with the same id and parent. (2) BySim greedily matches remaining pairs in descending order of similarity. It uses TF-IDF-weighted Jaccard similarity over token multisets with a single threshold of 0.5. (3) ByChildren matches remaining parents if at least 2 children match and nameSim>0.5 (class moves and renames). (4) It classifies relationships after matching (Same, Change Signature, Move, Rename, Move+Rename, Pull Up/Push Down) and resolves conflicts greedily in similarity order. (5) Extract/Inline combines uses edges and an asymmetric similarity measure that asks whether the new method body contains the multiset of removed code.
- Accuracy: On an oracle of 3,248 Java refactorings, precision was 96.4% and recall 80.4% (RMiner 1.0: 98.8% / 81.3%). Median time per commit was 157 ms (RMiner 109 ms) on an i5-750, analyzing only changed files.
- Limits: The oracle draws on the output of 3 tools, so recall is approximate, as the paper acknowledges. RefDiff depends on parsers (Java/C/JS plugins).
https://homepages.dcc.ufmg.br/~mtov/pub/2020-tse-refdiff.pdf
Unverified: JS/C precision of 88-91% comes from a search snippet. Only the evaluation design was confirmed on p.14.

[RefactoringMiner, Tsantalis et al.] Sources reviewed: README, accuracy.md, release 3.2.0 notes and arXiv abstract.
- Languages (as of 2026-11): Java, Python, Kotlin, TypeScript, JavaScript and C/C++. Release 3.2.0 replaced tree-sitter with native parsers (Kotlin PSI, swc4j for TS/JS, Python ANTLR and C++ Eclipse CDT). Rust is unsupported.
- Accuracy (accuracy.md, updated 2026-11): Java B1 P0.999/R0.984 (12725 TP), Java B2 P0.993/R0.986, Python P0.997/R0.998, Kotlin P0.998/R1.000 and JavaScript (Chart.js) P0.988/R1.000 (171 TP). There is no TypeScript-specific benchmark. The lowest results were Extract And Move Method recall 0.701 and Move And Inline Method recall 0.750 (B1).
- Speed (3.2.0 notes): AST diff parsing was 5.3x faster (155 s to 29.5 s over 1,091 cases). Some cases with runaway execution times were optimized.
- Technique: Refactoring-aware AST diff based on statement mapping (arXiv 2403.05939 benchmark of 800 bug-fix and 188 refactoring commits). It operates on the AST and statements rather than a symbol graph, at substantial cost.
https://github.com/tsantalis/RefactoringMiner
https://github.com/tsantalis/RefactoringMiner/blob/master/documentation/accuracy.md
https://github.com/tsantalis/RefactoringMiner/releases/tag/3.2.0
https://arxiv.org/abs/2403.05939
Unverified: Multi-mapping reached 99.7%/98.4% (search snippet).

[GumTree, Falleri et al. ASE 2014 / TSE 2023 / ICSE 2024] Source reviewed: README only. It lists support for C, Java, JavaScript, Python, R and Ruby and cites 3 papers. It gives no algorithm details or accuracy figures.
https://github.com/GumTreeDiff/gumtree
HAL (hal.science) returned an Anubis access block instead of the full text. I did not bypass it.
Unverified: The algorithm has 2 stages, top-down (minHeight) and bottom-up (minDice). A search snippet citing the TOSEM paper reports precision of 59% and recall of 10% for GumTree 3.0 simple on multi-mapping, and a statement F-score of 94.2%.

[ChangeDistiller, Fluri 2007] I did not open the paper. Unverified: Leaf nodes use bigram string similarity with f=0.6, and internal nodes use a dynamic threshold (search snippet).
Reference URL (search result): https://pinzger.github.io/papers/Fluri2007-changedistiller.pdf

[difftastic] Source reviewed: README. It computes structural diffs for 30+ languages using tree-sitter parsers and Dijkstra's shortest-path algorithm. The README does not mention move or rename detection. Files with many changes scale poorly and use substantial memory. The diff operates on files and has no concept of item identity.
https://github.com/Wilfred/difftastic

[sem (Ataraxy-Labs), a sem-style tool] Source reviewed: README. It uses tree-sitter to extract functions, classes and methods as entities. Matching has 3 stages: (1) exact ID (entityId format `src/auth.ts::function::validateToken`), (2) structural hash (same AST structure, ignoring whitespace and comments) for renames or moves where only the name changes, (3) fuzzy rename if at least 80% of tokens overlap. `sem impact` reports dependencies, dependents and tests. The README gives no accuracy figures. The 80% threshold is fixed.
https://github.com/Ataraxy-Labs/sem
Unverified: A search snippet says it is available as a Rust crate (sem-core).

[git rename detection] Source reviewed: git-diff docs. Detection operates on files with a default similarity of 50% (-M\<n>). Fast exact and basename stages precede an exhaustive O(N^2) comparison of remaining candidates, which the -l limit can skip. -C detects copies. Detection does not operate on individual items (functions).
https://git-scm.com/docs/diff-options

[Identity scheme: SCIP symbol] Source reviewed: scip.proto. `<scheme> <manager> <package-name> <version> <descriptor>+`. The syntax requires a version (the manager uses "." for an empty value). The proto describes descriptors as a fully qualified name unique within the package. It does not discuss stability across versions. A local symbol has the form `local N` and is scoped to a document.
https://github.com/scip-code/scip/blob/main/scip.proto
rust-analyzer implementation reviewed (scip.rs): The scheme is "rust-analyzer", the manager is "cargo", and the version is "." if the moniker lacks one. An inherent impl uses `impl#[SelfType]` and a trait impl method uses `impl#[MyStruct][MyTrait]func().`. For multiple inherent impls of the same Self, SymbolInformation retains only the first (FIXME #18772). The local counter resets for each document.
https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/rust-analyzer/src/cli/scip.rs
scip-go: The README lists `--module-name/--module-version` overrides, suggesting that symbols include the module version. It gives no detailed account of symbol construction. https://github.com/scip-code/scip-go
scip-typescript: The README does not describe the symbol format. https://github.com/sourcegraph/scip-typescript

[Kythe VName] Source reviewed: schema documentation. A VName has 5 fields (language, corpus, root, path, signature). The signature must be generated consistently for the same input but need not be stable across versions of the input. A hash fingerprint is permitted. The corpus can identify a revision (a full commit hash is recommended for vcs nodes). Only file nodes have stronger stability. The design does not aim for stable identity across versions.
https://kythe.io/docs/schema/

[rustdoc JSON Id] Source reviewed: docs.rs. An Id is "opaque", with no guarantees about its internal value. It is valid only within a single JSON blob and cannot be resolved across crate JSON files. This makes it unsuitable as a cross-version key. (cargo-semver-checks matches by public import path rather than id.)
https://docs.rs/rustdoc-types/latest/rustdoc_types/struct.Id.html

[Go objectpath] Sources reviewed: pkg.go.dev and source code. It names objects within a package by a sequence of operations starting at package scope. The aim is to match objects across type-checking instances (source versus export data). A source comment describes indices as stable across "different representations of the same package, even source and export data". A method index "ultimately derives from source order". The documentation does not specify stability across versions after edits. Only package-level types, exported non-types, methods, parameters/results and struct fields can be named. Unexported non-types and locals cannot.
https://pkg.go.dev/golang.org/x/tools/go/types/objectpath
https://raw.githubusercontent.com/golang/tools/master/go/types/objectpath/objectpath.go
Unverified: I infer that reordering methods could change their paths because the indices depend on source order.

[TS API Extractor] Source reviewed: declaration reference documentation (`Widget.render`, `pkg#Widget`, and the `(ShirtSize:enum)` selector for merged declarations). References always resolve relative to the entry point. The syntax is "still evolving". The page does not mention canonical references. It confirms only that an API report is a concise report that facilitates review. I did not confirm the filename (.api.md) on the page.
https://api-extractor.com/pages/tsdoc/declaration_references/
https://api-extractor.com/

Synthesis: None of these schemes aims for cross-version stability. The closest approach uses a name path without a version (the public path in cargo-semver-checks, name correspondence in apidiff, and entry-point-relative references in API Extractor). SCIP falls into the same category if its version field is removed.

### B. API-surface / breaking-change tools

[cargo-semver-checks] Sources reviewed: repository README and predr.ag post. It queries rustdoc JSON with the Trustfall query engine. Each lint is a query. The slides list 58 lints (FOSDEM 2024, the README says "dozens"). For each old public function, it looks for a public function with the same public API import path in the new version. A missing function is a breaking change. Some lints verify findings with a witness (compiling a downstream example). The baseline can be the preceding crates.io version, a git rev, a source directory or rustdoc JSON. Results show the lint name (e.g. function_missing), affected item, file and line, whether a major release is required, and a reference link. Blind spots include missing lints for some type/generic/lifetime changes, violations in feature subsets, macros, cross-crate re-exports (awaiting upstream functionality), target-specific APIs and susceptibility to rustdoc JSON format changes (nightly dependency, mitigated by multiple adapter versions). An adapter index fast path optimizes name lookups (2000x in the post title).
https://github.com/obi1kenobi/cargo-semver-checks
https://predr.ag/blog/semver-in-rust-tooling-breakage-and-edge-cases/
Unverified: Details of the 2000x figure come only from the search result title. I did not open the post (https://predr.ag/blog/speeding-up-rust-semver-checking-by-over-2000x/).

[cargo-public-api] Source reviewed: README. It lists public items from rustdoc JSON, one per line. `diff <version>` / `diff ref1..ref2` show added, removed and changed items. In CI, a text snapshot file, a failing `cargo test` and a git diff support review. Blanket, auto trait and auto-derived impls appear by default and add noise. It requires nightly, with a fixed range of supported nightlies per release. The README does not explain matching or rename detection.
https://github.com/cargo-public-api/cargo-public-api

[Go apidiff] Source reviewed: pkg.go.dev. It compares two packages or modules and classifies changes as compatible (minor) or incompatible (major) based on whether code can compile. Types correspond if they have the same name or can be renamed without changing the rest of the API. Multiple old types can correspond to one new type, but the reverse is disallowed. The rule extends structurally (slice elements must correspond). Blind spots: It cannot detect behavioral changes and ignores 5 cases, including unkeyed struct literals, embedding/shadowing, unsafe.Sizeof and merged type switches. Moving a module into a nested module is reported as removal.
https://pkg.go.dev/golang.org/x/exp/apidiff

[japicmp] Source reviewed: README. It compares class files in two jars using javassist, reports changes to classes, methods, fields and annotations, distinguishes source and binary compatibility, excludes private members by default, recommends a semver bump and produces text/Markdown/XML/HTML. The author claims it processes about 1,700 classes per archive in under 1 second. Non-class artifacts are deliberately out of scope.
https://github.com/siom79/japicmp

[revapi] Source reviewed: README. An API can include configuration or schemas as well as classes. It computes the API "surface" by tracing type usage to report dependency leakage, classifies changes by source/binary/semantic compatibility and allows reclassification. Reporter plugins include stdout/Maven site/FreeMarker/JSON. The project says it can extend to anything decomposable into a tree, but the current implementation covers only Java.
https://github.com/revapi/revapi

[TS api-extractor] See section A above. I confirmed only that the report is reviewed in PRs.

### C. Architecture-level change / drift

[Reflexion models, Murphy/Notkin/Sullivan FSE 1995] I did not open the paper (search result text only). Unverified: A human-authored high-level model and source mapping yield convergence/divergence/absence. The case study covers NetBSD at 250 KLOC, the tool is RMTool, and a follow-up appeared in TSE 2001.
https://www.cs.ubc.ca/~murphy/papers/rm/fse95.html (from a search result, unopened)

[Architecture erosion surveys] I did not open these sources. Unverified: The mapping study by Li et al. (arXiv 2112.10934) identifies 4 symptom categories (structural/violation/quality/evolution). One study found 606 violation symptoms in code review comments (arXiv 2212.12168). Another reports GPT-4o F1 of 0.851 for automated identification and an increase in developer detection from 25.9% to 64.7% when shown symptoms (arXiv 2306.08616). An SLR (arXiv 2504.04334) identifies the lack of tools that aggregate erosion signals longitudinally as a challenge.

[AI agent changes and architecture]
- Source reviewed: arXiv 2604.04990, "Architecture Without Architects" (Konrad et al., 2026-04). Agents make implicit architectural decisions such as framework and infrastructure choices, and humans do not review them as architecture. The authors propose review practices, decision records and tooling. The abstract gives no quantitative results or specific dependency-graph diff method.
https://arxiv.org/abs/2604.04990
- Unverified: Search snippets say ContextCov (arXiv 2603.00822) compiles constraints in instruction files into checks for layer violations, dependency cycles and module placement. arXiv 2608.21747 reports that static constraint checks (~80% architectural coverage) and an LLM judge complement each other. arXiv 2512.04273 measures illegal Domain layer imports in hexagonal architecture using AVR.
- I found no user study on which structural diffs reviewers find useful. The search returned only mining studies and practitioner guides. Unverified: Vendor figures (Faros AI 2026: 5x median PR review time, PR size +51%. CodeRabbit: 4.6x time to first review) come from non-peer-reviewed snippets. A practitioner guide (Aviator) recommends looking at dependency diffs and imports first after the PR description (snippet).

[DSM-based change views] Unverified: A search snippet says Cai & Huynh, WICSA 2008 (https://www.cs.drexel.edu/~yc349/papers/wicsa2008_dsm.pdf), build a class DSM per release and use the proportion of classes added or removed within a module as an architectural change metric (Tomcat case study). I found no dedicated "DSM diff" tool in the search. The usual approach builds a DSM per release and compares added or removed elements and dependencies, and propagation cost.
[Evolution Radar] Unverified: A snippet describes D'Ambros/Lanza, TSE 2009. The tool arranges file-level co-change radially around a module and distinguishes persistent from recent coupling. It measures logical coupling rather than structural (symbol) dependencies.
[CodeCharta Delta View] Unverified: It compares two maps (CCSH generated per commit) on a 3D city map and colors added and removed buildings. A search snippet notes that the documentation and README describe the colors differently. I did not open the official page https://codecharta.com/docs/visualization/delta-view (the attempted /docs/overview/delta-mode returned 404).
[CodeScene] I tried to open the documentation. The page returned only a "maintenance" notice. I could not verify the content and did not bypass the notice.

### D. Incremental indexing / snapshot storage

[Glean] Sources reviewed: blog and Meta engineering post.
- Glean stacks immutable databases. An upper layer adds facts to or hides facts from the layer below. Clients see a single database. An immutable base allows access to multiple revisions without a full copy.
- A unit (a string, usually a filename or module name) owns facts. Glean propagates ownership through the graph to prevent dangling references. Reindexing with `--incremental <old> --exclude A,B` hides units and adds new facts. A derived fact is visible only when all its supporting facts are visible.
- Measurements: Ownership data adds about +7% to database size. Indexing overhead is 2-3% for Python. Query overhead is generally <10% (about 3x for heavy queries). Full indexing took hours, prompting adoption. The post gives no end-to-end speedup. The goal is O(changes), but the post says fanout makes it O(fanout) in practice for languages such as C++.
https://glean.software/blog/incremental/
https://engineering.fb.com/2024/12/19/developer-tools/glean-open-source-code-indexing/
- Neither page says whether facts are content-addressed. Unverified: A snippet from the official incrementality docs says facts are deduplicated by key and stored once, with a database-local 64-bit integer ID rather than a content hash. https://glean.software/docs/implementation/incrementality/ (unopened)
- The post describes Glass symbol IDs (`REPOSITORY/cpp/folly/Singleton`) as stable when a definition moves.

[SCIP design intent] Source reviewed: SCIP announcement. LSIF's globally increasing IDs made it difficult to update individual documents. SCIP aims to enable incremental indexing of only changed files on each push, described as a "future capability". Compared with LSIF, output is 4x smaller with gzip and 5x smaller uncompressed.
https://sourcegraph.com/blog/announcing-scip
SCIP protocol documentation reviewed (docs/scip.md): An Index can be streamed, with 1 metadata record at the start. A Document is identified by relative_path, local symbols are scoped to the document, and external_symbols document symbols from other indexes. The page does not cover incremental indexing, merging or the CLI.
https://github.com/scip-code/scip/blob/main/docs/scip.md
The Sourcegraph writing-an-indexer documentation URL returned 404.

[Per-document caching and merging in practice (third-party)]
- scip-swift README reviewed: Each Document is cached under SHA-256(relative path + file content hash). Renaming a file causes a miss. A change in toolchain/indexstore/backend/symbolFormatVersion invalidates the entire cache. Overload indices depend on the entire repository, so a change in the overload fingerprint invalidates all cached documents (a conservative approach, with per-group improvements a v2 TODO). The README gives no general mechanism for cross-file references. Handling cross-file dependencies is therefore a central risk for per-document reuse.
https://github.com/alex2481kobe/scip-swift
- scip-io reviewed: It merges language-specific SCIP indexes, combines documents with the same Document.relative_path, deduplicates occurrences and document symbols, normalizes paths relative to the repository root and produces deterministic (byte-identical) output. This merges indexes across languages rather than updating them incrementally.
https://github.com/GlitterKill/scip-io
I found no evidence that Sourcegraph officially documents per-document SCIP merging as a practice (I could not open the official documentation).
- Unverified: A snippet from the CKB wiki describes rerunning scip-go in full and postprocessing only changed documents (protobuf cannot be partially updated, and import and type resolution require the full context). A scip-clang roadmap snippet mentions using build system incrementality.

[salsa] Source reviewed: overview. Salsa records the inputs a tracked function reads and memoizes the result. When inputs change, the function reruns only if an input it read has changed (red-green). The page does not explain durability or early cutoff.
https://salsa-rs.github.io/salsa/overview.html
Unverified: I found no accessible 1st-hand source in this investigation for Kythe/Bazel build-driven invalidation or Sourcegraph's handling of per-commit uploads.

### Implications (at most 12 lines)
1. Key = (language, package/crate name, version-stripped SCIP descriptor path, kind). The signature hash is an attribute outside the key. Exclude the SCIP version field (".") from the cross-version key and use only the descriptor path for the name. This follows the name path and name-based correspondence used by cargo-semver-checks/apidiff.
2. Disambiguate collisions on the same path (rust-analyzer merges inherent impls for the same Self, FIXME #18772) with a hash of generics/where clauses instead of a declaration-order ordinal. Report the collision itself as a diff warning.
3. Handle renames and moves after key matching. Stage 1 matches exact keys. Stage 2 detects signature changes for the same parent and name. Stage 3 groups unmatched removed×added pairs by kind and applies RefDiff-style similarity (body token multiset TF-IDF Jaccard ≥0.5, greedy in descending similarity order). Stage 4 confirms parent moves (types/modules) from the proportion of matched children (RefDiff ByChildren).
4. A symbol graph can capture Godfrey/Zou's call relationship evidence without an AST. Alongside body tokens, use Jaccard similarity for the set of referenced symbols (callee set) and the set of reference sites. Use file renames from git -M to narrow candidates (changed path prefix and the same relative name).
5. GumTree/ChangeDistiller/RefactoringMiner/difftastic operate on ASTs or statements, so per-language parsers are costly. RM lacks Rust support. I judge a lightweight RefDiff-style CST sufficient for item-level diffs (basis: RefDiff P96.4/R80.4, ~157 ms per commit). Unverified: Whether a SCIP-based symbol graph retains this accuracy requires measurement.
6. Borrow from API diff tools: (a) store the public surface as a git-diffable snapshot of normalized signature text per item (cargo-public-api), (b) classify changes as compatible/breaking and show the lint name, location and a link explaining why (cargo-semver-checks), (c) match types if a rename leaves the rest of the API unchanged, allowing many-to-one matches (apidiff), (d) hide blanket/auto/derive impl noise by default, and (e) expose dependency leakage (revapi). Unverified: I could not confirm whether SCIP carries visibility. If it does not, language-specific source information is needed.
7. State the blind spots in the report: macro-generated items, feature/cfg combinations, re-export paths and behavioral changes (apidiff and semver-checks both document limitations).
8. Lowest-cost incremental strategy: SCIP has no official incremental feature (the announcement describes it as future work). Index by package/crate and reuse the previous SCIP result when the key = hash(package sources + dependency package hashes + toolchain/indexer versions) is unchanged. In scip-swift, file-level caching falls back to full invalidation for cross-file dependencies such as overloads. Finer granularity (documents) requires handling fanout (Glean also runs in O(fanout) in practice).
9. For comparisons, one snapshot is usually already cached, so index only B. A diff between revisions can be restricted to documents whose content hash changed and documents that reference them (fanout). Unverified: I did not establish whether SCIP reference direction alone is sufficient to select those documents.
10. Snapshot storage: Store document bytes (SCIP Document) by content hash. Represent a snapshot as a path→hash manifest (git tree-style structural sharing). This gives the effect of Glean's immutable base and stacked layers without copying the entire database across revisions. Unverified: I found no evidence that Glean itself is content-addressed.
11. Evidence for review UX is weak. I found no user study of structural or dependency diff views for reviewing AI changes. Search snippets describe research that checks architectural violations (layer violations and dependency cycles) using dependency graphs (e.g. ContextCov). A list of new inter-module dependencies and cycle/layer violations has the strongest basis as a first output.
12. Unreachable: hal.science (Anubis block), CodeScene documentation (maintenance), Sourcegraph writing-an-indexer (404). I did not bypass these barriers.

## Licence notes, first pass

Licence notes (additional section). I checked the following directly in GitHub sidebars, LICENSE files, READMEs or the GitHub API license endpoint. This section covers only the recommendations in the preceding report.

Candidates for use or porting
- RefDiff (algorithm to port, https://github.com/aserg-ufmg/RefDiff): MIT. Porting or copying code requires only retaining the copyright notice and the full MIT text (attribution). The algorithm is published in the paper (TSE 2020), so a clean-room reimplementation is also possible. There is no patent clause.
- SCIP schema/CLI (https://github.com/scip-code/scip): Apache-2.0. Copying scip.proto or vendoring generated code requires including the LICENSE, marking modifications and retaining the NOTICE file if present. There is an explicit patent grant. It is compatible with a project dual-licensed under MIT/Apache-2.0.
- scip-go (https://github.com/scip-code/scip-go), scip-typescript (https://github.com/sourcegraph/scip-typescript): Both are Apache-2.0. If the engine only invokes them as external processes, there are effectively no distribution obligations. Bundling them entails the Apache obligations above.
- rust-analyzer (invoked as a SCIP generator, https://github.com/rust-lang/rust-analyzer): MIT OR Apache-2.0 (stated in the README). Invocation alone carries no obligations. Reusing code such as scip.rs requires retaining the relevant notices.
- salsa (https://github.com/salsa-rs/salsa): MIT OR Apache-2.0. Using it as a dependency requires only the notice. The evidence in this investigation for choosing salsa is conceptual only (red-green).
- cargo-semver-checks (https://github.com/obi1kenobi/cargo-semver-checks): Apache-2.0 OR MIT ("at your option"). Retain the applicable notices when reusing code or lint queries.
- cargo-public-api (https://github.com/cargo-public-api/cargo-public-api): MIT (confirmed via GitHub API). Retain the notice.
- Go apidiff (golang.org/x/exp/apidiff): BSD-3-Clause, Copyright The Go Authors. Source redistribution requires retaining the copyright notice, conditions and disclaimer. Binary redistribution requires including them in documentation or other materials. The license prohibits using the name for endorsement. Unverified: x/exp is an experimental module with no stability guarantee (general knowledge, source unconfirmed).

Reference or technique reuse only (code adoption discouraged or license requires care)
- GumTree (https://github.com/GumTreeDiff/gumtree): LGPL-3.0 (LICENSE text reviewed). Linking or modifying the library entails LGPL obligations (disclosing modified source and allowing users to relink), with substantial friction for statically linked Rust binaries. A port into a permissively licensed repository may be a derivative work subject to the LGPL. Reimplementing only the technique from the paper is the safe approach. (Determining derivative-work status calls for legal advice.)
- RefactoringMiner (https://github.com/tsantalis/RefactoringMiner): MIT. Its code can be reused, but a Rust engine port would be costly because it uses Java and language-specific parsers. Invoking it as an external tool requires only the MIT notice.
- sem (https://github.com/Ataraxy-Labs/sem): Apache-2.0 (confirmed via GitHub API. The raw LICENSE URL returned 404, so I could not check the text directly). Use the 3-stage matching idea only as a reference.
- difftastic: Not recommended. I did not investigate the license.
- Glean (https://github.com/facebookincubator/Glean): BSD (the README says only "BSD LICENSE". I could not confirm whether it is 2-clause or 3-clause). Only the design ideas are under consideration. There is no plan to adopt the dependency or code.

Technique or paper reuse only
- The reflexion model of Murphy et al., Godfrey/Zou origin analysis, Cai/Huynh DSM metrics and ChangeDistiller thresholds are techniques from papers. No code is adopted. Ideas are not subject to copyright, but write the implementation independently. Unverified: I did not check for patents.
- Formats consumed without a code dependency: rustdoc JSON (the Rust project's unofficial JSON format, subject to nightly changes) and the SCIP protocol.

Limits of verification: For some projects other than RefDiff and cargo-public-api, I checked only the sidebar and did not read the full LICENSE. I did not check whether dual-licensed projects had NOTICE files. None of the recommended items used GPL/AGPL/EPL/source-available licenses. GumTree was the sole LGPL item.

## Licence notes, second pass

Licence notes. Source key: "API" = GitHub API /license endpoint (reports the repo LICENSE file's SPDX id); "page" = repo page sidebar/README; "raw" = LICENSE file text. No licence file was read in full except GumTree's.
1. RefDiff: MIT (page). Permissive. Code reuse -> keep copyright + MIT text. Algorithm (TSE 2020 paper) can be reimplemented without any obligation. Plan: reimplement the technique; code reuse optional.
2. RefactoringMiner: MIT (page). Permissive. Java + native parsers, so reuse = only the published technique (statement mapping is not a port candidate for Rust). If invoked as external tool: no obligation beyond its notice when redistributed.
3. sem: repo API = Apache-2.0; crate sem-core on lib.rs = "MIT OR Apache-2.0" (lib.rs, repo ataraxy-labs/sem, v0.30.1). The two sources differ; Unverified which is authoritative (raw LICENSE URL was 404). Both permissive. Plan: only the 3-phase matching idea (exact id / structural hash / >80% token overlap); no code reuse. If the crate were used as a dependency: Apache-2.0 -> LICENSE + NOTICE + change marks, with patent grant.
4. GumTree: LGPL-3.0 (raw, title lines read). Weak copyleft. Linking/modifying/porting its code can pull LGPL obligations (modified-source disclosure, relinking). Do not port code; only the published ASE 2014/TSE 2023/ICSE 2024 technique (and I only saw the paper list, not the algorithm text; HAL was blocked).
5. difftastic: MIT (API, file LICENSE). Permissive. Plan: not used (no item identity, file-level only).
6. cargo-semver-checks: Apache-2.0 OR MIT at your option (page). Permissive. Plan: reuse of technique (public-path matching, lint categories); code reuse would need MIT or Apache notice kept.
7. Trustfall: Apache-2.0 (API, file LICENSE.txt). Permissive, patent grant, NOTICE/LICENSE required if redistributed or vendored; as a Cargo dependency you ship attribution in your dependency licence list. I did not recommend adopting it; mentioned only as cargo-semver-checks' engine.
8. cargo-public-api: MIT (API + page). Plan: copy the output idea (line-per-item text snapshot); no code needed.
9. scip-io: MIT (API, file LICENSE). Only evidence of deterministic per-language merge; no code reuse planned. If merge logic is ported: keep MIT notice.
10. scip-swift: Apache-2.0 (API, file LICENSE). Only its caching design (SHA-256 of path+content per Document, overload-fingerprint invalidation) as a lesson; no code reuse.
Implications items that reuse code vs technique: SCIP proto/CLI (Apache-2.0, scip-code/scip, page) is the one place where code (generated types from scip.proto) is actually consumed: Apache obligations apply (LICENSE, NOTICE if present, mark changes). rust-analyzer/scip-go/scip-typescript (MIT OR Apache-2.0 / Apache-2.0 / Apache-2.0, page) are invoked as external indexers only: no obligation unless bundled. Everything else above is "technique only".
No GPL/AGPL/EPL/source-available/non-OSI found among these; GumTree LGPL-3.0 is the only copyleft. Unverified: NOTICE file presence in any repo; patents.
