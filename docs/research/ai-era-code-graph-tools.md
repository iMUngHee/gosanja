# Code-graph tools of 2024-2026 and the comprehension problem — research record (2026-11-18)

Tools that build a code knowledge graph or code map (most of them for AI agents), writing
and studies on humans losing track of agent-written code, and a gap analysis for the
human-maintainer use case.

Collected by a web-research agent on 2026-11-18. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The text is kept as the researcher wrote it, in Korean or English.

## Report

AI 시대 코드 그래프 도구 조사 결과(Part A/B/C)입니다. 수치는 2026-11-18에 `gh api`로 측정했고, 별 수(★)·마지막 push·최신 릴리스를 함께 적었습니다. 내가 직접 열어 보지 못한 항목에는 "Unverified:"를 붙였습니다. 처음 시도한 비인증 GitHub API는 같은 IP를 쓰는 다른 팀원과 한도(60회/시간)를 나눠 써서 소진됐고, 이후 인증된 `gh api`(읽기 전용 GET)로 조회했습니다.

### Part A — 도구별 사실

#### 1. 엔진 후보: 오픈소스, 로컬, Rust/Go/TS 모두 지원

**code-graph-rag**
- URL: https://github.com/vitali87/code-graph-rag
- 라이선스·활동: MIT, ★5.4k, v0.7.1 (2026-11-17)
- 추출: tree-sitter가 기본입니다. README는 "compiler-grade frontends and runtime traces where available"라고 하지만 어떤 언어에 적용되는지는 밝히지 않습니다.
- 언어: README가 Rust/Go/TS를 "fully supported"로 표기합니다.
- 소비자: 에이전트입니다(MCP, Cypher를 생성하는 RAG, AST 패치 편집).
- 저장: 기본은 임베디드 SQLite(~/.cgr/graph.sqlite)이고, Memgraph와 Neo4j는 선택 사항입니다.
- 사람용 GUI: 확인하지 못했습니다.

