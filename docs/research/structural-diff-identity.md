# Structural diff, entity identity and incremental indexing — research record (2026-11-23)

Matching entities across versions (rename and move detection), symbol-naming schemes and
their stability, API-surface diff tools, architecture-level change and drift, and how
existing systems avoid re-indexing everything.

Collected by a web-research agent on 2026-11-23. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The text is kept as the researcher wrote it, in Korean or English.

## Report

### A. Entity identity / matching

[Godfrey & Zou 2005, origin analysis] 열람: abstract 페이지. Beagle 도구, procedural code의 file/function merge/split을 call relationship 변화를 근거로 탐지. 평가는 PostgreSQL 사례 연구; 정량 정확도 없음. 한계: 정량 근거 없음.
https://swag.uwaterloo.ca/publications/using-origin-analysis-to-detect-merging-and-splitting-of-source-code-entities.html
Unverified: 이름/시그니처/메트릭 중 무엇으로 매칭하는지는 abstract에 없음.

[RefDiff 2.0, Silva et al. TSE 2020] 열람: 논문 PDF 본문 (p.4-9, 12-14). 가장 이식 가능한 설계.
- 입력: 변경된 파일만 파싱해 Code Structure Tree(CST) 구성. 노드 = identifier(이름+파라미터 시그니처, overload 구분), namespace, node type, 토큰 목록, body 토큰, 그리고 단순 call graph / type hierarchy 간선(uses, subtype).
- 알고리즘(Fig.3): (1) ById: 같은 id+같은 parent면 매칭, 재귀. (2) BySim: 미매칭 쌍을 유사도 내림차순으로 탐욕 매칭. 유사도 = 토큰 multiset의 TF-IDF 가중 Jaccard, 단일 threshold 0.5. (3) ByChildren: 미매칭 부모는 자식이 2개 이상 매칭되고 nameSim>0.5면 매칭(클래스 move/rename). (4) 매칭 후에 관계 유형 분류(Same, Change Signature, Move, Rename, Move+Rename, Pull Up/Push Down), conflict는 유사도 순 greedy. (5) Extract/Inline은 uses 간선 + "제거된 코드 multiset이 새 메서드 body에 포함되는가" 비대칭 유사도.
- 정확도: Java 3,248 refactoring 오라클에서 precision 96.4%, recall 80.4% (RMiner 1.0: 98.8% / 81.3%). 커밋당 시간 median 157 ms (RMiner 109 ms), i5-750, 변경 파일만 분석.
- 한계: 오라클이 3개 도구 출력 기반이라 recall은 근사치(논문이 인정). 파서 의존(Java/C/JS 플러그인).
https://homepages.dcc.ufmg.br/~mtov/pub/2020-tse-refdiff.pdf
Unverified: JS/C precision 88-91% (검색 스니펫만; p.14에서 평가 설계만 확인).

[RefactoringMiner, Tsantalis et al.] 열람: README, accuracy.md, release 3.2.0 노트, arXiv abstract.
- 언어(2026-11 기준): Java, Python, Kotlin, TypeScript, JavaScript, C/C++. 3.2.0에서 네이티브 파서(Kotlin PSI, TS/JS는 swc4j, Python ANTLR, C++ Eclipse CDT)로 tree-sitter 대체. Rust 미지원.
- 정확도 (accuracy.md, 2026-11 갱신): Java B1 P0.999/R0.984 (12725 TP), Java B2 P0.993/R0.986, Python P0.997/R0.998, Kotlin P0.998/R1.000, JavaScript(Chart.js) P0.988/R1.000 (171 TP). TypeScript 전용 벤치마크는 없음. 최저 항목: Extract And Move Method recall 0.701, Move And Inline Method recall 0.750(B1).
- 속도(3.2.0 노트): AST diff 파싱 5.3x 빨라짐(155 s→29.5 s, 1,091 케이스), 일부 폭주 케이스 최적화.
- 기술: statement 매핑 기반 AST diff + refactoring-aware (arXiv 2403.05939: 800 bug-fix + 188 refactoring 커밋 벤치마크). 심볼 그래프가 아니라 AST/statement 위에서 동작, 비용이 큼.
https://github.com/tsantalis/RefactoringMiner
https://github.com/tsantalis/RefactoringMiner/blob/master/documentation/accuracy.md
https://github.com/tsantalis/RefactoringMiner/releases/tag/3.2.0
https://arxiv.org/abs/2403.05939
Unverified: multi-mapping 99.7%/98.4% (검색 스니펫).

