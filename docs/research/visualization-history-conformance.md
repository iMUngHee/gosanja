# History analysis, architecture conformance and graph rendering — research record (2026-11-18)

Behavioural (history) analysis tools, architecture-conformance checks that can run in CI,
building blocks for rendering code graphs, and research on which code visualisations
help comprehension.

Collected by a web-research agent on 2026-11-18. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The text is kept as the researcher wrote it, in Korean or English.

## Report

시각화·이력·아키텍처 규칙 도구 조사 결과입니다. 저장소 메타데이터는 2026-11-18에 gh api로 조회했고, "last push"는 해당 저장소의 마지막 push 시각입니다.


### Part A: 지표와 이력(행동) 분석

code-maat: https://github.com/adamtornhill/code-maat
- 라이선스 GPL-3.0(README 기준), 마지막 릴리스 v1.0.4(2023-02-20), 마지막 push 2025-07-03. 사실상 정체 상태입니다.
- 언어에 무관한 파일 단위 분석이라 Rust, Go, TS 모두 동일하게 다룹니다. 소스 파싱이나 복잡도 계산은 하지 않습니다.
- 입력: git log(`--numstat`, git2 형식), svn, hg, p4, tfs. 출력은 CSV이고 자체 UI는 없습니다.
- 분석 종류: revisions, coupling, soc, age, abs-churn, author-churn, entity-churn, entity-ownership, main-dev, refactoring-main-dev, entity-effort, communication, fragmentation, summary. `-g` 옵션으로 경로 정규식을 이용해 아키텍처 그룹을 묶을 수 있습니다.

CodeScene(code-maat 이후의 상용 제품): https://codescene.com/pricing
- 오픈소스가 아니고 cloud와 on-prem으로 제공됩니다. "CodeScene is free for open-source projects".
- code-maat에 없는 기능: Code Health 1-10점과 그 추이, function 단위 X-Ray(함수 수준 hotspot과 temporal coupling), knowledge distribution과 off-boarding 시뮬레이터. 30개 이상의 언어를 지원합니다.
- Unverified: Rust, Go, TS 개별 지원 여부는 가격 페이지에서 확인하지 못했습니다.
- 근거 연구로 Tornhill & Borg, "Code Red"(TechDebt 2022, 39개 상용 코드베이스, 30,737개 파일)가 있습니다. 저품질 코드는 결함이 15배 많고, 이슈 해결에 개발 시간이 평균 124% 더 들며, 최대 cycle time이 9배 깁니다. https://arxiv.org/abs/2203.04374

hercules: https://github.com/src-d/hercules
- 라이선스 Apache-2.0(LICENSE.md), 마지막 릴리스 v10.7.2(2020-01-14), 마지막 push 2023-02-07. 사실상 중단 상태입니다.
- 기능: burndown(프로젝트, 파일, 개발자 단위), overwrites matrix, ownership, couples(Swivel embedding을 쓰며 TensorFlow 필요), devs.
- burndown과 couples는 줄 단위라 언어에 무관합니다. shotness(함수 단위)는 Babelfish에 의존하는데, README 스스로 Babelfish가 "abandoned"라고 적어 두었습니다. 함수 단위 분석은 Rust, Go, TS 모두 사실상 쓸 수 없습니다.
- 출력은 YAML, Protobuf, JSON이고, 그래프는 Python labours(matplotlib)로 그립니다.

git-of-theseus: https://github.com/erikbern/git-of-theseus
- 라이선스 Apache-2.0, 태그 v0.2.0, 마지막 push 2023-11-25. 정체 상태입니다.
- 언어에 무관하고, 확장자별 분류만 있습니다.
- 계산 항목: 연도 cohort, author, 확장자별 코드량, Kaplan-Meier 줄 생존율. 출력은 JSON과 matplotlib PNG이고 UI는 없습니다.

git-quick-stats: https://github.com/git-quick-stats/git-quick-stats
- 라이선스 MIT, 2.11.0(2026-04-18). 유지되고 있습니다.
- 언어에 무관한 Bash 스크립트입니다. 커밋과 작성자 통계, heatmap, 리뷰어 제안을 제공합니다.
- 출력은 터미널 메뉴, CSV(`-V`), JSON(`-j`)이고 GUI는 없습니다.

