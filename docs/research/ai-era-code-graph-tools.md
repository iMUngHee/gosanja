# Code-graph tools of 2024-2026 and the comprehension problem — research record (2026-11-18)

Tools that build a code knowledge graph or code map (most of them for AI agents), writing
and studies on humans losing track of agent-written code, and a gap analysis for the
human-maintainer use case.

Collected by a web-research agent on 2026-11-18. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The report was written in Korean and is given here in English translation.

## Report

These are the results of the survey of AI-era code-graph tools (Parts A/B/C). Figures were measured with `gh api` on 2026-11-18, and stars (★), the last push and the latest release are given together. Items I could not open myself are marked "Unverified:". The unauthenticated GitHub API I tried first was exhausted because I shared its limit (60 per hour) with another team member on the same IP; after that I queried with the authenticated `gh api` (read-only GET).

### Part A — Facts per tool

#### 1. Engine candidates: open source, local, supporting all of Rust/Go/TS

**code-graph-rag**
- URL: https://github.com/vitali87/code-graph-rag
- Licence and activity: MIT, ★5.4k, v0.7.1 (2026-11-17)
- Extraction: tree-sitter by default. The README says "compiler-grade frontends and runtime traces where available" but does not say which languages that applies to.
- Languages: the README marks Rust/Go/TS as "fully supported".
- Consumer: agents (MCP, RAG that generates Cypher, editing through AST patches).
- Storage: embedded SQLite (~/.cgr/graph.sqlite) by default; Memgraph and Neo4j are optional.
- GUI for people: not found.

