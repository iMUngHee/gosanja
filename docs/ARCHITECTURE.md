# Architecture

gosanja turns a source tree into a snapshot: a set of facts about what is defined, what
contains what, what refers to what and what calls what, each with its source position and its
origin. The facts come from compilers and indexers. Nothing is guessed.

## Shape

```text
            adapter (external program, one per language ecosystem)
              │  index.scip      definitions, references          (base layer)
              │  syntax.jsonl    bodies, outline, attributes      (syntax layer, optional)
              │  calls.jsonl     compiler-backed call evidence    (calls layer, optional)
              │  build.jsonl     targets, file membership         (build layer, optional)
              ▼
  ┌────────────────────────────── core ──────────────────────────────┐
  │ decode ─► keys ─► entities, containment ─► ownership ─► relations │
  │                         ▲                                         │
  │               anchors join the side-cars                          │
  └──────────────────────────────┬───────────────────────────────────┘
                                 ▼
                    snapshot (eight files) ─► validate
                                 │
                                 ▼  two of them, old and new
                 validate ─► pair ─► compare ─► candidates      (core: diff)
                                 │
                                 ▼
                         diff (five files)
```

Three interfaces are specified and versioned, and everything else is free to change:

- the [snapshot](../spec/snapshot.md), which is what consumers read;
- the [adapter contract](../spec/adapter-contract.md), which is what a language adapter is
  asked for and writes: it produces only the layers the engine asks for, and the snapshot
  records a layer that was not asked for as `not-requested`;
- the [diff](../spec/diff.md), the differences between two snapshots, which is what a consumer
  of change reads.

## Why it is cut this way

**The core owns its model; SCIP is an input.** SCIP is a transmission format: it has no call
relation, its symbol strings embed a package version, and indexers differ in position units,
range forms and which optional fields they fill. The core reads SCIP for the base layer and
writes its own schema, with its own keys.
See [research/engine-architecture.md](research/engine-architecture.md) and
[research/scip-indexer-differences.md](research/scip-indexer-differences.md).

**Languages live outside the core.** Precise facts need each ecosystem's own tools: a Go
call graph needs `go/ssa`, TypeScript needs its compiler API. Those cannot be linked into one
Rust binary, so every adapter is a separate program behind a file contract, the Rust one
included. Adding a language means writing an adapter; the core is not rebuilt. Installed
adapters are executables named `gosanja-adapter-<name>` that the command line finds and
chooses by marker files in the project root; see [adapters.md](adapters.md).

**Side-cars join by position.** An adapter points at a definition by the position of its name,
not by a symbol string, so it does not have to reproduce an indexer's symbol grammar. An anchor
that matches nothing becomes a gap. Exact syntax token ranges can also join to base
references and declarations. The adapter identifies the written import or impl tokens,
and the core uses indexed occurrences and containment to establish the endpoints.

**Uncertainty is data.** A reference is recorded as a reference. A call exists only with
evidence, and carries how far its target was established, from `declared` to `direct`. What
could not be established is listed in `gaps.jsonl`. This is what lets a reader trust an absent
edge as little, and a present edge as much, as the inputs justify.
See [research/call-relations.md](research/call-relations.md).

**Identity is a key string.** Keys come from symbols without their package version, qualified
only when two symbols would otherwise clash. There are no numeric ids, so a row can be read,
compared and tested without a table of ids.
See [research/structural-diff-identity.md](research/structural-diff-identity.md).

**A diff compares records, not code.** It reads two snapshots and nothing else, so it works for
every adapter and lives in the core. Rows of the two sides are the same thing only where the
pairing rules justify exactly one partner; everything else is removed or added. A rename or a
move is never asserted: a removed and an added definition may be listed as candidates, with the
evidence they were ranked by. See [diff.md](diff.md) and
[research/snapshot-diff-prestudy.md](research/snapshot-diff-prestudy.md).

## Workspace

| Crate | Path | Responsibility |
| --- | --- | --- |
| `gosanja-schema` | `crates/schema` | Types of the three interfaces and the version compatibility rules. Shared with adapters written in Rust. |
| `gosanja-core` | `crates/core` | Decoding, keys, the graph, side-car joins, writing, validation, running adapters, comparing two snapshots. Knows no language. |
| `gosanja` | `crates/cli` | The command line. Reads files, finds installed adapters (`discover`), calls the core, sets exit codes. |
| `gosanja-adapter-rust` | `adapters/rust` | The Rust adapter runs `rust-analyzer scip` for the base layer and reads source with `syn` for the syntax layer. It depends on `gosanja-schema`, and its tests also use the core. |

Build the command line with `cargo build -p gosanja` and the Rust adapter with
`cargo build -p gosanja-adapter-rust`. Both builds use the root workspace's `Cargo.lock`.

The Go adapter, `gosanja-adapter-go`, is a Go module of its own in `adapters/go`, not a crate
of the workspace: it runs `scip-go` for the base layer and reads source with `go/packages` and
`go/types` for the syntax layer. It shares no code with the workspace. Its contract types are
Go structs in `adapters/go/internal/contract`, and its tests check every file it writes against
`spec/schema` and run the engine, built from the workspace, as a process.

Core modules:

| Module | Responsibility |
| --- | --- |
| `scip_stream` | Reads an index one top-level field at a time, so large and concatenated indexes work. |
| `identity` | Base keys, qualifiers, collision groups, containers. |
| `graph` | Entities, containment, ownership of references, relations. |
| `anchor` | Matches adapter anchors against definitions. |
| `sidecar` | Reads the syntax and calls layers and joins them to the graph. |
| `token_sites` | Checks exact token ranges and joins impl headers and direct members. |
| `build` | Validates and joins the build layer and applies item and file test evidence. |
| `adapter` | Runs one given adapter program and classifies what it produced. The program that was described is the one that is run. |
| `write` | Serialises a snapshot or a diff deterministically, and reads sources and snapshots from disk. |
| `publish` | Moves a finished snapshot or diff into place, and protects what was there and the inputs it was made from. |
| `validate` | Checks a snapshot on disk against the specification. |
| `schemas` | The normative JSON Schemas of `spec/schema`, compiled in once for the validator and the adapter runner. |
| `diff` | Validates two snapshots, pairs their rows, and finds the entity, external and relation changes and the candidates. |
| `diff::similarity` | The two pure measures of the candidates, in millionths: the edit-distance similarity of two sequences, and the ratio of the intersection of two multisets to their union. |

Dependencies point one way: `cli` → `core` → `schema`.

## What is deliberately not here

- No query interface and no storage engine. A snapshot is files.
- No viewer. A viewer is a consumer of the snapshot.
- No source text in a snapshot, only paths, sizes and digests.
- No incremental indexing. Every row is tied to a file, which keeps that possible later.
- No adapter for a language other than Rust and Go, and no calls layer for Go (see
  [adapters/go.md](adapters/go.md)). The Rust adapter's calls layer covers direct
  calls between definitions of the project that the calling target's own build defines;
  destructor calls, dynamic dispatch and calls into another target's build are not recorded
  (see [adapters/rust.md](adapters/rust.md)).
- No assertion that something was renamed or moved, and no diff of gaps, of more than two
  snapshots or of snapshots with several scopes.
- No defence against a tree that changes while it is being indexed. A path is resolved, checked
  to lie inside the root and then opened by its resolved name; a link swapped in between those
  steps is not detected. Index a tree that is not being written to.
