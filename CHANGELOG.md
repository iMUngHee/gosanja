# Changelog

Changes to gosanja and to its two interfaces. The interfaces are versioned separately from the
release; see [spec/README.md](spec/README.md). While an interface's major version is 0, a minor
change may break compatibility, and an entry here says so when it does.

## Unreleased

### Snapshot schema 0.1

First version. A snapshot is a directory of `manifest.json`, `files.jsonl`, `entities.jsonl`,
`externals.jsonl`, `relations.jsonl`, `syntax.jsonl` and `gaps.jsonl`, specified in
[spec/snapshot.md](spec/snapshot.md).

### Adapter contract 0.1

First version. An adapter answers `describe` and `index`, and writes `result.json`,
`index.scip` and optionally `syntax.jsonl` and `calls.jsonl`, specified in
[spec/adapter-contract.md](spec/adapter-contract.md).

### gosanja 0.1.0

- `gosanja index` builds a snapshot from a SCIP index or from an adapter.
- `gosanja validate` checks a snapshot against the specification.
- `gosanja adapter describe` prints an adapter's manifest.
