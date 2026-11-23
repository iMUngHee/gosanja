# Rust adapter measurements

Local observations on 2026-11-24, Linux x86_64. The synthetic probes used rustc 1.99.1 (58cf8f9c3), Cargo 1.99.1 (5f94df478), rustup 1.29.1 and rust-analyzer 0.3.3049-standalone. Span probes used proc-macro2 1.0.107 with span-locations and syn 2.0.119 with full. The sibling-dependency snapshot used Gosanja 0.2.0. The final two sections measure reference recall on a fixture and anchor misses on public crates.

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
