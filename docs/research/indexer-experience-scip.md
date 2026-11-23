# Indexer-building experience and practical SCIP — research record (2026-11-23)

What teams that built and ran code indexers wrote about it (Sourcegraph, GitHub, Google,
Meta, rust-analyzer, JetBrains, Sourcetrail), the practical state of producing and
consuming SCIP (the Rust crate, interoperability traps, scip-typescript coverage,
multi-language merging, virtual documents) and testing practice for indexers.

Collected by a web-research agent on 2026-11-23. Sources are given as raw URLs. A statement
prefixed `Unverified:` was not confirmed on a page the researcher opened, and vendor or
author claims are reported as claims. Sources that could not be opened are named in the
report. The text is kept as the researcher wrote it, in Korean or English.

## A. Case studies and postmortems

**Meta Glean**
- https://engineering.fb.com/2024/12/19/developer-tools/glean-open-source-code-indexing/ (2024-12-19)
  - 전체 재인덱싱은 대형 모노레포에서 수 시간 뒤처지므로 변경량에 비례하는 비용을 목표로 합니다. C++은 변경 헤더를 포함하는 파일을 반복 탐색해 영향 범위를 계산합니다.
  - 불변 DB를 겹겹이 쌓고 각 층이 사실을 추가하거나 숨깁니다.
  - 스키마 교훈: 언어별 세부 사실은 그대로 저장하고, 언어 중립 계층은 파생 뷰로 추가합니다(세부와 최소공통분모 중 택일하지 않음). 접두 필드로 조회가 빠르게 합니다.
  - 중요한 질의: 정의로 이동, 참조 찾기, 심볼 검색, 파일별 문서 심볼, 문서 생성, diff 분석, 죽은 코드 탐지, 테스트 선택, AI 어시스턴트용 RAG. 단순 이름 조회는 약 1ms. 인덱싱 주기는 글에 없습니다.
- https://glean.software/blog/incremental/ (2022-12-01, Simon Marlow)
  - unit(파일 등) 단위로 사실의 소유권을 기록하고, 증분 DB가 기반 DB의 해당 unit을 가립니다. 소유권 데이터는 DB 크기의 약 7%, 일반 질의 오버헤드 10% 미만, 검색이 무거운 질의는 약 3배 느립니다. 한계: 증분 파생은 일부 질의 종류에서만 변경량에 비례합니다.
- https://simonmar.github.io/posts/2025-05-22-Glean-Haskell.html (2025-05-22)
  - Hackage 전체 기준 hiedb 1021초 대 Glean 470초, 크기 5.2GB 대 0.8GB. 참조 찾기 hiedb 2.3초, Glean 원시 질의 0.03초(이름 기준 사전 색인 여부 차이).
  - 컴파일러 사실에서 얻기 어려운 것: 타입 시그니처 내 참조와 코드 참조의 구분, record selector 예외, 선언의 전체 AST 복원.
  - Glean의 증분 인덱싱은 IDE용이 아니라 전체 저장소 인덱싱 가속 목적이라고 저자가 밝혔습니다.
- https://github.com/facebookincubator/Glean/blob/main/glean/schema/source/scip.angle : 로컬 심볼은 문서 경로를 접두어로 붙여 전역 유일하게 만듭니다. 텍스트 인코딩은 메타데이터로만 보관하며 변환에 어떻게 쓰이는지는 발췌에서 확인되지 않았습니다.

**Google**
- https://abseil.io/resources/swe-book/html/ch17.html : 질의 유형은 How 약 1/3(API 사용 예), What 약 1/4(탐색), Where와 Why 각 약 16%, Who/When 약 8%. 검색 인덱스는 증분(지연 10초 미만), Kythe 교차 참조는 하루 한 번 수 시간짜리 분산 빌드이며 두 인덱스의 시차가 반복되는 혼란의 원인. 사고 대응 중 오래된 교차 참조가 특히 해롭고 버전은 하나만 유지. 생성 코드는 저장소에 없으면 인덱싱하지 않음.
- https://kythe.io/docs/schema/writing-an-indexer.html : 정방향 edge만 내보내기, 출력은 결정적이어야 함, 소스 위치를 VName에 넣지 않음(churn 방지), 바이트 오프셋(시작 포함, 끝 제외), 테스트 목표는 기능 관련 부분 그래프만 단언해 내부 선택 변경에 강하게.
- https://kythe.io/docs/schema/indexing-generated-code.html : 생성 코드와 원본은 별도 노드로 두고 `generates` edge로 연결. 생성기가 원본 오프셋 매핑(.meta)을 내보내고 인덱서가 읽음. 소비자가 따라갈지 선택.
- Kythe 사용 실태에 대한 경험 보고 논문은 찾지 못했습니다.

**GitHub**
- https://github.blog/open-source/introducing-stack-graphs/ (2021-12-09, 2024-07 갱신, Douglas Creager): 제약은 파일별 독립 분석(증분), 레포별 설정 없음, 다수 언어. 인덱스 시점에 파일별 그래프, 질의 시점에 경로 탐색. tree-sitter tags와 직접 비교하지 않았고 데이터플로나 제네릭 상속은 다루지 않음.
- https://github.com/github/stack-graphs : 2025-09-09 아카이브. README는 "더 이상 GitHub가 지원·갱신하지 않으며 fork 권장"만 있고 사유와 후속 도구는 없음. Unverified: Lobsters 댓글의 추측(정확한 탐색 기능 중단, 언어 추가 비용이 큼 - TS만 약 6천 줄 DSL)은 공식 설명이 아닙니다.
- Unverified: https://queue.acm.org/detail.cfm?id=3487022 는 403이라 열지 못했고 우회하지 않았습니다. 검색 요약상 tags 분석은 tree-sitter 기반이고 설정 없음이 목표. HN 댓글 요약상 GitHub 팀원이 C/C++처럼 근사 탐색 품질이 낮은 언어에는 빌드 기반 탐색을 나중에 지원하겠다고 말했습니다.