**GitNexus**
- URL: https://github.com/abhigyanpatwari/GitNexus
- 라이선스·활동: ★48.6k, v1.6.12 (2026-09-12), push 2026-11-17
- 라이선스 주의: PolyForm Noncommercial 1.0.0이라 OSI 기준 오픈소스가 아닙니다. 상업적 사용에는 Akon Labs의 별도 라이선스가 필요하므로 GUI 엔진으로 쓰기 어렵습니다.
- 추출: tree-sitter
- 언어: Rust/Go/TS에 "registered providers"가 있고, 언어별 결함 문서가 따로 있습니다(docs/languages/rust-ingestion-gaps.md, go-…, typescript-…).
- 저장: LadybugDB(KuzuDB 포크)에 Cypher, BM25, 벡터 검색
- 소비자: 에이전트가 우선입니다(MCP, skills, Claude Code hooks).
- 사람용 뷰: `gitnexus serve`를 띄우고 웹 UI(https://gitnexus.vercel.app)로 접속하면 그래프 탐색기와 AI 채팅을 쓸 수 있습니다.
- diff: `detect_changes`가 git diff를 영향 범위로 매핑합니다.

**Graphify**
- URL: https://github.com/Graphify-Labs/graphify (예전 safishamsi/graphify)
- 라이선스·활동: Apache-2.0, ★149k, v0.9.111 (2026-11-15)
- 추출: tree-sitter. 코드는 LLM 없이 처리하고, 문서·PDF·미디어는 선택적으로 LLM 패스를 거칩니다. 커뮤니티는 Leiden으로 묶습니다.
- 언어: 저장소에 graphify/extractors/rust.py와 go.py가 있고, TS는 tree-sitter-typescript와 TS 테스트로 확인했습니다.
- 출력: graphify-out/ 아래 graph.html, GRAPH_REPORT.md, graph.json. Neo4j와 FalkorDB로 push할 수 있습니다.
- 소비자: 에이전트 스킬과 호스티드 MCP가 주용도이고, graph.html이 사람용 정적 뷰입니다.

**CodeGraphContext**
- URL: https://github.com/CodeGraphContext/CodeGraphContext
- 라이선스·활동: MIT, ★4.4k, v0.5.7 (2026-08-08), push 2026-11-16
- 추출: tree-sitter. SCIP은 선택 사항인데, README가 SCIP을 명시한 언어는 C/C++(scip-clang)와 C#(scip-dotnet)뿐입니다. 따라서 Rust/Go/TS는 tree-sitter 휴리스틱으로 처리합니다.
- 언어: Go, Rust, TS 모두 지원 목록에 있습니다.
- 저장: FalkorDB Lite가 기본이고, Kuzu, LadybugDB, Neo4j도 쓸 수 있습니다.
- 소비자: MCP와 CLI. "interactive visualization"을 제공합니다.

**code-review-graph**
- URL: https://github.com/tirth8205/code-review-graph
- 라이선스·활동: MIT, ★32.8k, v2.4.0 (2026-11-11)
- 추출: tree-sitter와 targeted fallback
- 언어: Go, Rust, TS 지원. 다만 README 스스로 "JavaScript and Go flow detection needs work"라고 적고 있습니다.
- 저장: SQLite 단일 파일
- 소비자: 에이전트 리뷰가 주용도입니다(MCP, 위험도를 매긴 `detect_changes`, GitHub Action PR 코멘트).
- 사람용 뷰: D3 force-directed HTML, 심볼·플로우·변경을 시드로 한 neighbourhood 뷰. GraphML, Cypher, Obsidian, SVG로 export할 수 있습니다.
- 마케팅 주장: README의 "6.8× fewer tokens", "49×" 같은 토큰 절감 수치는 자체 벤치마크입니다.

**Understand-Anything**
- URL: https://github.com/Egonex-AI/Understand-Anything (예전 Lum1104)
- 라이선스·활동: MIT, ★93.0k, v2.9.8 (2026-11-15)
- 추출: tree-sitter가 구조 사실(import, def, call)을 뽑고, LLM 멀티에이전트가 요약·계층(layer)·도메인 매핑을 붙이는 하이브리드입니다.
- 언어: packages/core/src/languages/configs/ 아래 rust.ts, go.ts, typescript.ts가 있습니다.
- 저장: .ua/knowledge-graph.json (JSON을 커밋해 팀이 공유)
- 소비자: 사람 GUI가 중심인 몇 안 되는 도구입니다.
- 사람용 뷰: React Flow 대시보드. 아키텍처 계층별 색, 가이드 투어, `/understand-diff` diff overlay를 제공하고, LLM 없이 Node만으로 볼 수 있습니다.
- 한계: 계층 판정은 LLM이 하므로 결정적이지 않습니다.

**CodeGraph (colbymchenry)**
- URL: https://github.com/colbymchenry/codegraph
- 라이선스·활동: MIT, ★77.8k, v1.6.2 (2026-10-03), push 2026-11-17
- 추출: Rust 네이티브 커널에 tree-sitter를 내장했습니다.
- 언어: TS/Go/Rust 포함 20개 이상
- 저장: SQLite(.codegraph/codegraph.db)와 FTS5
- 소비자: 에이전트 전용(MCP)입니다. 사람용 GUI는 확인하지 못했습니다.
- 특기 사항:
  - 웹 프레임워크 라우트를 핸들러에 연결합니다.
  - Tauri의 TS `invoke('save_file')`과 Rust command를 양방향으로 연결합니다. 문자열 기반 언어 간 계약을 일부 다루는 셈입니다.
  - `codegraph affected --stdin`에 git diff를 넣으면 영향 범위를 냅니다.

**codebase-memory-mcp**
- URL: https://github.com/DeusData/codebase-memory-mcp
- 라이선스·활동: MIT, C로 작성, ★48.8k, v0.12.1 (2026-10-16)
- 추출:
  - 내장(vendored) tree-sitter 문법
  - "Hybrid LSP": 실제 언어 서버를 띄우지 않고, 타입 해석 알고리즘을 C로 경량 재구현한 것입니다(README 표현으로는 "structurally inspired by" LSP). TS, Go, Rust가 대상에 포함됩니다.
- 저장: SQLite
- 특기 사항:
  - HTTP route와 호출 지점을 신뢰도 점수로 매칭합니다.
  - gRPC, GraphQL, tRPC를 감지합니다.
  - 코드의 환경변수 읽기를 공유 `EnvVar` 노드로 묶는 `CONFIGURES` 엣지가 있습니다.
  - Kubernetes, compose, Terraform도 엣지로 연결합니다.
- 사람용 뷰: "CBM Lens" 브라우저 UI(open beta v0.0.1). 아키텍처, 라우트, 서비스 맵을 보여 줍니다.
- 관련 논문: https://arxiv.org/abs/2603.27277 (2026-03-28). 31개 저장소에서 답변 품질은 83%로, 파일 탐색 에이전트(92%)보다 낮았습니다. 대신 토큰은 약 10배, tool call은 2.1배 적게 썼습니다.

**Auryn (구 FalkorDB code-graph)**
- URL: https://github.com/FalkorDB/Auryn (FalkorDB/code-graph로 요청하면 이 저장소로 리다이렉트됩니다)
- 라이선스·활동: MIT, ★385, v0.8.1 (2026-11-16)
- 추출: Go, Rust, TS는 tree-sitter만 씁니다. C#은 Roslyn, Python은 Jedi로 정밀 분석합니다.
- 저장: FalkorDB(임베디드 또는 서버)
- 사람용 뷰: 로컬 viewer(:8080). 언어 간 경로(TS 버튼에서 C# 컨트롤러까지)를 하나의 스키마로 이어 보여 준다고 주장합니다.

**codegraph-rust**
- URL: https://github.com/Jakedismo/codegraph-rust
- 라이선스·활동: Cargo.toml에 `MIT OR Apache-2.0`이 있지만 루트에 LICENSE 파일은 없습니다. ★915, push 2026-10-12
- 추출: 이번 조사에서 실제 언어 서버로 해석하는 몇 안 되는 도구입니다. balanced/full 티어에서 rust-analyzer, gopls, typescript-language-server로 해석하고, Rust 로컬 dataflow 엣지를 만듭니다.
- 아키텍처 규칙: codegraph.boundaries.toml의 크레이트 단위 deny 규칙에 걸리면 `violates_boundary` 엣지를 만들고, 패키지 사이클을 표시합니다.
- 저장: Unverified: SurrealDB (schema/*.surql 파일이 있어서 추정한 것입니다)
- 소비자: 에이전트(MCP)

**sem**
- URL: https://github.com/Ataraxy-Labs/sem
- 라이선스·활동: Rust로 작성, 2026-02 생성, ★3.5k, push 2026-11-14. 라이선스는 API가 Apache-2.0으로, 배지는 MIT로 표기합니다(이중 라이선스로 보입니다).
- 추출: tree-sitter 28개 언어
- 기능:
  - git 위에서 엔티티(함수·클래스) 단위로 diff, blame, impact를 계산합니다.
  - `sem graph --modules metrics`는 JS/TS 모듈 그래프의 사이클·깊이·중심성을 냅니다.
  - `sem arch-diff RANGE`와 `sem certify`(커밋 범위의 시그니처 변경, 남겨진 caller, 모듈 도달성 변화)를 제공합니다.
- 저장: SQLite 캐시
- 언어: TS와 Go는 README에서 확인했습니다. Unverified: Rust 엔티티 추출 품질
- 소비자: 에이전트와 CI가 우선입니다(JSON, MCP).

#### 2. 엔진 적합성이 낮거나 성격이 다른 도구

**Serena**
- URL: https://github.com/oraios/serena
- 라이선스·활동: 앱은 GPL-3.0-or-later, SolidLSP 부분만 MIT. ★31.5k, v2.1.0 (2026-10-19)
- 성격: 그래프 DB가 없습니다. LSP(40개 이상 언어, Go/Rust/TS 포함) 또는 JetBrains 백엔드로 심볼 단위 조회·편집 도구를 MCP로 제공합니다. 에이전트용이고, 정밀도는 높지만 지속 그래프나 사람용 시각화가 없습니다.

**Axon**
- URL: https://github.com/harshkedia177/axon
- 라이선스·활동: 배지만 있고 LICENSE 파일이 없습니다(API 라이선스 null). ★820, v1.0.1 (2026-03-09), push 2026-08-03
- 언어: Python, TS, JS뿐이라 Rust와 Go가 없습니다.
- 기능: 웹 대시보드(Sigma.js, 커플링 히트맵, 건강 점수)와 `axon diff main..feature`. 후자는 git worktree로 심볼을 추가·수정·삭제 단위로 비교하는 구조 diff입니다. 대상 코드베이스에는 쓸 수 없습니다.

**Potpie**
- URL: https://github.com/potpie-ai/potpie
- 라이선스·활동: Apache-2.0, ★5.8k, v2.0.1 (2026-08-31)
- 성격: 현재 README는 코드 구조보다 "codebase and SDLC context graph"(결정·주장 기록)에 무게를 둡니다. 백엔드는 falkordb_lite, neo4j 등이고 `potpie ui` 탐색기가 있습니다.
- Unverified: 코드 추출 방식과 Rust/Go 지원

**deepwiki-open**
- URL: https://github.com/AsyncFuncAI/deepwiki-open
- 라이선스·활동: MIT, ★18.4k, 릴리스 없음, push 2026-11-15
- 성격: LLM이 위키, 다이어그램, "codemap" 투어를 생성합니다. 2.0은 "Grok-Wiki"로 이름을 바꿨습니다. 결정적 그래프가 아닙니다.
- Unverified: 임베딩 RAG 상세

**Sourcebot**
- URL: https://github.com/sourcebot-dev/sourcebot
- 라이선스·활동: MIT, ★4.0k, v6.1.0 (2026-11-10)
- 성격: 코드 검색이 본업입니다. 코드 내비게이션은 universal-ctags로 정의를 찾고, 참조는 텍스트 검색으로 찾는 휴리스틱입니다(문서가 "search heuristics"라고 명시합니다).
- Unverified: ee/ 디렉터리의 상용 라이선스 여부

**Repomix**
- URL: https://github.com/yamadashy/repomix
- 라이선스·활동: MIT, ★29.3k, v1.19.0
- 성격: 저장소를 LLM용 단일 파일로 묶는 도구입니다. `--compress`는 tree-sitter로 시그니처만 남깁니다. 그래프는 만들지 않습니다.

**Aider repo map**
- URL: https://aider.chat/docs/repomap.html
- 성격: 파일 의존성 그래프에 랭킹 알고리즘을 적용해 토큰 예산(기본 1k) 안의 텍스트 맵을 만들고 LLM에 보냅니다. 사람용 뷰가 아닙니다.
- Unverified: tree-sitter와 PageRank 사용(해당 페이지는 "graph ranking algorithm"이라고만 씁니다)

**Glamorous Toolkit**
- URL: https://github.com/feenkcom/gtoolkit
- 라이선스·활동: MIT, Pharo와 Rust로 작성, ★1.6k, v1.1.687 (2026-11-17)
- 성격: 사람이 문제마다 맞춤 뷰를 만드는 moldable 환경입니다. 사이트에는 Rust와 TS 분석 지원이 적혀 있고 Go는 없습니다. 엔진이라기보다 철학적 선례에 가깝습니다.

**Nogic**
- URL: https://www.nogic.dev/ , https://marketplace.visualstudio.com/items?itemName=Nogic.nogic
- 성격: VS Code 확장입니다. 사이트 문구는 "Your agent just edited 40 files…"로, 에이전트 diff를 의도별 walkthrough, 코드 그래프, 시퀀스 다이어그램으로 보여 준다고 합니다. 문제 설정이 이번 목적과 가장 가깝습니다.
- 한계: 오픈소스 여부를 확인하지 못했습니다. 정적 코드 맵은 JS/TS/Python만 지원해 Rust와 Go가 없습니다(검색 결과 기준, Marketplace 원문은 열지 않음). HN 사용자들이 2000개 이상 파일에서 느려지거나 크래시가 난다고 보고했습니다(2026-01).

**CodeViz (YC S24)**
- URL: https://codeviz.ai
- 성격: 독점 제품입니다. v1은 VS Code 언어 서버로 콜 그래프를 만들고 LLM으로 C4 다이어그램을 그렸습니다. v2는 gemini-cli를 개조해 C4와 PlantUML을 생성하고 편집하는 도구로 바뀌었습니다.
- 출처: https://news.ycombinator.com/item?id=46112279

**Windsurf Codemaps (Cognition)**
- URL: https://cognition.com/blog/codemaps
- 성격: 독점이며 2025-11-04에 나왔습니다. SWE-1.5 또는 Claude Sonnet 4.5가 작업 프롬프트별로 주석 달린 맵을 생성합니다. 결정적 그래프가 아니라 LLM 스냅샷입니다.

**Copilot / Cursor**
- Unverified: 내장 시각 코드 맵 기능을 찾지 못했습니다. 검색 범위는 웹 검색 2회로, 공식 changelog는 확인하지 않았습니다.
- Cursor는 AI용 임베딩 인덱스를 가지고 있습니다. "Code Map"은 Visual Studio Enterprise 기능입니다(일반 지식 기반, 출처 미확인).

#### 3. Part A 관찰

- 추출 방식: 거의 전부 tree-sitter 이름 해석 휴리스틱입니다. Rust trait 메서드, 제네릭, 매크로 호출의 정확한 해석은 codegraph-rust(rust-analyzer)와 Serena(LSP)만 기대할 수 있습니다.
- 그래프 방식 비교 근거: AST 그래프와 LLM 생성 그래프를 비교한 https://arxiv.org/abs/2601.08773 (2026-01, Java 3개 저장소, 단독 저자)에서 AST 그래프가 정확도·비용·커버리지 모두 우위였습니다. LLM-KB는 Shopizer에서 377개 파일을 누락했습니다.
- 소비자: 주 소비자는 압도적으로 에이전트(MCP, 토큰 절감)입니다. 사람용 뷰는 대부분 force-directed 그래프입니다.
- 라이선스: GUI 엔진 후보로서 걸리는 것은 GitNexus(비상업), Serena(GPL), Axon·codegraph-rust(LICENSE 파일 부재)입니다.

### Part B — 문제 자체 (핵심 출처 7개)

**1. Anthropic RCT "How AI Impacts Skill Formation"** (2026-01-29, arXiv:2601.20245)
- URL: https://www.anthropic.com/research/AI-assistance-coding-skills
- 설계: 주니어 위주 52명이 새 라이브러리(Trio)를 익히는 작업
- 결과:
  - AI 그룹의 퀴즈 점수는 50%, 수작업 그룹은 67%였습니다(d=0.738, p=0.01). 격차가 가장 큰 영역은 디버깅이었습니다.
  - 속도 차이는 유의하지 않았습니다(p=0.391).
  - 생성 후 이해 질문을 한 패턴은 약 86%, 위임 패턴은 40% 미만이었습니다. 다만 상관 관계이고 n이 작습니다.

**2. Addy Osmani, "Comprehension Debt"** (O'Reilly Radar, 2026-04-13)
- URL: https://www.oreilly.com/radar/comprehension-debt-the-hidden-cost-of-ai-generated-code/
- 주장: 코드량과 사람이 이해한 양의 격차가 쌓이고, 리뷰는 품질 게이트가 아니라 처리량 문제가 됩니다. DORA, PR 수, 커버리지로는 이해 손실을 감지하지 못합니다.
- 근거: 1번 RCT와 일화가 전부입니다. "<40% vs >65%" 수치는 출처 없이 인용됐는데, 1번 연구의 패턴별 수치로 보입니다. 이건 내 해석입니다.

**3. Margaret-Anne Storey의 cognitive debt와 Triple Debt Model**
- URL:
  - 블로그(2026-02-09): https://margaretstorey.com/blog/2026/02/09/cognitive-debt/
  - 후속 글: https://margaretstorey.com/blog/2026/02/18/cognitive-debt-revisited/
  - 논문(2026-03-23, v4 04-06): https://arxiv.org/abs/2603.22106 , ACM Queue: https://queue.acm.org/detail.cfm?id=3807966
- 주장: technical debt는 코드에, cognitive debt는 팀의 공유 이해에, intent debt는 외부화되지 않은 목표·제약·근거에 있습니다. GenAI는 문제를 없애지 않고 재분배합니다.
- 근거: 개념적 position paper입니다. 경험적 근거는 학생 팀 코칭 일화뿐입니다.
- Unverified: "To appear ICSE 2026"(ResearchGate 표기)

**4. GitClear 2025 "AI Copilot Code Quality"**
- URL: https://www.gitclear.com/ai_assistant_code_quality_2025_research
- 접근 문제: 원문이 403을 반환해 요약본 https://www.jonas.rs/2025/02/09/report-summary-gitclear-ai-code-quality-research-2025.html 으로 확인했습니다.
- 데이터: 2020–2024년 변경 라인 2억 1100만 줄
- 결과:
  - moved(리팩터링) 라인 비중: 24.1% → 9.5%
  - copy/paste 비중: 8.3% → 12.3%
  - 2주 내 재수정(churn): 3.1% → 5.7% (같은 페이지 안에 5.5% → 7.9%라는 다른 수치도 있어 일관되지 않습니다)
  - 중복 블록: 2024년 8배
- 인과: 코드가 AI 작성인지 구분하지 않으므로 인과는 입증되지 않았습니다.
- 2026 후속 "The Maintainability Gap": https://www.gitclear.com/the_ai_code_quality_maintainability_gap
  - Unverified: moved 3.8% (2026 YTD), 블록 중복은 백만 줄당 40.3 → 73.0. 원문 403이라 검색 요약으로만 봤습니다. 다른 경로로 받아오는 시도는 권한 분류기가 거부해서 우회하지 않았습니다.

**5. Faros AI (벤더 텔레메트리)**
- URL: https://www.faros.ai/ai-productivity-paradox , https://www.faros.ai/blog/comprehension-debt-in-software-engineering (2026-09-29)
- 원문에서 확인한 것:
  - 개발자 1만 명, 1,255개 팀에서 변경량은 약 2배로 늘었지만 "company metrics remain flat"
  - 블로그: PR 크기 +71.8%, PR당 파일 +52.0%, 리뷰 없는 merge +76.3%, 월간 인시던트 +125.4%. 표본과 방법론은 밝히지 않았습니다.
- Unverified: 리뷰 시간 +91%(2025)와 +441%(2026)는 검색 요약으로만 봤습니다.
- 평가: 보고서마다 수치가 다르고 벤더 자료이므로 주장으로 취급해야 합니다.

**6. Ahmad, "Comprehension Debt in GenAI-Assisted SE Projects"** (2026-04-14, EASE 2026 Companion, 5쪽)
- URL: https://arxiv.org/abs/2604.13277
- 설계: 학생 207명의 성찰 일지 621건을 질적으로 분석
- 결과: 축적 패턴 4가지(black-box 수용, context-mismatch, dependency-induced atrophy, verification-bypass)와 완화 패턴(comprehension scaffold)을 제시합니다.
- 주의: 검색 결과는 ICPC라고 했지만 arXiv 페이지에는 EASE Companion으로 되어 있습니다.

**7. Lyu et al., "How Do Practitioners Build SE Agents?"** (2026-07-12, v2 07-25, David Lo 등)
- URL: https://arxiv.org/abs/2607.10856
- 설계: 인터뷰 20명(12개 조직)과 설문 80명
- 결과: comprehension debt를 6대 과제 중 하나로 꼽고, 생성 코드 리뷰가 "new and increasingly central forms of work"라고 봅니다. 일부 팀은 유지보수를 다시 AI에 맡겨 부채를 상환하지 않고 우회했습니다. 이 부분은 검색 요약으로 봤고 abstract에는 상세가 없습니다.

**참고: 구조 수준 리뷰 도구와 실천**
- Nogic: AI diff walkthrough (위 Part A)
- Axon diff: 심볼 단위 브랜치 diff
- sem: 엔티티 diff와 arch-diff
- code-review-graph와 GitNexus의 `detect_changes`: 영향 범위
- Understand-Anything `/understand-diff`
- PR Lens: 베이스라인 그래프에 대비해 PR 아키텍처 다이어그램을 그립니다. https://prlens.dev/guides/architecture-diagram-from-a-git-diff . 이 글은 LinearB 2026 수치로 AI PR 75퍼센타일 408줄, 비AI 157줄을 인용합니다(2차 인용).
- WorkBraid: 초기 단계로, 에이전트가 제안한 아키텍처 변경을 3-way diagram diff로 보여 줍니다. https://dev.to/mech_app_ai/workbraid-visual-architecture-diffs-for-agent-proposed-code-changes-4ngi
- Unverified: CodeRabbit과 Greptile이 리뷰 컨텍스트로 코드 그래프를 쓴다는 점(2차 자료)
- METR RCT(2025-07, 16명이 19% 느림)와 2026-02 후속(선택 편향으로 "unreliable")은 생산성 연구이고 이해도 연구가 아니어서 제외했습니다.

### Part C — 갭 분석

1. 커밋 간 그래프 diff: Axon(Py/TS/JS 전용)과 sem(엔티티 수준)만 제공합니다. Rust 크레이트·모듈과 TS 모듈 경계 수준의 "아키텍처 diff 뷰"를 사람용 GUI로 주는 오픈소스는 찾지 못했습니다. 검색 범위는 위 도구 README와 웹 검색 3회입니다.
2. 릴리스에 걸친 이력: 그래프 스냅샷을 커밋·릴리스별로 쌓아 공개 표면(pub 아이템, export) 성장을 추적하는 기능은 본 도구들 README에서 찾지 못했습니다. 모두 "현재 HEAD 인덱스"만 다룹니다.
3. 레이어링 강제: codegraph-rust의 크레이트 단위 deny 규칙과 sem의 TS 모듈 사이클 메트릭이 전부입니다. 모듈·디렉터리 수준 규칙, 언어 공통 규칙, CI 게이트와 GUI 위반 표시를 묶은 도구는 없었습니다.
4. 에이전트 변경의 아키텍처 귀속: "이 에이전트 세션·PR이 어떤 계층과 경계를 건드렸나"를 구조 맵 위에 오버레이하는 기능은 Nogic(JS/TS/Py, 독점)과 Understand-Anything diff overlay(LLM 계층 판정) 정도입니다. 결정적이고 Rust를 지원하는 구현은 없었습니다.
5. 언어 간 문자열 계약: codebase-memory-mcp(HTTP route와 call-site, EnvVar), CodeGraph(Tauri invoke), Auryn이 일부 다룹니다. 모두 휴리스틱이고 에이전트 질의용입니다. 사람이 "계약 목록과 양쪽 사용처, 변경 시 깨짐"을 보는 뷰는 없었습니다.
6. 정밀도: Rust 해석은 대부분 tree-sitter 이름 매칭입니다. rust-analyzer나 SCIP(rust-analyzer scip, scip-typescript, scip-go) 기반으로 정밀 그래프를 만들고 diff까지 하는 GUI 지향 엔진은 없었습니다. CGC의 SCIP은 C 계열과 C#에 한정됩니다.
7. 평가 기준: 도구 벤치마크는 전부 토큰과 tool call 절감을 측정합니다. 사람의 이해도(Part B의 퀴즈형 측정)를 목표로 설계하거나 평가한 도구는 없었습니다.
8. 사람용 뷰의 형태: 대부분 force-directed 전역 그래프입니다. 변경 중심의 국소 뷰나 계층 고정 레이아웃(C4나 크레이트 의존 DAG)을 결정적으로 생성하는 오픈소스는 드뭅니다. CodeViz와 Codemaps는 LLM 생성이라 재현성이 없습니다.
