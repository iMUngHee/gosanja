# Conventions

Rules that the code does not show by itself. The reasons behind the larger ones are in
[ARCHITECTURE.md](ARCHITECTURE.md) and the records under [research/](research/README.md).

## What the engine may claim

- **Facts come from compilers and indexers.** No language model takes part in the analysis,
  and nothing is inferred from names or text patterns.
- **A gap, never a guess.** When an input does not establish something, the engine leaves it
  out, and when it can tell that something is missing it writes a row to `gaps.jsonl`. A
  reference is not a call; a declaration is not what runs.
- **Every row says where it came from.** A row of a snapshot has `src`, and a call says how far
  its target was established (`resolution`). A row of a diff is a comparison of two snapshots,
  and the diff names its two inputs in its manifest; only a relation row of a call carries a
  `src`, the snapshot's, because the layer that recorded a call is part of what the call is.
- **Same input, same bytes.** Output is sorted and does not depend on the order in which an
  indexer happened to write things. The engine adds no timestamp and no path of the machine it
  runs on; every path field is relative to the analysed root. A value copied from an input, such
  as an attribute an adapter reported, is written as it was recorded, whatever it contains.

## Boundaries

- **The core knows no language.** Language knowledge lives in adapters, which are separate
  programs speaking the [adapter contract](../spec/adapter-contract.md). Code in `crates/core`
  that branches on a language name, or recovers meaning from source text, is a defect.
- **Indexer quirks are keyed by SCIP scheme** and kept in one small module. A trait is
  introduced when a second indexer needs different behaviour, not before.
- **The core does no I/O** except running an adapter (`adapter.rs`) and writing a snapshot or
  a diff (`write.rs`, `publish.rs`). Source text reaches it through one reader, so a test can see what
  was asked for. The command-line crate does the rest.
- **The engine and its adapters leave the analysed tree alone.** The engine creates, changes
  and deletes nothing under the root being indexed, and the output path may not overlap it.
  A diff's output may not overlap either of the snapshots it compares, and both are only read.
  An adapter and the tools it runs do the same of their own accord; what the project's own
  build code or configuration writes there is outside the rule, as the adapter contract
  states, and an adapter documents it.

## Specifications

`spec/` is normative. A change to a document, its JSON Schema, the goldens under `fixtures/` and
`CHANGELOG.md` go in one commit; the change classes and the version rules are in
[spec/README.md](../spec/README.md).

## Fixtures and tests

- Fixtures are small projects written for this repository. Never copy code, names or paths from
  a private project into a fixture, a test or a document.
- Each fixture has a checked-in `index.scip` and one source-level fact a test asserts.
  `fixtures/regenerate.sh` rebuilds the indexes, and the snapshots that hold the Rust
  adapter's output (`rust-flow`, `rust-calls`, both sides of `diff-basic`), in an environment
  it makes and with the toolchain `rust-toolchain.toml` names. It replaces such a snapshot only
  when its calls layer is present and holds the control relation `fixtures/README.md` names.
  It also rebuilds `go-flow/expected` with the Go adapter, which it replaces only when the base
  and syntax layers are present.
  With `--out` it writes elsewhere and touches nothing under `fixtures/`. The tests need no
  indexer.
- `fixtures/diff-public` is the exception: a frozen pair of snapshots of a public crate, kept as
  the measurement read them, with its provenance and licence in `NOTICE.md` and the pairs listed
  before any matcher ran in `truth.tsv`. It has no index of its own and is never rebuilt; a
  change to it changes what the matcher's test measures. Its snapshots keep the schema they
  were written in when the schema moves on, and the reader refuses them, as it must refuse any
  other minor. The matcher's test therefore copies them and re-stamps only the copies'
  `schema` to the current version, which holds only while the matcher reads no field that the
  two schemas write differently, and the tests check that the originals are refused.
- Edge cases (encodings, colliding symbols, malformed input) are built in tests from `scip`
  types, not stored as binary files.
