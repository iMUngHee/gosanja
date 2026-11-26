# Rust call evidence measurements

Observed 2026-11-24 on x86_64-unknown-linux-gnu. A scratch reader and the existing snapshot engine corroborated 49 of 65 eligible instructions in ryu and 1,736 of 1,969 in regex-syntax. Eligibility here means that both the instruction site and the definition of its direct callee are in project source, and that the calling LLVM module defines that callee. The unit is one emitted instruction. Two instantiations of a source function can contribute two observations. These are static dev builds. No workload was executed to count runtime calls.

## Inputs and commands

| Published input | Source commit | Library target |
| --- | --- | --- |
| ryu 1.0.23 | f0b52bb194befe6fd242154f2182fafd43a819b8 | ryu |
| regex-syntax 0.8.12 | 4226f214c56e584304520adb9080bcc4c792390e | regex_syntax |

Archive sources: https://static.crates.io/crates/ryu/ryu-1.0.23.crate and https://static.crates.io/crates/regex-syntax/regex-syntax-0.8.12.crate. Commit identifiers come from the archives' .cargo_vcs_info.json. The pinned ryu archive SHA-256 is 9774ba4a74de5f7b1c1451ed6cd5285a32eddb5cccb8cc655a4e50009e06477f.

The current measurements used default package features, locked dependencies, a locally populated Cargo cache, rustc 1.99.1 (58cf8f9c3), Cargo 1.99.1 (5f94df478), LLVM 23.1.1 and rust-analyzer 0.3.3049-standalone. For each extracted crate, the data collection used the following command forms with a fresh output directory outside the source tree.

```sh
export RUSTUP_TOOLCHAIN=1.99.1 RUSTUP_AUTO_INSTALL=0 CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR="$out/target"
rust-analyzer scip . --num-threads 2 --output "$out/$pkg.scip"
cargo rustc --locked --offline -p "$pkg" --lib -- \
    --emit=llvm-ir="$out/$pkg.ll" -Ccodegen-units=1 -Cdebuginfo=2 -Copt-level=0
```

The retained run checked each admitted source against the hash before indexing, the hash after compilation and the MD5 in the LLVM DIFile. It used unchanged sources. The production design builds before indexing and checks file identity across those operations.

## Corroboration by instruction

The local analysis commands were `python3 probe.py` and `python3 audit.py` in the scratch measurement directory. The scripts read the emitted IR, index and source and produced per-instruction decisions. The following are their counts after each successive check.

| Check | ryu | regex-syntax |
| --- | ---: | ---: |
| 1. Direct instruction with a debug site in project source | 314 | 5,612 |
| 2. The calling module defines its callee in project source | 65 | 1,969 |
| 3. The definition's name and line identify one indexed position | 65 | 1,904 |
| 4. The site's line has enough matching call references in one enclosing definition | 65 | 1,736 |
| 5. Both written definitions and the caller's source position agree | 49 | 1,736 |
| Coverage (step 5 / step 2) | 75.38% | 88.17% |

First exclusions, in instruction counts:

| Result | ryu | regex-syntax | Observation |
| --- | ---: | ---: | --- |
| LLVM intrinsic, before step 1 | 145 | 2,594 | These compiler operations are skipped before the program-call checks. |
| Site outside verified project files | 91 | 3,619 | Includes library code emitted for the project's type arguments. |
| Callee only declared in this module | 233 | 808 | For example, ryu src/d2s_intrinsics.rs:44 calls core's panic routine. |
| Callee defined here, debug file outside verified project source | 16 | 2,835 | Includes a slice method instantiated in ryu. |
| No unique indexed callee at compiled name and line | 0 | 65 | regex-syntax src/error.rs:174 calls clone. |
| No qualifying reference to the resolved symbol on site line | 0 | 168 | regex-syntax src/ast/visitor.rs:339 calls a visitor method. |
| Endpoint under an excluded attribute | 16 | 0 | Eight calls each from format32 and format64 carry the conditional no_panic payload through their caller. |

For the separate declared-only measurement, the scratch probe grouped step-1 instructions by the declared linkage name and demangled v0 names using rustc-demangle 0.1.28. The name test examines the outer crate of the path or of the implementation's self type or trait, ignoring generic arguments. Its result counts matching names. The source origin of the remaining declarations was not classified.

```text
python3 public_record.py     # record declared symbols per project-site instruction
                       only declared    workspace crate name matches
ryu                              233                               0
regex-syntax                     808                               0
```

### Source positions and snapshot replay

The range probe took the typed SCIP range when present and otherwise the legacy range, in the document's UTF-8 byte unit. Example output below gives zero-based half-open positions. The index supplies a function or method's extent including its body. A closure expression inside make_adder has an enclosing function anchor at make_adder. The returned closure's own callable body has no indexed function definition at the compiled position. A written nested function such as imp does have a separate definition and extent.

```text
python3 public_record.py
file                    name         name range         enclosing range
calls-lib/src/lib.rs    leaf         11:7–11:11         11:0–13:1
calls-lib/src/lib.rs    make_adder   36:7–36:17         36:0–41:1
src/ast/parse.rs        bump         495:7–495:11       492:4–509:5
src/unicode.rs          imp          442:7–442:10       441:4–445:5
```

Fourteen otherwise excluded instructions in src/unicode.rs became attributable when the reader looked up the compiler's definition by position. For example, the imp() called in perl_digit on line 448 resolves to the imp definition on line 443. perl_space calls another imp on line 424, defined on line 419. These positions use one-based lines. Both helpers have the shared base symbol. The fourteen parents were perl_word, perl_space, perl_digit, is_word_character, canonical_prop, property_values, ages, gencat, script, script_extension, bool_property, gcb, wb and sb.

