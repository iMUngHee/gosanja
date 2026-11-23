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
              ▼
  ┌────────────────────────────── core ──────────────────────────────┐
  │ decode ─► keys ─► entities, containment ─► ownership ─► relations │
  │                         ▲                                         │
  │               anchors join the side-cars                          │
  └──────────────────────────────┬───────────────────────────────────┘
                                 ▼
                    snapshot (seven files) ─► validate
```

Two interfaces are specified and versioned, and everything else is free to change:

- the [snapshot](../spec/snapshot.md), which is what consumers read;
- the [adapter contract](../spec/adapter-contract.md), which is what a language adapter writes.

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
included. Adding a language means writing an adapter; the core is not rebuilt.

**Side-cars join by position.** An adapter points at a definition by the position of its name,
not by a symbol string, so it does not have to reproduce an indexer's symbol grammar. An anchor
that matches nothing becomes a gap.

**Uncertainty is data.** A reference is recorded as a reference. A call exists only with
evidence, and carries how far its target was established, from `declared` to `direct`. What
could not be established is listed in `gaps.jsonl`. This is what lets a reader trust an absent
edge as little, and a present edge as much, as the inputs justify.
See [research/call-relations.md](research/call-relations.md).

**Identity is a key string.** Keys come from symbols without their package version, qualified
only when two symbols would otherwise clash. There are no numeric ids, so a row can be read,
compared and tested without a table of ids.
See [research/structural-diff-identity.md](research/structural-diff-identity.md).

## Workspace

| Crate | Path | Responsibility |
| --- | --- | --- |
| `gosanja-schema` | `crates/schema` | Types of both interfaces and the version compatibility rules. Shared with adapters written in Rust. |
| `gosanja-core` | `crates/core` | Decoding, keys, the graph, side-car joins, writing, validation, running adapters. Knows no language. |
| `gosanja` | `crates/cli` | The command line. Reads files, calls the core, sets exit codes. |

Core modules:

| Module | Responsibility |
| --- | --- |
| `scip_stream` | Reads an index one top-level field at a time, so large and concatenated indexes work. |
| `identity` | Base keys, qualifiers, collision groups, containers. |
| `graph` | Entities, containment, ownership of references, relations. |
| `anchor` | Matches adapter anchors against definitions. |
| `sidecar` | Reads the syntax and calls layers and joins them to the graph. |
| `adapter` | Runs an adapter and classifies what it produced. |
| `write` | Serialises a snapshot deterministically, and reads sources and snapshots from disk. |
| `publish` | Moves a finished snapshot into place and protects what was there. |
| `validate` | Checks a snapshot on disk against the specification. |
| `schemas` | The normative JSON Schemas of `spec/schema`, compiled in once for the validator and the adapter runner. |

Dependencies point one way: `cli` → `core` → `schema`.

## What is deliberately not here

- No query interface and no storage engine. A snapshot is files.
- No viewer. A viewer is a consumer of the snapshot.
- No source text in a snapshot, only paths, sizes and digests.
- No incremental indexing. Every row is tied to a file, which keeps that possible later.
- No language adapters in this crate set yet, and no comparison of two snapshots.
- No defence against a tree that changes while it is being indexed. A path is resolved, checked
  to lie inside the root and then opened by its resolved name; a link swapped in between those
  steps is not detected. Index a tree that is not being written to.
