# Rust adapter measurements

Local observations on 2026-11-24, Linux x86_64. The synthetic probes used rustc 1.99.1 (58cf8f9c3), Cargo 1.99.1 (5f94df478), rustup 1.29.1 and rust-analyzer 0.3.3049-standalone. Span probes used proc-macro2 1.0.107 with span-locations and syn 2.0.119 with full. The sibling-dependency snapshot used Gosanja 0.2.0. Two later sections measure reference recall on a fixture and anchor misses on public crates. The last, from 2026-11-27 and 2026-11-28, measures peak memory and time by stage and states its own host and tools.

For the synthetic probes, in command examples ROOT is the workspace, OUT is an existing directory outside it, and TARGET and BUILD are pre-created directories under OUT. Cargo ran offline with cached dependencies. One workspace had a binary and library package, a procedural-macro member, a build script, unicode-ident 1.0.27 from the registry and a sibling path dependency. The configuration probes used a locked library named counter with no project build code, optionally depending on itoa 1.0.18.

## Lockfile creation and preflight

In a dependency-free edition-2021 library with its target directory redirected, each of the following commands started with Cargo.lock missing and finished successfully with a nonempty Cargo.lock. The index output was outside the workspace.

```sh
cargo metadata --format-version 1
rust-analyzer scip . --output "$OUT/index.scip"
```

With a valid lock in this library, the before/after tree listings also matched for `cargo metadata --no-deps --format-version 1`. In the separate single-library trial below, Linux inotify watched the project and its source directory. Generating a lockfile explicitly produced create, modify and close-write events as a positive control.

| Initial state | Command | Exit and output |
|---|---|---|
| Missing lock | `cargo metadata --format-version 1 --no-deps` | 0. Metadata JSON on stdout, empty stderr, lock still missing. |
| Missing lock | `cargo metadata --format-version 1 --locked` | 101. “cannot create the lock file … because --locked was passed to prevent this” |
| Lock generated, then local package version changed from 0.1.0 to 0.2.0 | Same locked command | 101. “cannot update the lock file … because --locked was passed to prevent this” |
| Valid lock restored by restoring the original manifest; isolated Cargo home with registry index entry but no crate archive or source | `cargo metadata --format-version 1 --locked --offline` | 101. “failed to download `unicode-ident v1.0.27`” and “attempting to make an HTTP request, but --offline was specified” |

All three failures had empty stdout. All four metadata commands produced zero watched write events. Inotify measures visible filesystem events, not every attempted system call. These observations cover events in the watched project directories and net changes in the before/after listings. They do not measure writes elsewhere.

## Project build code and workspace writes

The input was a writable two-member workspace: an application with a build script, a procedural macro crate defining an identity attribute used by the application, a cached registry dependency and a sibling path dependency. Starting with Cargo.lock present, the runner sets `CARGO_TARGET_DIR` outside the workspace, sets Cargo offline, and runs these commands from the trial directory:

```sh
(cd ws && find . -printf '%m %y %p\n' && find . -type f -exec sha256sum {} +) | sort > logs/before
(cd ws && rust-analyzer scip . --num-threads 2 --output "$D/a.scip")
(cd ws && find . -printf '%m %y %p\n' && find . -type f -exec sha256sum {} +) | sort > logs/after-a
diff -u logs/before logs/after-a
```

Here and below, `D` is the absolute trial directory. The commands show an equivalent Linux comparison. The original listing includes entry type and mode, relative path and SHA-256 of every regular file. Before and after the no-op build, all 21 entries matched, including Cargo.lock. Both listing digests were `36f6848fec479b4d2ddb72790433e08dfcc523f19500036200e6aa841c0d3d3d`. Stderr reported both “Loading build script trial-app run” and “Loading proc-macro macro-kit built”.

The second input replaces `app/build.rs` with a main function calling `std::fs::write("src/from_build.txt", b"written by build.rs\n").unwrap()`. Its baseline is taken **after** replacing the build script. The same index command writes to `$D/writes.scip`. The before/after diff adds exactly this row:

```text
0o100644 app/src/from_build.txt e71bef6c607f2694662563c817a18de18ace8c11a09076451cc72bef241d6c87
```

Both index commands exited 0. These comparisons cover the final workspace contents and modes. They do not detect transient writes subsequently undone, mtime-only updates or writes outside the workspace.

## Cargo build directories, temporary files and logging

Input: one locked crate named counter with a `[workspace]` section, one library function and no dependencies. Both paths below are under its root. In the template cases the build setting is `"{workspace-root}/configured-build"`. TARGET and BUILD below denote absolute, pre-created directories under that case's output, outside the project.

```toml
[build]
target-dir = "configured-target"
build-dir = "configured-build"
```

Each command ran in its own fresh copy, first without either build-directory environment variable, then with `CARGO_TARGET_DIR=$TARGET` and `CARGO_BUILD_BUILD_DIR=$BUILD`. The index output was always absolute and outside the project.

```sh
cargo metadata --locked --offline --format-version 1
rust-analyzer scip . --num-threads 2 --output "$OUT/index.scip"
cargo check --locked --offline
```

| Command | No overrides, literal or templated build-dir | Both overrides, literal or templated build-dir |
| --- | --- | --- |
| metadata | Exit 0, no changed entries, zero watched events. JSON reported the two configured paths. | Exit 0, no changed entries, zero events. JSON reported TARGET and BUILD. |
| scip | Exit 0, 16 entries added in each case, including configured-build/.rustc_info.json, debug/.cargo-build-lock and build output under configured-build, and configured-target/debug/.cargo-lock. | Exit 0, no changed entries, zero events. The index appeared under OUT. |
| check | Exit 0, 29 entries added in each case under the two configured directories. | Exit 0, no changed entries, zero events. |

Cargo 1.99.1 recognizes `build.build-dir` and expands `{workspace-root}` there. A separate metadata run with only `CARGO_BUILD_DIR` for the build path still reported `project/configured-build`. The effective spelling measured above is `CARGO_BUILD_BUILD_DIR`. The target path has different template behavior: setting `target-dir="{workspace-root}/configured-target"` produced the literal directory `project/{workspace-root}/configured-target`. With that same template in build-dir, metadata reported `project/configured-build` as expected. The SCIP run confirmed the distinction on disk.