The scratch adapter replay used the original SCIP and wrote each accepted instruction with caller and callee name-token columns converted to UTF-32. The engine's published evidence arrays were counted after resolution. A sample of 30 rows from each public crate was checked against the call text, source definitions and demangled compiler names. Sorting by site, endpoints and emitted function, the sample used indices floor(i × (N − 1) / 29) for i from 0 to 29. The 14 recovered calls were checked individually. No wrong source endpoint was identified in those checks.

```sh
gosanja index --adapter "$relay" --root "$crate" --out "$snapshot"
jq -s '[.[] | select(.src=="adapter:measure/calls") | .evidence[]] | length' "$snapshot/relations.jsonl"
jq -s '[.[] | select(.gap=="unanchored" and .layer=="calls")] | length' "$snapshot/gaps.jsonl"
```

| Snapshot replay | Evidence entries | Unanchored calls |
| --- | ---: | ---: |
| ryu | 49 | 0 |
| regex-syntax | 1,736 | 0 |

## Compiler controls

The explicit emit command was exercised on a synthetic workspace with a dual rlib/cdylib library, a binary and a procedural macro. Only the selected target receives the arguments after Cargo's double dash. In separate invocations, each requested path held one module. The binary also linked and printed 9. Cargo's reported linked artifacts did not list the separately requested IR.

```sh
selector=(--lib)  # For the binary, use selector=(--bin driver).
cargo rustc --locked --offline --message-format=json -p "$pkg" "${selector[@]}" -- \
    --emit=llvm-ir="$out/work/calls/$n.ll" -C codegen-units=1 -C debuginfo=2 -C opt-level=0
```

```text
selected target     requested IR files present    link output
rlib + cdylib                    1                .rlib and .so
binary                           1                executable
proc-macro                       1                .so
explicit path containing out,comma/work/explicit.ll:
exit 101; unknown emission type: `comma/work/explicit.ll`
```

As a control, bare --emit=llvm-ir created one new non-incremental .ll per invocation on a fresh build directory, including same-name bin and lib targets. Repeating those five successful invocations in the same directory produced zero new paths. The design selects an explicit path and treats a comma in that output path as unsupported.

Reading `target triple`, `DICompileUnit`, `DIFile` and `DILocation` from the synthetic modules gave one compile unit per module, producer `clang LLVM (rustc version 1.99.1 (58cf8f9c3 2026-10-13))` and target `x86_64-unknown-linux-gnu`. The source MD5 in each of eight checked modules matched its source bytes. A deliberate path-remapping control changed the directory while preserving the content checksum. At a call after the text 界😀, the observed one-based LLVM column was 59 (UTF-8 bytes), versus 55 in UTF-16 units and 54 in Unicode scalars. Its zero-based UTF-32 snapshot column was 53. Cargo profile overrides combined with the final explicit opt-level=0 yielded isOptimized: false. A compiler wrapper appending opt-level=3 yielded true.

```sh
rg 'target triple|distinct !DICompileUnit|CSK_MD5|DILocation\(line: 23, column: 59' "$module"
```

In the generic-name check, rustc-demangle rendered the function as typ::<u32> and the method as <Holder<i16>>::act with the crate path included. The subprogram's own name can include trailing generic arguments. The written item is the function or method at its source position. Multiple instantiations contribute separate direct-call observations.

To check the inert-attribute spelling, a proc-macro crate exported an attribute named marker which appends a distinct function to its input. The consumer contained `use mac::marker as inline;`, then `#[inline]` on a function. The compiler rejected the attribute at its use.

```sh
rustc +1.99.1 --edition 2021 --crate-type=proc-macro mac.rs -o libmac.so
rustc +1.99.1 --edition 2021 --crate-type=lib --extern mac=libmac.so consumer.rs
```

```text
error[E0659]: `inline` is ambiguous
note: ambiguous because of a name conflict with a builtin attribute
```

## Earlier callee-anchor experiment

The earlier study compared the compiler's instruction position to the SCIP reference token, before adopting the definition's position as the callee anchor. It used rustc 1.100.0 for regex-syntax and 1.99.1 for ryu. The scratch reference probe read the direct-edge and SCIP files alongside a base-only snapshot from `gosanja index --scip "$index" --root "$crate" --out "$base"`.

| Result | regex-syntax | ryu |
| --- | ---: | ---: |
| Direct observations with a retained project line and column | 6,069 | 314 |
| One base reference target starts there | 3,909 | 125 |
| Target agrees with the compiler callee | 1,459 | 63 |
| Agrees after moving a qualified-path start to its terminal identifier | 1,863 | 65 |
| Unique raw reference targets that explicitly disagree | 542 | 2 |

In src/hir/mod.rs in regex-syntax the instruction location could identify the start of a qualified path, where SCIP referenced the core module, while the compiler called core::str::converts::from_utf8. Even the token adjustment recovered only 118 of 3,808 observations without a unique definition-anchored callee in regex-syntax and 0 of 319 in ryu. The reference check was retained as corroboration on the call line. The endpoint uses the compiled definition's position.

A separate regex-syntax trial on **rustc 1.100.0** submitted 2,094 definition-anchored observations to the engine. It joined 1,909 observations into 1,184 call relations and left 185 observations unanchored. Those numbers describe that earlier extraction and use a different denominator from the pinned-toolchain coverage above.

## Measurement limits

The public-crate runs above used a scratch reader and a relay for the existing engine. They do not measure finished-adapter wall time, peak memory, IR footprint, or its final omission codes; for ryu these are in "Finished adapter on ryu 1.0.23, 2026-11-25" below. The final-fixture multiset check depends on the frozen fixture and its independently recorded source expectations. Windows, macOS and runtime call frequency were outside these trials.

## Frozen fixture comparison, 2026-11-25

The fixture and its source-and-index expectations are from commit `159d999`.
A fresh Linux x86-64 run copied the fixture to a temporary directory, used an
empty Cargo home and built four targets with Rust 1.99.1. For each target, from
the fixture root, the command was:

