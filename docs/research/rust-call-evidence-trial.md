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

On the pinned 1.99.1 toolchain, the rustc exact-mode trial used only the stable `--emit=llvm-ir` flag to recover a call graph and the set of concrete types converted to trait objects. It found callers for 102 of the 134 impl methods that previously had none. The trial recommends this route.


### Commands and key output

#### 1) `-Zprint-mono-items`
- Command: `RUSTC_BOOTSTRAP=1 cargo rustc -p <server-crate> --lib -- -Zprint-mono-items=yes`. It ran on 1.99.1 without nightly.
  - The core crate produced 695 lines in 3.4s. The server crate library produced 64,981 lines in 13.8s.
- For the full build including tests, the trial ran `cargo build --workspace --all-targets` with a small wrapper script and `RUSTC_WORKSPACE_WRAPPER`. In 48s it produced 74 files with 411,045 fn items.
- At least one build instantiated 127 of the 134 methods with no known caller.
  - All 36 Drop methods had `std::ptr::drop_glue::<T> - shim(Some(T))`.
  - No build contained the remaining 7, including 2 implementations of `Write::flush`, 1 of `From`, and poll_flush/poll_shutdown from a test implementation of `AsyncWrite`.

#### 2) cga
- The tool requires nightly-2026-10-18 and rustc-dev. It took 6s to build.
- Command: `cargo-cga cga extract -p <server-crate>`. It took 1m45s, including -Zbuild-std and dependency downloads.
- The 304MB JSON output contained 168,637 nodes and 442,971 edges.
  - Edge kinds included 398,672 Called, 25,556 VTableCalled and 710 FnPtrCast.
  - Each edge carried a call site (file, line and column). The graph also included drop_glue nodes.
- Results for the 114 server crate methods with no known caller:

| Classification | Count |
|---|---|
| Absent from the graph | 43 |
| Connected only through dyn (all impls) | 36 |
| Connected through drop glue, a direct call or a fn-ptr | 35 |

- Limitation 1: dyn resolution includes all non-generic impls of the trait, regardless of which types become trait objects. A comment in ext/cga/cga_analysis/src/dyn_object.rs:18 says "assume all impls of that trait are called". The tool filters out generic impls, omitting impls for generic wrappers such as `Wrapper<T>` and impls of the form `impl<S> Layer<S> for T`.
- Limitation 2: all 240 `tokio::runtime::task::raw::vtable::<..>` nodes had 0 outgoing edges, and the graph contained 0 Harness::poll nodes. The tool does not follow fn pointers in constant tables, leaving spawned futures disconnected from their creators.

#### 3) charon
- The tool requires nightly-2026-09-17 and rustc-dev. The build required `git submodule update --init` beforehand and took 38s.
- Command: `charon cargo --hide-marker-traits`. The core crate took 8.7s and the server crate library took 31s. Output occupied 294MB of LLBC.
- The run produced 3,128 warnings, mostly "Coroutines are not supported".
  - Of 6,279 local functions, 565 had an Error body. The async bodies were missing.
- Charon translates vtables only for impls where it sees an unsizing coercion. It translated 49 of 2,247 local dyn-compatible impls, mostly Debug.
  - The analysis runs before monomorphisation, so it misses coercions inside dependency generics such as `BoxBody::new`. It reported 0 Body impls.
  - The help for `--monomorphize` says it "doesn't currently support `dyn Trait`".

#### 4) LLVM IR (new route tested)
- Command: `cargo rustc -p <server-crate> --lib -- --emit=llvm-ir,link`. The trial used the pinned 1.99.1 toolchain without `RUSTC_BOOTSTRAP` or rustc-dev.
  - The run took 28.8s and produced a 555MB .ll file.
  - The trial ran the same command for the binary, the library test target and the two remaining crates.
- A Python script of about 100 lines parsed each file in 8s. It used `llvm-cxxfilt` to demangle v0 symbols.
- Results for the server crate library:
  - 58,763 functions.
  - Edges: 289,216 call, 7,391 vtable, 6,749 ref and 1,338 constant table.
  - After corrections to the handling of invoke and comment lines, every call edge carried a DILocation file and line.
- All 1,056 vtables had debuginfo names. One example was `<MapErr<Adapter<workspace::BodyWrapper<Incoming>>,..> as http_body::Body>::{vtable}`. Its creation site was box_body.rs:31 in `BoxBody::new::<..>`.
  - The library and binary together yielded 1,039 vtables. In 27, the outermost Self was a local type. In 591, a local type appeared somewhere in the type, including closures, async blocks and wrappers.
- For methods with no known caller, the trial traced backwards through dependency generics, drop glue, vtables and constant tables. It recorded the first local function it reached as the caller.

