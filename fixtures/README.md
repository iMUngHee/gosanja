# Fixtures

Small projects written for this repository. Each has a checked-in `index.scip` and one
known source-level fact that a test asserts. Edge cases such as other position encodings,
colliding symbols and corrupt input are built in the tests from `scip` types and are not
stored here. What each indexer writes for these fixtures is recorded in
[`docs/research/scip-indexer-differences.md`](../docs/research/scip-indexer-differences.md).

| Fixture | Known fact (zero-based, end exclusive, in the document's unit) | Checked-in index produced by |
| --- | --- | --- |
| `rust-basic` | `render` is defined at `src/views.rs` [2,7,13] with body [2,0,8,1]; the reference to `empty` at [4,8,13] lies in that body | `rust-analyzer 0.3.3049-standalone`, rustc 1.100.0 |
| `rust-flow` | a workspace of two path-linked crates; `flow` is defined at `app/src/main.rs` [16,9,13] and holds every construct of the syntax layer's outline; its hand-derived outline and the item facts of both crates are the literals of `crates/adapter-rust/tests/syntax_cases.rs` | `rust-analyzer 0.3.3049-standalone`, rustc 1.99.1 |
| `rust-recall` | every identifier token in `src/lib.rs` that names a definition of the crate, other than that definition's own name, is a row of `expected-refs.tsv`: file, line, start byte column, then the file, line and column of the name token it names | `rust-analyzer 0.3.3049-standalone`, rustc 1.99.1 |
| `rust-calls` | a workspace of a library, three binaries (one never built) and a procedural macro, holding the constructs the calls layer has to publish or withhold; `NOTE.md` records, per call, the reference the index holds on its line, the definition the call may reach and the rows that follow | `rust-analyzer 0.3.3049-standalone`, rustc 1.99.1 |
| `rust-calls-variants` | three copies of the package `calls-variant`: `v0.1.0`, `v0.2.0` (only the version differs) and `no-default` (version 0.1.0, `default` enables no feature); each copy has its own `Cargo.lock` and `index.scip`, and its library's `direct` calls `leaf` | `rust-analyzer 0.3.3049-standalone`, rustc 1.99.1 |
| `go-basic` | `greet` is defined at `main.go` [2,5,10]; the reference to it at [7,5,10] lies in `main`'s body [6,0,8,1] | `scip-go` 0.2.10 built with Go 1.27.3 |
| `ts-basic` | `greet` is defined at `src/index.ts` [0,16,21]; the reference to it at [5,9,14] lies in `run`'s body [4,0,6,1] (UTF-16 columns) | `@sourcegraph/scip-typescript` 0.6.2 on Node 26.13.1 with TypeScript 6.0.3 |
| `diff-basic` | two versions of one crate, `old/` and `new/`, each with its index and with its snapshot in `expected/`; one case of every kind of change a diff reports, the three candidate scores worked by hand in `NOTE.md`; the diff of the two snapshots is `expected/`, equal to `spec/examples/diff-basic` | `rust-analyzer 0.3.3049-standalone`, rustc 1.99.1; snapshots by this workspace's engine and Rust adapter |
| `diff-public` | a frozen pair of public snapshots, base64 v0.20.0 and v0.21.0, with `truth.tsv`, the pairs listed before any matcher ran; provenance and licence in `NOTICE.md`; no index, and not rebuilt; at snapshot schema 0.2, which the reader refuses, so the matcher's test reads copies re-stamped to the current schema | gosanja 0.2.0 from `rust-analyzer 0.3.3049-standalone` indexes, rustc 1.99.1 (see `NOTICE.md`) |

## Regenerating the indexes

The tests read the checked-in indexes and need none of the indexers. To rebuild them, put
the indexers on `PATH` or name them in `RUST_ANALYZER`, `SCIP_GO` and `SCIP_TYPESCRIPT`,
then run:

```sh
fixtures/regenerate.sh                          # every fixture
fixtures/regenerate.sh rust-basic               # one fixture
fixtures/regenerate.sh rust-flow rust-recall
fixtures/regenerate.sh --out /var/tmp/regen rust-calls   # write elsewhere, touch nothing here
fixtures/regenerate.sh --indexes-only diff-basic         # indexes, no snapshots
```

Indexers write the directory they ran in into the index metadata, so the script copies each
fixture to the fixed path `/tmp/gosanja-fixtures/<fixture>`, indexes it there and copies
back only what it made. Nothing is built inside the repository. That path is shared by every
checkout on the machine, so a run holds the lock `/tmp/gosanja-fixtures.lock` for its whole
length; a second run stops and names the process that holds it. A lock left by a run that
was killed has to be removed by hand.

The Rust fixtures are indexed in an environment the script makes, not in the caller's:
- of the caller's environment only `PATH` is kept, with the indexer's directory first;
- the toolchain is the one `rust-toolchain.toml` names;
- the home and the Cargo home are made for the run and hold no configuration.

A flag in the contributor's environment or Cargo home therefore cannot give an index of other
definitions than the snapshot beside it. Only a fixture's own Cargo configuration, such as
`rust-calls/.cargo/config.toml`, is in force. Cargo also reads the configuration of every
directory above the one it runs in, so the script stops when `/tmp/gosanja-fixtures`, `/tmp`
or `/` holds a `.cargo/config` or `.cargo/config.toml`.

Without `--out`, the script replaces the checked-in files. With `--out <dir>`, it writes each
file to the same relative path under `<dir>` and writes nothing under `fixtures/`. It indexes
at the same fixed path, so the bytes are those of a normal run. `scripts/check-live.sh` uses
this to compare a fresh run with the checked-in files.

The snapshots of `rust-flow`, `rust-calls` and both sides of `diff-basic` are the adapter's
output. The script builds this workspace's `gosanja` and `gosanja-adapter-rust` into a
temporary directory, or uses those in the directory named by `GOSANJA_BIN`. It then runs
`gosanja index --adapter <that adapter>` on each copy, in the same environment as the
indexer.

A staged snapshot replaces nothing unless both of these hold:
- every scope of its `manifest.json` has `layers.calls` equal to `present`;
- its `relations.jsonl` holds a `call` row with `src` `adapter:rust/calls` from the control
  caller to the control callee named below.

A failed or empty calls layer therefore cannot become a golden. A fixture's files are put
in place only once all of them are staged and checked, so a failed run leaves the fixture as
it was. The script does not touch the diff `expected/`, which is the specification's
example, nor `diff-public`.

### Control relations

The script reads this table. Each row names a snapshot, then the entity keys of its control
call's caller and callee. Each control is a direct call between two functions of one target,
each on lines of its own, chosen from the source and the fixture's index. The IR of rustc 1.99.1
at opt-level 0 holds each of the four calls, and each checked-in snapshot holds its relation.

| Snapshot | Caller | Callee | Where |
| --- | --- | --- | --- |
| `rust-flow/expected` | `cargo:flow-kit:ping().` | `cargo:flow-kit:pong().` | `kit/src/lib.rs` 53-55 |
| `rust-calls/expected` | `cargo:calls-lib:direct().` | `cargo:calls-lib:leaf().` | `calls-lib/src/lib.rs` 16-18 |
| `diff-basic/old/expected` | `cargo:diff-basic:caller().` | `cargo:diff-basic:hidden().` | `old/src/lib.rs` 31-33 |
| `diff-basic/new/expected` | `cargo:diff-basic:caller().` | `cargo:diff-basic:hidden().` | `new/src/lib.rs` 31-33 |

### Indexers that do not repeat themselves

rust-analyzer and scip-typescript give the same bytes on every run. scip-go 0.2.10 does not:
the order of `Document.symbols` changes between runs, so a regenerated `go-basic/index.scip`
can differ from the checked-in one without any change in its facts. The index is kept as
the indexer wrote it, and tests must not depend on that order.

## The fake adapter

`fake-adapter/adapter.sh` is a shell script that speaks the adapter contract and serves the
checked-in index of `rust-basic`. Tests use it to exercise the engine's side of the contract
and the discovery of installed adapters without any real adapter. Options before the command
set what its manifest says (`--name`, `--version-json`, `--languages`, `--layers`,
`--encoding`, `--markers`, `--requires`), `--mode` selects one of its misbehaviours, and
`--record <file>` appends each command it is asked to run to a file. A test never lets
`GOSANJA_FAKE_MODE` from the developer's shell reach it.