Gource: https://github.com/acaudwell/Gource
- 라이선스 GPL-3.0, gource-0.56(2026-03-06). 유지되고 있습니다.
- 언어를 인식하지 않습니다. 입력은 git, hg, bzr, svn 또는 파이프로 구분한 custom log(timestamp|user|A/M/D|file|colour)입니다.
- 출력은 OpenGL 창 애니메이션과 PPM 스트림(동영상용)입니다. 분석 도구라기보다 연출용입니다.

CodeCharta: https://github.com/MaibornWolff/codecharta
- 라이선스 BSD-3-Clause, vis-2.11.0(2026-11-10), 마지막 push 2026-11-17. 활발히 유지되고 있습니다.
- Unified Parser가 tree-sitter 기반이라 Rust, Go, TS, TSX를 모두 지원합니다. 지표는 complexity(McCabe 계열), logic complexity, RLOC, 함수별 max/mean/median, code smell입니다. https://codecharta.com/docs/parser/unified
- 그 외 git log parser(temporal coupling 포함), dependency parser(imports, cycles, upward dependencies)가 있고, importer는 SonarQube, Coverage, Tokei, Code Maat, CSV를 받습니다.
- 출력은 `.cc.json.gz`입니다. 자체 UI는 Web Studio 3D treemap(city)이고, 두 버전을 비교하는 delta 모드(증가 녹색, 감소 적색)가 있습니다.
- Unverified: desktop 앱이 어떤 기술로 만들어졌는지, dependency parser가 어떤 언어를 지원하는지는 확인하지 못했습니다.

emerge: https://github.com/glato/emerge
- 라이선스 MIT, 마지막 릴리스 2.0.7(2024-07-12), 마지막 push 2026-08-07. 유지보수가 느립니다.
- 지원: Go와 TS는 parsing만 되고, TS entity는 README 표에서 지원 안 함으로 나와 앞뒤가 맞지 않습니다. Rust는 지원하지 않습니다.
- 지표: SLOC, fan-in/out, Louvain modularity, whitespace complexity, 실험적인 git 지표(churn, change coupling).
- 출력은 GraphML, JSON, D3용 JS, 그리고 canvas force graph 웹 앱입니다.

SonarQube Community Build: https://github.com/SonarSource/sonarqube
- 라이선스 LGPL-3.0, 26.11.0.134175(2026-11-04).
- Go와 TS는 포함됩니다. Rust는 26.11부터 Community Build에서 빠졌습니다("does not include Ruby, Rust, and Scala"). 별도의 커뮤니티 플러그인이 필요합니다. https://docs.sonarsource.com/sonarqube-community-build/analyzing-source-code/languages/overview
- 입력은 소스와 coverage 리포트이고, 자체 웹 UI가 있습니다.
- Unverified: Community Build가 branch나 PR 분석을 제한하는지는 문서에서 확인하지 못했습니다.

rust-code-analysis: https://github.com/mozilla/rust-code-analysis
- 라이선스 MPL-2.0(Cargo.toml), 마지막 릴리스 v0.0.25(2023-01-13), 마지막 commit 2026-01-20. 릴리스가 정체되어 있습니다.
- 지원 언어(공식 목록): Rust와 TypeScript는 있고, Go와 TSX는 목록에 없습니다. https://mozilla.github.io/rust-code-analysis/languages.html
- CLI와 REST 서버(rust-code-analysis-web)를 제공하고, maintainability metric 11종을 계산합니다.
- Unverified: 출력 형식이 JSON, YAML, TOML, CBOR라는 점은 README에서 확인하지 못했습니다.

scc: https://github.com/boyter/scc
- 라이선스 MIT, v4.1.0(2026-09-07). 활발히 유지되고 있습니다.
- 세 언어 모두 지원합니다. complexity는 파일 단위로 분기와 반복 키워드를 세는 근사치입니다. AST가 없고 같은 언어끼리만 비교할 수 있습니다.
- 출력은 tabular, json, csv, csv-stream, html, sql, openmetrics, cloc-yaml이고, ULOC과 DRYness도 계산합니다.

tokei: https://github.com/XAMPPRocky/tokei
- 라이선스 MIT/Apache-2.0 이중, v15.0.0(2026-09-06).
- 세 언어 모두 지원하지만 줄 수만 셉니다(code, comment, blank). complexity는 없습니다.
- Unverified: 출력 형식이 JSON, YAML, CBOR라는 점은 확인하지 못했습니다.