```sh
CARGO_HOME="$work/home" CARGO_TARGET_DIR="$work/target" cargo +1.99.1 rustc \
  --locked --offline -p "$package" "${target[@]}" -- \
  -Cdebuginfo=2 -Ccodegen-units=1 -Copt-level=0 "--emit=llvm-ir=$work/$name.ll"
```

The targets were the libraries of calls-lib and calls-derive and the calls-app
and calls-twin binaries of calls-app. Inspection of their raw call and invoke
instructions with a named target or register and a nonzero project debug line
gave 183, 251, 7 and 1 observations respectively, excluding LLVM intrinsics.
The expected multiset combined the first part's permitted endpoints with the
actual targets, sites and copies found in these modules, before comparing the
scratch reader. Positions in the comparison include both endpoints, the site
in character columns and the emitted target. The engine then indexed the same
SCIP and the scratch reader's rows using a relay adapter.

```text
$ python inspect.py && python expected.py
relations 83 matched instructions 82
$ python probe.py
fixture 167 eligible instructions, 83 published
$ gosanja index --adapter "$work/relay" --root "$work/source" --out "$work/snapshot"
$ python compare.py
expected=82 reader=83 expected-only=1 reader-only=2
unanchored 0
```

The 83 in the first output line counts candidate source sites, including
multiple sites for one relation. The three differences are listed below.
Positions are one-based throughout.

| Result | Site | Endpoints and evidence |
| --- | --- | --- |
| Reader only | calls-lib/src/drops.rs:77:5 | `drop_lookalike` calls the instance of the lookalike `drop` defined at fake-core/src/lib.rs:4. Its definition is in calls-lib.ll alongside another crate's identically displayed `core::mem::drop`; their v0 linkage names differ. |
| Reader only | calls-lib/src/lib.rs:45:5 | `use_adder` calls `make_adder` at line 37. The IR also calls the returned closure body. The scratch reader published only the call that constructs the closure. |
| Expected only | calls-lib/src/lib.rs:604:76 | Part one permits the outer `run` at line 601 to call `leaf`. The index gives two written calls to that symbol on line 604, at byte columns 48 and 83, with different innermost enclosing functions. The reference check rejects both instructions before its site-position check. |

The fixture's note records three corrections after this first comparison.
The updated source table allows the lookalike drop and the closure constructor
and withholds both calls on line 604. The compiler observations stayed fixed.
Repeating the comparison from the corrected source table gave:

```text
$ python corrected.py
relations 84 matched instructions 83
$ python compare.py
expected=83 reader=83 expected-only=0 reader-only=0
unanchored 0
```

The 84 candidate sites cover 80 possible relations and 86 possible evidence
entries before checking the emitted code. Part two of the fixture note lists
the observed positions, multiplicities and targets. Equality here counts
evidence entries, including repeated sites in different emitted functions.

## Which target emits a generic instance

On 2026-11-25, rustc 1.99.1 (58cf8f9c3 2026-10-13) on x86_64-unknown-linux-gnu
compiled an rlib and its consumer in separate invocations. The library uses the
generic function at `u32`. The binary calls it at both `u32` and `u64`.

```rust
// sample.rs
#[inline(never)]
pub fn generic<T>(x: T) -> T { x }
pub fn shared(x: u32) -> u32 { generic(x) }
```

```rust
// app.rs
fn main() {
    std::hint::black_box(sample::generic(11_u32));
    std::hint::black_box(sample::generic(12_u64));
}
```

```sh
rustc +1.99.1 --edition=2024 --crate-type=rlib --crate-name sample \
  -Copt-level=0 -Cdebuginfo=2 -Ccodegen-units=1 --emit=link,llvm-ir sample.rs
rustc +1.99.1 --edition=2024 --extern sample=libsample.rlib \
  -Copt-level=0 -Cdebuginfo=2 -Ccodegen-units=1 --emit=llvm-ir app.rs
awk '/^; (call )?sample::generic/ { name=$NF; pending=1; next }
     pending && !/^;/ { print FILENAME, ($1=="define" || $1=="declare") ? $1 : "call", name; pending=0 }' sample.ll app.ll
```

```text
sample.ll define sample::generic::<u32>
sample.ll call sample::generic::<u32>
app.ll define sample::generic::<u64>
app.ll call sample::generic::<u32>
app.ll call sample::generic::<u64>
app.ll declare sample::generic::<u32>
```

Both calls survive in the binary's IR. Only the `u64` call has a callee definition
in that module. The binary reuses the library's `u32` instance. The calls layer
can therefore consider the binary's `u64` call for publication while the
binary's `u32` call is declared-only in this build.

## Two Rust targets with one LLVM triple

On the same host and compiler, an empty library with `no_core` let rustc emit a
real LLVM module for each target without installing either target's standard
library. This probe used `RUSTC_BOOTSTRAP=1` for `no_core`. Neither cross build
invoked a linker. The separate `--print cfg` call reads the Rust target features.

```sh
printf '%s\n' '#![feature(no_core)]' '#![no_core]' >empty.rs
for target in riscv32imc-unknown-none-elf riscv32imac-unknown-none-elf; do
  RUSTC_BOOTSTRAP=1 rustc +1.99.1 --crate-type lib --target "$target" \
    --emit llvm-ir empty.rs -o "$target.ll"
  rg '^target triple' "$target.ll"
  rustc +1.99.1 --print cfg --target "$target" | rg '^target_feature'
done
```

```text
target triple = "riscv32"
target_feature="c"
target_feature="m"
target_feature="zca"
target triple = "riscv32"
target_feature="a"
target_feature="c"
target_feature="m"
target_feature="zaamo"
target_feature="zalrsc"
target_feature="zca"
```

A Rust source branch guarded by `cfg(target_feature = "a")` can differ between
these builds. The calls-layer context records the module triple and Cargo
features. With the same package target ID, compiler, profile and Cargo features,
the two contexts compare equal.

