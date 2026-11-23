# Compiler-backed call evidence for Rust — trial record (2026-11-17)

A local feasibility trial, not a literature survey: four routes to call relations that a
SCIP index cannot give (monomorphised direct calls, and which concrete types become
trait objects) were run on a private Rust workspace of about 75k lines and three crates
(CLI, core, server) with its toolchain pinned to 1.99.1. Crate and type names from that
workspace are replaced with generic ones. From the index alone, 134 trait-impl methods
had no caller; the trial asks how many of them each route explains. `Unverified:` marks
statements that were neither run nor read in a source.

The route recommended here, textual LLVM IR, is re-examined against the literature in
call-relations.md.

rustc 기반 exact mode 시험 결과: 고정 툴체인 1.99.1에서 안정 플래그 `--emit=llvm-ir`만으로 호출 그래프와 trait 객체화 집합을 얻었고, 호출자가 없던 impl 메서드 134개 중 102개에 호출자가 생겼습니다. 이 경로를 추천합니다.


### 실행한 명령과 핵심 출력

#### 1) `-Zprint-mono-items`
- 명령: `RUSTC_BOOTSTRAP=1 cargo rustc -p <server-crate> --lib -- -Zprint-mono-items=yes`. nightly 없이 1.99.1에서 동작합니다.
  - 출력 줄 수: core crate 695줄 3.4s, server crate lib 64,981줄 13.8s.
- 테스트 포함 전체: 작은 wrapper 스크립트와 `RUSTC_WORKSPACE_WRAPPER`로 `cargo build --workspace --all-targets`를 돌렸습니다. 48s, 74개 파일, fn 항목 411,045개.
- 호출자가 없던 134개 중 127개가 어딘가에서 인스턴스화됩니다.
  - Drop 36개는 모두 `std::ptr::drop_glue::<T> - shim(Some(T))`가 있습니다.
  - 7개는 어떤 빌드에도 없습니다(`Write::flush` 구현 2개, `From` 구현 1개, 테스트용 `AsyncWrite` 구현의 poll_flush/poll_shutdown 등).

#### 2) cga
- 툴체인: nightly-2026-10-18과 rustc-dev가 필요합니다. 빌드는 6s였습니다.
- 명령: `cargo-cga cga extract -p <server-crate>`. 1m45s 걸렸습니다(-Zbuild-std, 의존성 다운로드 포함).
- 출력: 304MB JSON, 노드 168,637개, 간선 442,971개.
  - 간선 종류: Called 398,672, VTableCalled 25,556, FnPtrCast 710.
  - 간선마다 호출 위치(파일, 줄, 열)가 있고, drop_glue 노드도 들어 있습니다.
- server crate의 미호출 114개에 대입한 결과:

| 분류 | 개수 |
|---|---|
| 그래프에 없음 | 43 |
| dyn(모든 impl)으로만 연결 | 36 |
| drop glue, 직접 호출, fn-ptr 중 하나로 연결 | 35 |

- 한계 1: dyn 해석이 객체화 집합이 아니라 "trait의 모든 비제네릭 impl"입니다. ext/cga/cga_analysis/src/dyn_object.rs:18의 주석이 "assume all impls of that trait are called"이고, 제네릭 impl은 걸러냅니다. 그래서 제네릭 래퍼 타입의 impl(`Wrapper<T>` 형태)과 `impl<S> Layer<S> for T` 형태가 빠집니다.
- 한계 2: `tokio::runtime::task::raw::vtable::<..>` 노드 240개 모두 나가는 간선이 0이고, Harness::poll 노드도 0개입니다. 상수 테이블 안의 fn 포인터를 따라가지 않아서, spawn된 future가 생성한 쪽과 끊겨 있습니다.

#### 3) charon
- 툴체인: nightly-2026-09-17과 rustc-dev가 필요합니다. 빌드 전에 `git submodule update --init`이 필요했고, 빌드는 38s였습니다.
- 명령: `charon cargo --hide-marker-traits`. core crate 8.7s, server crate lib 31s, 출력 294MB LLBC.
- 경고 3,128개가 나왔고, 대부분 "Coroutines are not supported"입니다.
  - 로컬 함수 6,279개 중 본문이 Error인 것이 565개로, async 본문이 빠집니다.
