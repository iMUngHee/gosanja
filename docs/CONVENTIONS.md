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
  it makes and with the toolchain `rust-toolchain.toml` names. It replaces a snapshot only
  when its calls layer is present and holds the control relation `fixtures/README.md` names.
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

## The gate

```sh
scripts/check.sh
```

It runs the format check, clippy with `-D warnings`, the tests, the doctests and `cargo deny`.
There is no hosted CI yet, so this script is what a change has to pass.

## Commits

Conventional Commits: `feat(core): …`, `docs(spec): …`, `test(fixtures): …`. The subject is an
imperative phrase of about fifty characters; the body says why when the reason is not obvious.