**Sourcegraph (앞 부분 요약 보충)**
- https://sourcegraph.com/blog/announcing-scip (2022-06-08, Olafur Geirsson): LSIF는 스키마 없음, 숫자 ID 전역·순서 의존, 읽고 쓸 때 메모리 과다. 벤더 주장: gzip 기준 LSIF가 약 4배 큼, lsif-node를 scip-typescript로 바꿔 CI 10배 빨라짐(프로토콜 덕만은 아님), Meta 인용 "8배 작고 3배 빨리 처리". 약속한 증분 인덱싱과 언어 간 탐색은 이후 실현되지 않음.
- https://raw.githubusercontent.com/scip-code/scip/main/docs/DESIGN.md : SCIP은 저장·질의용이 아니라 전송 포맷, 생산자 쪽 최적화. 단일 인접 리스트 그래프 인코딩은 병렬성과 메모리에 불리하다고 명시. 문자열 ID는 LSIF의 off-by-one 같은 버그 피해를 한정. 양방향 링크는 저장하지 않음.
- https://sourcegraph.com/blog/announcing-scip-clang (2023-05-15, Varun Gandhi): Clang을 라이브러리로 사용, TU 하나가 죽어도 나머지 계속, 타입 체크 후 AST 순회 비용이 병렬 타입 체크 기준선 대비 30~50% 추가, 인덱스 크기는 LSIF의 10~20%. 테스트 방식과 생성 코드는 글에 없음.
- https://sourcegraph.com/docs/code-search/code-navigation/writing_an_indexer : 권장 순서(스키마, 단일 파일 occurrence, `scip snapshot` 검증, 점진 추가). 해시 테이블 순회가 비결정적이면 snapshot이 거짓 diff를 냄. C++ protobuf 2GB 한계는 문서를 하나씩 디스크에 쓰는 우회. 크로스 레포는 임포트한 정확한 커밋이 인덱싱돼야 동작.
- https://sourcegraph.com/docs/code-search/code-navigation/precise_code_navigation : 정확한 탐색이 안 되면 검색 기반으로 폴백, 자동 인덱싱은 executor 사용, 복잡한 빌드는 CI 권장. 규모 수치 없음. scip-python 블로그와 scip-ruby 설계 노트는 열지 못함.

**rust-analyzer와 IDE 인덱싱**
- https://rust-analyzer.github.io/blog/2020/07/20/three-architectures-for-responsive-ide.html (2020-07-20, matklad): map-reduce(IntelliJ, stub), 헤더 활용(clangd), 쿼리 기반(salsa). 속도는 증분성이 아니라 지연 평가에서 나온다는 논지. 인덱스 export는 다루지 않으므로 "배치 export를 LSP 경로와 분리하는 이유"를 직접 설명한 matklad 글은 찾지 못했습니다.
- https://matklad.github.io/2022/04/25/why-lsp.html : 배치 컴파일러는 처리량, 언어 서버는 지연 최적화. LSP는 직렬화 형식일 뿐 내부 모델로 쓰지 말 것. LSIF/SCIP 언급 없음.
- https://rust-analyzer.github.io//blog/2021/11/21/ides-and-macros.html (2021-11-21): 매크로는 되돌릴 수 없는 변환이라 소스 매핑이 어려움. 최상위 이름을 추가하는 매크로는 이름 해석을 순차적으로 만듦. proc macro는 별도 프로세스, 비결정적 proc macro는 재파싱 가정을 깸.
- https://plugins.jetbrains.com/docs/intellij/indexing-and-psi-stubs.html : stub은 외부에서 보이는 선언만 담은 PSI 부분 트리의 압축 바이너리. lexer와 light AST 선호. stub이 존재하는 이유 자체는 이 페이지에 없음.

**Sourcetrail**
- https://github.com/CoatiSoftware/Sourcetrail : 2021-12-14 아카이브, README에 사유 없음. 중단 공지 블로그는 archive.org라 이 도구로 열 수 없었습니다.
- Unverified: 2차 요약(Grokipedia)에 따르면 창업자들의 직장·거주 변경으로 협업이 어려웠고 여러 언어의 의존성과 빌드 시스템 유지가 부담이었다고 합니다. 원문 인용은 확인하지 못했습니다.

**사내 멀티 언어 그래프**
- Unverified: Uber의 "AI Context Graph"(노드 2,400만, 엣지 8,000만, 노드 유형 86, 엣지 유형 117)는 X 게시물 요약만 봤고 원문을 열지 못했습니다. 코드 인덱서가 아니라 에이전트용 서비스·테이블 그래프로 보입니다. Stripe, Dropbox, Airbnb, Spotify의 솔직한 글은 찾지 못했습니다.

