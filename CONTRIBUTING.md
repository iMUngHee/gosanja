# Contributing to gosanja

gosanja is at an early stage: the snapshot schema and the adapter contract are at version 0.3
and the diff schema at 0.2, and all three may still change in breaking ways. Read [docs/CONVENTIONS.md](docs/CONVENTIONS.md) before changing code; it is short
and holds the rules the code cannot show.

## Getting started

You need the Rust toolchain named in `rust-toolchain.toml` (rustup installs it on first use),
[`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny), and Go 1.27.3 for the Go adapter
in `adapters/go`. The gate never downloads a Go module: it keeps your module cache and
`GOPATH` and drops every other Go setting of yours, `GOPRIVATE` and `GONOPROXY` included. So
fill the module cache once, the one step that needs the network:

```sh
(cd adapters/go && go mod download)
scripts/check.sh
```

This is the gate every change has to pass. The tests need no indexer; they read the indexes
checked in under `fixtures/`. Run it as an unprivileged user: one Go test needs an output
directory it cannot write to, which the super-user can. The only tests it skips are the
case-alias cases, which need a case-insensitive volume, as macOS's default one is; on Linux
they skip with that reason.

## Kinds of contribution

**A fix or a feature in the engine.** Add a test that fails without your change. If the change
alters what a snapshot contains, the goldens under `fixtures/*/expected` change too; say in the
pull request why each difference is right.

**A change to an interface.** The snapshot and the adapter contract are specified under
[spec/](spec/README.md). Open an issue first and describe what a reader or an adapter author
would have to do differently. A change touches the document, the JSON Schema, the goldens and
`CHANGELOG.md` together.

**A language adapter.** An adapter is a separate program; it does not have to live in this
repository or be written in Rust. [docs/adapters.md](docs/adapters.md) says how the engine
finds an installed adapter, and [spec/adapter-contract.md](spec/adapter-contract.md) is
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
