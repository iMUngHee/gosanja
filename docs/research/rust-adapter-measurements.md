# Rust adapter measurements

Local observations on 2026-11-24, Linux x86_64. The trials used rustc 1.99.1 (58cf8f9c3), Cargo 1.99.1 (5f94df478), rustup 1.29.1 and rust-analyzer 0.3.3049-standalone. Span probes used proc-macro2 1.0.107 with span-locations and syn 2.0.119 with full. The sibling-dependency snapshot used Gosanja 0.2.0. These measurements concern the index producer and its inputs. The final two sections reserve space for measurements of the adapter itself.

All inputs were synthetic. In command examples ROOT is the workspace, OUT is an existing directory outside it, and TARGET and BUILD are pre-created directories under OUT. Cargo ran offline with cached dependencies. One workspace had a binary and library package, a procedural-macro member, a build script, unicode-ident 1.0.27 from the registry and a sibling path dependency. The configuration probes used a locked library named counter with no project build code, optionally depending on itoa 1.0.18.

## Lockfile creation and preflight (F1, F3)

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

All three failures had empty stdout. All four metadata commands produced zero watched write events. Inotify measures visible filesystem events, not every attempted system call. Cache writes outside the watched project were outside the observation.

## Project build code and workspace writes (F2, D2)

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

## Cargo build directories, temporary files and logging (D2)

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

## Cargo cache placement (D2)

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

## Definition anchors and enclosing ranges (F4)

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

## Target documents and sibling dependencies (F5)

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

The ninth document was app/src/child.rs. Stderr also reported duplicate symbols for the different target roots and their main functions. The producer emitted an index despite these warnings. Buildability of disabled targets was not tested.

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

## Fixed-path index reproducibility (F6)

The runner executed two identical scip commands at the same path, specifying two cache-priming threads, before changing the build script. A second runner restored the original fixture and ran the same indexer four more times with its default thread setting. Output files and Cargo targets are outside the workspace. The first pair compared equal with `cmp a.scip b.scip`. All six files were 5,902 bytes and had this SHA-256:

```text
aa65ff4869181b0ca88a951a3d6c309c73f38f2a909a98b2b03432a1932b5d5e
```

This is evidence for one dependency-bearing workspace at a fixed path on one Linux host. Stderr varied between runs. The index metadata contains the absolute project URI. Relocated, cross-machine and cross-version byte equality remain unmeasured.

## Missing pinned toolchain (F7, D2)

The probe writes a toolchain file pinning `nightly-2001-01-01`, removes the toolchain override from the child environment and sets `RUSTUP_DIST_SERVER` to an ephemeral loopback HTTP server. The listener records paths and always answers 404. From that project the script runs `cargo --version` twice.

| RUSTUP_AUTO_INSTALL | Exit | Listener and stderr |
|---|---|---|
| Unset | 1 | GET /dist/2001-01-01/channel-rust-nightly.toml.sha256 and GET /dist/2001-01-01/channel-rust-nightly.sha256. Rustup reported syncing channel updates, then “no release found”. |
| 0 | 1 | Zero requests. Rustup reported that the toolchain was not installed and printed an installation suggestion. |

Both commands had empty stdout. The listener served no toolchain data. These results concern rustup 1.29.1 and this missing pin. In a second trial, the proxy started with a missing RUSTUP_HOME directory inside a synthetic project. Even with auto-install disabled it created that directory, then exited 1 reporting that toolchain 1.99.1 was unavailable in the new home.

```sh
RUSTUP_HOME="$ROOT/new-rustup" RUSTUP_TOOLCHAIN=1.99.1 RUSTUP_AUTO_INSTALL=0 cargo --version
```

## Unicode columns, span joins and BOM (F8)

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

## Reference recall (step 6, pending)

The real adapter test will record the hand-labelled total, the raw SCIP match count, and the snapshot reference match count, each over that same total. Record both exact lists of missed sites and the engine rule responsible for each reference lost during ingestion.

## Anchor miss rate (step 7, pending)

For each of two public crates, record its tag, tool versions and commands, syntax-layer state, skipped files, rows sent and unanchored rows divided by rows sent. Classify each unanchored row as inactive cfg, a base definition anchored elsewhere, or other. Record position-out-of-range and opaque-region counts separately.
