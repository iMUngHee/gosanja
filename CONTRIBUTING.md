# Contributing to gosanja

gosanja is at an early stage: the two interfaces are at version 0.1 and may still change in
breaking ways. Read [docs/CONVENTIONS.md](docs/CONVENTIONS.md) before changing code; it is short
and holds the rules the code cannot show.

## Getting started

You need the Rust toolchain named in `rust-toolchain.toml` (rustup installs it on first use) and
[`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny).

```sh
scripts/check.sh
```

This is the gate every change has to pass. The tests need no indexer; they read the indexes
checked in under `fixtures/`.

## Kinds of contribution

**A fix or a feature in the engine.** Add a test that fails without your change. If the change
alters what a snapshot contains, the goldens under `fixtures/*/expected` change too; say in the
pull request why each difference is right.

**A change to an interface.** The snapshot and the adapter contract are specified under
[spec/](spec/README.md). Open an issue first and describe what a reader or an adapter author
would have to do differently. A change touches the document, the JSON Schema, the goldens and
`CHANGELOG.md` together.

**A language adapter.** An adapter is a separate program; it does not have to live in this
repository or be written in Rust. [spec/adapter-contract.md](spec/adapter-contract.md) is
everything it has to satisfy, and `spec/schema/` lets you test its output on your side.

**A fixture.** Write the project yourself and keep it small. Do not copy code from anywhere
else. Add its regeneration command to `fixtures/regenerate.sh` and its known fact to
`fixtures/README.md`.

**Research.** Records of what was measured or read live in `docs/research/`; follow the rules
at the top of [docs/research/README.md](docs/research/README.md).

## Licence

gosanja is licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Unless you state otherwise, any contribution you
submit for inclusion is licensed the same way, without additional terms or conditions.
