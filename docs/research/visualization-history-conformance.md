# History analysis, architecture conformance and graph rendering — research record (2026-11-18)

Behavioural (history) analysis tools, architecture-conformance checks that can run in CI,
building blocks for rendering code graphs, and research on which code visualisations
help comprehension.

Collected by a web-research agent on 2026-11-18. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The report was written in Korean and is given here in English translation.

## Report

This report covers tools for visualization, history analysis and architecture rules. I retrieved repository metadata with gh api on 2026-11-18. "Last push" is the time of the most recent push to that repository.


### Part A: Metrics and history (behavioral) analysis

code-maat: https://github.com/adamtornhill/code-maat
- The README lists GPL-3.0. The last release was v1.0.4 (2023-02-20) and the last push was 2025-07-03. Development is effectively stalled.
- Its language-independent, file-level analysis treats Rust, Go and TS alike. It does not parse source or calculate complexity.
- It accepts git log (`--numstat`, git2 format), svn, hg, p4 and tfs. It produces CSV and has no UI of its own.
- Analyses include revisions, coupling, soc, age, abs-churn, author-churn, entity-churn, entity-ownership, main-dev, refactoring-main-dev, entity-effort, communication, fragmentation and summary. The `-g` option groups paths into architectural units using regular expressions.

CodeScene (the commercial successor to code-maat): https://codescene.com/pricing
- It is closed source and available in cloud and on-premises editions. "CodeScene is free for open-source projects".
- Features beyond code-maat include a Code Health score of 1-10 and its trend, function-level X-Ray (hotspots and temporal coupling), knowledge distribution and an off-boarding simulator. It supports at least 30 languages.
- Unverified: I could not confirm individual support for Rust, Go or TS on the pricing page.
- Tornhill & Borg, "Code Red" (TechDebt 2022), studied 39 commercial codebases and 30,737 files. Low-quality code had 15 times as many defects, took an average of 124% more development time to resolve issues and had a maximum cycle time 9 times as long. https://arxiv.org/abs/2203.04374

hercules: https://github.com/src-d/hercules
- LICENSE.md specifies Apache-2.0. The last release was v10.7.2 (2020-01-14) and the last push was 2023-02-07. Development has effectively stopped.
- Features include burndown (by project, file and developer), an overwrites matrix, ownership, couples (uses Swivel embedding and requires TensorFlow) and devs.
- Burndown and couples operate on lines and are language-independent. Function-level shotness depends on Babelfish, which the README itself describes as "abandoned". Function-level analysis is effectively unusable for Rust, Go and TS.
- It outputs YAML, Protobuf and JSON. The Python tool labours draws graphs using matplotlib.

git-of-theseus: https://github.com/erikbern/git-of-theseus
- License: Apache-2.0. The tag is v0.2.0 and the last push was 2023-11-25. Development has stalled.
- It is language-independent and classifies files only by extension.
- It measures code volume by year cohort, author and extension, and estimates line survival with Kaplan-Meier. It outputs JSON and matplotlib PNGs and has no UI.

git-quick-stats: https://github.com/git-quick-stats/git-quick-stats
- License: MIT. Version 2.11.0 dates to 2026-04-18. The project is maintained.
- This language-independent Bash script provides commit and author statistics, a heatmap and reviewer suggestions.
- Output options are a terminal menu, CSV (`-V`) and JSON (`-j`). There is no GUI.

Gource: https://github.com/acaudwell/Gource
- License: GPL-3.0. Release gource-0.56 dates to 2026-03-06. The project is maintained.
- It is language-independent. It accepts git, hg, bzr, svn or a pipe-delimited custom log (timestamp|user|A/M/D|file|colour).
- It outputs an animation in an OpenGL window and a PPM stream for video. Its main use is presentation rather than analysis.