- Goldens under `fixtures/*/expected` are compared byte for byte. Tests that look for a
  particular fact find rows by name and file, not by key, so a change of key format shows up in
  the goldens and nowhere else.
- A test must be able to fail: assert the expected value, not only that two outputs agree.

## Rust

- Edition 2024; the toolchain is pinned in `rust-toolchain.toml`.
- Lints come from `[workspace.lints]` and warnings are errors in the gate. An exception is an
  item-level `#[expect(lint, reason = "...")]`, never `#[allow]`.
- `unsafe` is not used.
- An error that a caller can act on is a typed error; a broken invariant panics with a message
  that says what was expected.
- Before using a dependency's API, read the locked version (`cargo tree -i <crate>`), not
  memory. A new dependency's licence must be in `deny.toml`; adding a licence there needs a
  reason in a comment.

## Go

The Go adapter in `adapters/go` is a Go module of its own.

- `gofmt` formats it, and `go vet` passes.
- Its tests that run the adapter or the engine build the environment of every process they
  start, as the Rust tests do, and run against the stub `scip-go` in `fixtures/stub-tools`.
  A test that needs the engine fails, and is never skipped, when the engine does not build.
- Before using a dependency's API, read the version `go.mod` requires (`go list -m <module>`),
  not memory. A new module in the build list needs its licence recorded in
  `docs/research/go-adapter-measurements.md`; there is no Go counterpart of `cargo deny`.

## The gate

```sh
scripts/check.sh
```

It runs the format check, clippy with `-D warnings`, the tests, the doctests and `cargo deny`.
Then, for `adapters/go` and with `GOFLAGS=-mod=readonly GOPROXY=off GOTOOLCHAIN=local
GOWORK=off`, it checks that `go version` is Go 1.27.3, that `gofmt -l` prints nothing, and
runs `go vet`, `go test -count=1 -v` and `go mod verify`. The only tests it may skip are the
case-alias cases, which need a case-insensitive volume, and the tool-tag case on an
architecture that has no architecture-level build tag.
There is no hosted CI yet, so this script is what a change has to pass.

## The engine's map of its own code

```sh
scripts/self-check.sh            # against the merge base of HEAD and main
scripts/self-check.sh <base>     # against any commit
```

A change to code is checked with the engine itself, beside the gate. The script builds
`gosanja` and the Rust adapter from the working tree, indexes the base commit and the working
tree with the base and syntax layers, and reports four things:

- **Function.** Both snapshots validate, no layer failed, and the row counts, the gap counts
  by kind and a `gosanja diff` of the two are printed. A gap kind that grows without a reason
  is a defect of the change or a finding about the engine.
- **Layering.** The references between workspace packages, checked against the dependencies
  [AGENTS.md](../AGENTS.md) allows: the command line uses the core and the schema, the core uses
  the schema, and the Rust adapter uses the schema and, in its tests only, the core. Any other
  edge fails the check.
- **Coupling.** The pairs of source modules (a crate's top-level file or directory under
  `src/`) whose references to each other grew, largest growth first. A new pair, or a large
  growth between modules that had little to do with each other, is something the change has to
  justify, or move into the module that owns the concept.
- **Dead code.** Definitions that nothing in the snapshot refers to and that this change added
  or left unreferenced, leaving out what is reachable from outside (`pub` items, exported Go
  names, `main`, trait implementations) and test code. The list is of candidates: a use the
  indexer does not record, inside a `macro_rules!` body, in a format string or in an attribute
  string, leaves a used item listed. Each line is removed with the code or explained.

It exits 1 when a snapshot does not validate, a layer failed, or an edge breaks the layering.
The Go module under `adapters/go` is mapped the same way when the change touched it and `go`
and `scip-go` are on `PATH`. The check needs `jq`, and writes only to a temporary directory.

## Commits

Conventional Commits: `feat(core): …`, `docs(spec): …`, `test(fixtures): …`. The subject is an
imperative phrase of about fifty characters; the body says why when the reason is not obvious.