**GitNexus**
- URL: https://github.com/abhigyanpatwari/GitNexus
- Licence and activity: ★48.6k, v1.6.12 (2026-09-12), push 2026-11-17
- Licence caveat: PolyForm Noncommercial 1.0.0, which is not open source by the OSI definition. Commercial use needs a separate licence from Akon Labs, so it is hard to use as a GUI engine.
- Extraction: tree-sitter
- Languages: Rust/Go/TS have "registered providers", and each language has its own document of gaps (docs/languages/rust-ingestion-gaps.md, go-…, typescript-…).
- Storage: Cypher, BM25 and vector search on LadybugDB (a KuzuDB fork)
- Consumer: agents first (MCP, skills, Claude Code hooks).
- View for people: running `gitnexus serve` and opening the web UI (https://gitnexus.vercel.app) gives a graph explorer and an AI chat.
- diff: `detect_changes` maps a git diff to its impact range.

**Graphify**
- URL: https://github.com/Graphify-Labs/graphify (formerly safishamsi/graphify)
- Licence and activity: Apache-2.0, ★149k, v0.9.111 (2026-11-15)
- Extraction: tree-sitter. Code is processed without an LLM; documents, PDFs and media optionally go through an LLM pass. Communities are grouped with Leiden.
- Languages: the repository has graphify/extractors/rust.py and go.py, and TS was confirmed through tree-sitter-typescript and the TS tests.
- Output: graph.html, GRAPH_REPORT.md and graph.json under graphify-out/. It can push to Neo4j and FalkorDB.
- Consumer: mainly agent skills and a hosted MCP; graph.html is a static view for people.

**CodeGraphContext**
- URL: https://github.com/CodeGraphContext/CodeGraphContext
- Licence and activity: MIT, ★4.4k, v0.5.7 (2026-08-08), push 2026-11-16
- Extraction: tree-sitter. SCIP is optional, and the only languages for which the README names SCIP are C/C++ (scip-clang) and C# (scip-dotnet). Rust/Go/TS are therefore handled with tree-sitter heuristics.
- Languages: Go, Rust and TS are all on the supported list.
- Storage: FalkorDB Lite by default; Kuzu, LadybugDB and Neo4j can also be used.
- Consumer: MCP and CLI. It offers "interactive visualization".

**code-review-graph**
- URL: https://github.com/tirth8205/code-review-graph
- Licence and activity: MIT, ★32.8k, v2.4.0 (2026-11-11)
- Extraction: tree-sitter with a targeted fallback
- Languages: supports Go, Rust and TS. The README itself notes, however, "JavaScript and Go flow detection needs work".
- Storage: a single SQLite file
- Consumer: mainly agent review (MCP, `detect_changes` with risk ratings, PR comments from a GitHub Action).
- View for people: D3 force-directed HTML and neighbourhood views seeded from a symbol, a flow or a change. It can export to GraphML, Cypher, Obsidian and SVG.
- Marketing claim: the token-saving figures in the README, such as "6.8× fewer tokens" and "49×", come from its own benchmark.

**Understand-Anything**
- URL: https://github.com/Egonex-AI/Understand-Anything (formerly Lum1104)
- Licence and activity: MIT, ★93.0k, v2.9.8 (2026-11-15)
- Extraction: a hybrid in which tree-sitter extracts structural facts (import, def, call) and an LLM multi-agent adds summaries, layers and domain mapping.
- Languages: packages/core/src/languages/configs/ contains rust.ts, go.ts and typescript.ts.
- Storage: .ua/knowledge-graph.json (the JSON is committed so that the team shares it)
- Consumer: one of the few tools centred on a GUI for people.
- View for people: a React Flow dashboard. It offers colours per architecture layer, guided tours and a `/understand-diff` diff overlay, and can be viewed with Node alone, without an LLM.
- Limit: layers are decided by an LLM, so they are not deterministic.

**CodeGraph (colbymchenry)**
- URL: https://github.com/colbymchenry/codegraph
- Licence and activity: MIT, ★77.8k, v1.6.2 (2026-10-03), push 2026-11-17
- Extraction: tree-sitter embedded in a native Rust kernel.
- Languages: more than 20, including TS/Go/Rust
- Storage: SQLite (.codegraph/codegraph.db) with FTS5
- Consumer: agents only (MCP). No GUI for people was found.
- Notable:
  - It connects web framework routes to their handlers.
  - It connects a TS `invoke('save_file')` in Tauri with the Rust command in both directions, so it covers part of the string-based contracts between languages.
  - Feeding a git diff to `codegraph affected --stdin` gives the impact range.

**codebase-memory-mcp**
- URL: https://github.com/DeusData/codebase-memory-mcp
- Licence and activity: MIT, written in C, ★48.8k, v0.12.1 (2026-10-16)
- Extraction:
  - vendored tree-sitter grammars
  - "Hybrid LSP": rather than running real language servers, it reimplements type-resolution algorithms lightly in C (in the README's words, "structurally inspired by" LSP). TS, Go and Rust are among its targets.
- Storage: SQLite
- Notable:
  - It matches HTTP routes with call sites by a confidence score.
  - It detects gRPC, GraphQL and tRPC.
  - `CONFIGURES` edges group environment-variable reads in code onto shared `EnvVar` nodes.
  - Kubernetes, compose and Terraform are connected by edges too.
- View for people: the "CBM Lens" browser UI (open beta v0.0.1), which shows architecture, routes and a service map.
- Related paper: https://arxiv.org/abs/2603.27277 (2026-03-28). Over 31 repositories its answer quality was 83%, lower than a file-exploring agent's (92%). In exchange it used about 10 times fewer tokens and 2.1 times fewer tool calls.

**Auryn (formerly FalkorDB code-graph)**
- URL: https://github.com/FalkorDB/Auryn (a request for FalkorDB/code-graph redirects to this repository)
- Licence and activity: MIT, ★385, v0.8.1 (2026-11-16)
- Extraction: Go, Rust and TS use tree-sitter only. C# is analysed precisely with Roslyn, Python with Jedi.
- Storage: FalkorDB (embedded or server)
- View for people: a local viewer (:8080). It claims to show paths across languages (from a TS button to a C# controller) joined in one schema.

**codegraph-rust**
- URL: https://github.com/Jakedismo/codegraph-rust
- Licence and activity: Cargo.toml has `MIT OR Apache-2.0`, but there is no LICENSE file at the root. ★915, push 2026-10-12
- Extraction: one of the few tools in this survey that resolves through real language servers. In the balanced/full tiers it resolves with rust-analyzer, gopls and typescript-language-server, and it builds local Rust dataflow edges.
- Architecture rules: a crate-level deny rule in codegraph.boundaries.toml that is broken produces a `violates_boundary` edge, and package cycles are marked.
- Storage: Unverified: SurrealDB (inferred from the presence of schema/*.surql files)
- Consumer: agents (MCP)

**sem**
- URL: https://github.com/Ataraxy-Labs/sem
- Licence and activity: written in Rust, created 2026-02, ★3.5k, push 2026-11-14. The API states the licence as Apache-2.0 and the badge as MIT (it appears to be dual-licensed).
- Extraction: tree-sitter, 28 languages
- Features:
  - It computes diff, blame and impact per entity (function, class) on top of git.
  - `sem graph --modules metrics` gives cycles, depth and centrality of the JS/TS module graph.
  - It offers `sem arch-diff RANGE` and `sem certify` (signature changes in a commit range, callers left behind, changes in module reachability).
- Storage: an SQLite cache
- Languages: TS and Go were confirmed in the README. Unverified: the quality of Rust entity extraction
- Consumer: agents and CI first (JSON, MCP).

#### 2. Tools with a poor fit as an engine, or of a different kind

**Serena**
- URL: https://github.com/oraios/serena
- Licence and activity: the application is GPL-3.0-or-later, only the SolidLSP part is MIT. ★31.5k, v2.1.0 (2026-10-19)
- Kind: there is no graph DB. It offers symbol-level lookup and editing tools over MCP with an LSP (more than 40 languages, including Go/Rust/TS) or a JetBrains backend. It is for agents; its precision is high, but it has no persistent graph and no visualisation for people.

**Axon**
- URL: https://github.com/harshkedia177/axon
- Licence and activity: only a badge, no LICENSE file (the API licence is null). ★820, v1.0.1 (2026-03-09), push 2026-08-03
- Languages: Python, TS and JS only, so no Rust or Go.
- Features: a web dashboard (Sigma.js, coupling heat map, health score) and `axon diff main..feature`. The latter is a structural diff that compares symbols as added, modified or deleted using git worktrees. It cannot be used on the target codebase.

**Potpie**
- URL: https://github.com/potpie-ai/potpie
- Licence and activity: Apache-2.0, ★5.8k, v2.0.1 (2026-08-31)
- Kind: the current README weighs a "codebase and SDLC context graph" (a record of decisions and claims) over code structure. Backends include falkordb_lite and neo4j, and there is a `potpie ui` explorer.
- Unverified: how it extracts code, and Rust/Go support

**deepwiki-open**
- URL: https://github.com/AsyncFuncAI/deepwiki-open
- Licence and activity: MIT, ★18.4k, no releases, push 2026-11-15
- Kind: an LLM generates a wiki, diagrams and "codemap" tours. Version 2.0 was renamed "Grok-Wiki". It is not a deterministic graph.
- Unverified: details of its embedding RAG

**Sourcebot**
- URL: https://github.com/sourcebot-dev/sourcebot
- Licence and activity: MIT, ★4.0k, v6.1.0 (2026-11-10)
- Kind: code search is its main job. Code navigation is a heuristic that finds definitions with universal-ctags and references with text search (the documentation says "search heuristics").
- Unverified: whether the ee/ directory is under a commercial licence

**Repomix**
- URL: https://github.com/yamadashy/repomix
- Licence and activity: MIT, ★29.3k, v1.19.0
- Kind: a tool that bundles a repository into a single file for an LLM. `--compress` keeps only signatures, using tree-sitter. It builds no graph.

**Aider repo map**
- URL: https://aider.chat/docs/repomap.html
- Kind: it applies a ranking algorithm to the file dependency graph, builds a text map within a token budget (1k by default) and sends it to the LLM. It is not a view for people.
- Unverified: use of tree-sitter and PageRank (the page only says "graph ranking algorithm")

**Glamorous Toolkit**
- URL: https://github.com/feenkcom/gtoolkit
- Licence and activity: MIT, written in Pharo and Rust, ★1.6k, v1.1.687 (2026-11-17)
- Kind: a moldable environment in which people build a custom view for each problem. The site lists support for analysing Rust and TS; Go is absent. It is closer to a philosophical precedent than to an engine.

**Nogic**
- URL: https://www.nogic.dev/ , https://marketplace.visualstudio.com/items?itemName=Nogic.nogic
- Kind: a VS Code extension. The site's wording is "Your agent just edited 40 files…", and it says it shows an agent's diff as walkthroughs by intent, a code graph and sequence diagrams. Its problem statement is the closest to this project's purpose.
- Limits: whether it is open source could not be confirmed. Its static code map supports only JS/TS/Python, so there is no Rust or Go (based on search results; the Marketplace page itself was not opened). HN users reported slowdowns or crashes beyond 2000 files (2026-01).

**CodeViz (YC S24)**
- URL: https://codeviz.ai
- Kind: a proprietary product. v1 built a call graph with the VS Code language servers and drew C4 diagrams with an LLM. v2 turned into a tool that generates and edits C4 and PlantUML with a modified gemini-cli.
- Source: https://news.ycombinator.com/item?id=46112279

**Windsurf Codemaps (Cognition)**
- URL: https://cognition.com/blog/codemaps
- Kind: proprietary, released 2025-11-04. SWE-1.5 or Claude Sonnet 4.5 generates an annotated map per task prompt. It is an LLM snapshot, not a deterministic graph.

**Copilot / Cursor**
- Unverified: no built-in visual code map feature was found. The search covered 2 web searches; the official changelogs were not checked.
- Cursor has an embedding index for its AI. "Code Map" is a Visual Studio Enterprise feature (general knowledge, source not checked).

#### 3. Part A observations

- Extraction: almost all of them use tree-sitter name-resolution heuristics. Precise resolution of Rust trait methods, generics and macro calls can be expected only from codegraph-rust (rust-analyzer) and Serena (LSP).
- Evidence comparing graph approaches: in https://arxiv.org/abs/2601.08773 (2026-01, 3 Java repositories, single author), which compares AST graphs with LLM-generated graphs, the AST graph was ahead on accuracy, cost and coverage. The LLM-KB missed 377 files in Shopizer.
- Consumer: the main consumer is overwhelmingly agents (MCP, token savings). Views for people are mostly force-directed graphs.
- Licences: the ones that block use as a GUI engine are GitNexus (noncommercial), Serena (GPL) and Axon and codegraph-rust (no LICENSE file).

### Part B — The problem itself (7 key sources)

**1. Anthropic RCT "How AI Impacts Skill Formation"** (2026-01-29, arXiv:2601.20245)
- URL: https://www.anthropic.com/research/AI-assistance-coding-skills
- Design: 52 people, mostly juniors, learning a new library (Trio)
- Results:
  - The AI group scored 50% on the quiz and the manual group 67% (d=0.738, p=0.01). The widest gap was in debugging.
  - The difference in speed was not significant (p=0.391).
  - The pattern of asking comprehension questions after generation scored about 86%, the delegation pattern below 40%. This is a correlation, however, and n is small.

**2. Addy Osmani, "Comprehension Debt"** (O'Reilly Radar, 2026-04-13)
- URL: https://www.oreilly.com/radar/comprehension-debt-the-hidden-cost-of-ai-generated-code/
- Claim: a gap builds up between the amount of code and the amount people have understood, and review turns from a quality gate into a throughput problem. DORA, PR counts and coverage do not detect the loss of understanding.
- Evidence: the RCT in item 1 and anecdotes, nothing else. The "<40% vs >65%" figures are cited without a source and appear to be the per-pattern figures of study 1. That is my interpretation.

**3. Margaret-Anne Storey's cognitive debt and the Triple Debt Model**
- URL:
  - Blog (2026-02-09): https://margaretstorey.com/blog/2026/02/09/cognitive-debt/
  - Follow-up post: https://margaretstorey.com/blog/2026/02/18/cognitive-debt-revisited/
  - Paper (2026-03-23, v4 04-06): https://arxiv.org/abs/2603.22106 , ACM Queue: https://queue.acm.org/detail.cfm?id=3807966
- Claim: technical debt lives in code, cognitive debt in a team's shared understanding, and intent debt in goals, constraints and rationale that were never externalised. GenAI does not remove the problem; it redistributes it.
- Evidence: a conceptual position paper. Its only empirical basis is anecdotes from coaching student teams.
- Unverified: "To appear ICSE 2026" (as ResearchGate states)

**4. GitClear 2025 "AI Copilot Code Quality"**
- URL: https://www.gitclear.com/ai_assistant_code_quality_2025_research
- Access problem: the original returned 403, so it was checked through the summary at https://www.jonas.rs/2025/02/09/report-summary-gitclear-ai-code-quality-research-2025.html .
- Data: 211 million changed lines, 2020–2024
- Results:
  - share of moved (refactored) lines: 24.1% → 9.5%
  - share of copy/paste: 8.3% → 12.3%
  - rework within 2 weeks (churn): 3.1% → 5.7% (the same page also gives a different figure, 5.5% → 7.9%, so it is inconsistent)
  - duplicated blocks: 8 times in 2024
- Causation: it does not distinguish whether code was written by AI, so causation is not shown.
- The 2026 follow-up "The Maintainability Gap": https://www.gitclear.com/the_ai_code_quality_maintainability_gap
  - Unverified: moved 3.8% (2026 YTD), block duplication 40.3 → 73.0 per million lines. The original returned 403, so this was seen only in search summaries. An attempt to fetch it another way was refused by the permission classifier, and I did not work around it.

**5. Faros AI (vendor telemetry)**
- URL: https://www.faros.ai/ai-productivity-paradox , https://www.faros.ai/blog/comprehension-debt-in-software-engineering (2026-09-29)
- What was confirmed in the original:
  - Across 10,000 developers and 1,255 teams the amount of change grew roughly 2 times, yet "company metrics remain flat"
  - Blog: PR size +71.8%, files per PR +52.0%, merges without review +76.3%, monthly incidents +125.4%. The sample and method are not disclosed.
- Unverified: review time +91% (2025) and +441% (2026), seen only in search summaries.
- Assessment: the figures differ from report to report and come from a vendor, so they must be treated as claims.

**6. Ahmad, "Comprehension Debt in GenAI-Assisted SE Projects"** (2026-04-14, EASE 2026 Companion, 5 pages)
- URL: https://arxiv.org/abs/2604.13277
- Design: a qualitative analysis of 621 reflection journals by 207 students
- Results: it presents 4 accumulation patterns (black-box acceptance, context-mismatch, dependency-induced atrophy, verification-bypass) and a mitigation pattern (comprehension scaffold).
- Caution: search results called it ICPC, but the arXiv page says EASE Companion.

**7. Lyu et al., "How Do Practitioners Build SE Agents?"** (2026-07-12, v2 07-25, David Lo and others)
- URL: https://arxiv.org/abs/2607.10856
- Design: 20 interviews (12 organisations) and a survey of 80 people
- Results: it names comprehension debt as one of 6 major challenges and sees review of generated code as "new and increasingly central forms of work". Some teams handed maintenance back to AI, working around the debt rather than paying it down. This part was seen in a search summary; the abstract has no details.

**For reference: structure-level review tools and practice**
- Nogic: AI diff walkthrough (Part A above)
- Axon diff: symbol-level branch diff
- sem: entity diff and arch-diff
- `detect_changes` in code-review-graph and GitNexus: impact range
- Understand-Anything `/understand-diff`
- PR Lens: draws a PR architecture diagram against a baseline graph. https://prlens.dev/guides/architecture-diagram-from-a-git-diff . The article cites LinearB 2026 figures of 408 lines at the 75th percentile for AI PRs and 157 for non-AI PRs (a secondary citation).
- WorkBraid: early stage; it shows architecture changes an agent proposes as a 3-way diagram diff. https://dev.to/mech_app_ai/workbraid-visual-architecture-diffs-for-agent-proposed-code-changes-4ngi
- Unverified: that CodeRabbit and Greptile use a code graph as review context (secondary sources)
- The METR RCT (2025-07, 16 people, 19% slower) and its 2026-02 follow-up ("unreliable" because of selection bias) are productivity studies, not comprehension studies, so they were left out.

### Part C — Gap analysis

1. Graph diff between commits: only Axon (Py/TS/JS only) and sem (entity level) offer it. No open-source tool was found that gives an "architecture diff view" at the level of Rust crates and modules and TS module boundaries as a GUI for people. The search covered the READMEs of the tools above and 3 web searches.
2. History across releases: a feature that accumulates graph snapshots per commit or release to track the growth of the public surface (pub items, exports) was not found in the READMEs of these tools. All of them handle only "the index of the current HEAD".
3. Enforcing layering: the crate-level deny rules of codegraph-rust and sem's TS module cycle metrics are all there is. No tool combined module- or directory-level rules, rules common to all languages, a CI gate and display of violations in a GUI.
4. Attributing agent changes to architecture: overlaying "which layers and boundaries did this agent session or PR touch" on a structural map is offered only by roughly Nogic (JS/TS/Py, proprietary) and the Understand-Anything diff overlay (layers decided by an LLM). There was no deterministic implementation that supports Rust.
5. String contracts between languages: codebase-memory-mcp (HTTP route and call site, EnvVar), CodeGraph (Tauri invoke) and Auryn cover part of it. All are heuristic and meant for agent queries. There was no view in which a person sees "the list of contracts, the uses on both sides, and what breaks on a change".
6. Precision: Rust resolution is mostly tree-sitter name matching. There was no GUI-oriented engine that builds a precise graph on rust-analyzer or SCIP (rust-analyzer scip, scip-typescript, scip-go) and also diffs it. CGC's SCIP is limited to the C family and C#.
7. Evaluation criteria: every tool benchmark measures savings in tokens and tool calls. No tool was designed or evaluated with human understanding (the quiz-style measurement of Part B) as its goal.
8. Shape of the views for people: mostly force-directed global graphs. Open-source tools that deterministically generate change-centred local views or layer-fixed layouts (C4 or a crate dependency DAG) are rare. CodeViz and Codemaps are LLM-generated, so they are not reproducible.