## Finished adapter on ryu 1.0.23, 2026-11-25

These are measurements of the production reader: the calls layer of `gosanja-adapter-rust`
built at commit `5c5b057` (the working tree's only other changes to its sources were unit
tests under `#[cfg(test)]`, which the binary does not contain), run by the ignored test `a_public_crate_keeps_its_counts`.
That test was in `crates/adapter-rust/tests/live.rs` at the measured commit.
`scripts/check-live.sh` runs it from [adapters/rust/tests/live.rs](../../adapters/rust/tests/live.rs).
The preceding sections measure a scratch reader.

Input: the published archive `ryu-1.0.23.crate`, taken from the local Cargo cache, with
SHA-256 `9774ba4a74de5f7b1c1451ed6cd5285a32eddb5cccb8cc655a4e50009e06477f`. The test checks
that digest. Its `.cargo_vcs_info.json` names commit f0b52bb194befe6fd242154f2182fafd43a819b8.
The crate is unpacked under `/var/tmp` and built with the lock file the archive carries.

Host and settings:
- `x86_64-unknown-linux-gnu`, an AMD Ryzen 7 9850X3D with 16 threads and 30 GiB of memory;
- rustc 1.99.1 (58cf8f9c3 2026-10-13), Cargo 1.99.1 (5f94df478 2026-08-27) and
  rust-analyzer 0.3.3049-standalone;
- the toolchain named in `rust-toolchain.toml`, through `RUSTUP_TOOLCHAIN`;
- a Cargo home made for the test, holding no configuration, only the index entries and
  archives that the local cache has of the registry packages in the crate's lock file. That is
  67 of 86. The other 19 are the wasi and wasm tooling and its dependencies, `hermit-abi` and
  `no-panic`, and the build needs none of them;
- Cargo offline (`CARGO_NET_OFFLINE=true`);
- of the caller's environment, only `PATH` and `HOME`.

Before anything is compared, the test checks every place Cargo reads configuration from:
`.cargo/config` and `.cargo/config.toml` in the crate's resolved directory and every
directory above it up to `/`, and `config` and `config.toml` in the private Cargo home. A
control places `.cargo/config.toml` one directory above the crate and requires that the check
names it. On this run the control was found and no other file existed.

~~~sh
scripts/check-live.sh
cargo test --locked -p gosanja-adapter-rust --test live -- --ignored --test-threads=1 \
    --nocapture a_public_crate_keeps_its_counts
~~~

~~~text
ryu-1.0.23: archives copied 67, absent from the local cache 19
builds [{"features":[],"id":"ryu-lib-ryu","profile":"dev","target":"x86_64-unknown-linux-gnu","toolchain":"rustc 1.99.1 (58cf8f9c3 2026-10-13)"}]
rows 49
skipped [("src/pretty/mod.rs", "uncorroborated")]
call relations 22 with 49 evidence entries
gaps {"opaque-region": 414, "open-call": 4, "skipped-file uncorroborated": 1, "symbol-collision": 18}
adapter wall time 7.0 s, peak resident memory 14076 KiB, IR 749775 bytes
~~~

| Measure | Value |
| --- | --- |
| Targets built | one, the library (`ryu-lib-ryu`); the crate has no binary, and its tests and example are not built |
| Rows of `calls.jsonl` | 49 |
| Call relations of the calls layer in the snapshot | 22, with 49 evidence entries |
| `skipped` | `src/pretty/mod.rs` `uncorroborated`; no other code |
| Gaps of the snapshot | `opaque-region` 414, `symbol-collision` 18, `open-call` 4, `skipped-file` 1 (the entry above) |
| `gosanja validate` on the snapshot | passes |
| Adapter wall time, whole run | 7.0, 7.8 and 7.9 s in three runs |
| IR read | one file of 749 775 bytes |
| Peak resident memory of the adapter process | 13 680 to 14 076 KiB in the same three runs |
| Disk under the adapter's output directory | 95 712 831 bytes (92 MiB), of which Cargo's build directory 94 336 145, the IR 749 879, the target directory 298 295, the index 531 319; the crate's own source is 183 093 bytes |

The rows and the `skipped` entries were the same in every run, and they equal the 49 of 65
that the scratch reader published for this crate above.

The wall time covers everything the adapter does: the calls build, rust-analyzer's index, the
syntax layer and the rows. It was measured in the test, from the start of the adapter process
to its exit. The memory is the high-water mark of the adapter's own process (`VmHWM` in
`/proc/<pid>/status`), read every 5 ms while it ran. It covers the reader and the rows, not
Cargo, rustc or rust-analyzer, which run as separate processes. An earlier run on the same
day, before the final rule fixes of `5c5b057`, took 12.6 s with the same counts.

The disk was measured once more with the same adapter, crate, private Cargo home and
environment, by running the adapter by hand and measuring its output directory before
anything was deleted. The run gave the same 49 rows.

~~~sh
gosanja-adapter-rust index --root "$crate" --out "$out"
du -sb "$out" "$out/work" "$out/work/build" "$out/work/target" "$out/work/calls" \
    "$out/work/tmp" "$out/index.scip" "$crate"
~~~

~~~text
95712831  out
95086769  out/work
94336145  out/work/build
298295    out/work/target
749879    out/work/calls
0         out/work/tmp
531319    out/index.scip
183093    ryu-1.0.23
~~~

Nearly all of it is Cargo's intermediate build directory, `work/build`, which the adapter
places in its output directory with everything else it writes.

### The thirty sampled rows

`calls.jsonl` is sorted by the bytes of its rows. The sample takes the rows at indices
floor(i × (N − 1) / 29) for i from 0 to 29, with N = 49, which are thirty distinct rows. The
test holds them as literals and requires each in a fresh run's `calls.jsonl`.