CodeCharta: https://github.com/MaibornWolff/codecharta
- License: BSD-3-Clause. Release vis-2.11.0 dates to 2026-11-10 and the last push was 2026-11-17. The project is actively maintained.
- The tree-sitter-based Unified Parser supports Rust, Go, TS and TSX. Metrics include complexity (McCabe family), logic complexity, RLOC, per-function max/mean/median and code smells. https://codecharta.com/docs/parser/unified
- It also has a git log parser (including temporal coupling) and a dependency parser (imports, cycles and upward dependencies). The importer accepts SonarQube, Coverage, Tokei, Code Maat and CSV.
- It outputs `.cc.json.gz`. Its Web Studio UI presents a 3D city treemap. Delta mode compares two versions, showing increases in green and decreases in red.
- Unverified: I could not confirm which technology the desktop app uses or which languages the dependency parser supports.

emerge: https://github.com/glato/emerge
- License: MIT. The last release was 2.0.7 (2024-07-12) and the last push was 2026-08-07. Maintenance is slow.
- Go and TS have parsing support only. The README table marks TS entities as unsupported, an apparent inconsistency. Rust is unsupported.
- Metrics include SLOC, fan-in/out, Louvain modularity, whitespace complexity and experimental git metrics (churn and change coupling).
- It outputs GraphML, JSON and JS for D3, and provides a web app with a canvas force graph.

SonarQube Community Build: https://github.com/SonarSource/sonarqube
- License: LGPL-3.0. Version 26.11.0.134175 dates to 2026-11-04.
- It includes Go and TS. Community Build dropped Rust in 26.11 ("does not include Ruby, Rust, and Scala"). Rust requires a separate community plugin. https://docs.sonarsource.com/sonarqube-community-build/analyzing-source-code/languages/overview
- It takes source and coverage reports as input and has its own web UI.
- Unverified: I could not confirm from the documentation whether Community Build restricts branch or PR analysis.

rust-code-analysis: https://github.com/mozilla/rust-code-analysis
- Cargo.toml specifies MPL-2.0. The last release was v0.0.25 (2023-01-13) and the last commit was 2026-01-20. Releases have stalled.
- The official language list includes Rust and TypeScript. Go and TSX are absent. https://mozilla.github.io/rust-code-analysis/languages.html
- It provides a CLI and REST server (rust-code-analysis-web) and computes 11 maintainability metrics.
- Unverified: I could not confirm JSON, YAML, TOML and CBOR output in the README.

scc: https://github.com/boyter/scc
- License: MIT. Release v4.1.0 dates to 2026-09-07. The project is actively maintained.
- It supports all three languages. It approximates file-level complexity by counting branch and loop keywords. It has no AST, and comparisons are meaningful only within the same language.
- Output formats are tabular, json, csv, csv-stream, html, sql, openmetrics and cloc-yaml. It also computes ULOC and DRYness.

tokei: https://github.com/XAMPPRocky/tokei
- Dual license: MIT/Apache-2.0. Release v15.0.0 dates to 2026-09-06.
- It supports all three languages but only counts lines (code, comment, blank). It has no complexity metric.
- Unverified: I did not confirm JSON, YAML and CBOR output.

lizard: https://github.com/terryyin/lizard
- LICENSE.txt uses the MIT wording. Version 1.26.0 dates to 2026-11-10. The project is actively maintained.
- It supports Rust, Golang and TypeScript (including TSX).
- It computes CCN, NLOC, token count and parameter count per function. Extensions add nesting, cognitive complexity and duplicate detection.
- It outputs CSV, HTML and Jenkins XML. Exceeding a `-C` or `-a` threshold produces a non-0 exit code, so it can serve as a CI gate.

Complexity lints in compilers and linters:
- clippy `cognitive_complexity` (Rust) belongs to the restriction group and defaults to allow. The default `cognitive-complexity-threshold` is 25. The clippy documentation calls the name misleading and recommends `excessive_nesting` (`excessive-nesting-threshold`, default 0) and `too_many_lines` (default 100) as alternatives. https://doc.rust-lang.org/clippy/lint_configuration.html
- gocyclo (Go): https://github.com/fzipp/gocyclo, BSD-3, v0.6.0, last push 2025-12-27.
- gocognit(Go): https://github.com/uudashr/gocognit, MIT, v1.3.0(2026-10-06).
- Both are included in golangci-lint. Related linters include cyclop (also covers packages), funlen, nestif and maintidx. https://golangci-lint.run/docs/linters/
- eslint `complexity` (TS) measures cyclomatic complexity. The default max is 20 and the `variant` is classic or modified. The core documentation lists no cognitive complexity rule. https://eslint.org/docs/latest/rules/complexity
- Unverified: I understand that eslint-plugin-sonarjs provides cognitive complexity, but I did not check it.

