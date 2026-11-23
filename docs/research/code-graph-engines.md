# Open-source code-graph engines for Rust, Go and TypeScript — research record (2026-11-18)

Survey of engines that extract symbols, references, call graphs and dependency graphs and
could sit under a separate viewer: maintenance state, licence, precision, output format
and limits, ending with a ranking for covering the three languages together.

Collected by a web-research agent on 2026-11-18. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The text is kept as the researcher wrote it, in Korean or English.

## Report

코드 그래프 엔진 조사 결과입니다. 결론: Rust+Go+TS를 함께 다룰 기반으로는 SCIP(per-language indexer) + LSP 보강이 유일하게 맞습니다. Kythe은 Rust를 지원하지 않고, CodeQL은 라이선스 문제, stack-graphs는 archived입니다.
조회일 2026-11-18. 유지보수 날짜는 별도 표기가 없으면 https://api.github.com/repos/\<owner>/\<repo> 의 pushed_at, /releases/latest 를 gh api(인증)로 확인한 값입니다.

### SCIP 계열
- SCIP 포맷 + scip CLI: https://github.com/scip-code/scip (sourcegraph/scip에서 scip-code org로 이전, 기존 URL은 redirect). Apache-2.0. v0.10.0 (2026-09-03), push 2026-11-11, 활발함.
  - 표현할 수 있는 것: Occurrence(range, symbol, symbol_roles = Definition/Import/WriteAccess/ReadAccess/Generated/Test/ForwardDefinition), enclosing_range, SymbolInformation.relationships(is_reference/is_implementation/is_type_definition/is_definition), enclosing_symbol, signature_documentation.
  - call edge 타입은 없습니다. proto 주석이 call hierarchy를 "enclosing_range로 함수 본문 안의 참조를 구하라"고 안내하므로, 호출과 단순 참조의 구분은 소비자가 해야 합니다. SyntaxKind.IdentifierFunction은 하이라이트용이며 주석은 "Function references, including calls"입니다. dynamic dispatch, macro expansion, generics instantiation 필드도 없습니다. 출처: https://github.com/scip-code/scip/blob/main/scip.proto
  - v0.9.0(2026-06-29)에서 range가 typed oneof(single_line_range/multi_line_range)로 바뀌고 기존 repeated int32 range/enclosing_range는 deprecated가 됐습니다. scip-typescript와 scip-go는 typed 형식, rust-analyzer는 deprecated 형식을 씁니다(각 소스 확인). 따라서 리더는 두 형식을 모두 처리해야 합니다.
  - CLI 서브커맨드: lint, print --json, snapshot, stats, test, expt-convert(EXPERIMENTAL, SQLite 변환). 출처: https://github.com/scip-code/scip/blob/main/docs/CLI.md
  - 증분 지원 없음. Unverified: 인덱스는 항상 전체 재생성이며 문서/패키지 단위 병합은 소비자 책임으로 보입니다.
- rust-analyzer scip: https://github.com/rust-lang/rust-analyzer, Apache-2.0/MIT. release 2026-11-16, scip.rs 마지막 변경 2026-10-16.
  - 소스 확인 결과(crates/rust-analyzer/src/cli/scip.rs): relationships를 항상 Vec::new()로 내보내므로 trait impl과 "implements" 관계가 없습니다. symbol_roles는 Definition만 세팅하고 Import/Read/Write는 없습니다. enclosing_range는 정의 body에만 있습니다.
  - proc-macro 서버를 사용합니다(ProcMacroServerChoice::Sysroot). Unverified: 매크로 확장 결과 안의 참조가 원본 토큰 위치로 매핑되는 범위.
  - 배치 전용입니다. LSP 모드는 salsa 기반으로 증분 갱신됩니다.
  - LSP capabilities: callHierarchy, implementation, references는 지원하고 type_hierarchy_provider는 None입니다(crates/rust-analyzer/src/lsp/capabilities.rs).
  - 같은 CLI에 lsif 명령도 남아 있습니다.
- scip-go: https://github.com/scip-code/scip-go, Apache-2.0. v0.2.9 (2026-10-22), push 2026-11-10, 활발함.
  - go/packages 기반이고 internal/implementations에서 interface 구현 관계를 내보냅니다. Bazel/Buck은 GOPACKAGESDRIVER로 처리하지만 이 경우 cross-repo navigation은 안 됩니다(README).
  - 증분 없음.
