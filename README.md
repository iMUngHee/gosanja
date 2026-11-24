# gosanja

gosanja builds a map of a codebase from compiler facts: what is defined where, what contains
what, what refers to what and what calls what. It is meant for people who have to keep
understanding code that was mostly written by coding agents.

- **Deterministic.** The same input gives the same bytes. No language model takes part.
- **Honest about uncertainty.** A reference is not reported as a call, a declaration is not
  reported as what runs, and what could not be established is listed as a gap.
- **Language-neutral core.** Languages are handled by separate adapter programs behind a
  documented contract.

The name is the pen name of Kim Jeong-ho (古山子), who drew the Daedongyeojido, a map of Korea
made from surveyed facts.

## Status

Early. The engine reads a [SCIP](https://github.com/scip-code/scip) index, or runs an
installed adapter, and writes a snapshot; it compares two snapshots and writes what differs
between them. A Rust adapter gives the base graph, syntactic facts and compiler-backed call
evidence: calls that a build of the project emitted and that a reference in the source on the
same line corroborates. Adapters for Go and TypeScript are planned and not yet here. The
snapshot and the adapter contract are at version 0.2 and the diff at 0.1, and all three may
change.

## Try it

```sh
cargo run -p gosanja -- index --scip fixtures/rust-basic/index.scip \
    --root fixtures/rust-basic --out /tmp/gosanja-demo
cargo run -p gosanja -- validate /tmp/gosanja-demo --root fixtures/rust-basic
```

The output directory holds seven files, described in [spec/snapshot.md](spec/snapshot.md). To
index your own project, produce a SCIP index for it first, for example with
`rust-analyzer scip .`, and pass it the same way. The output path must be outside the project.

To compare two snapshots of one project:

```sh
cargo run -p gosanja -- diff fixtures/diff-basic/old/expected \
    fixtures/diff-basic/new/expected --out /tmp/gosanja-diff-demo
```

A row of the diff says that one snapshot holds a statement and the other does not, never that
the code changed, and a rename or move is only ever suggested. [docs/diff.md](docs/diff.md)
says how to read it.

## Documentation

| | |
| --- | --- |
| [spec/](spec/README.md) | The snapshot, the adapter contract and the diff, with JSON Schemas |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | How the engine is cut and why |
| [docs/diff.md](docs/diff.md) | Comparing two snapshots: the command, what a row claims, candidates |
| [docs/CONVENTIONS.md](docs/CONVENTIONS.md) | Rules for changing the code |
| [docs/research/](docs/research/README.md) | The papers, tools and measurements behind the design |
| [docs/adapters.md](docs/adapters.md) | How the engine finds and chooses installed adapters |
| [docs/adapters/rust.md](docs/adapters/rust.md) | The Rust adapter: what it needs, builds, reports and cannot see |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute, including writing an adapter |

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