lizard: https://github.com/terryyin/lizard
- 라이선스 MIT 문구(LICENSE.txt), 1.26.0(2026-11-10). 활발히 유지되고 있습니다.
- Rust, Golang, TypeScript(TSX 포함)를 모두 지원합니다.
- 함수 단위로 CCN, NLOC, token 수, parameter 수를 계산하고, extension으로 nesting, cognitive complexity, 중복 탐지를 더할 수 있습니다.
- 출력은 CSV, HTML, Jenkins XML입니다. `-C`나 `-a` 임계값을 넘으면 0이 아닌 exit code로 끝나므로 CI gate로 쓸 수 있습니다.

컴파일러와 linter의 complexity lint:
- clippy `cognitive_complexity`(Rust): restriction 그룹이라 기본값이 allow이고, `cognitive-complexity-threshold`의 기본값은 25입니다. clippy 문서는 이름이 오해를 부른다며 `excessive_nesting`(`excessive-nesting-threshold`, 기본 0)과 `too_many_lines`(기본 100)를 대안으로 권합니다. https://doc.rust-lang.org/clippy/lint_configuration.html
- gocyclo(Go): https://github.com/fzipp/gocyclo, BSD-3, v0.6.0, 마지막 push 2025-12-27.
- gocognit(Go): https://github.com/uudashr/gocognit, MIT, v1.3.0(2026-10-06).
- 둘 다 golangci-lint에 포함되어 있습니다. 같은 계열로 cyclop(패키지 단위 포함), funlen, nestif, maintidx가 있습니다. https://golangci-lint.run/docs/linters/
- eslint `complexity`(TS): cyclomatic이고 기본 max는 20, `variant`는 classic 또는 modified입니다. 코어 문서에 cognitive complexity 규칙은 없습니다. https://eslint.org/docs/latest/rules/complexity
- Unverified: cognitive complexity는 eslint-plugin-sonarjs로 쓸 수 있다고 알고 있지만 확인하지 않았습니다.

### Part B: CI에서 쓸 아키텍처 규칙

Rust:
- cargo-modules: https://github.com/regexident/cargo-modules
  - 라이선스 MPL-2.0, 태그 v0.28.0, 마지막 push 2026-11-15.
  - `dependencies --acyclic`은 module 단위 순환을 찾으면 오류로 끝납니다. `--no-fns --no-types --no-traits --no-uses --no-externs --no-sysroot`를 주면 module만 남은 그래프를 DOT로 출력하고, `structure`(트리)와 `orphans`(어디에도 연결되지 않은 파일)도 있습니다.
  - "config must not import server" 같은 규칙 언어는 없습니다. DOT를 스크립트로 검사해야 한다는 것은 제 추론입니다.
  - Unverified: rust-analyzer 기반이라 경로를 정확히 해석할 것으로 보지만 확인하지 않았습니다.
- cargo-deny: https://github.com/EmbarkStudios/cargo-deny
  - 라이선스 Apache-2.0, 0.21.0(2026-11-16).
  - crate 단위만 다룹니다. bans의 `deny`, `wrappers`(특정 crate만 금지된 crate를 직접 의존하도록 허용), `allow` 목록, features allow/deny, build script 검사를 제공하고, 문서에 module 단위 기능은 없습니다. https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html
  - Unverified: `deny=[{crate="server", wrappers=["app"]}]`로 workspace crate 간 규칙을 흉내 낼 수 있는지는 확인하지 못했습니다.
- arch_test: https://github.com/tdymel/arch_test
  - 라이선스 AGPL-3.0, 마지막 push 2023-08. fork인 https://github.com/magistrser/arch_test 는 마지막 push가 2026-09입니다.
  - 규칙: MayNotAccess, MayOnlyAccess, MayNotBeAccessedBy, MayOnlyBeAccessedBy, NoParentAccess, NoModuleCyclicDependencies. JSON 설정이나 `#[test]`로 module 단위 검사를 합니다.
  - 이 내용은 검색 요약 기준입니다. README를 직접 열지 않았으므로 Unverified입니다.
