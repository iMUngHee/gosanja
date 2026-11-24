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

The public-crate runs used a scratch reader and a relay for the existing engine. They do not measure finished-adapter wall time, peak memory, IR footprint, or its final omission codes. The final-fixture multiset check depends on the frozen fixture and its independently recorded source expectations. Windows, macOS and runtime call frequency were outside these trials.

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