| Target | Caller found | Remaining |
|---|---|---|
| server crate, 114 (lib+bin+unit test) | 93 | 16 emitted only as roots with no use site (mostly manual Debug impls), 5 never instantiated |
| server crate, 114 (lib+bin only) | 69 | |
| CLI crate, 10 | 8 | |
| core crate, 1 | 1 | |
| tests/, 9 | Not analysed | |

  - For example, the caller of a test type's `poll_write` was the generic wrapper's `<Wrapper<T> as AsyncWrite>::poll_write`. An error type's `Display::fmt` traced back through the anyhow ErrorVTable constant table to `Into<anyhow::Error>`.

#### 5) cargo-call-stack
The trial examined only the README and did not build the tool. It requires `-Z stack-sizes` and fat LTO, was last tested on nightly 2023-11-13 and targets embedded systems (Cortex-M). The README says indirect calls "will likely be missing edges or contain incorrect edges".
https://github.com/japaric/cargo-call-stack

### Answers by question

**Q1. Existing tools**
cga provides a post-monomorphisation call graph and drop glue, but overestimates dyn targets, loses connections through tokio task tables and requires its own nightly. charon lacks async support and analyses code before monomorphisation, making it unsuitable for this project. cargo-call-stack targets a different environment.

**Q2. print-mono-items**
The output shows:
- Whether an impl method is instantiated
- Concrete generic arguments
- `drop_glue::<T>`(drop glue)
- `shim(vtable)`, limited to 227 FnOnce closure shims.

The output does not show:
- Which function calls which
- Which types become trait objects (vtable entries appear mixed in with ordinary fn items)
- Whether an item is actually used. In a library, `is_reachable_non_generic` items become roots, so listing an item does not establish use. The trial checked this in 1.99.1 collector.rs:1766.

The output omits the internal UsageMap (collector.rs:260) and Unsize handling (collector.rs:743).
https://raw.githubusercontent.com/rust-lang/rust/1.99.1/compiler/rustc_monomorphize/src/collector.rs

**Q3. Stability**
- print-mono-items is a -Z flag with no guaranteed output format. In 1.99.1, drop glue appears as `std::ptr::drop_glue::<T>`.
  - Unverified: Earlier versions used `drop_in_place::<T>`.
- cga and charon depend on rustc_middle from specific nightly versions. Each toolchain upgrade requires porting, and neither uses the project's pinned toolchain.
- rustc_public (1.99.1) is at 0.1.0-preview and is not yet on crates.io. It is used through rustc-dev and rustc_private.
  - Supported interfaces include `TerminatorKind::Call`/`Drop`, `PointerCoercion::Unsize`, `Instance::resolve`/`body()`/`resolve_drop_in_place`, `InstanceKind::Virtual` and `GlobalAlloc::VTable(Ty, trait)`. They already cover call terminators and unsizing casts.
  - There is no mono collector. Users must implement their own reachability traversal.
  - https://raw.githubusercontent.com/rust-lang/rust/1.99.1/compiler/rustc_public/README.md
  - https://raw.githubusercontent.com/rust-lang/rust/1.99.1/compiler/rustc_public/src/mir/body.rs
  - https://raw.githubusercontent.com/rust-lang/rust/1.99.1/compiler/rustc_public/src/mir/alloc.rs
  - rustc-dev is available as an x86_64 component for 1.99.1 (`rustup component list --toolchain 1.99.1`). The trial did not install it.
- LLVM IR: `--emit=llvm-ir` is a stable flag. The route depends on the textual IR format, demangled v0 names and the debuginfo naming convention `<T as Trait>::{vtable}` (1.99.1 type_names.rs:520, `compute_debuginfo_vtable_name`).
  - Unverified: These formats rarely change between versions. One fixture should suffice to check them on a toolchain upgrade.

**Q4. Recommendation (within 10 lines)**
1. The trial recommends adding exact mode now using LLVM IR. The pinned toolchain works without rustc-dev, nightly or RUSTC_BOOTSTRAP.
2. The route found callers and call sites (file and line) for 102 of the 134 methods with no known caller, including 93 of 114 in the server crate.
3. A dyn conversion can be represented by its vtable creation site and concrete type (1,039 in the server crate). For Body and AsyncWrite, the output shows the conversion to a trait object inside a wrapper and the subsequent static call.
4. Cost 1: all workspace targets must emit IR in a debug build. This takes about 1–2 minutes, with about 1.4GB of .ll files and about 8GB in target.
5. Cost 2: the parser needs about 150 lines. Matching demangled names to SCIP symbols takes the most work.
6. Maintenance cost: on each toolchain upgrade, check the IR, demangled names and vtable naming format with a fixture.
7. Limitations: an indirect call is not itself an edge. This trial did not cover the 9 integration tests.
8. Consider a rustc_public driver later if IR proves insufficient, for example when MIR-level spans are needed. The driver would require a rebuild per toolchain and a custom reachability traversal.
9. The recommendation excludes cga and charon.