- ArchUnitRust: https://github.com/dragonsec-ai/ArchUnitRust
  - 라이선스 MIT, 마지막 push 2026-10-02, star 65개.
  - Unverified: crates.io `archunit` 0.0.2라는 점, layer allowlist/denylist, `// archunit: ignore` 기능은 검색 요약에만 근거합니다.
- clippy `disallowed_methods`, `disallowed_types`, `disallowed_macros`:
  - clippy.toml에 `{ path, reason, replacement, allow-invalid }` 형식으로 적습니다.
  - 특정 경로의 함수, 타입, 매크로 사용을 금지할 수 있습니다. 예를 들어 `crate::server::*`를 하나씩 나열하는 식이고, module 경로 전체를 wildcard로 막을 수 있는지는 문서에 없습니다.
  - 적용 범위는 clippy.toml 위치에 따라 crate 단위입니다. "lint를 deny하고 허용할 module에서만 `#[allow]`" 패턴은 lint attribute 규칙에서 끌어낸 추론입니다.
- ast-grep: https://github.com/ast-grep/ast-grep
  - 라이선스 MIT, 0.51.3(2026-11-16). Rust, Go, TS, TSX가 built-in입니다.
  - rule YAML의 `files`와 `ignores` glob으로 범위를 정합니다. 예를 들어 `files: [src/config/**]` 범위에 `use crate::server::$$$` 패턴을 severity error로 겁니다.
  - 한계: 구문 매칭이라 `super::`, re-export, 본문 안의 전체 경로는 놓칩니다(제 추론).
  - Unverified: scan의 CI exit code 동작은 이 페이지에서 확인하지 못했습니다.
- semgrep: https://github.com/semgrep/semgrep
  - 라이선스 LGPL-2.1. 문서상 Rust, Go, TS 모두 GA입니다.
  - Rust는 cross-function dataflow만 있고 cross-file은 없습니다. https://docs.semgrep.dev/supported-languages
- 가장 강한 수단은 컴파일러입니다. 레이어를 workspace crate로 나누면 Cargo.toml에 없는 의존은 컴파일되지 않고, crate 안에서는 `pub(crate)`나 `pub(in path)`로 노출을 제한합니다(일반 지식 기반, 출처 미확인).

Go:
- go-arch-lint: https://github.com/fe3dback/go-arch-lint
  - 라이선스 MIT, v1.19.2(2026-11-17).
  - `.go-arch-lint.yml`의 components(`in` glob)와 deps(`mayDependOn`)로 "handlers는 services만 의존 가능" 같은 규칙을 둡니다. import 단위로 검사합니다.
  - `check`는 위반이 있으면 exit 1로 끝나고, 출력은 ascii 또는 json입니다. `graph` 명령도 있습니다.
- depguard: https://github.com/OpenPeeDeeP/depguard
  - 라이선스 GPL-3.0, v2.0.0(2023-03-25), 마지막 push 2026-11-02.
  - 이름 붙인 rule마다 `files` glob(`$all`, `!$test`)과 `listMode`(Original/Strict/Lax), allow/deny prefix, 메시지를 둡니다.
  - 예: `files: ["**/config/**"]`에 `deny: {"example.com/app/server": "config must not import server"}`. golangci-lint 안에서 돌릴 수 있습니다.
- `internal/`: `.../a/b/c/internal/d`는 `.../a/b/c` 아래에서만 import할 수 있고, Go 1.5부터 모든 저장소에서 컴파일러가 강제합니다. https://go.dev/doc/go1.4#internalpackages
- `go list -deps -json ./...` 또는 `go list -f '{{.ImportPath}} {{join .Imports " "}}' ./...`로 import 그래프를 뽑아 스크립트로 검사할 수 있습니다(일반 지식 기반, 출처 미확인).
- 참고: arch-go https://github.com/fdaines/arch-go 는 MIT이고 마지막 push가 2024-10입니다. 기능은 확인하지 않았습니다.

TypeScript:
- dependency-cruiser: https://github.com/sverweij/dependency-cruiser
  - 라이선스 MIT, v18.8.0(2026-11-16).
  - forbidden, allowed, required 규칙을 씁니다. `from`/`to`의 `path`/`pathNot`은 정규식이고 `$1` group 참조로 "자기 component 밖 import 금지"를 표현할 수 있습니다. `circular`(via/viaOnly), `orphan`, `reachable`, `dependencyTypes`(type-only 등)도 있습니다.
  - 예: `{name:"no-config-to-server", severity:"error", from:{path:"^src/config"}, to:{path:"^src/server"}}`
  - 출력은 dot, mermaid, html, json 등 여러 가지입니다. https://github.com/sverweij/dependency-cruiser/blob/main/doc/rules-reference.md