Each was checked by hand against the source of the archive, unpacked separately:
1. The caller's anchor is the name token of a written `fn`, and the site's line lies inside
   that function's extent from its attribute to its closing brace.
2. The callee's anchor is the name token of a `fn` of that name, and the crate defines no
   other function of that name.
3. At the site's column, the text is the callee's name followed by `(`.
4. The name is in scope at the site, either through a `use` with no `cfg` in the calling file
   or because both functions are in one module.

Every row passed all four checks. Lines and columns are one-based. The callers' extents are:

| Caller | Lines |
| --- | --- |
| `d2d` | 90-302 |
| `f2d` | 39-177 |
| `multiple_of_power_of_5` | 55-59 |
| `mul_shift_all_64` | 77-89 |
| `multiple_of_power_of_5_32` | 43-46 |
| `write_mantissa_long` | 4-25 |

`d2s.rs` imports its callees from `common` and `d2s_intrinsics`, and `f2s.rs` from `common`
and `f2s_intrinsics`, without a `cfg` on either import.

| # | Caller | Callee | Site | Checks 1-4 |
| --- | --- | --- | --- | --- |
| 1 | `d2d` src/d2s.rs:91 | `pow5bits` src/common.rs:64 | src/d2s.rs:129:44 | yes |
| 2 | `d2d` src/d2s.rs:91 | `pow5bits` src/common.rs:64 | src/d2s.rs:171:17 | yes |
| 3 | `d2d` src/d2s.rs:91 | `log10_pow5` src/common.rs:90 | src/d2s.rs:168:17 | yes |
| 4 | `d2d` src/d2s.rs:91 | `div5` src/d2s_intrinsics.rs:24 | src/d2s.rs:153:70 | yes |
| 5 | `d2d` src/d2s.rs:91 | `div10` src/d2s_intrinsics.rs:29 | src/d2s.rs:220:28 | yes |
| 6 | `d2d` src/d2s.rs:91 | `div10` src/d2s_intrinsics.rs:29 | src/d2s.rs:237:32 | yes |
| 7 | `d2d` src/d2s.rs:91 | `div10` src/d2s_intrinsics.rs:29 | src/d2s.rs:242:32 | yes |
| 8 | `d2d` src/d2s.rs:91 | `div10` src/d2s_intrinsics.rs:29 | src/d2s.rs:280:28 | yes |
| 9 | `d2d` src/d2s.rs:91 | `div10` src/d2s_intrinsics.rs:29 | src/d2s.rs:285:28 | yes |
| 10 | `d2d` src/d2s.rs:91 | `div100` src/d2s_intrinsics.rs:34 | src/d2s.rs:263:25 | yes |
| 11 | `d2d` src/d2s.rs:91 | `div100` src/d2s_intrinsics.rs:34 | src/d2s.rs:267:29 | yes |
| 12 | `d2d` src/d2s.rs:91 | `multiple_of_power_of_5` src/d2s_intrinsics.rs:56 | src/d2s.rs:160:40 | yes |
| 13 | `d2d` src/d2s.rs:91 | `multiple_of_power_of_5` src/d2s_intrinsics.rs:56 | src/d2s.rs:163:23 | yes |
| 14 | `d2d` src/d2s.rs:91 | `mul_shift_all_64` src/d2s_intrinsics.rs:78 | src/d2s.rs:132:13 | yes |
| 15 | `multiple_of_power_of_5` src/d2s_intrinsics.rs:56 | `pow5_factor` src/d2s_intrinsics.rs:39 | src/d2s_intrinsics.rs:58:5 | yes |
| 16 | `mul_shift_all_64` src/d2s_intrinsics.rs:78 | `mul_shift_64` src/d2s_intrinsics.rs:71 | src/d2s_intrinsics.rs:86:20 | yes |
| 17 | `mul_shift_all_64` src/d2s_intrinsics.rs:78 | `mul_shift_64` src/d2s_intrinsics.rs:71 | src/d2s_intrinsics.rs:88:5 | yes |
| 18 | `f2d` src/f2s.rs:40 | `pow5bits` src/common.rs:64 | src/f2s.rs:108:33 | yes |
| 19 | `f2d` src/f2s.rs:40 | `pow5bits` src/common.rs:64 | src/f2s.rs:74:43 | yes |
| 20 | `f2d` src/f2s.rs:40 | `log10_pow2` src/common.rs:81 | src/f2s.rs:72:17 | yes |
| 21 | `f2d` src/f2s.rs:40 | `mul_pow5_div_pow2` src/f2s_intrinsics.rs:101 | src/f2s.rs:104:14 | yes |
| 22 | `f2d` src/f2s.rs:40 | `mul_pow5_div_pow2` src/f2s_intrinsics.rs:101 | src/f2s.rs:105:14 | yes |
| 23 | `f2d` src/f2s.rs:40 | `mul_pow5_div_pow2` src/f2s_intrinsics.rs:101 | src/f2s.rs:109:35 | yes |
| 24 | `f2d` src/f2s.rs:40 | `multiple_of_power_of_5_32` src/f2s_intrinsics.rs:44 | src/f2s.rs:93:40 | yes |
| 25 | `f2d` src/f2s.rs:40 | `multiple_of_power_of_5_32` src/f2s_intrinsics.rs:44 | src/f2s.rs:95:23 | yes |
| 26 | `f2d` src/f2s.rs:40 | `mul_pow5_inv_div_pow2` src/f2s_intrinsics.rs:75 | src/f2s.rs:76:14 | yes |
| 27 | `f2d` src/f2s.rs:40 | `mul_pow5_inv_div_pow2` src/f2s_intrinsics.rs:75 | src/f2s.rs:78:14 | yes |
| 28 | `f2d` src/f2s.rs:40 | `mul_pow5_inv_div_pow2` src/f2s_intrinsics.rs:75 | src/f2s.rs:85:18 | yes |
| 29 | `multiple_of_power_of_5_32` src/f2s_intrinsics.rs:44 | `pow5factor_32` src/f2s_intrinsics.rs:27 | src/f2s_intrinsics.rs:45:5 | yes |
| 30 | `write_mantissa_long` src/pretty/mantissa.rs:5 | `write_mantissa` src/pretty/mantissa.rs:28 | src/pretty/mantissa.rs:24:5 | yes |