### Part B: Architecture rules for CI

Rust:
- cargo-modules: https://github.com/regexident/cargo-modules
  - License: MPL-2.0. The tag is v0.28.0 and the last push was 2026-11-15.
  - `dependencies --acyclic` exits with an error on a module-level cycle. With `--no-fns --no-types --no-traits --no-uses --no-externs --no-sysroot`, it outputs a module-only graph as DOT. It also provides `structure` (tree) and `orphans` (files with no connections).
  - It has no rule language for constraints such as "config must not import server". Checking the DOT with a script is my inference.
  - Unverified: I expect accurate path resolution because it uses rust-analyzer, but I did not check it.
- cargo-deny: https://github.com/EmbarkStudios/cargo-deny
  - License: Apache-2.0. Version 0.21.0 dates to 2026-11-16.
  - It operates only on crates. Its bans include `deny`, `wrappers` (allow only designated crates to depend directly on a banned crate), an `allow` list, feature allow/deny rules and build script checks. The documentation lists no module-level features. https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html
  - Unverified: I did not confirm whether `deny=[{crate="server", wrappers=["app"]}]` can approximate rules between workspace crates.
- arch_test: https://github.com/tdymel/arch_test
  - License: AGPL-3.0. The last push was 2023-08. The fork at https://github.com/magistrser/arch_test last received a push in 2026-09.
  - Rules include MayNotAccess, MayOnlyAccess, MayNotBeAccessedBy, MayOnlyBeAccessedBy, NoParentAccess and NoModuleCyclicDependencies. JSON configuration or `#[test]` runs checks at module level.
  - Unverified: This account is based on a search summary. I did not open the README.
- ArchUnitRust: https://github.com/dragonsec-ai/ArchUnitRust
  - License: MIT. The last push was 2026-10-02 and the repository had 65 stars.
  - Unverified: Only a search summary identifies the crates.io package as `archunit` 0.0.2 and describes layer allowlists/denylists and `// archunit: ignore`.
- clippy `disallowed_methods`, `disallowed_types`, `disallowed_macros`:
  - Entries in clippy.toml take the form `{ path, reason, replacement, allow-invalid }`.
  - It can prohibit functions, types and macros at specific paths. For example, paths under `crate::server::*` can be listed individually. The documentation does not say whether a wildcard can block an entire module path.
  - The location of clippy.toml determines the crate-level scope. The pattern "deny the lint and use `#[allow]` only in permitted modules" is my inference from the lint attribute rules.
- ast-grep: https://github.com/ast-grep/ast-grep
  - License: MIT. Version 0.51.3 dates to 2026-11-16. Rust, Go, TS and TSX have built-in support.
  - The `files` and `ignores` globs in rule YAML set the scope. For example, a rule can flag the pattern `use crate::server::$$$` with error severity within `files: [src/config/**]`.
  - Limit: I infer that syntactic matching misses `super::`, re-exports and full paths inside a body.
  - Unverified: I could not confirm the CI exit code behavior of scan on this page.
- semgrep: https://github.com/semgrep/semgrep
  - License: LGPL-2.1. The documentation lists Rust, Go and TS as GA.
  - Rust supports cross-function dataflow but lacks cross-file dataflow. https://docs.semgrep.dev/supported-languages
- The compiler provides the strongest enforcement. When layers are separate workspace crates, dependencies absent from Cargo.toml do not compile. Within a crate, `pub(crate)` and `pub(in path)` restrict visibility (general knowledge, source unconfirmed).

Go:
- go-arch-lint: https://github.com/fe3dback/go-arch-lint
  - License: MIT. Release v1.19.2 dates to 2026-11-17.
  - In `.go-arch-lint.yml`, components (`in` glob) and deps (`mayDependOn`) express rules such as "handlers may depend only on services". Checks operate on imports.
  - `check` exits with code 1 on a violation and outputs ascii or json. There is also a `graph` command.
