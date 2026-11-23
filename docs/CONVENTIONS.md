# Conventions

Rules that the code does not show by itself. The reasons behind the larger ones are in
[ARCHITECTURE.md](ARCHITECTURE.md) and the records under [research/](research/README.md).

## What the engine may claim

- **Facts come from compilers and indexers.** No language model takes part in the analysis,
  and nothing is inferred from names or text patterns.
- **A gap, never a guess.** When an input does not establish something, the engine leaves it
  out, and when it can tell that something is missing it writes a row to `gaps.jsonl`. A
  reference is not a call; a declaration is not what runs.
- **Every row says where it came from** (`src`), and a call says how far its target was
  established (`resolution`).
- **Same input, same bytes.** Output is sorted, carries no timestamp and no absolute path, and
  does not depend on the order in which an indexer happened to write things.

## Boundaries

- **The core knows no language.** Language knowledge lives in adapters, which are separate
  programs speaking the [adapter contract](../spec/adapter-contract.md). Code in `crates/core`
  that branches on a language name, or recovers meaning from source text, is a defect.
- **Indexer quirks are keyed by SCIP scheme** and kept in one small module. A trait is
  introduced when a second indexer needs different behaviour, not before.
- **The core does no I/O** except running an adapter (`adapter.rs`) and writing a snapshot
  (`write.rs`, `publish.rs`). Source text reaches it through one reader, so a test can see what
  was asked for. The command-line crate does the rest.
- **The engine and its adapters leave the analysed tree alone.** The engine creates, changes
  and deletes nothing under the root being indexed, and the output path may not overlap it.
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
  `fixtures/regenerate.sh` rebuilds the indexes; the tests need no indexer.
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