[GumTree, Falleri et al. ASE 2014 / TSE 2023 / ICSE 2024] 열람: README만. C, Java, JavaScript, Python, R, Ruby 지원, 논문 3편 인용; README에는 알고리즘·정확도 수치 없음.
https://github.com/GumTreeDiff/gumtree
HAL 원문(hal.science)은 Anubis 접근 차단 페이지가 반환되어 우회하지 않음 (도달 불가).
Unverified: top-down(minHeight)/bottom-up(minDice) 2단계 구조; GumTree 3.0 simple이 multi-mapping에서 precision 59%/recall 10%, F-score 94.2%(statement) — TOSEM 논문을 인용한 검색 스니펫.

[ChangeDistiller, Fluri 2007] 열람 안 함. Unverified: 잎 노드 bigram 문자열 유사도 f=0.6, 내부 노드 동적 threshold(검색 스니펫).
참고 URL(검색 결과): https://pinzger.github.io/papers/Fluri2007-changedistiller.pdf

[difftastic] 열람: README. tree-sitter 파서 + Dijkstra 그래프 최단경로로 구조 diff, 30+ 언어. README에 move/rename 탐지 언급 없음, 변경이 많은 파일에서 scale이 나쁘고 메모리 많이 씀. 파일 단위 diff이며 항목 정체성 개념이 없음.
https://github.com/Wilfred/difftastic

[sem (Ataraxy-Labs), "sem 류" 도구] 열람: README. tree-sitter로 function/class/method를 entity로 추출. 3단계 매칭: (1) exact ID(entityId 형식 `src/auth.ts::function::validateToken`), (2) structural hash(AST 구조 동일, 공백/주석 무시) = 이름만 바뀐 rename/move, (3) 토큰 80% 이상 겹치면 fuzzy rename. `sem impact`(의존/피의존/테스트). 정확도 수치 없음, 80% 고정 threshold.
https://github.com/Ataraxy-Labs/sem
Unverified: Rust 크레이트(sem-core)로 제공된다는 점은 검색 스니펫.

[git rename detection] 열람: git-diff docs. 파일 단위; 기본 similarity 50% (-M\<n>); 빠른 exact/basename 단계 후 남은 것에 대해 O(N^2) 전수 비교, -l 한도로 생략; -C로 copy. 항목(함수) 단위 아님.
https://git-scm.com/docs/diff-options

[Identity scheme: SCIP symbol] 열람: scip.proto. `<scheme> <manager> <package-name> <version> <descriptor>+`; version은 문법상 필수(manager는 "."로 빈 값 표기). "descriptors는 package 내에서 unique한 fully qualified name". 버전 간 안정성에 대한 언급은 proto에 없음. Local symbol은 `local N`, document 범위만.
https://github.com/scip-code/scip/blob/main/scip.proto
rust-analyzer 구현 열람(scip.rs): scheme "rust-analyzer", manager "cargo", version은 moniker에 없으면 ".". inherent impl은 `impl#[SelfType]`, trait impl 메서드는 `impl#[MyStruct][MyTrait]func().`. 같은 Self의 inherent impl이 여러 개면 SymbolInformation은 첫 것만 유지하고 FIXME #18772. local 카운터는 document마다 리셋.
https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/rust-analyzer/src/cli/scip.rs
scip-go: README에 `--module-name/--module-version` 오버라이드가 있음(심볼에 모듈 버전이 들어감을 시사), 심볼 구성 상세는 README에 없음. https://github.com/scip-code/scip-go
scip-typescript: README에 심볼 포맷 설명 없음. https://github.com/sourcegraph/scip-typescript

