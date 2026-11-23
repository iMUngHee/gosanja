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

Early. The engine reads a [SCIP](https://github.com/scip-code/scip) index and writes a
snapshot; language adapters for Rust, Go and TypeScript, and the comparison of two snapshots,
are planned and not yet here. Both interfaces are at version 0.2 and may change.

## Try it

```sh
cargo run -p gosanja -- index --scip fixtures/rust-basic/index.scip \
    --root fixtures/rust-basic --out /tmp/gosanja-demo
cargo run -p gosanja -- validate /tmp/gosanja-demo --root fixtures/rust-basic
```

The output directory holds seven files, described in [spec/snapshot.md](spec/snapshot.md). To
index your own project, produce a SCIP index for it first, for example with
`rust-analyzer scip .`, and pass it the same way. The output path must be outside the project.

## Documentation

| | |
| --- | --- |
| [spec/](spec/README.md) | The snapshot and the adapter contract, with JSON Schemas |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | How the engine is cut and why |
| [docs/CONVENTIONS.md](docs/CONVENTIONS.md) | Rules for changing the code |
| [docs/research/](docs/research/README.md) | The papers, tools and measurements behind the design |
| [docs/adapters.md](docs/adapters.md) | How the engine finds and chooses installed adapters |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute, including writing an adapter |

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