- vtable은 unsizing coercion을 본 impl만 번역합니다. 로컬 dyn-compatible impl 2,247개 중 49개(대부분 Debug)였습니다.
  - 단 단형화 전 분석이라, 의존성 제네릭 안에서 일어나는 coercion을 놓칩니다(예: `BoxBody::new`). Body impl은 0개로 나왔습니다.
  - `--monomorphize` 도움말에 "doesn't currently support `dyn Trait`"라고 되어 있습니다.

#### 4) LLVM IR (새로 시도한 경로)
- 명령: `cargo rustc -p <server-crate> --lib -- --emit=llvm-ir,link`. 1.99.1 그대로이고, `RUSTC_BOOTSTRAP`도 rustc-dev도 쓰지 않았습니다.
  - 시간과 크기: 28.8s, 555MB .ll 파일.
  - bin, lib test와 나머지 두 crate에도 같은 명령을 돌렸습니다.
- 파서는 Python 스크립트(약 100줄, 파일당 8s)이고, 이름 복원은 v0 심볼을 `llvm-cxxfilt`로 했습니다.
- server crate lib 결과:
  - 함수 58,763개.
  - 간선: call 289,216, vtable 7,391, ref 6,749, 상수 테이블 1,338.
  - invoke와 주석 줄을 바로잡은 뒤에는 모든 호출 간선에 DILocation 파일과 줄이 붙었습니다.
- vtable 1,056개 전부에 debuginfo 이름이 있습니다. 예: `<MapErr<Adapter<workspace::BodyWrapper<Incoming>>,..> as http_body::Body>::{vtable}`. 이 vtable을 만드는 곳은 box_body.rs:31 `BoxBody::new::<..>`입니다.
  - lib과 bin을 합치면 vtable 1,039개입니다. 이 중 로컬 타입이 바깥쪽 Self인 것은 27개이고, 로컬 타입이 어딘가에 들어간 것(클로저, async 블록, 래퍼 포함)은 591개입니다.
- 미호출 메서드 대입 결과. 호출자는 의존성 제네릭, drop glue, vtable, 상수 테이블을 거슬러 올라가 처음 만나는 로컬 함수로 잡았습니다.

| 대상 | 호출자 생김 | 나머지 |
|---|---|---|
| server crate 114개 (lib+bin+unit test) | 93 | 루트로만 생성되고 사용처 없음 16(대부분 수동 Debug), 미인스턴스 5 |
| server crate 114개 (lib+bin만) | 69 | |
| CLI crate 10개 | 8 | |
| core crate 1개 | 1 | |
| tests/ 9개 | 분석 안 함 | |

  - 예: 테스트용 타입의 `poll_write`는 제네릭 래퍼의 `<Wrapper<T> as AsyncWrite>::poll_write`가 호출자로 잡히고, 오류 타입의 `Display::fmt`는 anyhow ErrorVTable 상수 테이블을 거쳐 `Into<anyhow::Error>`로 이어집니다.

#### 5) cargo-call-stack
README만 읽었고 빌드하지 않았습니다. `-Z stack-sizes`와 fat LTO가 필요하고, 마지막으로 시험된 nightly가 2023-11-13이며, 임베디드(Cortex-M) 대상입니다. 간접 호출은 "will likely be missing edges or contain incorrect edges"라고 적혀 있습니다.
https://github.com/japaric/cargo-call-stack

### 질문별 답

**Q1. 기존 도구**
cga는 단형화 후 호출 그래프와 drop glue를 주지만, dyn을 과대 추정하고 tokio 태스크 테이블이 끊기며 자체 nightly가 필요합니다. charon은 async를 지원하지 않고 단형화 전 분석이라 이 프로젝트에 맞지 않습니다. cargo-call-stack은 대상이 아닙니다.

**Q2. print-mono-items**
얻을 수 있는 것:
- impl 메서드가 인스턴스화되는지 여부
- 구체 제네릭 인자
- `drop_glue::<T>`(drop glue)
- `shim(vtable)`. 다만 이것은 FnOnce 클로저 shim 227개뿐입니다.