[Kythe VName] 열람: schema 문서. 5필드(language, corpus, root, path, signature). signature는 "같은 입력이면 일관되게 생성"되어야 하나 "입력의 버전 간에는 안정적일 필요 없음"; 해시 fingerprint 허용. corpus는 revision을 식별할 수 있음(vcs 노드는 full commit hash 권장). file 노드만 더 강한 안정성. => 버전 간 안정 identity를 목표로 설계되지 않음.
https://kythe.io/docs/schema/

[rustdoc JSON Id] 열람: docs.rs. "opaque", 내부 값 보장 없음, 단일 JSON blob 안에서만 유효, 다른 crate JSON 간 해석 불가. => 버전 간 키로 부적합. (cargo-semver-checks는 id가 아니라 public import path로 매칭.)
https://docs.rs/rustdoc-types/latest/rustdoc_types/struct.Id.html

[Go objectpath] 열람: pkg.go.dev + 소스. 패키지 내부 객체를 package scope에서 시작하는 연산 열로 명명. 목적은 서로 다른 type-check 인스턴스 간(소스 vs export data) 대응. 소스 주석: 색인이 "different representations of the same package, even source and export data" 간 안정적이라 함; 메서드 인덱스는 "ultimately derives from source order". 버전 간(편집 후) 안정성은 문서에 명시 없음. 이름을 붙일 수 있는 대상은 package-level 타입, exported non-type, 메서드, 파라미터/결과, struct field로 제한(unexported non-type, 로컬은 불가).
https://pkg.go.dev/golang.org/x/tools/go/types/objectpath
https://raw.githubusercontent.com/golang/tools/master/go/types/objectpath/objectpath.go
Unverified: 소스 순서 기반 인덱스라서 메서드 재배치 시 경로가 바뀔 것이라는 추론.

[TS API Extractor] 열람: declaration reference 문서(`Widget.render`, `pkg#Widget`, 병합 선언은 `(ShirtSize:enum)` selector, 항상 entry point 기준 해석, 문법은 "still evolving"). canonical reference 언급은 없음. API report는 "리뷰를 쉽게 하는 간결한 리포트"라고만 확인, 파일명(.api.md)은 페이지에서 미확인.
https://api-extractor.com/pages/tsdoc/declaration_references/
https://api-extractor.com/

종합: 버전 간 안정을 의도한 것은 없음. 가장 가까운 것은 "이름 경로 + 버전 제거"(cargo-semver-checks의 public path, apidiff의 이름 대응, API Extractor의 entry point 기준 참조)이며 SCIP도 마찬가지로 version 필드만 제거하면 이 범주.

### B. API-surface / breaking-change 도구