**2025~2026 에이전트용 코드 그래프 도구**
- https://github.com/ale-hop-ai/codeintel : tree-sitter 계층과 SCIP 계층을 span 포함 관계로 조인. 모든 사실에 `exact`/`name` 출처 부여. upstream `tags.scm`은 커버리지가 고르지 않아 직접 쿼리 작성(TS는 호출 참조 없음, Python은 메서드 캡처 없음). 앵커율: Rust 80%(빠진 9개는 모두 모듈), Go 96.2%, Python 97.0%, TS 95.1%. 컬럼이 어긋나면 추측하지 않고 앵커링 포기.
- https://github.com/colbymchenry/codegraph/pull/2427 (PoC, 작성자 자체 측정, 주장): SCIP 병합으로 vscode 재현율 82/74에서 100/98, 정밀도 93/94에서 100/100. 비용은 vscode 빌드 2m06s에서 3m51s, 인덱스 1.77GB에서 1.98GB(scip-typescript 원본 1,103MB는 압축하면 176MB). 심볼 정체 불일치: SCIP은 오버로드를 첫 시그니처에 정의(미검증 엣지의 약 45%), rust-analyzer는 중첩 `fn imp`를 모두 같은 이름으로 만듦. 호출자 귀속을 SCIP enclosing range 대신 자체 노드 span으로 바꿔 중복 엣지 3,820개에서 0. 작성자는 Sonnet이 grep만으로도 많은 질문에 답했으니 이득은 에이전트 행동 변화보다 그래프 정확도라고 밝힘.
- https://github.com/orgs/sheeptechnologies/discussions/4 (RFC, 2026-01, 주장): SCIP이 파일 증분이 아니라는 이유로 tree-sitter로 되돌리자는 제안. 수치는 대부분 프로젝션.
- https://arxiv.org/abs/2607.25431 (CodeNib): 사전 계산 정의가 라이브 서버와 86% 일치. 사전 계산 인덱스는 갱신이 에러 없이 끝나도 틀릴 수 있다는 논지. 초록에는 SCIP/tree-sitter가 나오지 않습니다.
- "tree-sitter에서 컴파일러 기반으로 이주했다"는 단일 회고 글은 찾지 못했습니다. 위 도구들은 tree-sitter를 버리지 않고 SCIP을 덮어씌웁니다.

**라이선스·상표·이름 문제 사례**
- Sourcegraph 재라이선스: https://www.devclass.com/development/2024/08/21/sourcegraph-makes-core-repository-private-co-founder-complains-open-source-means-extra-work-and-risk/1623688 (2024-08-21). 2018년 Apache로 시작, 2023-06 대부분을 엔터프라이즈 라이선스로 변경, 2024-08 핵심 저장소 비공개. 마지막 공개 스냅샷이 GitHub에 남음. 기사에 SCIP/인덱서 라이선스 언급 없음. 교훈: 한 회사가 후원하는 프로토콜과 인덱서는 라이선스·지배구조 위험이 있고, SCIP 쪽 대응이 독립 지배구조 전환(2026-03-25).
- Sourcetrail: GPLv3(2019 공개), 2021-12 아카이브. 라이선스 충돌이 아니라 유지보수 문제.
- 의존성 라이선스 충돌을 겪은 코드 인덱싱 프로젝트는 찾지 못했습니다. SCIP 이름이나 인덱서 이름의 공개 상표 분쟁도 찾지 못했습니다. https://sourcegraph.com/blog/the-future-of-scip 는 상표를 다루지 않고 발음만 "skip"이라고 밝힘. Unverified: 같은 약어의 무관한 프로젝트(정수 계획 솔버 등)는 일반 지식입니다.

## B. Producing and consuming SCIP

**공식 자료**
- 저장소는 https://github.com/scip-code/scip 로 이전(crates.io repository 값). sourcegraph/scip URL도 열리지만 그쪽 `docs/DESIGN.md` 경로는 404.
- 스키마 주석 https://raw.githubusercontent.com/scip-code/scip/main/scip.proto :
  - 새 생산자는 `typed_range` 설정, deprecated `range`는 쓰지 말 것(SHOULD NOT). 소비자는 typed 우선, `repeated int32` 폴백.
  - `single_line_range`(한 줄이면 생산자 SHOULD)와 `multi_line_range`(소비자는 항상 수용 MUST), 둘 다 0 기반 반개구간 [start, end).
  - `typed_enclosing_range`가 있으면 deprecated `enclosing_range`보다 우선.
  - 위치 인코딩은 UTF-8(바이트)/UTF-16/UTF-32, `Unspecified`는 새 인덱서가 쓰면 안 됨.
  - 로컬 심볼은 한 Document 안에서만 사용(MUST). `Relationship`은 is_reference, is_implementation, is_type_definition, is_definition이 있고 호출 엣지는 없음.
  - `Index.metadata`는 스트림 맨 앞에 한 번만. `external_symbols`는 "별도 Index 메시지에 정의된" 심볼. `Document.text`는 선택, `language`는 문자열, `SymbolRole.Generated`는 비트 0x10.
- `scip` CLI https://raw.githubusercontent.com/scip-code/scip/main/docs/CLI.md : `lint`(형식 점검, 인덱서 작성자용), `print [--json]`, `snapshot`(`--from`, `--to scip-snapshot`, `--strict`, `--comment-syntax`), `stats`, `test`(사람이 읽는 테스트 파일과 대조, `--check-documents`), `expt-convert`(SQLite 변환, 실험적). 별도 `convert`나 병합 명령은 없음.
- https://raw.githubusercontent.com/scip-code/scip/main/docs/Development.md : 자체 테스트용 `reprolang`(작은 언어, tree-sitter 문법), 스냅샷은 `go test ./cmd/scip -update-snapshots`.