- scip-typescript: https://github.com/sourcegraph/scip-typescript, Apache-2.0. v0.6.2 (2026-11-10), 활발함(v0.6.0에서 Svelte 추가).
  - 의존성은 typescript ^6.0.3입니다. TS7(Go native)의 API는 "not ready"이고(https://github.com/microsoft/typescript-go README, 2026-08 archived), 따라서 scip-typescript는 TS6 컴파일러 API에 묶여 있습니다.
  - isImplementation relationship(클래스 implements)을 내보내고 typedEnclosingRange를 사용합니다.
  - 옵션: --infer-tsconfig, --yarn-workspaces, --pnpm-workspaces. 증분 없음. 대형 레포에서는 OOM 주의(README).
- LSIF: https://github.com/sourcegraph/lsif-go 는 archived(2023-09). https://github.com/microsoft/lsif-node 는 MIT이며 push 2026-10-22지만 최근 커밋이 action pin, deps bump뿐인 유지보수 모드입니다. Unverified: Sourcegraph는 LSIF를 SCIP로 대체했습니다. 신규 소비자 포맷으로는 비추천입니다.

### 범용 엔진
- stack-graphs: https://github.com/github/stack-graphs, Apache-2.0. archived(마지막 push 2025-09-09, 마지막 release 2024-12-13). README: "no longer supported or updated by GitHub". 탈락.
- tree-sitter-graph: https://github.com/tree-sitter/tree-sitter-graph, Apache-2.0. 마지막 커밋과 v0.12.0이 2024-12-11이고 archived는 아니지만 정체 상태입니다. 순수 syntactic DSL이라 정확도 요건에 맞지 않습니다.
- Kythe: https://github.com/kythe/kythe, Apache-2.0. v0.0.76 (2026-07-16), push 2026-09-18.
  - 언어 디렉터리는 cxx, go, java, typescript입니다. Rust indexer는 2023-08에 제거됐습니다(커밋 "fix(rust_common): finish removing rust (#5803)"). TS indexer의 마지막 변경은 2025-03입니다.
  - Unverified: Bazel 중심이라 무겁고, 출력은 protobuf entry stream과 serving table입니다.
  - Rust가 없어서 탈락.
- Meta Glean: https://github.com/facebookincubator/Glean, BSD. tag release는 없고 push 2026-11-17, 활발함.
  - Rust, Go, TS 모두 SCIP indexer(rust-analyzer scip, scip-go, scip-typescript)와 scip-to-glean 변환으로 인덱싱합니다(glean/website/docs/indexer/lsif-rust.md, lsif-go.md, lsif-typescript.md). 그래서 정밀도는 SCIP와 동일하고 추가 의미 정보는 없습니다.
  - 장점은 저장과 질의(Angle)이고, unit 기반 incrementality가 구현돼 있습니다(docs/implementation/incrementality.md).
  - Unverified: Haskell 빌드와 RocksDB 기반이라 데스크톱 앱 번들로는 무겁습니다.
- Joern(CPG): https://github.com/joernio/joern, Apache-2.0. v4.0.685 (2026-11-17), 매우 활발함.
  - frontends에 rust2cpg, gosrc2cpg, jssrc2cpg가 있습니다. rust2cpg는 2026-11-17까지 커밋이 이어지고 있고, rust_ast_gen이 ra_ap_hir/ra_ap_ide 0.0.328, 즉 rust-analyzer 라이브러리를 씁니다(https://github.com/joernio/rust_ast_gen Cargo.toml).
  - call graph를 포함한 하나의 그래프 모델에 세 언어를 모두 넣을 수 있는 유일한 엔진입니다.
  - Unverified: JS/TS 호출 해석은 type-recovery 휴리스틱이고, 출력은 joern-export(dot/graphml/neo4j csv 등)입니다. JVM 기반 배치 처리이고 증분이 없습니다.
- CodeQL: https://github.com/github/codeql (MIT, 쿼리), CLI는 https://github.com/github/codeql-cli-binaries v2.27.4 (2026-11-02).
  - Rust(edition 2021/2024), Go(≤1.27), TS(2.6–7.0)를 지원합니다(docs/codeql/reusables/supported-versions-compilers.rst).
  - 라이선스(LICENSE.md): "may not be used ... in connection with any codebase that is not an Open Source Codebase (e.g., code in a private repo)". GHAS 유료 라이선스일 때만 예외입니다. 사설 코드와 배포용 GUI에는 쓸 수 없어 탈락.
- Sourcetrail:
  - 원본 https://github.com/CoatiSoftware/Sourcetrail 는 GPL-3.0, 2021 archived.
  - https://github.com/petermost/Sourcetrail 는 2026.10 (2026-10-02), push 2026-11-16로 활동 중입니다. 다만 C/C++와 Java만 지원하고, 2026-08-01 공지로 개발 속도를 줄였습니다.
  - https://github.com/OpenSourceSourceTrail/Sourcetrail 는 3.0.0을 내고(MCP server 포함) 2026-09-05에 archived됐습니다.
  - Go indexer(TOMATOFQY/SourcetrailGolangIndexer, 2023-02)와 TS indexer(LouisStAmour, 2022-02)는 방치 상태이고 Rust indexer는 검색되지 않았습니다. GUI 참고용일 뿐 엔진으로는 탈락.

### Rust 전용
- cargo-modules: https://github.com/regexident/cargo-modules, MPL-2.0. v0.28.0, 커밋 2026-11-15, 활발함.
  - ra_ap_hir =0.0.358 기반으로 module tree와 owns/uses 의존 그래프를 컴파일러 수준 해석으로 만듭니다. call graph는 없습니다.
  - 출력은 텍스트 트리와 DOT뿐입니다. 크레이트 단위이고 증분은 없습니다.
- crabviz: https://github.com/chanhx/crabviz, AGPL-3.0. 커밋 2026-08-22, 마지막 release vscode_v0.5.0 (2025-08-24).
  - LSP call hierarchy를 시각화하는 VS Code 확장으로, 정밀도는 각 LSP에 따릅니다. AGPL이라 코드 재사용 시 주의해야 합니다.
- cargo-call-stack: https://github.com/japaric/cargo-call-stack, Apache-2.0/MIT. push 2024-10.
  - "Last tested nightly: 2023-11-13"이고 임베디드 대상의 LLVM IR 기반입니다. fn pointer와 dyn Trait 지원은 "imperfect"입니다. 사실상 정체 상태.
- cga(MIR call graph): https://github.com/OpenDevicePartnership/cga, MIT. 2026-06 생성, push 2026-11-12.
  - monomorphized MIR에서 rustc-driver로 call graph를 뽑고 vtable 후보도 해석합니다("possible vtable resolution"). 고정된 nightly가 필요하고 entrypoint가 있는 크레이트만 대상입니다. 용도가 panic 검사라 범용 출력은 아닙니다.
  - README의 대안 목록: charon(https://github.com/AeneasVerif/charon, Apache-2.0, push 2026-11-17, MIR exporter), cargo-callgraph(robinmoussu, 2022 정체).
- rustdoc JSON: https://github.com/rust-lang/rustdoc-types, FORMAT_VERSION 61 (v0.61.0), push 2026-11-10.
  - Unverified: nightly 전용 unstable 포맷이고 item과 impl 목록만 담습니다. 함수 본문 참조나 call graph는 없습니다.
- cargo-depgraph: https://github.com/jplatte/cargo-depgraph, archived(2026-05). 크레이트 단위 의존성 그래프라 cargo metadata로 대체할 수 있습니다.
- rust-code-analysis: https://github.com/mozilla/rust-code-analysis. 마지막 커밋 2026-01-20, 마지막 release v0.0.25 (2023-01).
  - tree-sitter 기반 메트릭 도구라 그래프 엔진이 아니어서 무관합니다. Unverified: 라이선스는 MPL-2.0(API에 spdx 정보가 없음).

### Go 전용
- golang.org/x/tools: https://github.com/golang/tools, BSD-3. gopls/v0.24.0 (2026-10-15).
  - go/callgraph 알고리즘:
    - static: 정적 호출만 잡으며 unsound.
    - cha: implements 관계 전체를 보수적으로 가정해 partial program(라이브러리)에서도 sound하지만 spurious edge가 생깁니다.
    - rta: main/root에서 도달 가능한 부분만 다룹니다.
    - vta: 타입 전파 방식이라 가장 정밀하고 "experimental phase"입니다.
  - generic 함수 본문 처리는 TODO로 남아 있습니다(callgraph.go).
  - go/pointer는 x/tools/go에서 제거됐습니다(디렉터리 목록 확인).
  - cmd/callgraph는 -algo=static|cha|rta|vta와 -format 템플릿을 제공합니다.
- gopls: implementation(method set 매칭, func 타입은 signature 매칭), type hierarchy, call hierarchy를 지원합니다. 단 call hierarchy는 "Dynamic calls are not included"입니다(gopls/doc/features/navigation.md). 실험적 MCP 서버도 내장돼 있습니다(mcp.md).
- go/packages + go/types: Unverified: 컴파일러 수준의 Defs/Uses를 라이브러리로 직접 얻을 수 있습니다(scip-go의 기반).
- go-callvis: https://github.com/ondrajz/go-callvis, MIT. v0.7.1 (2025-01), push 2026-03.
  - -algo static|cha|rta|vta(기본 static)를 지원하지만 README는 여전히 go/pointer를 언급합니다. DOT와 웹 뷰어 출력이고, 엔진이라기보다 시각화 도구입니다.

### TS 전용
- dependency-cruiser: https://github.com/sverweij/dependency-cruiser, MIT. v18.8.0 (2026-11-16), 활발함.
  - 모듈(import) 그래프만 만듭니다. JSON 출력을 지원하고 --cache와 --cache-strategy로 증분 실행이 됩니다(doc/cli.md). 심볼과 호출 정보는 없습니다.
- madge: https://github.com/pahen/madge, MIT. 마지막 커밋 2024-08-05 (v8.0.0). 정체 상태이므로 dependency-cruiser를 쓰면 됩니다.
- ts-morph: https://github.com/dsherret/ts-morph, MIT. 28.0.0 (2026-04-12), push 2026-11-16.
  - TS 컴파일러 API 래퍼 라이브러리라 컴파일러 수준 정확도를 냅니다. 그래프는 직접 구성해야 합니다. TS7 API가 없어서 TS6에 묶입니다(위 TS7 근거와 동일).
- knip: https://github.com/webpro-nl/knip, ISC. knip@6.45.1 (2026-11-11).
  - 미사용 file, export, dependency 탐지기이고 --reporter json을 지원합니다. 그래프 자체는 노출하지 않으므로 보조 신호로만 쓸 수 있습니다.

### LSP-as-engine
- rust-analyzer: callHierarchy O, implementation O, typeHierarchy X.
- gopls: call hierarchy, type hierarchy, implementation O. 단 call hierarchy에서 dynamic call은 제외됩니다.
- tsserver: Unverified: call hierarchy를 지원합니다(TS 3.8+). TS7 LSP는 "Nearly all features implemented"입니다.
- 공통적으로 질의 단위가 위치 하나씩이라 전체 그래프를 덤프하면 느리고, 대신 편집에 즉시 증분 반응합니다.
- multilspy: https://github.com/microsoft/multilspy, MIT. 커밋 2026-10-13.
  - rust-analyzer, gopls, typescript-language-server를 지원합니다. README에 나열된 API는 definition, references, completion, hover, documentSymbol이고 call hierarchy는 나열돼 있지 않습니다. Python 전용입니다.
- serena: https://github.com/oraios/serena. v2.1.0 (2026-10-19). Unverified: LSP 위의 agent 도구 계층입니다.

### 순위 (Rust+Go+TS 단일 데이터 모델)
1. SCIP + rust-analyzer scip, scip-go, scip-typescript: 세 언어 모두 컴파일러 수준 def/ref를 하나의 protobuf 스키마로 주고 지금도 활발히 유지되며 이미 로컬에서 동작을 확인했습니다. 다만 Rust trait impl 관계는 비어 있고, call edge는 enclosing_range로 직접 유도해야 하며, 증분이 없어 파일·패키지 단위 재실행 후 병합해야 합니다.
2. LSP 보강 계층(rust-analyzer, gopls, tsserver, 직접 구현하는 LSP 클라이언트): SCIP에 빠진 implementation과 call hierarchy를 채우고 커밋 사이 증분 갱신을 맡는 데 가장 싼 방법입니다. 다만 dynamic dispatch는 어느 서버도 해결하지 못합니다.
3. Glean(SCIP 수집): 증분 unit과 질의 언어가 필요해질 때의 저장·질의 백엔드입니다. 정밀도 이득은 없고 운영 비용이 큽니다.
4. Joern CPG: 세 언어의 call graph와 data flow까지 한 그래프로 주지만 TS는 휴리스틱이고 JVM 배치 엔진이라, 주 엔진보다는 보안·흐름 분석 보조에 맞습니다.

dynamic dispatch 정밀도가 필요한 곳의 보조 수단: Go는 go/callgraph vta, Rust는 MIR 기반(cga, charon).