[cargo-semver-checks] 열람: repo README + predr.ag 포스트. rustdoc JSON을 Trustfall 쿼리 엔진으로 질의; lint가 쿼리. 슬라이드 기준 58개 lint(FOSDEM 2024; README는 "dozens"). 매칭: old의 public 함수에 대해 new에서 같은 public API import path의 public 함수를 찾고, 없으면 breaking. 일부 lint는 witness(다운스트림 예제 컴파일)로 검증. 기준선은 crates.io 직전 버전/git rev/소스 디렉터리/rustdoc JSON. 결과 표시: lint 이름(예 function_missing), 대상 항목, 파일·줄, major 여부, 참고 링크. 블라인드 스팟: 타입/제네릭/lifetime 변경 일부 lint 없음, feature 부분집합 위반, 매크로, cross-crate re-export("upstream 기능 대기"), 타깃별 API 차이, rustdoc JSON 포맷 변경에 취약(nightly 의존, 어댑터 다중 버전으로 완화). 성능: 어댑터 인덱스 fast-path로 이름 조회 최적화(2000x 포스트 제목).
https://github.com/obi1kenobi/cargo-semver-checks
https://predr.ag/blog/semver-in-rust-tooling-breakage-and-edge-cases/
Unverified: 2000x 수치 상세(검색 결과 제목만; 포스트 본문은 미열람 — https://predr.ag/blog/speeding-up-rust-semver-checking-by-over-2000x/).

[cargo-public-api] 열람: README. rustdoc JSON에서 public 항목을 한 줄씩 나열, `diff <version>` / `diff ref1..ref2`로 added/removed/changed 표시, CI에서는 텍스트 스냅샷 파일을 두고 `cargo test` 실패 + git diff로 리뷰. blanket/auto trait/auto-derived impl이 기본으로 나와 노이즈, nightly 필요, 릴리스마다 지원 nightly 범위가 고정됨. 매칭/rename 탐지 방식은 README에 없음.
https://github.com/cargo-public-api/cargo-public-api

[Go apidiff] 열람: pkg.go.dev. 두 패키지/모듈 비교, compatible(minor) vs incompatible(major) 분류, 컴파일 가능성 기준. 타입 대응 규칙: 같은 이름이거나, "나머지 API를 바꾸지 않고 rename 가능하면" 대응; 여러 old가 한 new에 대응 가능하나 반대는 불가; 구조적으로 확장(slice는 element 대응). 블라인드: 동작 변화 불가, unkeyed struct literal·embedding/shadowing·unsafe.Sizeof·type switch 병합 등 5가지 무시, 모듈이 nested module로 옮겨지면 removal로 보고.
https://pkg.go.dev/golang.org/x/exp/apidiff

[japicmp] 열람: README. class 파일 두 jar 비교(javassist), 클래스/메서드/필드/어노테이션 변경, source/binary compatible 구분, private 기본 제외, semver 권고, text/Markdown/XML/HTML 출력, 약 1,700 클래스/아카이브 1초 미만(저자 주장). 비-class 아티팩트는 의도적 범위 밖.
https://github.com/siom79/japicmp

[revapi] 열람: README. API = 클래스뿐 아니라 설정/스키마도 가능, API "surface"를 타입 사용 추적으로 계산해 의존성 leakage 보고, 변경을 source/binary/semantic 호환성으로 분류, 변경 재분류 가능, reporter 플러그인(stdout/Maven site/FreeMarker/JSON). 트리로 분해 가능한 것은 확장 가능하다고 하나 현재 구현은 Java뿐.
https://github.com/revapi/revapi

[TS api-extractor] 위 A 절 참고. 리포트가 PR 리뷰 대상이라는 점만 확인.

### C. Architecture-level change / drift

[Reflexion models, Murphy/Notkin/Sullivan FSE 1995] 열람 안 함(검색 결과 텍스트만). Unverified: 사람이 만든 high-level model + 소스 매핑 → convergence/divergence/absence 계산, NetBSD 250 KLOC 사례, 도구 RMTool, 후속 TSE 2001.
https://www.cs.ubc.ca/~murphy/papers/rm/fse95.html (검색 결과에서 확보, 미열람)

[Architecture erosion surveys] 열람 안 함. Unverified: Li et al. mapping study(arXiv 2112.10934): 증상 4범주(structural/violation/quality/evolution); code review 코멘트에서 violation symptom 606개(arXiv 2212.12168); 자동 식별 GPT-4o F1 0.851, 증상을 보여주면 개발자 탐지율 25.9%→64.7%(arXiv 2306.08616); SLR(arXiv 2504.04334)은 "erosion 신호를 종단적으로 집계하는 도구 부재"를 도전과제로 지적.

[AI 에이전트 변경 + 아키텍처]
- 열람: arXiv 2604.04990 "Architecture Without Architects"(Konrad et al., 2026-04): 에이전트가 프레임워크/인프라 선택 같은 암묵적 아키텍처 결정을 하고 사람은 이를 아키텍처로 리뷰하지 않음, review 관행/decision record/tooling 제안. 정량 수치 없음, 특정 dependency-graph diff 방법 제시 없음(abstract 기준).
https://arxiv.org/abs/2604.04990
- Unverified (스니펫): ContextCov(arXiv 2603.00822)는 지침 파일 제약을 layer 위반/순환 의존/모듈 배치 검사로 컴파일; arXiv 2608.21747은 정적 제약 검사(~80% 아키텍처 커버리지)와 LLM judge가 상호보완이라 보고; arXiv 2512.04273은 hexagonal 아키텍처에서 Domain 계층 불법 import를 AVR로 측정.
- 리뷰어가 어떤 구조 diff를 유용하게 여기는지에 대한 사용자 연구는 찾지 못함(검색: 마이닝 연구와 실무 가이드뿐). Unverified: 벤더 수치(Faros AI 2026: PR 리뷰 시간 중앙값 5배, PR 크기 +51%; CodeRabbit: 첫 리뷰까지 4.6배)는 비 peer-reviewed 스니펫. 실무 가이드(Aviator)는 PR 설명 후 dependency diff·import를 먼저 보라고 권고(스니펫).

[DSM 기반 변경 보기] Unverified(스니펫): Cai & Huynh WICSA 2008 (https://www.cs.drexel.edu/~yc349/papers/wicsa2008_dsm.pdf)가 릴리스별 클래스 DSM을 만들고 "모듈 내 추가/삭제 클래스 비율"을 architectural change 메트릭으로 사용(Tomcat 사례). 전용 "DSM diff" 도구는 검색에서 못 찾음; 통상 릴리스별 DSM을 만들어 추가/삭제 요소·의존과 propagation cost 비교.
[Evolution Radar] Unverified(스니펫): D'Ambros/Lanza TSE 2009, 모듈 기준 file-level co-change를 방사형 배치, persistent vs recent coupling 구분. 구조(심볼) 의존이 아니라 logical coupling.
[CodeCharta Delta View] Unverified: 두 맵(커밋별 CCSH 생성)을 3D city map에서 비교, 추가/삭제 건물 색 표시; 문서와 README의 색 설명이 서로 다르다고 검색 스니펫이 지적. 공식 페이지 https://codecharta.com/docs/visualization/delta-view 는 열람 안 함(시도한 /docs/overview/delta-mode는 404).
[CodeScene] 열람 시도: 문서 페이지가 "maintenance" 안내만 반환해 내용 확인 불가(우회하지 않음).

### D. Incremental indexing / snapshot storage

[Glean] 열람: 블로그 + Meta 엔지니어링 포스트.
- 불변 DB를 스택: 위 레이어가 아래 레이어의 사실을 추가하거나 hide. 클라이언트는 하나의 DB로 봄. base가 불변이라 여러 revision을 전체 복사 없이 볼 수 있음.
- unit(문자열, 보통 파일명/모듈명)에 사실의 ownership을 부여하고 dangling 방지를 위해 ownership을 그래프로 전파. 재색인 = `--incremental <old> --exclude A,B` (unit hide + 새 사실 추가). derived fact는 모든 근거 사실이 visible일 때만 visible.
- 수치: ownership 데이터로 DB 크기 약 +7%, 색인 오버헤드 Python 2-3%, 쿼리 오버헤드 일반 <10%(무거운 쿼리 약 3x). 전체 색인이 "수 시간"이라 도입. end-to-end 속도 향상 수치는 없음. 목표는 O(changes)이나 포스트는 C++ 같은 언어에선 fanout 때문에 실제로는 O(fanout)이라고 명시.
https://glean.software/blog/incremental/
https://engineering.fb.com/2024/12/19/developer-tools/glean-open-source-code-indexing/
- fact가 content-addressed인지는 두 페이지 모두 명시 없음. Unverified: 공식 docs(incrementality) 스니펫에 따르면 fact는 key 기준으로 중복 제거되어 한 번만 저장되고 ID는 DB 로컬 64비트 정수(content hash 아님). https://glean.software/docs/implementation/incrementality/ (미열람)
- Glass가 부여하는 symbol ID(`REPOSITORY/cpp/folly/Singleton`)는 정의가 이동해도 안정적이라 서술.

[SCIP 설계 의도] 열람: SCIP 발표 포스트. LSIF의 전역 증가 ID 때문에 일부 문서만 갱신하기 어려웠고 SCIP은 push마다 변경 파일만 재색인하는 incremental indexing을 가능하게 하려는 것이나, "future capability"로 서술. LSIF 대비 gzip 4x/비압축 5x 작음.
https://sourcegraph.com/blog/announcing-scip
SCIP 프로토콜 문서 열람(docs/scip.md): Index는 스트림 가능(metadata가 맨 앞 1회), Document는 relative_path로 식별, local symbol은 document 한정, external_symbols는 다른 인덱스의 심볼 문서화용. incremental/merge/CLI 내용은 이 페이지에 없음.
https://github.com/scip-code/scip/blob/main/docs/scip.md
Sourcegraph writing-an-indexer 문서 URL은 404.

[per-document 캐시/병합 실무 (서드파티)]
- scip-swift 열람: Document별 캐시, 키 = SHA-256(상대 경로 + 파일 내용 해시). 파일명이 바뀌면 miss. toolchain/indexstore/backend/symbolFormatVersion이 바뀌면 캐시 전체 폐기. overload 인덱스는 repo 전체에 의존하므로 overload fingerprint가 바뀌면 "모든 캐시 문서 무효화"(보수적, per-group 개선은 v2 TODO). cross-file 참조 일반 메커니즘은 README에 없음 => per-document 재사용은 cross-file 의존 처리가 핵심 위험.
https://github.com/alex2481kobe/scip-swift
- scip-io 열람: 언어별 SCIP 인덱스 병합. 같은 Document.relative_path는 하나로 합치고 occurrence/document symbol 중복 제거, 경로를 repo root 기준 정규화, 결정적(byte-identical) 출력. 이는 "언어 간 병합"이며 incremental 증분 갱신이 아님.
https://github.com/GlitterKill/scip-io
=> per-document SCIP 병합이 Sourcegraph 공식 문서화된 관행이라는 근거는 못 찾음(공식 문서 열람 실패).
- Unverified: CKB 위키는 scip-go를 전체 재실행하고 변경 문서만 후처리(protobuf는 부분 갱신 불가, import/타입 해석에 전체 컨텍스트 필요) — 스니펫. scip-clang 로드맵에 빌드 시스템 증분성 활용 언급 — 스니펫.

[salsa] 열람: overview. tracked function이 읽은 입력을 기록, memoize, 입력 변경 시 읽은 입력이 바뀐 경우에만 재실행(red-green). durability와 early cutoff는 해당 페이지에 설명 없음.
https://salsa-rs.github.io/salsa/overview.html
Unverified: Kythe/Bazel 빌드 주도 무효화, Sourcegraph의 커밋별 업로드 처리는 이번에 열람 가능한 1차 자료를 못 찾음.

### Implications (12줄 이내)
1. Key = (language, package/crate 이름, 버전 제거한 SCIP descriptor 경로, kind, 시그니처 해시는 key 아닌 속성). SCIP의 version 필드는 버전 간 키에서 제외(".")하고 descriptor 경로만 사용. 이는 cargo-semver-checks/apidiff의 "이름 경로 + 이름 기반 대응"과 같은 범주.
2. 같은 경로 충돌(rust-analyzer가 동일 Self의 inherent impl을 하나로 합침, FIXME #18772)은 별도 disambiguator(선언 순서 ordinal이 아니라 generics/where 해시)로 분리하고, 충돌 자체를 diff 리포트의 경고로 노출.
3. Rename/move는 key에 맡기지 말고 후처리: 1단계 exact key, 2단계 같은 parent+같은 이름에서 시그니처 변경, 3단계 미매칭 removed×added를 kind별로 묶어 RefDiff식 유사도(body 토큰 multiset TF-IDF Jaccard ≥0.5, 유사도 내림차순 greedy) 적용, 4단계 자식 매칭 비율로 부모(타입/모듈) move 확정(RefDiff ByChildren).
4. 심볼 그래프의 이점: body 토큰 외에 "참조하는 심볼 집합(callee set)"과 "참조되는 곳" Jaccard를 함께 사용하면 Godfrey/Zou의 call 관계 증거를 AST 없이 확보. git -M의 파일 rename을 후보 제한 힌트로 사용(경로 접두 변경 + 같은 상대 이름).
5. GumTree/ChangeDistiller/RefactoringMiner/difftastic은 AST/문장 수준이라 언어별 파서 비용이 크고 Rust 미지원(RM). 항목 수준 diff에는 RefDiff식 CST-lite로 충분하다고 판단(근거: RefDiff P96.4/R80.4, 커밋당 ~157 ms). Unverified: 이 정확도가 SCIP 기반 심볼 그래프에서도 유지될지는 측정 필요.
6. API diff 도구에서 차용: (a) public surface를 항목별 정규화 시그니처 텍스트로 뽑아 git에서 diff되는 스냅샷으로 저장(cargo-public-api), (b) 변경을 compatible/breaking으로 분류하고 lint 이름+위치+이유 링크 표시(cargo-semver-checks), (c) 타입 대응은 "나머지 API를 바꾸지 않는 rename이면 대응, 다대일 허용"(apidiff), (d) blanket/auto/derive impl 노이즈 기본 숨김, (e) 의존성 leakage 표면(revapi). Unverified: SCIP이 visibility를 담는지는 확인 못함; 없으면 언어별 소스 보조 필요.
7. 블라인드 스팟은 리포트에 명시: 매크로 생성 항목, feature/cfg 조합, re-export 경로, 동작 변화(apidiff·semver-checks 모두 한계로 문서화).
8. Incremental 최저비용 전략: SCIP 자체는 증분 공식 기능이 없으므로(발표 포스트가 미래 기능으로 서술) 색인 단위를 package/crate로 잡고, 키 = hash(해당 패키지 소스 + 의존 패키지 해시 + toolchain/indexer 버전)이 같으면 이전 SCIP 결과 재사용. scip-swift는 파일 단위가 cross-file 의존(overload 등)에서 전체 무효화로 후퇴한다는 사례. 그 이하 단위(문서)는 fanout 처리가 필요(Glean도 실제 O(fanout)).
9. 비교 용도에는 대부분 한쪽 스냅샷이 이미 캐시되어 있으므로 B만 색인; 두 리비전 비교는 문서 content hash가 바뀐 문서 + 그 문서를 참조하는 문서(fanout)만 diff 대상으로 한정 가능. Unverified: 이 한정이 SCIP 참조 방향 정보만으로 충분한지는 미검증.
10. 스냅샷 저장: 문서(SCIP Document) 바이트를 content hash로 저장하고 스냅샷 = path→hash 매니페스트(git tree식 structural sharing). Glean의 불변 base + 스택 구조와 같은 효과(리비전 간 전체 복사 없음)를 단순하게 얻음. Unverified: Glean 자체가 content-addressed라는 근거는 없음.
11. 리뷰 UX 근거는 약함: AI 변경 리뷰용 구조/의존 diff 뷰의 사용자 연구는 못 찾음. 아키텍처 위반(layer/순환 의존)을 dependency graph로 검사하는 연구(ContextCov 등)는 스니펫 수준에서 존재 → "새 모듈 간 의존 + 순환/계층 위반" 목록이 가장 근거 있는 첫 출력.
12. 도달 불가: hal.science(Anubis 차단), CodeScene 문서(maintenance), Sourcegraph writing-an-indexer(404). 우회하지 않음.

## Licence notes, first pass

Licence notes (추가 섹션). 아래 모두 GitHub 사이드바/LICENSE/README 또는 GitHub API license 엔드포인트에서 직접 확인함. 이전 보고서의 추천 항목만 다룸.

사용/포팅 후보
- RefDiff (알고리즘 이식 대상, https://github.com/aserg-ufmg/RefDiff): MIT. 코드를 포팅·복사하면 저작권 고지와 MIT 전문 유지(attribution)만 필요. 알고리즘 자체는 논문(TSE 2020)에 공개되어 있어 clean-room 재구현도 가능. 특허 조항 없음.
- SCIP 스키마/CLI (https://github.com/scip-code/scip): Apache-2.0. scip.proto를 복사·생성 코드를 vendoring 하면 LICENSE 포함, 수정 표시, NOTICE 파일이 있으면 유지 필요; 명시적 특허 허여 있음. MIT/Apache-2.0 이중 라이선스 프로젝트와 호환.
- scip-go (https://github.com/scip-code/scip-go), scip-typescript (https://github.com/sourcegraph/scip-typescript): 둘 다 Apache-2.0. 엔진이 외부 프로세스로 호출만 하면 배포 의무는 사실상 없음(번들 시 위 Apache 의무).
- rust-analyzer (SCIP 생성기로 호출, https://github.com/rust-lang/rust-analyzer): MIT OR Apache-2.0 (README 명시). 호출만 하면 의무 없음; scip.rs 등 코드를 가져오면 해당 고지 유지.
- salsa (https://github.com/salsa-rs/salsa): MIT OR Apache-2.0. 의존성으로 사용 시 고지만 필요. 참고로 이번 조사에서 salsa가 맞는 도구라는 근거는 개념 수준(red-green)뿐임.
- cargo-semver-checks (https://github.com/obi1kenobi/cargo-semver-checks): Apache-2.0 OR MIT ("at your option"). 코드/lint 쿼리 재사용 시 해당 고지 유지.
- cargo-public-api (https://github.com/cargo-public-api/cargo-public-api): MIT (GitHub API 확인). 고지 유지.
- Go apidiff (golang.org/x/exp/apidiff): BSD-3-Clause, Copyright The Go Authors. 소스 재배포 시 저작권 고지·조건·면책 문구 유지, 바이너리 배포 시 문서/자료에도 포함, 이름으로 홍보 금지 조항. 단 x/exp는 실험 모듈이라 안정성 보장 없음(일반 지식 기반, 출처 미확인).

참고/기술만 재사용 (코드 채택 비권장 또는 라이선스 주의)
- GumTree (https://github.com/GumTreeDiff/gumtree): LGPL-3.0 (LICENSE 원문 열람). 라이브러리로 링크·수정 시 LGPL 의무(수정본 소스 공개, 사용자가 재링크 가능해야 함)가 따르고 Rust 정적 링크 바이너리와는 마찰이 큼. 허용 라이선스 레포에 포팅하면 파생물이 되어 LGPL 영향 가능. 논문 기법만 재구현하는 것이 안전. (파생물 판단은 법률 자문 대상.)
- RefactoringMiner (https://github.com/tsantalis/RefactoringMiner): MIT. 코드는 재사용 가능하나 Java 기반 + 언어별 파서 의존이라 Rust 엔진에 포팅 비용이 큼; 외부 도구로 호출하는 것은 MIT 고지만 필요.
- sem (https://github.com/Ataraxy-Labs/sem): Apache-2.0 (GitHub API 확인; LICENSE 원문 raw URL은 404라서 본문 직접 확인은 못함). 3단계 매칭 아이디어는 참고만.
- difftastic: 추천 안 함, 라이선스 미조사.
- Glean (https://github.com/facebookincubator/Glean): BSD (README에 "BSD LICENSE"라고만 명시; 2-clause/3-clause 구분은 확인 못함). 설계 아이디어만 참고 중이라 의존·코드 채택 계획 없음.

기술/논문만 재사용
- Murphy 등 reflexion model, Godfrey/Zou origin analysis, Cai/Huynh DSM 메트릭, ChangeDistiller 임계값: 논문 속 기법이며 코드 채택 없음. 아이디어는 저작권 대상이 아니나 구현은 직접 작성; 특허 여부는 확인하지 않음(Unverified).
- 코드 의존이 아니라 포맷 소비인 항목: rustdoc JSON(Rust 프로젝트의 비공식 JSON 형식, nightly 변동), SCIP 프로토콜.

도달 못한 확인: RefDiff·cargo-public-api 외 일부는 사이드바 표시만 확인했고 LICENSE 본문 전체는 열람하지 않음. 이중 라이선스 프로젝트의 NOTICE 파일 유무는 확인하지 않음. GPL/AGPL/EPL/source-available 해당 항목은 이번 추천 대상 중에는 없음; LGPL은 GumTree 하나.

## Licence notes, second pass

Licence notes. Quelle: "API" = GitHub API /license endpoint (reports the repo LICENSE file's SPDX id); "page" = repo page sidebar/README; "raw" = LICENSE file text. Keine Datei selbst vollständig gelesen außer GumTree.
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