- depguard: https://github.com/OpenPeeDeeP/depguard
  - License: GPL-3.0. Release v2.0.0 dates to 2023-03-25 and the last push was 2026-11-02.
  - Each named rule specifies a `files` glob (`$all`, `!$test`), `listMode` (Original/Strict/Lax), allow/deny prefixes and a message.
  - Example: For `files: ["**/config/**"]`, set `deny: {"example.com/app/server": "config must not import server"}`. It can run inside golangci-lint.
- `internal/`: The package `.../a/b/c/internal/d` can be imported only by code under `.../a/b/c`. Since Go 1.5 the compiler has enforced this in all repositories. https://go.dev/doc/go1.4#internalpackages
- A script can extract and check the import graph with `go list -deps -json ./...` or `go list -f '{{.ImportPath}} {{join .Imports " "}}' ./...` (general knowledge, source unconfirmed).
- Reference: arch-go https://github.com/fdaines/arch-go uses MIT and its last push was 2024-10. I did not check its features.

TypeScript:
- dependency-cruiser: https://github.com/sverweij/dependency-cruiser
  - License: MIT. Release v18.8.0 dates to 2026-11-16.
  - It supports forbidden, allowed and required rules. The `path`/`pathNot` fields of `from`/`to` accept regular expressions. A `$1` group reference can prohibit imports from outside a component's own boundary. Other checks include `circular` (via/viaOnly), `orphan`, `reachable` and `dependencyTypes` (including type-only).
  - Example: `{name:"no-config-to-server", severity:"error", from:{path:"^src/config"}, to:{path:"^src/server"}}`
  - Output formats include dot, mermaid, html and json. https://github.com/sverweij/dependency-cruiser/blob/main/doc/rules-reference.md
- eslint-plugin-boundaries: https://github.com/javierbrea/eslint-plugin-boundaries
  - License: MIT. Release v7.4.4 dates to 2026-11-15.
  - Define a type and pattern in `boundaries/elements`. The `boundaries/dependencies` rule supports `default:"disallow"` and policies for `from`/`to` element types. `element-types` is a deprecated alias.
  - Other rules include no-unknown-files, no-unknown-dependencies and no-ignored-dependencies. The entry-point, external and no-private rules are deprecated. https://www.jsboundaries.dev/docs/rules/
  - Unverified: I did not confirm whether resolving TS path aliases requires a resolver.
- madge: https://github.com/pahen/madge
  - License: MIT. The tag is v8.0.0 and the last commit was 2024-08-05. Development has stalled.
  - It provides `--circular`, `--orphans`, `--leaves` and `--image`/`--dot` (Graphviz required), and resolves aliases using tsConfig. It has no rule language.
  - Unverified: I did not confirm the exit code for `--circular`.

### Part C: GUI components

Graph rendering and layout:
- Cytoscape.js: https://github.com/cytoscape/cytoscape.js
  - License: MIT. Release v3.35.0 dates to 2026-11-10. It supports compound nodes (`parent`) and has 70 extensions, including elk, dagre, fcose and cola.
  - A preview WebGL renderer has been available since 3.31. In the official benchmark, canvas reached about 20 FPS and WebGL at least 100 FPS with 1.2k nodes and 16k edges. With 3.2k nodes and 68k edges, canvas reached 3 FPS and WebGL 10 FPS.
  - WebGL mode supports only some styles. It does not support segments edges. https://blog.js.cytoscape.org/2025/01/13/webgl-preview/
- Sigma.js and graphology: https://github.com/jacomyal/sigma.js (MIT, sigma@4.0.6, 2026-11-10), https://github.com/graphology/graphology (MIT)
  - It uses WebGL. Graphology provides the layout (ForceAtlas2) and algorithms.
  - Unverified: The scale estimate ("tens of thousands of nodes, hundreds of thousands of edges") comes only from a search summary of the v2 release post. The official site gives no figures.