얻을 수 없는 것:
- 누가 누구를 호출하는지
- 어떤 타입이 객체화되는지(vtable 항목은 일반 fn으로 섞여 나옵니다)
- 실제로 쓰이는지 여부. lib에서는 `is_reachable_non_generic` 항목이 루트가 되므로, 목록에 있다고 쓰인다는 뜻이 아닙니다. 1.99.1 collector.rs:1766에서 확인했습니다.

내부 UsageMap(collector.rs:260)과 Unsize 처리(collector.rs:743)는 출력되지 않습니다.
https://raw.githubusercontent.com/rust-lang/rust/1.99.1/compiler/rustc_monomorphize/src/collector.rs

**Q3. 안정성**
- print-mono-items: -Z 플래그라 출력 형식이 보장되지 않습니다. 1.99.1에서는 drop glue가 `std::ptr::drop_glue::<T>`로 나옵니다.
  - Unverified: 예전 버전은 `drop_in_place::<T>`였습니다.
- cga와 charon: 특정 nightly의 rustc_middle에 묶여 있습니다. 툴체인을 올릴 때마다 포팅이 필요하고, 프로젝트의 고정 툴체인을 쓰지 않습니다.
- rustc_public(1.99.1): 0.1.0-preview이고 아직 crates.io에 없습니다. rustc-dev와 rustc_private로 씁니다.
  - 지원하는 것: `TerminatorKind::Call`/`Drop`, `PointerCoercion::Unsize`, `Instance::resolve`/`body()`/`resolve_drop_in_place`, `InstanceKind::Virtual`, `GlobalAlloc::VTable(Ty, trait)`. 호출 terminator와 unsizing cast는 지금도 다룹니다.
  - 없는 것: mono collector. 도달성 탐색은 직접 구현해야 합니다.
  - https://raw.githubusercontent.com/rust-lang/rust/1.99.1/compiler/rustc_public/README.md
  - https://raw.githubusercontent.com/rust-lang/rust/1.99.1/compiler/rustc_public/src/mir/body.rs
  - https://raw.githubusercontent.com/rust-lang/rust/1.99.1/compiler/rustc_public/src/mir/alloc.rs
  - rustc-dev는 1.99.1용 x86_64 컴포넌트로 제공됩니다(`rustup component list --toolchain 1.99.1`). 설치하지는 않았습니다.
- LLVM IR: `--emit=llvm-ir` 자체는 안정 플래그입니다. 의존하는 부분은 IR 텍스트 형식, v0 디맹글 이름, debuginfo의 `<T as Trait>::{vtable}` 이름(1.99.1 type_names.rs:520 `compute_debuginfo_vtable_name`)입니다.
  - Unverified: 이 형식들은 버전 간에 거의 바뀌지 않습니다. 툴체인을 올릴 때 fixture 하나로 확인하면 됩니다.

**Q4. 추천 (10줄 이내)**
1. exact mode는 지금 추가할 가치가 있고, 경로는 LLVM IR입니다. 고정 툴체인 그대로이고 rustc-dev, nightly, RUSTC_BOOTSTRAP이 모두 필요 없습니다.
2. 얻는 것: 미호출 134개 중 102개에 호출자와 호출 위치(파일, 줄)가 생깁니다. server crate만 보면 114개 중 93개입니다.
3. dyn은 vtable 생성 지점과 구체 타입 쌍으로 표시할 수 있습니다(server crate 1,039개). Body와 AsyncWrite는 래퍼 안에서 객체화된 뒤 정적 호출로 이어지는 것까지 보입니다.
4. 비용 1: debug 빌드에서 모든 워크스페이스 타깃을 IR로 내야 합니다. 약 1~2분, .ll 약 1.4GB, target 약 8GB입니다.
5. 비용 2: 파서 약 150줄과, 디맹글 이름을 SCIP 심볼에 맞추는 작업(가장 손이 많이 감)입니다.
6. 유지 비용: 툴체인을 올릴 때 IR, 디맹글, vtable 이름 형식을 fixture로 확인하는 정도입니다.
7. 한계: 간접 호출 자체는 간선이 아닙니다. 통합 테스트 9개는 이번에 다루지 않았습니다.
8. rustc_public 드라이버는 나중에, IR로 부족할 때 고려합니다(MIR 단위 span이 필요할 때). 툴체인마다 다시 빌드해야 하고 도달성 탐색을 직접 짜야 합니다.
9. cga와 charon은 채택하지 않습니다.