The TMPDIR and RA_LOG_FILE cases used the same two Cargo overrides, the same external Cargo home and a fresh counter workspace. Locked metadata with TMPDIR inside the project produced no changed entries or watched events. For SCIP with the same TMPDIR, the watch recorded events such as the following, relative to the project:

```text
temp/rust-analyzer10ca60-0 CREATE,ISDIR
temp/rust-analyzer10ca60-0/Cargo.lock MODIFY
temp/rust-analyzer10ca60-0/Cargo.lock CLOSE_WRITE,CLOSE
temp/rust-analyzer10ca60-0/Cargo.lock DELETE
temp/rust-analyzer10ca60-0 DELETE,ISDIR
```

In the third-directory trial, a second watcher outside the root recorded creations and deletions while the project watcher recorded zero events. The RA_LOG_FILE cases left a persistent path and therefore showed up even in the final tree comparison. No claim is made about other operating systems or untested environment settings.

The temporary-directory case produced 15 watched events under the project and identical final contents. Pointing TMPDIR to a third directory outside the project and output produced 11 watched events there. With RA_LOG_FILE pointing at analyzer.log under the project, SCIP exited 0 and left an empty file when RA_LOG was unset. With RA_LOG=info it left 91,994,205 bytes. Directing TMPDIR under OUT and unsetting RA_LOG_FILE gave zero observed project write events in the tested counter case. Recursive watches can miss short-lived entries before a new directory receives its watch. A close-write event records the closing of a writable descriptor, even when bytes stay identical.

A separate dependency-free library combined a build script that wrote three bytes to src/build-marker with a project .cargo/config.toml setting `build.rustflags` to `["--emit=llvm-ir=<absolute project path>/configured.ll"]`. Both Cargo directory overrides pointed outside the project. The following index command exited 0. The build marker contained `ran` and configured.ll was 40,331 bytes.

```sh
rust-analyzer scip . --config-path "$OUT/settings.json" --output "$OUT/settings.scip"
```

The supplied JSON set cargo.buildScripts.enable and procMacro.enable to false. The indexer logged both a configuration diagnostic and “Loading build script probe run”. This run establishes that the script ran with that input file; it does not establish how the indexer interpreted each setting.

## Cargo cache placement

Input: counter now depends on exactly itoa 1.0.18. The same valid lockfile is present before every run. Cargo's home points to an existing `project/cache` directory. Both build-directory overrides point outside. Each case ran:

```sh
CARGO_HOME="$ROOT/cache" cargo metadata --locked --offline --format-version 1
```

| Initial contents of Cargo home | Exit and persistent difference under the project |
| --- | --- |
| Sparse index entry and cached .crate archive, no unpacked source | 0; 26 new entries. Cargo added .package-cache, .global-cache and registry/CACHEDIR.TAG, created registry/src and unpacked itoa-1.0.18 there. |
| Fully warm after one successful identical run | 0; no entry or content changed. Three close-write events on .package-cache and one on .global-cache. |
| Sparse index entry but neither archive nor unpacked source | 101; three files added: .package-cache, .global-cache and registry/CACHEDIR.TAG. Stderr said “failed to download `itoa v1.0.18`” and “attempting to make an HTTP request, but --offline was specified”. |
| Empty Cargo home | 101; .package-cache, the registry directory and registry/CACHEDIR.TAG were added. Stderr said “no matching package named `itoa` found”. |

All listed changes are under cache/. The input project's Cargo.lock remained byte-identical. Offline failure still creates cache entries in the project.

## Definition anchors and enclosing ranges

The probe decoded the SCIP Document occurrences with the Definition role, reading the packed range and enclosing_range fields. Each pair below is name range, then enclosing range. The SCIP triplet is [line,start,end]; a four-element range is [startLine,startColumn,endLine,endColumn]. Byte columns and lines are zero-based.

| Source or symbol | Name range | Enclosing range |
|---|---|---|
| app/src/lib.rs module, crate/ | [0,0,6,0] | [0,0,6,0] |
| app/src/child.rs module, child/ | [0,0,1,0] | [0,0,1,0] |
| union Mix | [2,10,13] | [2,0,45] |
| function r#type | [3,7,13] | [3,0,18] |
| nested method, impl#[Inner]nested(). | [4,51,57] | [4,44,62] |
| uses_dep with identity attribute on the same line | [5,26,34] | [5,19,99] |

The raw identifier starts at the `r` and includes `#`. The nested method has an anchor even though its symbol omits the enclosing function name. The whole-file module ranges end at column 0 after the final newline. The identity attribute at column 0 is outside the recorded function enclosure; the following probes supply two ordinary attribute examples.

In a four-line library, the producer placed the identifier of the generated function at the argument token in the invocation. The ordinary inline attribute lay inside its function extent. These were all the definition-role descriptors in this document: crate/, make!, made(). and visible().

```rust
macro_rules! make { ($n:ident) => { fn $n() {} }; }
make!(made);
#[cfg(any())] fn inactive() {}
#[inline] pub fn visible() {}
```

| Descriptor | Name | Enclosing |
| --- | --- | --- |
| make! | [0,13,17] | [0,0,51] |
| made(). | [1,6,10] | [1,0,12] |
| visible(). | [3,17,24] | [3,0,29] |

A second index of a two-member fixture (flow-app and flow-kit) contained the following definition-role occurrences in the kit library. An impl descriptor occurred as a prefix of a method symbol, with no definition occurrence at the impl header. The inactive function had no definition occurrence. Each observed range below is in UTF-8 bytes.

| Item | Name | Enclosing |
| --- | --- | --- |
| Struct Op / field f | [5,11,13] / [5,20,21] | [5,0,39] / [5,16,37] |
| Enum Flag / variant On | [6,9,13] / [6,16,18] | [6,0,25] / [6,16,18] |
| Union Bits | [7,10,14] | [7,0,46] |
| Type alias Count | [8,9,14] | [8,0,21] |
| Constant LIMIT / static TOTAL | [9,10,15] / [10,15,20] | [9,0,27] / [10,0,32] |
| Trait Adjust | [12,10,16] | [12,0,17,1] |
| Associated type Output / constant START | [13,9,15] / [14,10,15] | [13,4,16] / [14,4,25] |
| Trait method required / inherent method apply | [15,7,15] / [20,11,16] | [15,4,23] / [20,4,54] |
| Function step, including its documentation and ordinary attributes | [33,7,11] | [29,0,33,40] |
| Inline module a | [38,8,9] | [38,0,41,1] |