- eslint-plugin-boundaries: https://github.com/javierbrea/eslint-plugin-boundaries
  - 라이선스 MIT, v7.4.4(2026-11-15).
  - `boundaries/elements`에 type과 pattern을 정의하고, `boundaries/dependencies` 규칙에서 `default:"disallow"`와 `from`/`to` element type 정책을 씁니다. `element-types`는 deprecated alias입니다.
  - 그 밖에 no-unknown-files, no-unknown-dependencies, no-ignored-dependencies가 있고, entry-point, external, no-private는 deprecated입니다. https://www.jsboundaries.dev/docs/rules/
  - Unverified: TS path alias를 해석하는 데 resolver가 필요한지는 확인하지 못했습니다.
- madge: https://github.com/pahen/madge
  - 라이선스 MIT, 태그 v8.0.0, 마지막 commit 2024-08-05. 정체 상태입니다.
  - `--circular`, `--orphans`, `--leaves`, `--image`/`--dot`(Graphviz 필요)를 제공하고 tsConfig로 alias를 해석합니다. 규칙 언어는 없습니다.
  - Unverified: `--circular`의 exit code는 확인하지 못했습니다.

### Part C: GUI 구성 요소

그래프 렌더링과 레이아웃:
- Cytoscape.js: https://github.com/cytoscape/cytoscape.js
  - 라이선스 MIT, v3.35.0(2026-11-10). compound node(`parent`)를 지원하고 extension이 70개입니다(elk, dagre, fcose, cola 포함).
  - 3.31부터 WebGL 렌더러 preview가 있습니다. 공식 벤치마크: 1.2k node, 16k edge에서 canvas 약 20 FPS, WebGL 100 FPS 이상. 3.2k node, 68k edge에서 canvas 3 FPS, WebGL 10 FPS.
  - WebGL 모드는 일부 스타일만 지원합니다(segments edge 없음). https://blog.js.cytoscape.org/2025/01/13/webgl-preview/
- Sigma.js와 graphology: https://github.com/jacomyal/sigma.js (MIT, sigma@4.0.6, 2026-11-10), https://github.com/graphology/graphology (MIT)
  - WebGL 기반이고, 레이아웃(ForceAtlas2)과 알고리즘은 graphology에 있습니다.
  - Unverified: 규모 수치("tens of thousands of nodes, hundreds of thousands of edges")는 v2 릴리스 글의 검색 요약에만 근거하고, 공식 사이트에는 수치가 없습니다.
- React Flow/xyflow: https://github.com/xyflow/xyflow
  - 라이선스 MIT, @xyflow/react 13.0.1(2026-11-16).
  - 성능 문서는 node 수 상한을 제시하지 않고 re-render 최적화와 `hidden`으로 접기를 권합니다. 레이아웃 엔진은 들어 있지 않습니다. https://reactflow.dev/learn/advanced-use/performance
  - Unverified: DOM 기반이라 수백~수천 node가 한계라는 것은 일반 지식 기반, 출처 미확인입니다.
- ELK/elkjs: https://github.com/kieler/elkjs
  - 라이선스 EPL-2.0(LICENSE.md), 0.12.1(2026-10-28).
  - 레이아웃만 계산하고 렌더링은 하지 않습니다. Java ELK를 GWT로 변환한 것이고, layered(port와 nested children 지원), stress, mrtree, radial, force를 제공합니다. Web Worker로 실행할 수 있습니다.
  - 공개된 규모 수치는 없습니다.