### Not measured

- regex-syntax with the adapter of this run. Its numbers above are the scratch reader's. A
  later run of the adapter on regex-syntax, before and after the rule for crate inner
  attributes, is in "The rule for inner attributes on regex-syntax 0.8.12" below.
- The memory of Cargo, rustc and rust-analyzer during the run.
- Whether the 19 packages missing from the local cache matter on another host. On this one,
  the build did not need them.
- Other host targets than `x86_64-unknown-linux-gnu`, and other toolchains.
- How often the published calls run at run time. A row is one instruction of one static build.

## Coverage extensions in a scratch probe, 2026-11-25

These are scratch measurements, not the production adapter's. A harness linked the calls
layer's row code of commit `8cf2ee8` as a library, changed one rule per mode with a recorded
patch (`experiment.py`), and ran it over IR recorded from one workspace build of each corpus
and over an index of the same sources. `gosanja-adapter-rust index` did not run. Five
extensions of the layer were measured, one at a time against the same build: calls into
another target, dynamic dispatch, drop glue, attribute macros that return their input
unchanged, and crate-level inner attributes. A sixth question was whether the indexes record
trait and implementation relationships.

### Inputs

| Corpus | Sources | Index | Manifest as built |
| --- | --- | --- | --- |
| the `rust-calls` fixture | `fixtures/rust-calls` at `8cf2ee8`, identical file for file | the fixture's checked-in index | unchanged |
| ryu 1.0.23 | `src/` identical to the published archive's (SHA-256 `9774ba4a74de5f7b1c1451ed6cd5285a32eddb5cccb8cc655a4e50009e06477f`) | from the measurements above | unchanged |
| regex-syntax 0.8.12 | `src/` identical to the published archive's (SHA-256 `d902046e6a28a1e4c3cab97258a0ce11e994903f01c2d97a9447a0b3df85f4f6`) | from the measurements above | the optional `arbitrary` dependency and its feature removed, an empty `[workspace]` table added |
| zbus 5.19.0 | `src/` identical to the published archive's (SHA-256 `5db4be7c075cb421e4b7ee645541604239bd243ba7c357511f4ff3a74b555907`) | generated for this probe, under the adjusted manifest | every dev-dependency removed; of the features, only `default` and the features it enables kept, and the optional dependencies that only the removed features select (`tokio`, `tokio-vsock`, `vsock`) removed |

The offline build could not resolve `arbitrary` for regex-syntax, nor `codspeed-criterion-compat`
and then `tokio-vsock` for zbus; the manifests were adjusted for that reason alone. zbus was
built first as a path dependency of a scratch package, then as its own root. For the
attribute-macro run only, its manifest later gained a `[patch.crates-io]` table naming
instrumented copies of its procedural macro dependencies; the IR the other columns use was
recorded before that change, from the registry's macros. The archive digests are those of the
archives in the local Cargo cache, against which the `src/` trees were compared.

Host and settings: `x86_64-unknown-linux-gnu`, rustc 1.99.1 (58cf8f9c3), LLVM 23.1.1, the `dev`
profile and default features. Every library and binary target of the workspace was built in one
invocation. A compiler wrapper added an LLVM IR output to the same compiler run that produced
each workspace artifact other than a build script, added `-Ccodegen-units=1 -Cdebuginfo=2
-Copt-level=0`, and recorded the compiler's arguments, output and working directories. Tests,
examples and downstream programs were not built and are not counted.

```sh
# in the scratch directory, once per corpus
CARGO_TARGET_DIR="$d/target/$corpus" RECORD="$d/ir/$corpus" RUSTC_WORKSPACE_WRAPPER="$d/wrapper" \
    cargo +1.99.1 build --offline --locked --workspace --lib --bins -j2
# the rows of each mode, then their differences
./run
python stats.py
python edges.py fixture
```

### Rows added by each extension

| Corpus | Rows of the unchanged rules | Calls into another target | Dynamic dispatch¹ | Drop glue¹ | Unchanged attribute macro | Crate inner attributes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `rust-calls` | 83 | +2 | 0 | 0 | +8 | 0 |
| ryu 1.0.23 | 49 | 0 | 0 | 0 | 0 | 0 |
| regex-syntax 0.8.12 | 1 728 | 0 | 0 | 0 | 0 | +8 |
| zbus 5.19.0 | 1 376 | 0 | not measured | not measured | 0 | 0 |

A row is one serialized evidence entry with its build context. The rows of each mode were
compared with those of the unchanged rules as multisets. No row of the unchanged rules
disappeared in any mode.