These observations describe producer anchors. Whether the adapter should emit a syntax row for a variant, field or other construct is a separate interface decision.

## Target documents and sibling dependencies

`cargo metadata --locked --format-version 1` recorded an empty enabled-feature list for trial-app. Comparing its target paths with the index document list gave:

| Target | Indexed document |
|---|---|
| Library | app/src/lib.rs |
| Binary | app/src/main.rs |
| Procedural macro | macro-kit/src/lib.rs |
| Example | app/examples/sample.rs |
| Integration test | app/tests/integration.rs |
| Benchmark | app/benches/bench.rs |
| Binary requiring optional-bin, feature disabled | app/src/gated.rs |
| Custom build | app/build.rs |

The ninth document was app/src/child.rs. Stderr also reported duplicate symbols for the different target roots and their main functions. The producer emitted an index despite these warnings. Buildability of disabled targets was not tested. In a separate edition-2021 package named roots (0.1.0), src/lib.rs contained one line, `pub fn library() {}`, and src/main.rs contained `fn main() {}`, both newline-terminated. Running `rust-analyzer scip . --output "$OUT/index.scip"` with 0.3.3049 produced two definition occurrences of `rust-analyzer cargo roots 0.1.0 crate/`: src/lib.rs [0,0,1,0] and src/main.rs [0,0,1,0].

`outside-kit={path="../../outside-kit"}` in the application points to a sibling of the workspace. Locked metadata resolved that dependency. All nine SCIP document paths above are workspace-relative. No document represents outside-kit. The caller's occurrences do include the external symbols `rust-analyzer cargo outside-kit 0.1.0 crate/` and `rust-analyzer cargo outside-kit 0.1.0 letter().`. This was a direct sibling path, not a symlink or a dependency vendored beneath the root.

Input: `project/Cargo.toml` declares `sibling-kit={path="../sibling"}`. The sibling lives outside the caller's workspace. Its library defines `pub fn step(n: u8) -> u8 { n + 1 }`. The caller contains one line: `pub fn answer() -> u8 { sibling_kit::step(4) }`. Cargo.lock contains both packages.

With offline Cargo, both build-directory overrides and an external Cargo home, the following commands exited 0:

```sh
rust-analyzer scip . --num-threads 2 --output "$OUT/index.scip"
gosanja index --scip "$OUT/index.scip" --root "$ROOT" --out "$OUT/snapshot"
gosanja validate "$OUT/snapshot" --root "$ROOT"
```