- React Flow/xyflow: https://github.com/xyflow/xyflow
  - License: MIT. @xyflow/react 13.0.1 dates to 2026-11-16.
  - The performance documentation gives no node limit. It recommends optimizing re-renders and collapsing nodes with `hidden`. There is no built-in layout engine. https://reactflow.dev/learn/advanced-use/performance
  - Unverified: The estimated limit of hundreds to thousands of nodes for DOM rendering is based on general knowledge, with no confirmed source.
- ELK/elkjs: https://github.com/kieler/elkjs
  - LICENSE.md specifies EPL-2.0. Version 0.12.1 dates to 2026-10-28.
  - It computes layouts only and does no rendering. It is a GWT translation of Java ELK and provides layered (with ports and nested children), stress, mrtree, radial and force layouts. It can run in a Web Worker.
  - No scale figures are published.
- Graphviz: https://gitlab.com/graphviz/graphviz
  - License: EPL-2.0 (https://graphviz.org/license/). The GitHub mirror is archived.
  - cargo-modules, madge and dependency-cruiser all output DOT, so it can serve as a common interchange format.
- AntV G6: https://github.com/antvis/G6
  - License: MIT. Version 5.1.1 dates to 2026-04-17. It offers at least 10 layouts and claims WebGPU/WASM acceleration. It gives no scale figures.
- cosmos.gl: https://github.com/cosmosgl/graph
  - License: MIT. Release v3.6.0 dates to 2026-11-04. It runs both simulation and rendering on the GPU and claims support for "hundreds of thousands of points and links".
  - It requires luma.gl (WebGL2) and `EXT_float_blend`. The visualization resembles a point cloud, with little emphasis on labels or hierarchy.
- D3: https://github.com/d3/d3 (ISC). It can render DSMs, treemaps, sunbursts and edge bundling (d3.cluster and curveBundle).
  - Unverified: The estimate that SVG force layouts are limited to thousands of nodes is based on general knowledge.
- 3d-force-graph and force-graph: https://github.com/vasturiano/3d-force-graph, https://github.com/vasturiano/force-graph (both MIT and still active in 2026). They use three.js/WebGL or canvas.
  - I did not confirm any scale figures.
- Native Qt Quick: QuickQanava https://github.com/cneben/QuickQanava
  - License: BSD-3. It requires Qt 6.5 or later and the last push was 2026-11-12. The project targets "medium-sized directed graphs" and provides no automatic layout.
  - Unverified: I did not check Qt's LGPL-3/GPL/commercial licensing terms or whether the Qt Graphs module is for charts rather than node-link diagrams.
- egui_graphs: https://github.com/blitzarx1/egui_graphs
  - License: MIT. Release v0.32.0 dates to 2026-08-31. It uses petgraph, and the README says the "project is not in active development". The force layout uses a naive O(n²) algorithm.
- Sourcetrail fork: https://github.com/petermost/Sourcetrail
  - License: GPL-3.0. It uses Qt6. Only C/C++ and Java are supported. Python was removed in 2026.10. Rust, Go and TS are unsupported.
  - The original repository at https://github.com/CoatiSoftware/Sourcetrail is archived. Only the UI pattern is worth borrowing: a neighborhood graph centered on the selected symbol, linked to the code panel.

Shell:
- Tauri: https://github.com/tauri-apps/tauri
  - License: Apache-2.0. The tauri-cli version is 2.12.5. It uses WebKitGTK on Linux.
  - The official documentation says a WebGL2 context can be created without error even on a slow path or software rasterizer, and the renderer string always reports "Apple GPU". It recommends that developers "provide a non-WebGL fallback on Linux". There are also NVIDIA DMA-BUF issues. https://v2.tauri.app/develop/debug/linux-graphics/
  - The Chromium runtime (tauri-runtime-cef) is available only in Tauri 3 alpha (alpha.7, 2026-11-06). https://v3.tauri.app/develop/cef/
  - Unverified: The report that Linux packages currently fail to build with CEF 154 comes only from a search summary.
- Electron: https://github.com/electron/electron
  - License: MIT. The project is actively maintained. Bundling Chromium makes WebGL behavior predictable on Linux (general knowledge, source unconfirmed).
- Qt (QML): Native rendering performs well, but there are few graph widgets. QuickQanava is representative. DSMs, treemaps and layouts require custom implementations.

Visualization forms and evidence:
- DSM: Sangal, Jordan, Sinha, Jackson, OOPSLA 2005. The authors use static analysis to extract dependencies and show how to manage architecture and identify design flaws with a DSM. Ant is the case study. https://groups.csail.mit.edu/sdg/pubs/2005/oopsla05-dsm.pdf
- Matrix versus node-link: Ghoniem, Fekete, Castagliola, InfoVis 2004. Above 20 vertices, matrices performed better on most tasks. Node-link performed better only for path finding, and even that performance deteriorated on large, dense graphs. This account comes from a search summary. The full text returned 403. https://www.researchgate.net/publication/221005943_A_Comparison_of_the_Readability_of_Graphs_Using_Node-Link_and_Matrix-Based_Representations
- City/treemap: Wettel, Lanza, Robbes, ICSE 2011, with 41 participants from academia and industry. I checked the figures in the original PDF. https://wettel.github.io/download/Wettel11a-icse.pdf
  - Compared with Eclipse and Excel together, correctness increased by +24.26% and the change in completion time was -12.01%.
  - Correctness was 40-50% higher on tasks requiring an overview (estimating the scope of an impact).
  - Excel was faster on precise-value lookup tasks: "a spreadsheet is faster at finding precise answers".
- Hierarchical edge bundling: Holten, IEEE TVCG 12(5), 2006. Bundling dependency edges over a hierarchy (module tree) shows both high-level and lower-level relationships. The evidence comes from an informal user study. https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Holten06.pdf
- Hairball critique: Kosara, "Graphs Beyond the Hairball". The author says the node-link representation "breaks down beyond a few dozen nodes" and presents matrices, node quilts and PivotGraph (aggregation by attribute) as alternatives. https://eagereyes.org/blog/2012/graphs-hairball
- Quality of evidence: Merino et al., JSS 2018. An analysis of 181 out of 387 SOFTVIS/VISSOFT papers found that "62% lack a strong evaluation". This points to limited evidence for the effectiveness of software visualization itself. https://homepages.ecs.vuw.ac.nz/~craig/publications/jss2018-merino.pdf

### Recommended stack
1. I recommend implementing the history engine directly with gix or git2. Use the revisions, coupling, soc, age and ownership concepts from code-maat, aggregated over release-tag intervals (for 24 releases, diffs between tags suffice). code-maat is GPL and has had no release since 2023. Development of hercules and git-of-theseus has stopped.
2. Use lizard by default for function-level complexity (MIT, Rust/Go/TS support and a CI exit code). Import CodeCharta cc.json for file metrics and delta comparisons (BSD-3, tree-sitter support for all three languages). rust-code-analysis lacks Go, and SonarQube CB dropped Rust in 26.11.
3. Calculate hotspots as the number of changes in an interval multiplied by function complexity. Calculate change coupling per release interval and show it with a time slider.
4. Collect dependency graphs with cargo-modules DOT for Rust (module-level, `--acyclic`), `go list -json` for Go and dependency-cruiser JSON for TS.
5. For Rust CI rules, combine separate crates, `pub(crate)` and cargo-modules `--acyclic` with custom rules over DOT. I advise against direct dependencies on ArchUnitRust, which is at 0.0.x, and arch_test, which is AGPL. For Go, combine go-arch-lint, `internal/` and depguard. For TS, use dependency-cruiser.
6. I recommend Electron for the shell because Linux comes first. The Tauri 2 documentation acknowledges the WebKitGTK WebGL slow path, and the CEF runtime is in alpha. Reassess when Tauri 3 CEF stabilizes.
7. Use a module DSM (custom canvas implementation), a hotspot treemap (D3) and a focused neighborhood graph (Cytoscape.js with elkjs layered, compound=module) as the main views. Keep the full node-link graph in sigma.js for exploration only. Use React Flow only for editable views with a few dozen nodes.
8. Given the weak evidence (Merino 62%), I recommend starting the default screen with a DSM, treemap and list instead of a hairball. Let users explore from a specific question about impact, ownership or coupling.