¹ For dynamic dispatch and drop glue, the number is how many rows the existing relation
(`mechanism: static`, `resolution: direct`, corroborated by a reference on the site's line)
could support. The probe derived it by examining the instructions and the index occurrences;
no resolver was built for either.

The 1 728 rows of regex-syntax come from the harness and are not comparable with the scratch
reader's 1 736 of the first sections. The two readers differ, and so do the builds: one
workspace build through a wrapper here, one `cargo rustc` of the library there.

```text
$ ./run
fixture base 83
fixture cross 85
fixture macro 91
fixture inner 83
ryu-1.0.23 base 49
ryu-1.0.23 cross 49
ryu-1.0.23 macro 49
ryu-1.0.23 inner 49
regex-syntax-0.8.12 base 1728
regex-syntax-0.8.12 cross 1728
regex-syntax-0.8.12 macro 1728
regex-syntax-0.8.12 inner 1736
zbus-5.19.0 base 1376
zbus-5.19.0 cross 1376
zbus-5.19.0 macro 1376
zbus-5.19.0 inner 1376
```

### Calls into another target

The probe resolved a `v0` symbol that the calling module only declares, to its definition in
exactly one direct workspace dependency of the caller. A dependency was matched when its
artifact existed in the output directory the compiler recorded for it, with the same crate
name and Cargo suffix as the `--extern` path given to the caller. A definition in the caller
still took precedence, and every other check of the layer ran afterwards.

The fixture gained two rows: `main` calling `calls_lib::direct` at
`calls-app/src/main.rs:12`, and `hinted` calling `leaf` at `calls-lib/src/lib.rs:559` in the
`calls_app` build. ryu, regex-syntax and zbus each built one target, so their zero says
nothing about calls between targets. The probe did not resolve unmangled exports, packages
outside the workspace or transitive dependencies, and it used one machine and one
configuration. A production reader would have to bind each output to its artifact across
retries and caches.

### Dynamic dispatch

The fixture holds two indirect calls with a site in the project, ryu and regex-syntax none:
`calls_lib::apply` calls a function pointer at `calls-lib/src/lib.rs:410`, and
`calls_lib::dyn_call` loads a method from the table its caller passes, at
`calls-lib/src/lib.rs:88`. The fixture's own table puts `Square::area` in the slot it loads.
The probe also compiled a separate crate that implements the same trait for another type and
passes it to the same prebuilt library. Its table puts that type's method in the same slot:

```sh
rustc +1.99.1 --crate-type lib --emit=llvm-ir \
    --extern calls_lib=$(ls target/fixture/debug/deps/libcalls_lib*.rlib) \
    -L dependency=target/fixture/debug/deps downstream.rs -o downstream.ll
```

The receiver set is open. The slot names a method only for a known table, and the index names
the trait's method at the site. Neither shows which table reaches the call. One concrete
callee would need the table's provenance at the site, which neither instruction supplies
locally, so neither gives a supportable row under the existing relation. A relation to the
trait's declaration, or a "possible target" relation, would need meanings of its own.

### Drop glue

| Corpus | Glue call instructions | Distinct sites | Local `Drop` implementations | Sites with a same-line reference to a local destructor |
| --- | ---: | ---: | ---: | ---: |
| `rust-calls` | 130 | 44 | 3 | 0 |
| ryu 1.0.23 | 0 | 0 | 0 | 0 |
| regex-syntax 0.8.12 | 584 | 247 | 3 | 0 |

The probe counted direct calls to `core::ptr::drop_glue` or `drop_in_place` with a nonzero line
in the project, monomorphizations and cleanup paths included. It then looked, on the same line,
for a non-definition occurrence of any local symbol ending in `[Drop]drop().`. That check is
broader than the layer's call-head match, and it still found none: at
`calls-lib/src/drops.rs:40` and `:44`, `drop(None::<Guard>)` and `drop(Some(Guard(8)))`
reference `core`'s `mem::drop`, `Option`'s variants and the guard type, not its destructor, and
the glue for `Option<Guard>` branches on the variant before it calls the guard's. A walk of
reachable glue would attach the destructor to the `None` case. An implicit destruction relation
would need a source anchor, ownership semantics and a rule for conditional effects. Zero here
does not count the destructors the program runs.

### Attribute macros that return their input

The probe wrapped the `proc_macro_attribute` entry points of scratch copies of the macro
crates. Each invocation logged its path, line, column and name, and whether its input and the
stream it returned had equal `Debug` representations in that invocation. Rows were then
admitted only at the logged positions of equal invocations.

```text
$ cut -f4,5 attributes.tsv | sort | uniq -c
32 async_trait false; 14 instrument false; 9 proxy false; 4 interface false
3 bitflags_internal false; 1 async_recursion false; 1 retype false
1 retrait false; 1 copy_to false; 4 keep true
```

All 61 invocations logged at zbus's own source paths returned a different representation, so
zbus gained nothing. In the fixture, the four invocations of `keep` (lines 423, 430, 454 and
468 of `calls-lib/src/lib.rs`, two of them through aliases) were equal and gave four rows out
of and four into the items they hold; two `proxy` invocations and `retype`, `retrait` and
`copy_to` were not.

The test compares `Debug` representations, which is an experiment, not a stable provenance.
A control that re-parses the printed input shows that printed text alone loses span
differences:

```text
$ cat innerprobe/compare.log
text_equal=true debug_equal=false
```

The join was by source coordinates in distinct measured paths. A production witness would
also need the originating build, the input file's identity, the expansion stack, the resolved
macro, and a token-by-token comparison of delimiters, spacing, spans and hygiene. It is also
whole-item identity: a macro that keeps every method but changes the item around them fails
it.

### Crate inner attributes

A crate-level inner attribute other than the layer's inert attributes takes every item of its
file. ryu's `src/lib.rs` has `#![no_std]`; regex-syntax's has `#![no_std]` and
`#![cfg_attr(docsrs_regex, feature(doc_cfg))]`, besides lint attributes that are already
inert. The probe exempted exactly two attributes: `#![no_std]`, and the written
`cfg_attr(docsrs_regex, feature(doc_cfg))` compared as token text. The recorded compiler
arguments list `docsrs_regex` under `--check-cfg` and not under `--cfg`, so in this build the
`cfg_attr` contributes no attribute. That is a narrower rule than one over a list of
built-in attributes.

ryu gained nothing. regex-syntax gained eight rows: five at sites in `src/lib.rs` (lines 205,
216, 307, 351 and 371) and three at sites in other files calling into it
(`src/ast/parse.rs:1532` and `:1539`, `src/hir/print.rs:310`).

A blanket exemption is not sound. On rustc 1.99.1, an inner attribute macro that replaces its
crate with a generated function is refused, and accepted once `RUSTC_BOOTSTRAP` unlocks the
feature:

```sh
cd innerprobe
rustc +1.99.1 --crate-type proc-macro attr.rs
rustc +1.99.1 --crate-type lib --edition=2024 --extern attr=libattr.so lib.rs
# error[E0658]: inner macro attributes are unstable
RUSTC_BOOTSTRAP=1 rustc +1.99.1 --cfg probe --crate-type lib --edition=2024 \
    --extern attr=libattr.so lib.rs --emit=llvm-ir -o lib.ll
rg '^; .*generated' lib.ll
# ; lib::generated
```

This counterexample is not part of the corpus counts.

### Trait and implementation relationships in the index

The indexes of the fixture, ryu and regex-syntax, and of a synthetic control crate, held no
`relationships` on any document symbol. zbus's index was not counted. The control is:

```rust
pub trait Root { fn value(&self) -> u32; }
pub trait Peer {}
pub trait Child: Root + Peer { type Output: Root; }
pub struct Leaf;
impl Root for Leaf { fn value(&self) -> u32 { 1 } }
impl Peer for Leaf {}
impl Child for Leaf { type Output = Leaf; }
```

In it, the first and the last `Root` of `Child`'s declaration have the same symbol and the
same reference role, though only the first is a supertrait. `impl Root for Leaf` gives
references to `Root` and `Leaf` and a definition of the method, under a symbol whose spelling
is the indexer's own; an `impl` without methods has nothing to inspect. With these indexes,
the occurrences can anchor the ends of such a relation but do not label an `impl` header or a
supertrait bound. Inferring either from occurrences on one line would be guessing.

### Not measured

- Dynamic dispatch and drop glue on zbus, and the relationships of zbus's index.
- Features other than the defaults, release builds, tests and examples.
- Calls into another target on any corpus with more than one target except the fixture.
- Other hosts and toolchains than rustc 1.99.1 on `x86_64-unknown-linux-gnu`.
- The production adapter, except for the crate inner attributes on regex-syntax below.

## The rule for inner attributes on regex-syntax 0.8.12, 2026-11-25

These are measurements of the production adapter: `gosanja-adapter-rust index` built from
commit `8cf2ee8`, before the calls layer allowed crate-level inner attributes such as
`#![no_std]`, and from the change that allows them and judges every attribute in a file. They
measure that change on the input where the scratch probe above found its gain.

Input: the published archive `regex-syntax-0.8.12.crate` from the local Cargo cache, SHA-256
`d902046e6a28a1e4c3cab97258a0ce11e994903f01c2d97a9447a0b3df85f4f6`, unpacked under `/var/tmp`
and compared file for file with the archive: identical, its manifest included, so the optional
`arbitrary` dependency is kept. It was built with the lock file the archive carries.

Host and settings:
- `x86_64-unknown-linux-gnu`; rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1, through
  `RUSTUP_TOOLCHAIN`; rust-analyzer 0.3.3049-standalone;
- a Cargo home made for the run, holding no configuration, filled by `cargo fetch --locked`
  from the archive's lock file, which downloaded its six packages (`arbitrary`,
  `derive_arbitrary`, `proc-macro2`, `quote`, `syn` and `unicode-ident`);