The index was 521 bytes. The snapshot manifest reports 1 file (the caller's src/lib.rs), 2 entities, 2 externals, 2 relations and 0 gaps. The external rows name `cargo:sibling-kit:crate/` and `cargo:sibling-kit:step().`. Both carry package manager cargo, package name sibling-kit and version 0.1.0. The function-specific row and relation are:

```jsonl
{"key":"cargo:sibling-kit:step().","symbol":"rust-analyzer cargo sibling-kit 0.1.0 step().","package":{"manager":"cargo","name":"sibling-kit","version":"0.1.0"},"src":"scip:rust-analyzer"}
{"from":"cargo:caller:answer().","to":"cargo:sibling-kit:step().","rel":"ref","roles":[],"sites":[["src/lib.rs",0,37,0,41]],"src":"core:reference-in-body"}
```

The other relation goes from the same caller to the sibling crate at [0,24,0,35]. This base-only snapshot records reference edges; the syntax and calls layers are absent.

```text
$ wc -c sibling/out/snapshot/gaps.jsonl
0 sibling/out/snapshot/gaps.jsonl
```

## Fixed-path index reproducibility

The runner executed two identical scip commands at the same path, specifying two cache-priming threads, before changing the build script. A second runner restored the original fixture and ran the same indexer four more times with its default thread setting. Output files and Cargo targets are outside the workspace. The first pair compared equal with `cmp a.scip b.scip`. All six files were 5,902 bytes and had this SHA-256:

```text
aa65ff4869181b0ca88a951a3d6c309c73f38f2a909a98b2b03432a1932b5d5e
```

This is evidence for one dependency-bearing workspace at a fixed path on one Linux host. Stderr varied between runs. The index metadata contains the absolute project URI. Relocated, cross-machine and cross-version byte equality remain unmeasured.

## Missing pinned toolchain

The probe writes a toolchain file pinning `nightly-2001-01-01`, removes the toolchain override from the child environment and sets `RUSTUP_DIST_SERVER` to an ephemeral loopback HTTP server. The listener records paths and always answers 404. From that project the script runs `cargo --version` twice.

| RUSTUP_AUTO_INSTALL | Exit | Listener and stderr |
|---|---|---|
| Unset | 1 | GET /dist/2001-01-01/channel-rust-nightly.toml.sha256 and GET /dist/2001-01-01/channel-rust-nightly.sha256. Rustup reported syncing channel updates, then “no release found”. |
| 0 | 1 | Zero requests. Rustup reported that the toolchain was not installed and printed an installation suggestion. |

Both commands had empty stdout. The listener served no toolchain data. These results concern rustup 1.29.1 and this missing pin. In a second trial, the proxy started with a missing RUSTUP_HOME directory inside a synthetic project. Even with auto-install disabled it created that directory, then exited 1 reporting that toolchain 1.99.1 was unavailable in the new home.

```sh
RUSTUP_HOME="$ROOT/new-rustup" RUSTUP_TOOLCHAIN=1.99.1 RUSTUP_AUTO_INSTALL=0 cargo --version
```

## Unicode columns, span joins and BOM

`(cd span && cargo run --offline --quiet)` runs an ordinary executable using proc-macro2. Each input `/*é*/ fn first() {}`, `/*界*/ fn first() {}` and `/*🦀*/ fn first() {}` gave the identical half-open name span, line 1, columns 9–14. The characters take two, three and four UTF-8 bytes respectively. Columns count Unicode scalar values in these probes. Joining the first token's span to the identifier returned Some, columns 6–14, in all three cases. This exercises joins within one input outside a procedural macro.

The first source line of the indexed library is a UTF-8 BOM followed by `/*é界🦀*/ pub fn first() {}`. The table uses half-open columns. Rust-analyzer's lines are zero-based; proc-macro2's are one-based.

| Reader and input | Line, unit | Name span |
|---|---|---|
| Standalone proc-macro2 parses original source | 1, scalars | 16–21 |
| Standalone proc-macro2 parses source after stripping BOM | 1, scalars | 15–20 |
| syn::parse_file on original source | 1, scalars | 15–20 |
| rust-analyzer SCIP on the same file | 0, UTF-8 bytes | 24–29 |

A separate executable using syn::parse_file measured syn on the original file, including a successful join from `fn` to the end of `first` at scalar columns 12–20. On later lines the raw-identifier positions also agreed after accounting for the line base. These measurements do not establish joins across source files.

## Scope of the proposed write guarantee

The adapter design redirects Cargo target and build directories and the temporary directory into the output. It supplies an explicit child environment, checks the resolved Cargo and rustup homes and the registry and git cache locations, disables automatic toolchain installation, and checks locked metadata before indexing. The trials above explain these choices. They do not test all inherited variables, symlink layouts, operating systems or configurations. Project build scripts and procedural macros run with the process's permissions. Project and user configuration can direct compiler output elsewhere. A redirected target directory alone did not contain the source-writing build script.

## Reference recall

Local measurement on 2026-11-24 with rust-analyzer 0.3.3049-standalone, rustc 1.99.1 and Gosanja 0.2.0. The corpus is synthetic and small: `fixtures/rust-recall`, one 19-line library written for this purpose. `fixtures/rust-recall/expected-refs.tsv` lists by hand 34 identifier tokens that name something the crate defines, other than that thing's own name token, each with its site (file, zero-based line, start byte column) and the position of the name token it names. The checked-in index was made by `fixtures/regenerate.sh rust-recall`.

```sh
cargo test -p gosanja-adapter-rust --test recall
gosanja index --scip fixtures/rust-recall/index.scip --root fixtures/rust-recall --out "$OUT"
```

| Measure | Rule | Found |
|---|---|---|
| Raw | a reference occurrence of the index starts at the site, and its symbol is the symbol of the definition occurrence at the target | 34 of 34 |
| Snapshot | the site is among the `sites` of a `ref` relation whose `to` is the entity defined at the target | 26 of 34 |

The index misses none of the 34. The snapshot misses eight, all at sites whose occurrence carries a `local` symbol in the index (zero-based line and byte column): the parameters `c` (line 9, column 26), `t` (10, 35) and `d` (11, 33), the type parameter `T` (10, 24), and the `let` bindings `c` (14, 25), `w` (15, 24), `v` (16, 33) and `n` (17, 18). The engine drops them by the rule in `spec/snapshot.md` that local symbols have no key and are not rows, and the snapshot's `dropped_local_references` count is 8. No other rule dropped a reference.

These eight rows name parameters, a type parameter and local variables. The plan for the corpus lists the kinds it covers (calls, method calls, trait methods through generics and `dyn`, re-exports and `use` paths, struct literals and patterns, types in signatures and fields, associated items, tokens inside `macro_rules!` and `format!`-family invocations) and does not name these; the hand-made list includes them because its author counted a local binding as a definition. Over the 26 rows that name an item with a key, both measures find 26 of 26. The test pins the total, both counts and both lists of missing rows as literals. A corpus this small says nothing about recall on real crates.

## Anchor miss rate on two public crates

On 2026-11-24 the installed Rust adapter sent 224 syntax rows for byteorder and 694 for memchr. The engine matched 223 and 648 respectively. Every unmatched anchor was on an inactive conditional declaration or inside an inactive module. The denominator counts only rows sent for documents in the base index. It excludes source files absent from that index and items produced only by macro expansion.

| Local measurement | byteorder 1.5.0 | memchr 2.7.6 |
|---|---:|---:|
| Files in snapshot | 3 | 35 |
| Syntax rows sent / joined | 224 / 223 | 694 / 648 |
| Syntax layer | present | present |
| Unanchored / sent | 1/224 (0.45%) | 46/694 (6.63%) |
| Inactive cfg / base position elsewhere / other | 1 / 0 / 0 | 46 / 0 / 0 |
| Skipped entries (all codes) | 0 | 0 |
| Syntax position-out-of-range gaps | 0 | 0 |
| Opaque-region gaps | 322 | 223 |
| Open-call gaps | 0 | 90 |
| Index wall time (one run, concurrent trials) | 1.647 s | 2.147 s |
| Snapshot size (sum of regular-file bytes) | 630,253 | 2,394,961 |

The most frequent opaque invocations in byteorder were `qc_byte_order!` (62), `bench_num!` (58), `qc_bytes_ext!` (58), `too_small!` (54) and `assert_eq!` (17). In memchr they were `debug_assert!` (57), `assert!` (37), `assert_suffix_min!` (23), `assert_suffix_max!` (23) and `debug_assert_eq!` (18). Each count uses the invocation at the reported gap's start. For example, byteorder's `benches/bench.rs:125` invokes a macro whose template defines benchmark functions at lines 17, 27 and 37.

**Classification.** Line numbers below are one-based. Each unmatched anchor was inspected at its source position, including its enclosing module's attributes. An inactive declaration takes precedence when the base defines a same-named, mutually exclusive alternative. “Base position elsewhere” counts an active item whose own definition is located elsewhere.

- **byteorder.** The sole miss is `src/lib.rs:1902`, the big-endian `NativeEndian` alias. Line 1901 gates it on big-endian targets. The host is little-endian.
- **memchr.** Two anchors in `src/cow.rs` require `not(feature = "alloc")`; default `std` enables `alloc`. Six in `src/memmem/searcher.rs` require AArch64 or Wasm SIMD. Thirty-three in `src/vector.rs` are the modules gated at lines 292 and 454 and their contained items. Five in `src/tests/memchr/prop.rs` require `not(miri)`. The index selects the alternative macro definition at line 4 behind `cfg(miri)`; the five misses are at lines 11, 257, 291, 299 and 312. This describes rust-analyzer's indexed configuration.
- Five representative memchr anchors are `src/cow.rs:25` (`struct Imp`, alloc disabled), `src/cow.rs:104` (`as_slice`, alloc disabled), `src/memmem/searcher.rs:404` (`searcher_kind_simd128`, Wasm SIMD), `src/memmem/searcher.rs:425` (`searcher_kind_neon`, AArch64) and `src/vector.rs:305` (`splat` inside the AArch64 module). The other two cause classes have zero examples in both trials.

**Inputs and reproduction.** The engine and adapter were Gosanja 0.2.0 built from `99aa7a3` outside the shared target. The adapter ran the real rust-analyzer 0.3.3049-standalone. The clones used rustc 1.100.0 (0ef1bc386) and Cargo 1.100.0 (3acc79c0c) on x86_64 Linux. Source tags:

- https://github.com/BurntSushi/byteorder/tree/1.5.0 — `ec068eefa042d494475db125c4b034bd8e9e34dd`
- https://github.com/BurntSushi/memchr/tree/2.7.6 — `9ba486e4ba7e865c0510305c5dacba73988d9f31`

Both clones needed a generated Cargo.lock. Cargo updated the registry index during preparation. For byteorder it downloaded getrandom 0.1.16, rand_chacha 0.2.2, quickcheck 0.9.2, rand_hc 0.2.0, wasi 0.9.0+wasi-snapshot-preview1, rand_core 0.5.1 and rand 0.7.3. For memchr it downloaded quickcheck 1.1.0 and rand 0.10.4. The registry cache was warm. The caller set `CARGO_NET_OFFLINE=true`, which the adapter does not forward to its tools. Network activity during indexing was not monitored. The generated lockfile SHA-256 digests were `9ff31e9c38e091c79bc21eae3dc0be3aecba656ffec260a73873a630ce4270ee` and `9c4365b17d6655df8fe2dddec58d1c24fbad9003e6a5ee486bcc5bfd815c70c8`, respectively. Generating locks against a later registry may select different versions.

Equivalent Linux commands, with the two built binaries in `BIN` and a fresh absolute directory under `/var/tmp` in `D`:

```sh
git clone --depth 1 --branch 1.5.0 https://github.com/BurntSushi/byteorder "$D/byteorder"
git clone --depth 1 --branch 2.7.6 https://github.com/BurntSushi/memchr "$D/memchr"
mkdir "$D/adapters"
cp "$BIN/gosanja-adapter-rust" "$D/adapters/"
export GOSANJA_ADAPTER_PATH="$D/adapters"
listing() {
  (cd "$root" && { find . -printf '%y %m %p %l\n'; find . -type f -exec sha256sum {} +; }) | LC_ALL=C sort
}
for name in byteorder memchr; do
  root="$D/$name"
  out="$D/logs/$name"
  mkdir -p "$out/raw"
  (cd "$root" && cargo generate-lockfile && cargo metadata --locked --format-version 1 > "$out/metadata")
  (
    export CARGO_NET_OFFLINE=true
    listing > "$out/before"
    "$BIN/gosanja" adapter list
    time "$BIN/gosanja" index --root "$root" --out "$out/snapshot"
    "$BIN/gosanja" validate "$out/snapshot" --root "$root"
    listing > "$out/after"
    cmp "$out/before" "$out/after"
    "$D/adapters/gosanja-adapter-rust" index --root "$root" --out "$out/raw"
    wc -l "$out/raw/syntax.jsonl"
    jq -r '.gap' "$out/snapshot/gaps.jsonl" | sort | uniq -c
  )
done
```

Both snapshots validated. Before and after listings were byte-identical in each clone, including the generated lockfile and Git metadata. Their SHA-256 digests were `e5a745a4ea0ba7eb4536bc56f00eb430a1eebbf424f0e2d426ecf7dd0b9abddd` (byteorder, 91 listing lines) and `e1c29cc381dc228c0f9b92f56e1dc4b5e2e183d10a4ff9b07efd8cb533c35225` (memchr, 450). Cargo metadata listed zero custom-build targets for either root package. The listings measure net changes inside each clone. They do not track transient writes or dependency build outputs outside it.

## Peak memory and time by stage

Local measurements on 2026-11-27 and 2026-11-28, made for https://github.com/iMUngHee/gosanja/issues/10, on `x86_64-unknown-linux-gnu`: an AMD Ryzen 7 9850X3D with 16 threads and 30 GiB of memory, transparent huge pages `always`. The adapter was built with `cargo build --release` from `35a8843`. rust-analyzer was 0.3.3049-standalone, the 2026-09-14 release.

Variants of the adapter and of its rust-analyzer input in this section are trials made outside the repository. None of them is what the adapter does at `35a8843`.

### Inputs

| Input | Source | `.rs` files, lines | Packages in `Cargo.lock` | Toolchain |
| --- | --- | --- | --- | --- |
| this repository | `35a8843`, 4 members | 89 outside `fixtures/`, 40,365 | 104 | rustc 1.99.1 (58cf8f9c3 2026-10-13), Cargo 1.99.1 (5f94df478 2026-08-27) |
| typst | https://github.com/typst/typst/tree/v0.15.1, `9dfd3a08500b7896045f907433cf7b4b02434fad` | 405, 155,352 | 425 | rustc 1.100.0 (0ef1bc386 2026-11-09), Cargo 1.100.0 (3acc79c0c 2026-11-05) |
| private workspace P | the tree issue 10 describes: 3 members, toolchain file pinning 1.99.1 | 272 tracked, 137,858 | 388 | rustc 1.99.1, Cargo 1.99.1 |

Each input was a fresh clone under `/var/tmp` with its dependencies fetched once by `cargo fetch --locked`. `git status --short` printed nothing after every run.

### Method

A sampler started the measured command and, every 20 ms, walked `/proc/<pid>/task/*/children` from it. For every process it kept the largest `VmHWM` of `/proc/<pid>/status`, and for every sample it summed `VmRSS` over the tree. The sum counts a shared page once per process that maps it. A process that lives less than one interval can be missed. Stages are the lifetimes of the tools the adapter starts. Runs were pinned to 4 CPUs, the size of the host in issue 10, unless a row says otherwise.

```sh
taskset -c 0-3 python3 -I peaks.py run.tsv -- gosanja-adapter-rust index --root "$ROOT" --out "$OUT"
cd "$ROOT" && CARGO_TARGET_DIR="$SCRATCH" taskset -c 0-3 python3 -I peaks.py ra.tsv -- \
    rust-analyzer scip . --output "$SCRATCH/native.scip"
```

### Whole run of the adapter

P, all layers, 4 CPUs, 364.4 s:

| Stage | Window | Largest single process | Largest tree sum |
| --- | --- | --- | --- |
| calls build (`cargo rustc`) | 0.1-72.2 s | rustc 3,407 MiB; rust-lld 1,328 MiB | 3,332 MiB |
| unit capture (`cargo check --workspace --all-targets`) | 72.2-102.9 s | rustc 1,547 MiB | 2,749 MiB |
| `rust-analyzer scip` on the generated `rust-project.json` | 103.8-245.0 s | rust-analyzer 6,417 MiB | 6,572 MiB |
| build layer, four `cargo check` runs | 246.0-360.6 s | rustc 1,549 MiB | 3,377 MiB |
| adapter after its last tool (IR and rows) | 360.6-364.4 s | the adapter, 2,711 MiB | - |

The other two inputs:

- typst, 4 CPUs, 415.7 s. The calls build took 0.1-357.9 s, with rust-lld at 3,834 MiB, ld.lld at 3,309 MiB, rustc at 2,955 MiB and a tree sum of 4,297 MiB. The capture check took 358.0-415.4 s, with rustc at 999 MiB. The base layer then failed with `missing proc-macro library` (https://github.com/iMUngHee/gosanja/issues/11), so no rust-analyzer stage ran.
- This repository, 12 CPUs, 54.8 s. The calls build peaked at rustc 1,257 MiB and a tree sum of 2,784 MiB. rust-analyzer peaked at 1,900 MiB. The adapter peaked at 1,145 MiB after its last tool.

### rust-analyzer in cargo mode and on the split graph

The adapter indexed all three inputs through its split path. Cargo mode is the command above without the adapter.

| Input | Cargo mode | Split graph | Index bytes, cargo mode / split |
| --- | --- | --- | --- |
| this repository | 1,478 MiB, 6.4 s | 1,913 MiB, 6.5 s | 7,481,692 / 8,962,614 |
| typst | 3,207 MiB, 47.5 s | not run (issue 11) | 26,701,063 / - |
| P | 3,611 MiB, 43.7 s | 6,417 MiB, 141.2 s | 28,173,503 / 44,932,294 |

P's graph held 475 crates. Of these, 79 were workspace units: 3 libraries, 3 library tests, 2 binaries, 2 binary tests, 67 integration tests and 2 build scripts. The other 396 were units of 270 dependency packages. This repository's graph held 140 crates, 31 of them workspace units.

Every workspace unit reads its own copy of the files it compiles. P's 67 integration-test units held 587 copies of 75 paths, and 8 helper modules under `tests/common/` were copied 65 times each. In the raw index each copy is a document of its own. In the published index, `tests/common/mod.rs` has 259 definition ranges. 184 of them carry 65 `local` symbols each, one per unit. The other 75 carry one global symbol.

### Variants of the rust-analyzer input

Each variant rewrote P's generated `rust-project.json` and ran `rust-analyzer scip` on it directly, with the adapter's configuration `{"cfg":{"setTest":false}}`. V0 is the graph as generated. A comparer read the workspace documents of both raw indexes and compared each document's multiset of (range, symbol, roles) and its symbol information, with the adapter's unit names (`<package>__gosanja_unit_<stem>`) kept in the symbols. For shared views it compared, for each `tests/` path, the non-local occurrences of every V0 copy with the single shared document. Batched variants were merged first; a document that two batches both wrote had to be equal in both. Every index had empty `external_symbols` and equal metadata.

A first pass of these comparisons removed the unit names before comparing. That hid changes in published names: the dependency merge below looked identical then and differs with the names kept. The table gives only comparisons made with the names kept.

| Variant | Peak | Time | Output against V0, unit names kept |
| --- | --- | --- | --- |
| V0 | 6,419 MiB | 140.3 s | - |
| dependency crates equal in cfg, env, edition, target and merged dependencies merged into one (475 to 422) | 6,234 MiB | 145.2 s | 116 of 965 documents differ: references into a merged crate now name another unit |
| as above, dependencies reading their package directory instead of a copy | - | killed by SIGSEGV after 32.3 s | - |
| dependencies of every non-member proc-macro crate removed, then unreachable crates dropped (475 to 331) | 5,436 MiB | 147.1 s | identical |
| as above, keeping the dependencies of the 2 of 18 non-member proc-macro crates whose sources hold an item-level macro invocation (475 to 338) | 5,500 MiB | 131.9 s | identical |
| workspace units in 2 batches split by source size, each the closure of its roots, workspace crates outside a batch's roots read from a hard-linked mirror outside the index root | 5,109 MiB | 84.3 + 70.0 s | identical; no document in two batches |
| proc-macro trim of all 18, then the same 2 mirrored batches | 4,164 MiB | 85.7 + 69.7 s | identical; no document in two batches |
| integration-test units share one view, with the proc-macro trim of all 18 | 4,226 MiB | 52.2 s | non-test documents identical; the 67 test paths of a single unit identical; the 8 helper paths under `tests/common/` differ, as V0's 65 copies of each also differ among themselves in the test crate's unit name; `local` occurrences 207,374 to 37,390 |
| as above, in 2 mirrored batches | 3,365 MiB | 23.0 + 29.8 s | as the row above |
| `--num-threads 1` | 6,406 MiB | 186.2 s | identical |
| `{"lru":{"capacity":16}}` added | 6,417 MiB | 187.2 s | identical |
| `MALLOC_ARENA_MAX=2` | 6,289 MiB | 199.5 s | identical |
| transparent huge pages disabled with `prctl(PR_SET_THP_DISABLE)` | 6,416 MiB | 151.5 s | identical |
| diagnostic: library and binary test units removed | 5,656 MiB | 133.0 s | lossy |
| diagnostic: integration-test units removed | 4,400 MiB | 43.6 s | lossy |

The four settings rows ran while other trials used CPUs 4-15, so only their peaks compare with V0, not their times. At the 2026-09-14 tag, `update_base_query_lru_capacities` and `update_lru_capacities` have their bodies commented out (https://raw.githubusercontent.com/rust-lang/rust-analyzer/2026-09-14/crates/ide-db/src/lib.rs). `scip` loads with `prefill_caches: true` and takes only its thread count from `--num-threads` (https://raw.githubusercontent.com/rust-lang/rust-analyzer/2026-09-14/crates/rust-analyzer/src/cli/scip.rs). The binary links glibc dynamically and contains no mimalloc or jemalloc symbol names. A second session ran V0 with `LD_PRELOAD=/usr/lib/libjemalloc.so.2` (jemalloc 5.4.0): 6,320 MiB / 141.2 s, and 6,260 MiB / 144.8 s with `MALLOC_CONF=dirty_decay_ms:0,muzzy_decay_ms:0,narenas:4`, with the index bytes after its header equal to V0's.

Why the trim keeps two crates' dependencies: the indexer finds a procedural macro by analysing the macro crate's own source, and a dependency's macro can generate the declaring function, as `synstructure::decl_derive!` does. A crate whose files hold an item-level macro invocation keeps its dependency edges. On P the two kinds of trim gave the same output.

Why shared views were not pursued: sharing is only sound when every crate resolves the shared files the same way, and syntax cannot show that in general. A trait implemented in one crate root and not in another changes which method a helper's call resolves to. On P, 1 of the 8 helper files uses a `crate::` or `super::` path and 12 of the 67 test roots hold `impl` items, so a rule strict enough to be sound shares nothing there.

### The calls build and the IR reader

`-C debuginfo=line-tables-only` replaced `-C debuginfo=2` in a copy of the adapter. With all layers:

| Input | `calls.jsonl` | IR bytes, all modules | Largest module | rustc peak | Adapter peak |
| --- | --- | --- | --- | --- | --- |
| this repository, 12 CPUs | 1,654 rows, sorted files equal | 412,704,364 to 220,610,433 | - | 1,255 to 939 MiB | 1,143 to 635 MiB |
| P, 12 CPUs | 5,192 rows, sorted files equal; calls layer of `result.json` equal | 747,521,209 to 468,089,718 | 616,587,180 to 389,972,718 | 3,407 (4 CPUs) to 2,694 MiB | 2,711 to 1,771 MiB |

The adapter's peak comes from reading one module, not from keeping all of them. A probe read every module of a run as `calls::finish` does, once keeping each parsed module and once dropping it:

| IR | Modules kept | Modules dropped |
| --- | --- | --- |
| this repository, `debuginfo=2`, largest 235,419,360 bytes | 1,114,628 kB | 1,088,272 kB |
| typst, `debuginfo=2`, 23 modules, 1,215,783,482 bytes, largest 543,460,297 | 2,327,268 kB | 2,294,752 kB |
| P, `debuginfo=2`, 747,521,209 bytes | 2,448,872 kB | 2,424,824 kB |

P's largest `line-tables-only` module held 952,275 metadata nodes, 602,071 of them `DILocation`, and 325,727 call or invoke instructions. The reader keeps the module's text and every node of an understood kind as a `BTreeMap<String, Value>`. A copy that kept only the twelve keys the calls code reads, and read the file line by line in two passes instead of holding its text, lowered the probe's peak on P's `line-tables-only` IR from 1,465,300 kB to 937,648 kB with the first change and to 556,644 kB with both. The adapter built from that copy, with `line-tables-only`, peaked at 884 MiB on P. Its `calls.jsonl` and the calls layer of `result.json` equalled the HEAD run's. The adapter's tests passed in that copy except `the_probe_gives_its_row`, which expects `-C debuginfo=2` in the command.

On typst, a copy that also passed `-C linker=` a program that only creates the file named by `-o` wrote all 23 modules byte-identical. rust-lld's peak fell from 3,832 to 127 MiB, since only build scripts and procedural macros were still linked. The rustc peak went from 2,264 to 2,312 MiB and the tree sum from 5,383 to 5,980 MiB on 12 CPUs.

A second session measured why the calls build takes 72 s on P: each member's library is compiled once as a dependency of its binaries and once more by `cargo rustc --lib` with the IR options. A probe with Cargo alone replaced the per-target builds by one `cargo build` per package and a `RUSTC_WRAPPER` that added the IR options to the selected units only: 70.9 s became 54.3 s, and 4 of the 5 modules were byte-identical, the fifth differing in one line that names a file under `OUT_DIR`. It was not taken further, for two risks a plan review raised and nobody constructed. Unverified: a wrapper appends its options after the configured `rustflags` and so reverses their precedence. Unverified: a member library that Cargo already built as another package's dependency is fresh, so the wrapper never sees it and its IR is missing; on P every module was written.

### Cargo checks sharing one build directory

Local measurement on 2026-11-27 and 2026-11-28, Linux x86_64, 16 logical CPUs with each run pinned to 4 by `taskset`, gosanja `35a8843`, rustc and Cargo 1.99.1, rust-analyzer 0.3.3049-standalone. Inputs: private workspace P (3 members, 475 compiler artifacts in `cargo check --workspace --all-targets`) and a `git archive` copy of gosanja at `35a8843`.

At `35a8843` the adapter runs five checks, each in a directory of its own: the capture check before the indexer (`work/unit-target`, `work/unit-build`) and the build layer's four (`work/check-0` to `work/check-3`). Every one compiles the dependencies from nothing.

**Cargo alone.** The five commands were run in order with one `CARGO_TARGET_DIR` and `CARGO_BUILD_BUILD_DIR`, the adapter's child environment rebuilt by hand, a pass-through script as `RUSTC_WRAPPER` for the first and `RUSTC` set to the sysroot compiler for the rest. C stands for `cargo check --workspace --locked --message-format=json -v`.

| Command on P | Wall | `Running` lines | Artifacts, fresh |
|---|---|---|---|
| C `--all-targets` (wrapper) | 31.0 s | 506 | 475, 0 |
| C `--all-targets` | 2.6 s | 74 | 475, 402 |
| C `--all-targets --profile test` | 2.5 s | 73 | 473, 401 |
| C `--all-targets --all-features` | 2.6 s | 74 | 475, 402 |
| C `--lib --bins` | 9.2 s | 25 | 385, 361 |

The sum is 47.9 s. The check under the wrapper and the one with `RUSTC` set share fingerprints. The units rebuilt every time belong to one member whose build script names a `rerun-if-changed` path that does not exist, and to what depends on it; Cargo's `Dirty` lines say so.

**Why the adapter cannot share the directory as it is.** `units::read` takes a unit's options from the `Running` line that `-v` prints, and Cargo prints none for a unit it finds fresh. A fresh artifact then has no invocation, the run is incomplete, and `inventory` refuses a dep-info file no invocation of the same run explains.

**Prototype (a proposal, not in the repository).** A copy of `35a8843` changed in three files: the capture check passes `-v` and keeps its stderr; the four checks use the capture check's directories; `units::read` keeps a map from dep-info path to invocation across the runs, seeded from the capture log, matches an artifact reported `fresh` against the whole map and any other artifact against its own run only, and checks the inventory against the map's keys. Baseline and prototype ran at the same time on separate sets of 4 CPUs:

```sh
taskset -c "$CPUS" gosanja-adapter-rust index --root "$ROOT" --out "$OUT" --layers base,syntax,build
for f in build.jsonl syntax.jsonl result.json index.scip; do cmp "$BASE/$f" "$PROTO/$f" && echo "$f identical"; done
```

```text
build.jsonl identical
syntax.jsonl identical
result.json identical
index.scip identical
```

That output was the same for both inputs.

| Input | Baseline | Prototype | Checks, baseline | Checks, prototype |
|---|---|---|---|---|
| P | 299.2 s | 202.5 s | 34.5 + 117.9 s | 34.6 + 21.5 s |
| gosanja copy | 34.1 s | 14.3 s | not split | not split |

Known gaps: one run of each; the two sides ran concurrently, so the times include contention. On P the baseline already skips all 270 files as `targets-incomplete` (a `-l` option makes three dependency units opaque, https://github.com/iMUngHee/gosanja/issues/7), so equal output there says little about the path where a run is complete; the gosanja copy has no skips and covers it. Not tried: a capture check that fails or is not run, a second index into a directory that already holds artifacts, Windows, and the calls layer. The prototype's unit tests were not updated.

Unverified: that Cargo's fingerprint covers everything `units::read` takes from a command line, so that a fresh unit's earlier `Running` line still describes it. The identical outputs agree with this and do not prove it.

### The changes together

Measured on 2026-11-28 with two binaries built `--release`: B1 from `9b4b5bc`, which fixes https://github.com/iMUngHee/gosanja/issues/7 and https://github.com/iMUngHee/gosanja/issues/11 and nothing else, and T from `9b2a0d1`, which adds the procedural-macro trim, the shared build-layer checks, `-C debuginfo=line-tables-only` and the IR reader that keeps only the fields it reads and reads a module in two passes. The inputs were frozen: this repository as an archive of `9b4b5bc`, typst at `9dfd3a0` and P at its tagged commit, each with a warm Cargo home. This repository and P pin Cargo 1.99.1; typst pins no toolchain and ran with the host's Cargo 1.100.0. Every run used `taskset -c 0-3`, all layers and a fresh output directory, and ran `gosanja index` and `gosanja validate` for the snapshot and then the adapter directly, under the sampler, for its own output. The sampler recorded each process's parent and refreshed its command line, so a stage is one direct child of the adapter.

B1 ran three times on each input. In each input its three snapshots agreed row for row, `manifest.json` included: 57,396 rows for this repository, 202,422 for typst and 197,653 for P. So no run-to-run noise was seen. T's snapshot of each input equalled B1's in every row. Its `calls.jsonl`, sorted, and the `layers.calls` and `builds` of its `result.json` equalled B1's too. On this repository and P no build-layer check of T ran again in directories of its own. On typst T's second check did: Cargo 1.100 saves what a build script printed in `run/stdout` beside its `OUT_DIR`, where earlier versions write a file named `output`, and T read only `output`. T2, built from `8093bfb`, reads whichever of the two is there; on typst no check of T2 ran again, and its snapshot, sorted `calls.jsonl`, `layers.calls` and `builds` equalled B1's.

On P, B1 and T then ran three times each, alternating. Medians:

| Stage | B1 | T |
| --- | --- | --- |
| whole run | 358.4 s | 281.2 s |
| calls build | 70.3 s; rustc 3,349 MiB; tree 3,425 MiB | 67.5 s; rustc 2,660 MiB; tree 2,705 MiB |
| capture check | 31.4 s; 1,509 MiB; tree 2,663 MiB | 31.3 s; 1,508 MiB; tree 2,705 MiB |
| `rust-analyzer scip` | 133.7 s; 6,414 MiB; tree 6,569 MiB | 130.8 s; 5,667 MiB; tree 5,831 MiB |
| build-layer checks | 115.1 s; 1,513 MiB; tree 3,301 MiB | 45.7 s; 1,503 MiB; tree 3,276 MiB |
| adapter process | 2,710 MiB | 881 MiB |

A stage's tree sum can include the last processes of the stage before it, which is why the capture check's matches the calls build's. T's graph for P held 339 crates, B1's 475. The trim T implements reads only the root file of each outside procedural-macro crate and accepts no `derive`, so its graph differs from the trial's 338 crates and its peak, 5,667 MiB, from the trial's 5,500 MiB.

One run of each binary on the other inputs, B1's being the first of its three. On typst T2 replaces T; T's run there is kept below for the checks it repeated:

| Input | B1 | T, or T2 on typst |
| --- | --- | --- |
| this repository | 57.6 s; rust-analyzer 1,913 MiB, 6.7 s | 38.3 s; rust-analyzer 1,804 MiB, 6.4 s |
| typst | 861.8 s; rust-analyzer 5,502 MiB; calls build 472.1 s, rust-lld 3,838 MiB; build-layer checks 285.1 s; adapter 2,538 MiB | 513.5 s; rust-analyzer 5,186 MiB; calls build 350.8 s, rust-lld 3,830 MiB; build-layer checks 79.9 s; adapter 793 MiB |

T's run on typst took 652.3 s, 216.8 s of it in the build-layer checks; its other stages were within 1% of T2's.

The gate ran on T2's tree: `scripts/check.sh` and `scripts/self-check.sh` against `35a8843` passed, and the Rust part of `scripts/check-live.sh` reproduced every checked-in Rust index and snapshot byte for byte and passed the 12 live tests. Its Go part did not run: this host has scip-go 0.2.9, and the script requires 0.2.10.

### Known gaps

- Every figure before "The changes together" comes from one run, and so do that subsection's runs on this repository and typst.
- The graph variants were measured on rust-analyzer's raw output. The adapter did not restore paths from them or publish a snapshot.
- None of the runs was on a 7 GB host or under a memory limit.
- A shared view assumes that a file shared by several units resolves the same way in each. On P all 65 copies of every shared path agreed. Where a shared module's `crate::` paths reach items that differ between crates, this has not been tested.
- The graph variants have no typst measurement: at `35a8843` its base layer failed (issue 11). B1 and T index it.
- rust-analyzer releases after 2026-09-14 were not measured.