- Graphviz: https://gitlab.com/graphviz/graphviz
  - 라이선스 EPL-2.0(https://graphviz.org/license/). GitHub 미러는 archived 상태입니다.
  - cargo-modules, madge, dependency-cruiser가 모두 DOT를 출력하므로 공통 교환 포맷으로 쓸 수 있습니다.
- AntV G6: https://github.com/antvis/G6
  - 라이선스 MIT, 5.1.1(2026-04-17). 레이아웃이 10종 이상이고 WebGPU/WASM 가속을 주장합니다. 규모 수치는 없습니다.
- cosmos.gl: https://github.com/cosmosgl/graph
  - 라이선스 MIT, v3.6.0(2026-11-04). GPU에서 시뮬레이션과 렌더링을 모두 수행하고 "hundreds of thousands of points and links"를 주장합니다.
  - 요구 사항: luma.gl(WebGL2), `EXT_float_blend`. 레이블이나 계층 구조가 아니라 점 구름에 가까운 시각화입니다.
- D3: https://github.com/d3/d3 (ISC). DSM, treemap, sunburst, edge bundling(d3.cluster와 curveBundle)에 쓸 수 있습니다.
  - Unverified: SVG force는 수천 node가 한계라는 것은 일반 지식입니다.
- 3d-force-graph와 force-graph: https://github.com/vasturiano/3d-force-graph, https://github.com/vasturiano/force-graph (둘 다 MIT, 2026년에도 활발). three.js/WebGL 또는 canvas 기반입니다.
  - 확인한 규모 수치는 없습니다.
- 네이티브 Qt Quick: QuickQanava https://github.com/cneben/QuickQanava
  - 라이선스 BSD-3, Qt 6.5 이상, 마지막 push 2026-11-12. 스스로 "medium-sized directed graphs" 대상이라고 밝히고, 자동 레이아웃은 없습니다.
  - Unverified: Qt 자체의 LGPL-3/GPL/상용 라이선스 조건과 Qt Graphs 모듈이 node-link가 아니라 차트용이라는 점은 확인하지 않았습니다.
- egui_graphs: https://github.com/blitzarx1/egui_graphs
  - 라이선스 MIT, v0.32.0(2026-08-31). petgraph 기반이고, README에 "project is not in active development"라고 적혀 있습니다. force layout은 naive O(n²)입니다.
- Sourcetrail fork: https://github.com/petermost/Sourcetrail
  - 라이선스 GPL-3.0, Qt6. 지원 언어는 C/C++와 Java뿐이고 Python은 2026.10에 제거됐습니다. Rust, Go, TS 지원은 없습니다.
  - 원본 https://github.com/CoatiSoftware/Sourcetrail 은 archived 상태입니다. UI 패턴(초점 symbol 중심의 이웃 그래프와 코드 패널 연동)만 참고할 만합니다.

Shell:
- Tauri: https://github.com/tauri-apps/tauri
  - 라이선스 Apache-2.0, tauri-cli 2.12.5. Linux에서는 WebKitGTK를 씁니다.
  - 공식 문서: WebGL2 context가 slow path나 software rasterizer로 뒷받침될 때도 아무 오류 없이 만들어지고, renderer 문자열은 항상 "Apple GPU"로 보고됩니다. 그래서 "provide a non-WebGL fallback on Linux"를 권합니다. NVIDIA DMA-BUF 문제도 있습니다. https://v2.tauri.app/develop/debug/linux-graphics/
  - Chromium 런타임(tauri-runtime-cef)은 Tauri 3 alpha에만 있습니다(alpha.7, 2026-11-06). https://v3.tauri.app/develop/cef/
  - Unverified: 현재 CEF 154로는 Linux 패키지가 빌드되지 않는다는 보고는 검색 요약에만 근거합니다.
- Electron: https://github.com/electron/electron
  - 라이선스 MIT, 활발히 유지되고 있습니다. Chromium을 번들하므로 Linux에서도 WebGL 동작이 예측 가능합니다(일반 지식 기반, 출처 미확인).
- Qt(QML): 네이티브라 성능은 좋지만 그래프 위젯 생태계가 얇습니다. QuickQanava 수준이고, DSM, treemap, 레이아웃은 직접 구현해야 합니다.

시각화 형태와 근거:
- DSM: Sangal, Jordan, Sinha, Jackson, OOPSLA 2005. 정적 분석으로 의존성을 추출해 DSM으로 아키텍처를 관리하고 설계 결함을 표시하는 방법을 보이고, Ant를 사례로 씁니다. https://groups.csail.mit.edu/sdg/pubs/2005/oopsla05-dsm.pdf
- 행렬과 node-link 비교: Ghoniem, Fekete, Castagliola, InfoVis 2004. vertex가 20개를 넘으면 대부분의 과제에서 matrix가 우세했고, path finding만 node-link가 나았으며 그마저 크고 조밀한 그래프에서는 나빠졌습니다. 검색 요약 기준이고 원문은 403이었습니다. https://www.researchgate.net/publication/221005943_A_Comparison_of_the_Readability_of_Graphs_Using_Node-Link_and_Matrix-Based_Representations
- city/treemap: Wettel, Lanza, Robbes, ICSE 2011, 참가자 41명(학계와 산업계). 원문 PDF에서 확인한 수치입니다. https://wettel.github.io/download/Wettel11a-icse.pdf
  - Eclipse와 Excel 조합 대비 정답률 +24.26%, 소요 시간 -12.01%였습니다.
  - 개요가 필요한 과제(영향 범위 추정)에서는 정답률이 40-50% 높았습니다.
  - 정확한 값을 찾는 과제는 Excel이 더 빨랐습니다: "a spreadsheet is faster at finding precise answers".
- Hierarchical edge bundling: Holten, IEEE TVCG 12(5), 2006. 계층(module 트리) 위에 의존 edge를 다발로 묶어 상위 수준 관계와 하위 관계를 함께 보여 줍니다. 근거는 비공식 사용자 연구입니다. https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Holten06.pdf
- hairball 비판: Kosara, "Graphs Beyond the Hairball". node-link는 "breaks down beyond a few dozen nodes"이고, 대안으로 matrix, node quilt, PivotGraph(속성별 집계)를 제시합니다. https://eagereyes.org/blog/2012/graphs-hairball
- 근거의 질: Merino et al., JSS 2018. SOFTVIS/VISSOFT 논문 387편 가운데 181편을 분석한 결과 "62% lack a strong evaluation"이었습니다. 소프트웨어 시각화 효과에 대한 증거 자체가 얇다는 뜻입니다. https://homepages.ecs.vuw.ac.nz/~craig/publications/jss2018-merino.pdf

### 권장 스택
1. 이력 엔진은 직접 구현하기를 권합니다(gix나 git2 기반). code-maat의 revisions, coupling, soc, age, ownership 개념을 따르되 release tag 구간 단위로 집계합니다(24개 릴리스면 tag 간 diff만으로 충분합니다). code-maat는 GPL이고 2023년 이후 릴리스가 없으며, hercules와 git-of-theseus는 중단 상태입니다.
2. 함수 단위 복잡도는 lizard(MIT, Rust/Go/TS 모두, CI exit code)를 기본으로 하고, 파일 지표와 delta 비교는 CodeCharta cc.json(BSD-3, tree-sitter로 세 언어 지원)을 가져와 씁니다. rust-code-analysis는 Go가 없고, SonarQube CB는 26.11부터 Rust가 빠졌습니다.
3. hotspot은 "구간 내 변경 횟수 × 함수 복잡도"로 계산하고, change coupling은 release 구간별로 계산해 시간 슬라이더로 보여 줍니다.
4. 의존 그래프 수집: Rust는 cargo-modules DOT(module 단위, `--acyclic`), Go는 `go list -json`, TS는 dependency-cruiser JSON입니다.
5. CI 규칙: Rust는 crate 분리, `pub(crate)`, cargo-modules `--acyclic`에 DOT 기반 자체 규칙 검사를 더합니다. ArchUnitRust는 0.0.x이고 arch_test는 AGPL이라 직접 의존은 비추천합니다. Go는 go-arch-lint, `internal/`, depguard 조합이고, TS는 dependency-cruiser입니다.
6. Shell은 Linux 우선이므로 Electron을 권합니다. Tauri 2는 WebKitGTK의 WebGL slow path를 공식 문서가 인정하고, CEF 런타임은 alpha입니다. Tauri 3 CEF가 안정화되면 재평가하면 됩니다.
7. 주 뷰는 module DSM(canvas 직접 구현), hotspot treemap(D3), 초점 이웃 그래프(Cytoscape.js와 elkjs layered, compound=module)입니다. 전체 node-link는 sigma.js로 탐색용으로만 둡니다. React Flow는 수십 node 규모의 편집형 뷰에만 씁니다.
8. 근거가 약하므로(Merino 62%) 기본 화면은 hairball이 아니라 DSM, treemap, 목록 조합으로 시작하고, 구체적인 질문에서 출발하는 탐색(영향 범위, 소유자, 결합)을 제공하는 쪽을 권합니다.