**Rust `scip` 크레이트** (https://docs.rs/scip/latest/scip/ , https://crates.io/api/v1/crates/scip)
- 0.10.0(2026-09-03). 직전 0.9.0(2026-06-29), 0.8.x(2026-06), 0.7.x(2026-03~04). 라이선스 Apache-2.0.
- protobuf 런타임은 rust-protobuf, `protobuf = "=3.7.2"` 고정(https://raw.githubusercontent.com/scip-code/scip/main/bindings/rust/Cargo.toml). prost가 아닙니다. 생성 코드는 저장소에 체크인, 빌드 의존성 없음.
- https://raw.githubusercontent.com/scip-code/scip/main/bindings/rust/src/symbol.rs : `parse_symbol`, `format_symbol`, `format_symbol_with(SymbolFormatOptions)`, `is_local_symbol`, `is_global_symbol`, `try_parse_local_symbol`, `Symbol::new_local`. 이스케이프는 백틱.
- docs.rs 페이지는 모듈이 `symbol`과 `types`뿐이고 typed range/인코딩 언급은 없다고만 확인. 보조 함수는 `write_message_to_file` 하나만 봤습니다.
- Unverified: 0.10.0 생성 코드가 `typed_range`, `typed_enclosing_range`를 포함하는지 직접 확인하지 못했습니다(최신 proto에는 있음). `bindings/rust/src/generated`를 열거나 `cargo doc`으로 확인 필요.
- 대안(.proto를 prost로 직접 컴파일)의 사례는 찾지 못했습니다. 큰 인덱스는 `Index.documents`(필드 2)를 와이어 태그 수준에서 직접 스트리밍하는 접근이 여러 도구에서 반복됩니다(Unverified: https://github.com/cristiangirlea/adanext-map/issues/11 등은 검색 요약만 봄). 문서 크기 한도 문제는 https://github.com/SquashMerge/trellis/issues/394 에서 확인.

**상호 운용 함정**
- 위치 인코딩: rust-analyzer는 UTF-8 (https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/rust-analyzer/src/cli/scip.rs). scip-typescript는 UTF-16 (`ProjectIndexer.ts`가 모든 문서에 UTF16 설정). https://github.com/Trivyn/moosedev/issues/60 에서 UTF-8만 기대한 소비자가 TS 인덱스 전체를 거부했고 경고만 남기고 끝나 낡은 노드 약 4,700개가 남았음(이후 PR로 해결). callix 대조에서도 컬럼 차이 911곳.
- `enclosing_range`: rust-analyzer는 2025-11-28에 정의 쪽 추가(https://github.com/rust-lang/rust-analyzer/pull/21141), 같은 파일 안 정의 본문일 때만 채움. https://github.com/SquashMerge/trellis/issues/444 (소비자 보고): scip-typescript 0.4.0 인덱스에서 참조 76,133개 중 0개가 enclosing range를 가짐. scip-java, scip-python도 같음, rust-analyzer는 일부 참조에만. 호출 귀속에 enclosing range만 쓰면 JS에서 호출 엣지가 0이 되고 정확한 callee span 일치로 해결. Unverified: scip-go의 enclosing range 제공 여부는 직접 확인 못함.
- 심볼 문자열: rust-analyzer 스킴은 `rust-analyzer cargo <crate> <version|.> ...`, disambiguator 항상 빈 값. 함수 안 중첩 struct 등은 이름 충돌로 중복 심볼 발생(알려진 버그 #18771, #18772, #21281, stderr 출력). scip-typescript는 이름 없는 중첩 `package.json`이 익명 패키지 `npm . .`이 되어 서로 다른 패키지의 `index.d.ts`가 같은 심볼이 됨(https://github.com/sourcegraph/scip-typescript/issues/525, 2026-11-02 열림, 11-03 PR #527로 닫힘). pnpm `.pnpm` 실경로가 패키지명 `.`이 되는 사례(https://github.com/manji-0/dagayn/issues/219, 0.5.2 기준 소비자 보고). `@types/*`는 런타임 패키지명이 아니라 `@types/<pkg>`로 심볼이 붙음(Unverified: codeintel #58 검색 요약만).
- rust-analyzer 출력 특성: `relationships`는 항상 빔, 정의 역할 비트는 토큰 범위가 정의 범위와 정확히 같을 때만. 소비자 보고(주장): 모든 `signature_documentation` Document에 필드 6(position_encoding)이 들어 있어 엄격한 디코더가 거부(trellis #394). 실제 인덱스는 24MB, 문서 158개, occurrence 266,420개, 한 문서에 최대 32,293개.
- 매크로·생성 코드: 같은 파일 안 `macro_rules!`만 확장된다는 주장이 callix 대조에 나옴. 매크로 생성 항목과 파일 간·proc 매크로 본문이 누락의 대부분(약 3,250곳 누락, 694곳 정의 누락). `scip.rs` 자체는 매크로 확장 항목 처리를 설명하지 않음. `use serde::de::Error;`에 참조가 안 나오는 SCIP-LSP 불일치가 열려 있음(https://github.com/rust-lang/rust-analyzer/issues/23565, 2026-10-27, 댓글 없음). 매크로 사용 SCIP 패닉 사례: https://github.com/rust-lang/rust-analyzer/pull/23369, 중복 심볼 오류 https://github.com/rust-lang/rust-analyzer/issues/21281 (열림, 사용자는 출력이 쓸 만하다고 평가).
- scip-go: 빌드 태그는 `GOFLAGS=-tags`로 넘기면 해당 태그 파일만 인덱싱(소비자 쪽 글, 검색 요약). Windows에서 빌드 캐시의 테스트 main이 문서로 들어가 변환기가 인덱스 전체를 거부한 사례가 0.2.10에서 수정. README(https://raw.githubusercontent.com/sourcegraph/scip-go/main/README.md)의 한계: go.mod 없는 프로젝트, 표준 라이브러리 탐색 실패 가능성, Go Packages Driver 경로의 크로스 레포 불가.
- 인덱스 크기: protobuf 2GB 한계와 문서 단위 쓰기 우회는 writing_an_indexer 문서에 있음.

**SCIP 옆에 별도 데이터를 두는 경우** (모두 비표준, SCIP 심볼 문자열을 키로 삼음)
- https://github.com/liza-mas/scip-search/ : `scip.graph-export.v1` 별도 JSON. 인덱스 바이트의 sha256 지문과 프로젝트 루트를 기록하고 심볼 문자열은 그대로 보존. 비로컬 정의가 인덱스 간에 충돌하면 병합 실패. 호출 정보는 enclosing range가 없으면 "unavailable"로 보고하고 추측하지 않음.
- https://github.com/ale-hop-ai/codeintel : span 포함 관계로 조인하고 불일치 시 앵커링 포기.
- Unverified: codecite #243의 "심볼 문자열 약 100바이트를 occurrence마다 반복 저장하니 정수 키로 바꾸자"는 검색 요약만 봤음.
- SCIP 파일 안에 커스텀 필드를 넣는 사례나 공식 확장 지점은 찾지 못했습니다.

**scip-typescript 커버리지 (질문 1)**
- 옵션 (https://raw.githubusercontent.com/sourcegraph/scip-typescript/main/src/CommandLineOptions.ts): `--yarn-workspaces`, `--pnpm-workspaces`, `--npm-workspaces`, `--infer-tsconfig`, `--no-global-caches`, `--max-file-byte-size`(기본 1mb), `[projects...]`.
- `src/main.ts`: 워크스페이스 디렉터리마다 `-p <디렉터리>`로 그 `tsconfig.json`을 읽음(파일별 "가장 가까운 tsconfig 탐색"이 아니라 패키지 디렉터리 단위). tsconfig가 없으면 `--infer-tsconfig`일 때만 생성, 아니면 건너뜀. 프로젝트 레퍼런스는 자기 파일 전에 재귀적으로 먼저 인덱싱(사이클 방지). 프로젝트는 순차 처리(병렬화는 주의 필요하다는 주석). 출력은 하나의 `index.scip`에 `Index` 메시지가 연속 직렬화, 첫 메시지가 메타데이터. 문서가 없으면 출력을 지우고 오류 종료.
- `ProjectIndexer.ts`: tsconfig `fileNames`에 있는 파일만 문서로 내보냄(import로 딸려 온 파일은 제외). `indexedFiles`로 겹치는 프로젝트의 중복 문서 방지. 모든 문서 UTF-16.
- 크로스 패키지 심볼 연결: 패키지명을 프로젝트에 매핑하는 코드는 없고, `paths`와 워크스페이스 import 해석은 TypeScript 모듈 해석에 위임. 심볼의 패키지명/버전은 `Packages.ts`가 가장 가까운 `name` 있는 `package.json`에서 정함(Unverified: `Packages.ts` 자체는 열지 않았고 아래 이슈로만 확인).
- 알려진 문제: 이름 없는 중첩 `package.json`(#525, #527), 프로젝트 루트 밖 경로 유출(https://github.com/sourcegraph/scip-typescript/pull/529, 2026-11-03 병합: 이름 있는 `package.json`을 찾을 때만 상위로 올라가고 못 찾으면 파일명만 남겨 체크아웃 위치 비의존), pnpm 실경로 `.`(dagayn #219), 워크스페이스 패키지 심볼이 정의 없이 이름만 나오는 현상(dagayn PR #227, 소비자 쪽 보고, 0.5.2 기준).
- Unverified: `paths`와 프로젝트 레퍼런스 조합을 직접 다룬 scip-typescript 이슈는 찾지 못했습니다. 이 조합은 직접 재현 실험이 필요합니다.
- Svelte: https://github.com/sourcegraph/scip-typescript/pull/509 (eseliger, 2026-10-20 열림, 2026-10-30 병합). `svelte2tsx`로 변환하고 소스맵으로 심볼·범위를 원본 `.svelte`로 되돌림. 작성자는 "완벽하지 않은 시작"이라 함. `svelte2tsx/svelte-shims-v4.d.ts`가 내부 구조에 의존한다는 리뷰 지적에 작성자는 Svelte 5까지 같았지만 유지 보장 못 한다고 답함. https://raw.githubusercontent.com/sourcegraph/scip-typescript/main/src/Svelte.ts : 소스맵 지연 디코드(메모리, PR #526으로 약 600MB 절감), 식별자는 매핑 후 원문 텍스트가 이름과 일치하는지 검사, 매핑 없는 항목은 보정(`export let` props는 `$$render` 변수 범위 재사용, `$store`는 스토어 선언), `svelte2tsx`가 예외를 던지면 빈 소스 파일과 소스맵 없는 정보가 생겨 파일 단위 탐색만 가능.
- 알려진 구멍: https://github.com/sourcegraph/scip-typescript/issues/547 (열림, 0.6.1). 변환 실패 컴포넌트가 import 없이 조용히 색인되고 종료 코드 0. 유지보수자는 그 문서를 빼는 게 맞다고 했고 실패 목록 최대 5개 출력을 제안.
- README(https://raw.githubusercontent.com/sourcegraph/scip-typescript/main/README.md): SvelteKit은 먼저 `npx svelte-kit sync` 실행. `$lib` 별칭은 코드에서 별도 처리 없이 TypeScript 설정을 따름(Unverified: sync가 생성하는 tsconfig의 `paths`로 풀린다는 것은 추정).
- Vue와 Astro: scip-typescript 이슈나 PR을 찾지 못했고 Vue용 SCIP 인덱서나 문서화된 접근도 찾지 못했습니다. 검색에서는 Volar가 `.vue`를 가상 TS 파일과 역매핑으로 바꾼다는 설명(https://blog.vuejs.org/posts/volar-a-new-beginning)만 나왔고 직접 열지 않았습니다. Unverified: 같은 방식(가상 TS 생성과 소스맵 역매핑)을 Vue에 적용할 수 있다는 것은 추론.

**다국어·다중 패키지 저장소의 인덱스 구성 (질문 2)**
- 문서화된 방식은 언어나 프로젝트마다 인덱스를 따로 만들어 각각 올리는 것(https://sourcegraph.com/docs/code-navigation/how-to/index-other-languages: 인덱서 실행 후 `src code-intel upload -file=index.scip`). Unverified: 모듈이 여럿인 Go는 모듈 루트마다 scip-go를 돌리고 업로드 시 `-root`를 지정(https://sourcegraph.com/docs/code-navigation/how-to/index-a-go-repository 검색 요약). https://sourcegraph.com/docs/code-navigation/how-to/adding-scip-to-workflows 에는 `-root`, 겹치는 문서, 우선순위가 없음.
- `scip` CLI와 스키마에는 병합 절차가 없습니다. scip.proto 파일 헤더에 "보완 정보를 여러 출처에서 병합 가능"이라는 문구만 있고, DESIGN.md는 병합을 연결(concatenation)로 가능하게 선택했다고 함. scip-typescript는 실제로 `Index`를 이어 붙여 한 파일로 내보내지만, 같은 파일에 `Index`가 여러 개일 때 소비자가 어떻게 해석할지는 스키마 주석에 없음.
- 경로 정합: `Metadata.project_root`(URI 인코딩된 절대 경로)와 `Document.relative_path`(루트 기준, 필수·유일)가 기준. 한 인덱스에 여러 언어를 넣으려면 공통 루트를 골라야 하고, 서브디렉터리별로 만들면 루트가 달라 경로가 어긋남.
- 심볼 정합: 심볼 문자열은 scheme, manager, package, version을 포함하므로 인덱서가 달라도 같은 엔티티가 일치할지는 인덱서 규약에 달림. 외부 정의는 `external_symbols`. Sourcegraph 크로스 레포는 임포트한 정확한 커밋이 인덱싱돼야 동작.
- 비공식 병합 도구 사례: scip-search의 `aggregate`/`reroot`(심볼 문자열 보존, 외부 심볼은 문자열이 같으면 첫 기록 우선, 로컬 심볼은 문서 범위라 반복 허용, 비로컬 정의 충돌 시 실패, `reroot`는 루트만 변경). Glean은 로컬 심볼을 문서 경로 접두로 전역 유일화.
- 결론: 공식 병합 규약은 없으므로 엔진이 인덱서별로 읽고 심볼을 내부에서 정규화해 합치는 것이 현실적.

**생성·가상 문서의 위치 (질문 3)**
- SCIP과 `scip` CLI에는 소스맵, 가상 문서, 언어 재지정을 위한 일급 개념이 없고 원본 좌표로 되돌리는 일은 전적으로 인덱서의 몫. scip-typescript Svelte는 원문 텍스트를 `originalText`로 따로 두고 소스맵으로 범위를 원본 `.svelte` 위치로 바꿔 내보냄.
- 스키마에서 가장 가까운 필드: `Document.text`(선택, 기본 포함 기대 안 함), `Document.language`(문자열), `Document.position_encoding`, `SymbolRole.Generated`. 생성 코드에서 원본으로의 링크를 나타내는 필드는 없음(Kythe의 `generates`와 대비).
- 열람한 `ProjectIndexer.ts`는 Svelte 문서에 `text`를 설정하지 않음. Unverified: `FileIndexer`가 쓰는지는 확인 못함.
- 설계 의미: 가상 문서에서 나온 occurrence는 이미 원본 좌표여야 한다는 규약을 두고, 매핑이 불완전한 경우를 별도 사실로 표시해야 함(#547처럼 조용한 누락이 위험).

**SCIP 라이선스 관련 사실**
- 아래 "라이선스 노트"는 메시지 4에 있습니다.

## C. Testing practice

- 스냅샷(골든): 모든 SCIP 인덱서가 `scip snapshot` 형식(소스에 occurrence 주석 삽입)을 쓰고 입력은 `snapshots/input`, 출력은 `snapshots/output`에 둠. scip-dotnet은 `SCIP_UPDATE_SNAPSHOTS=true dotnet test`(https://github.com/sourcegraph/scip-dotnet, Verify 라이브러리 사용, 불일치 시 unified diff), scip-go는 `go test ./internal/index -update-snapshots`(README). Unverified: 공용 Go 헬퍼가 scip 저장소 `bindings/go/scip/testutil`에 있다는 것은 검색 요약만 봤고 열지 않음.
- 기능별 작은 픽스처: scip 저장소의 `reprolang`과 `cmd/scip/tests/`가 가장 직접적인 모델(https://raw.githubusercontent.com/scip-code/scip/main/docs/Development.md). 새 의미를 추가할 때는 테스트 파일과 기능을 먼저 구현한 뒤 스냅샷을 재생성.
- 인라인 단언: https://kythe.io/docs/schema/writing-an-indexer.html 의 verifier는 소스 주석에 목표를 쓰고 필요한 사실의 존재·부재를 단언. 스냅샷이 내부 선택에 민감한 약점을 줄임. 목표는 기능 관련 부분 그래프만 검사.
- 단위형 검증: rust-analyzer의 `check_symbol`(`$0` 마커로 토큰을 찍고 심볼 문자열을 `assert_eq!`)과 테스트 목록(https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/rust-analyzer/src/cli/scip.rs). 대상별 테스트(proc-macro, bin, example, test, bench, build-script)와 알려진 오작동을 FIXME 테스트로 박아 두는 방식이 참고할 만함. 스냅샷 테스트는 없음.
- 인덱서 간 차분 비교: https://github.com/Callix-Tools/callix/issues/13 (소비자 쪽 comparator, 작성자 본인 분류, 독립 검증 아님). 정의 위치로 심볼을 맞춘 뒤 참조마다 match/wrong/external/missing으로 분류하고 재현율·정밀도를 냄.
  - scip-go: gin, hugo에서 정밀도 1.000, 재현율 0.53~0.63(수정 후 0.96~0.99).
  - rust-analyzer: ripgrep, axum에서 정밀도 1.000, 재현율 0.36~0.51(수정 후 0.75~0.81). 매크로 생성 항목이 누락의 주원인, `.await`가 `Future::poll` 호출로 인덱싱되는 문제도 있음.
  - scip-typescript: zod에서 정밀도 0.992, 재현율 0.314(수정 후 0.917). 객체 리터럴 키는 정의로 취급.
- 자기 인덱싱·코퍼스 실행: 자기 인덱싱 사례는 이 조사에서 직접 확인하지 못했습니다. 코퍼스 실행 실례는 위 callix(gin, hugo, ripgrep, axum, zod)와 codegraph PoC(vscode, Playwright, Django, cobra, ripgrep). rust-analyzer SCIP 패닉은 wasmtime, curve25519-dalek 같은 실제 크레이트에서 드러남(#21281, PR #23369).
- 소비자 쪽 교훈: trellis는 합성 SCIP로만 테스트하다 실제 rust-analyzer 출력에서 디코더가 깨졌고(issue #394), 호출 바인딩 0건 버그도 `facts > 0`만 단언한 종단 테스트가 놓침(issue #444).
- 빠진 것: 일반적인 퍼저·프로퍼티 테스트 사례는 찾지 못했습니다.

**열지 못한 소스 (우회하지 않음)**
- https://queue.acm.org/detail.cfm?id=3487022 : 403
- https://web.archive.org/web/20211115131149/https://www.sourcetrail.com/blog/discontinue_sourcetrail/ : 도구에서 차단
- https://drops.dagstuhl.de/storage/01oasics/oasics-vol109-evcs2023/OASIcs.EVCS.2023.8/OASIcs.EVCS.2023.8.pdf : PDF 텍스트 추출 불가
- https://github.com/sourcegraph/scip-typescript/blob/main/docs/Development.md : 404

## Lessons and licence notes

**교훈 (설계에 미치는 영향 순)**
1. SCIP만으로는 호출 그래프를 만들 수 없다고 전제하고 호출 증거는 별도 계층으로 설계. 참조 occurrence와 enclosing range 조합은 인덱서마다 다르고(TS는 참조에 없음) rust-analyzer는 일부 호출에서 누락.
2. 부가 데이터는 SCIP 심볼 문자열을 불투명 키로 삼는 사이드카에 둘 것(모든 사례가 이렇게 함). 매칭이 안 되면 추측하지 말고 "unavailable"로 표시(scip-search, codeintel).
3. 모든 사실에 출처와 정확도 등급(compiler/heuristic)을 붙일 것. 조용한 저하가 가장 흔한 실패(moosedev의 TS 인덱스 무시, 호출 0건, #547의 변환 실패 컴포넌트).
4. 위치는 소비 시점에 문서별 인코딩을 읽어 내부 단일 기준(예: UTF-8 바이트)으로 변환. 인덱서가 UTF-8과 UTF-16으로 갈림.
5. 심볼 정체는 인덱서별로 불안정(오버로드는 첫 시그니처, 중첩 함수 충돌, 익명 패키지 `npm . .`, 중복 심볼). 정규화 계층과 충돌 감지를 두고 한 인덱스 내 중복은 오류가 아니라 경고로.
6. 스트리밍 디코드(와이어 태그로 `documents`를 하나씩)를 처음부터 넣을 것. rust-analyzer 인덱스가 이미 24MB, 한 문서에 occurrence 수만 개. `scip` 크레이트(rust-protobuf 고정)로 전체 `Index`를 읽는 것을 기본으로 삼지 말 것.
7. 증분은 SCIP이 해결해 주지 않음. Google(교차 참조 하루 한 번, 검색 인덱스와 시차가 혼란의 원인)과 Glean(unit 단위 소유권)의 경험상 "파일 단위 소유권 + 영향 범위 재계산"이 필요.
8. 매크로·생성 코드·빌드 태그·가상 문서(Svelte/Vue)는 인덱서마다 정책이 다르므로 "누락"이 정상 출력. 누락 위치를 별도 사실로 기록. Kythe의 별도 노드 + `generates` edge가 참고할 만함. 가상 문서 역매핑은 SCIP에 규약이 없고 인덱서 몫.
9. 인덱서 출력과 별개로 자체 차분 테스트를 만들 것(같은 코퍼스에서 인덱서 대 LSP 또는 컴파일러 백엔드, 정밀도와 재현율을 따로). 합성 입력만으로는 부족.
10. 테스트는 (a) 기능별 작은 픽스처의 스냅샷, (b) 인라인 단언(Kythe verifier식), (c) 실제 크레이트 코퍼스의 세 층으로. 출력은 결정적이어야 함.
11. 다국어 병합에는 공식 규약이 없으므로 인덱서별로 읽고 내부에서 심볼을 정규화해 합치는 계층을 설계. 공통 `project_root` 선택과 로컬 심볼의 문서 범위 처리를 명시.
12. 실험으로 확인할 것: (a) `scip` 0.10.0 생성 코드의 typed range 필드 존재와 rust-analyzer의 옛 범위 필드 처리, (b) 대상 코퍼스에서 rust-analyzer 정의 `enclosing_range` 적용 비율(참조에는 없음), (c) TS 모노레포(pnpm, 패키지별 `paths`, 프로젝트 레퍼런스)에서 심볼 패키지명과 크로스 패키지 해석, (d) Go 빌드 태그·테스트 파일이 심볼 중복을 만드는지, (e) SvelteKit `$lib`가 `svelte-kit sync` 후 scip-typescript에서 풀리는지.
피할 것: tree-sitter만으로 정확한 호출 그래프를 약속하는 설계, stack graphs식 언어별 규칙 DSL 직접 유지(아카이브 사유는 미확인, 커뮤니티는 규칙 유지 비용 지목), 사전 계산 인덱스가 라이브 서버와 일치한다는 가정(CodeNib 일치율 86%).

**라이선스 노트** (법률 자문이 아님. 확인한 사실과 일반 관행을 구분함)
- `scip.proto`와 저장소: https://raw.githubusercontent.com/scip-code/scip/main/LICENSE 는 Apache-2.0, 저작권자는 명시되지 않음. https://scip-code.org/governance.html 은 "© 2026 The SCIP Authors | Apache-2.0", CLA 없음(기여자가 해당 라이선스로 제출할 권리를 확인하는 방식), DCO와 상표 소유는 페이지에 없음. 후원 주도형 지배구조에서 Sourcegraph가 위원을 임명할 수 있고 운영위원은 Meta, Uber, Sourcegraph 소속(https://sourcegraph.com/blog/the-future-of-scip, 2026-03-25).
- Rust `scip` 크레이트: docs.rs에 Apache-2.0. `protobuf = "=3.7.2"` 의존이며 crates.io 데이터상 3.x는 MIT, 최신 4.x는 BSD-3-Clause(별도 크레이트 세대이므로 섞지 않도록 주의). `cargo deny`로 의존 트리 확인 권장. Unverified: 3.7.2의 LICENSE 파일 자체는 열지 않음.
- 서브프로세스 인덱서: rust-analyzer는 MIT와 Apache-2.0 이중(https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/README.md). scip-typescript(https://raw.githubusercontent.com/sourcegraph/scip-typescript/main/LICENSE)와 scip-go(https://github.com/scip-code/scip-go/blob/main/LICENSE, 이전 URL은 sourcegraph/scip-go)는 Apache-2.0. 별도 프로세스로만 실행하면 일반적으로 엔진 라이선스에 전염되지 않음(일반 지식 기반, 출처 미확인). 인덱서 바이너리를 같이 배포할 때만 각 라이선스의 고지 의무가 생김. 번들 배포 요구사항은 이 조사에서 확인하지 않음.
- `scip.proto` 또는 생성 바인딩 벤더링(일반 지식 기반, 출처 미확인): Apache-2.0 파일은 자기 라이선스를 유지하므로 해당 파일에 라이선스·저작권 고지를 남기고 수정했다면 변경 사실을 표시. Apache-2.0 텍스트 사본을 포함하고 원 저장소에 NOTICE 파일이 있으면 그 내용도 유지. 이 저장소에 NOTICE가 있는지는 확인하지 못함. MIT 또는 MIT/Apache-2.0 이중 프로젝트에 포함하는 것은 허용 범위지만 해당 파일은 Apache-2.0으로 표시해 두는 편이 깔끔. 헤더는 저장소 것을 그대로 따를 것("The SCIP Authors" 표기, CLA 부재). 가장 단순한 길은 vendoring 대신 `scip` 크레이트를 의존성으로 쓰는 것.