- no `.cargo/config` or `.cargo/config.toml` in the crate's directory or any directory above
  it up to `/`;
- of the environment, only `PATH` (with rust-analyzer's directory first), `HOME` (an empty
  directory), `CARGO_HOME`, `RUSTUP_HOME` and `RUSTUP_TOOLCHAIN`.

~~~sh
env -i PATH="$ra_dir:$PATH" HOME="$d/home" CARGO_HOME="$d/cargo" RUSTUP_HOME="$rustup_home" \
    RUSTUP_TOOLCHAIN=1.99.1 "$adapter" index --root "$crate" --out "$d/$label-out"
python3 compare-production.py
~~~

`compare-production.py` requires the calls layer `ok` in both `result.json` files and compares
the two `calls.jsonl` as multisets of rows.

~~~text
base: calls ok, 1728 rows
branch: calls ok, 1736 rows
+8 -0
~~~

Both runs built one target, the library (`regex-syntax-lib-regex_syntax`), with the default
features. No row went. The eight rows added are the eight sites the scratch probe named:

| Site | Caller | Callee |
| --- | --- | --- |
| src/lib.rs:205:5 | `escape` src/lib.rs:203 | `escape_into` src/lib.rs:213 |
| src/lib.rs:216:12 | `escape_into` src/lib.rs:213 | `is_meta_character` src/lib.rs:262 |
| src/lib.rs:307:8 | `is_escapeable_character` src/lib.rs:305 | `is_meta_character` src/lib.rs:262 |
| src/lib.rs:351:5 | `is_word_character` src/lib.rs:350 | `try_is_word_character` src/lib.rs:368 |
| src/lib.rs:371:5 | `try_is_word_character` src/lib.rs:368 | `is_word_character` src/unicode.rs:463 |
| src/ast/parse.rs:1532:12 | `parse_escape` src/ast/parse.rs:1482 | `is_meta_character` src/lib.rs:262 |
| src/ast/parse.rs:1539:12 | `parse_escape` src/ast/parse.rs:1482 | `is_escapeable_character` src/lib.rs:305 |
| src/hir/print.rs:310:12 | `write_literal_char` src/hir/print.rs:309 | `is_meta_character` src/lib.rs:262 |

Each was checked by hand against the unpacked archive. Each site's line lies inside the
caller's body and calls the callee by name, followed by `(`; at src/lib.rs:371 the column is
that of the path `unicode::is_word_character`, which names the function of `src/unicode.rs`,
not the `is_word_character` of `src/lib.rs`. The callers in `src/ast/parse.rs` and
`src/hir/print.rs` import their callees from the crate root with `use crate::{...}`.

`src/lib.rs` was the one file whose `uncorroborated` entry went: `skipped` held twelve files
before and eleven after, the other eleven unchanged.
