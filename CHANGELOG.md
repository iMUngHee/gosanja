# Changelog

Changes to gosanja and to its two interfaces. The interfaces are versioned separately from the
release; see [spec/README.md](spec/README.md). While an interface's major version is 0, a minor
change may break compatibility, and an entry here says so when it does.

## Unreleased

### Rust adapter: the calls layer

`gosanja-adapter-rust` now gives the calls layer. Neither interface changes: the adapter
contract stays 0.2 and the snapshot schema 0.2, so no reader or adapter has to change. What a
snapshot of a Rust project now holds:

- `call` relations with `src: adapter:rust/calls`, `mechanism: static` and
  `resolution: direct`, one evidence entry per call instruction that passes the layer's five
  checks (49 on ryu 1.0.23), each with its site and the build context it
  was observed in;
- one build context per compiled library and binary target under `scopes[].builds`, and
  `layers.calls` `present`, or `failed` with a code when the build or its IR cannot be used;
- `skipped-file` gaps of the calls layer, with one of the codes `indirect-call`,
  `no-debug-location`, `callee-unresolved`, `parse-failed` and `uncorroborated`.

Indexing a Rust project now compiles it, so it needs a linker and the time and disk of a debug
build: on ryu 1.0.23, 7 to 8 seconds and 92 MiB under the adapter's output directory. When
the build fails, only the calls layer fails. [docs/adapters/rust.md](docs/adapters/rust.md)
says what a call relation claims, what is withheld and why.

The snapshots checked in under `fixtures/` that hold the Rust adapter's output now carry the
calls layer. `diff-basic`'s new side builds one binary more than its old side, so their build
contexts differ, and the diff of the two (`fixtures/diff-basic/expected` and
`spec/examples/diff-basic`) now has `not_compared: ["calls"]`; its other rows are unchanged.

`fixtures/regenerate.sh` now indexes the Rust fixtures in an environment it makes:

- the pinned toolchain;
- a home and a Cargo home that hold no configuration;
- of the caller's environment, only `PATH` (with the indexer's directory first) and rustup's
  home, `RUSTUP_HOME` or the one under `HOME`.

It takes `--out <dir>` to write elsewhere. It refuses a snapshot whose calls layer is not
present or lacks the fixture's control relation, which `fixtures/README.md` names.
`scripts/check-live.sh` compares a fresh run with the checked-in files and no longer writes
them, and checks ryu 1.0.23's call counts.

### Diff schema 0.1

A new interface, specified in [spec/diff.md](spec/diff.md): the directory `gosanja diff`
writes, five files marked by `format: "gosanja-diff"` in its manifest. A reader of a diff
has to:

- read a row as a difference between two recorded snapshots, not as a change of the code,
  and weigh it with each side's gap counts and layer states in the manifest;
- take identity across the two sides from the pairing rules alone: a row names an entity by
  `old` and `new`, and two keys that are equal as text are not the same thing unless they are
  paired;
- read `candidates.jsonl` as suggestions with their evidence, never as renames or moves;
- treat a comparison named in `not_compared` as not made: the absence of rows of that kind
  says nothing.

A diff of snapshot schema 0.2 only: both inputs are refused unless they are 0.2.

### Snapshot schema 0.2

Breaks 0.1. A reader written for 0.1 must refuse a 0.2 snapshot until it is upgraded. An
upgraded reader has to:

- know the gap `skipped-file` (`file`, `layer`, `code`), whose provenance may name the base
  layer of an adapter, as in `adapter:rust/base`;
- accept a `call` node of an outline without `callee`, and read the `range` of an `open-call`
  gap as the whole call node when the node had no callee token.

### Adapter contract 0.2

Breaks 0.1. A 0.2 engine refuses an adapter that declares 0.1; an adapter has to declare
`0.2`. New for adapters, all optional:

- `markers` in the manifest: file names whose presence in a project root lets the engine
  choose the adapter when the user names none;
- `skipped` on a layer of `result.json`: files the layer could not process, in whole or in part;
- a `call` node may leave out `callee` when the callee is not a name. An adapter must no
  longer put anything but a name token there.

The rule that an adapter and its tools write nothing under the root now states its scope:
it does not cover code of the analysed project that the project's own build runs, nor a
destination that the project's or the user's configuration gives a tool. An adapter
documents which of these it runs or leaves in force.

### gosanja 0.2.0

- `gosanja index --root <dir> --out <dir>` finds the installed adapters, executables named
  `gosanja-adapter-<name>`, and runs the one whose marker file is in the root.
- `gosanja-adapter-rust`, the first language adapter: a base layer from `rust-analyzer scip`
  and a syntax layer read with `syn`. What it needs, reports and cannot see is in
  [docs/adapters/rust.md](docs/adapters/rust.md).
- `gosanja adapter list` shows the installed adapters and whether the tools they require are
  on `PATH`.
- `--adapter` and `adapter describe` take an adapter name or a path. **Changed:** a bare word
  is now an adapter name and is no longer run as a program found on `PATH`; to run a program,
  give its path.
- When an adapter's base layer fails, the error names the adapter's first diagnostic code.
- The adapter cache directory may not lie in the output directory.
- `gosanja diff <old> <new> --out <dir>` compares two snapshots and writes a diff (diff schema
  0.1). It exits 2 for an input of another schema or with no scope or several, and for an
  output that is, contains or lies inside an input, or that is a non-empty directory other
  than an earlier diff; it exits 3 for an input that fails validation. How to read a diff is
  in [docs/diff.md](docs/diff.md).
- `gosanja validate` also reports `parent-cycle`, a chain of `parent` or `member_of` that
  returns to itself, and `invalid-symbol`, a `symbol` that is not a global SCIP symbol. A
  snapshot that holds either now fails validation; the engine writes neither.
- Rust API: `AdapterManifest` and `LayerResult` have new fields that a struct literal must
  give; `GapKind` has a new variant that an exhaustive `match` must handle;
  `adapter::describe` takes an `Invocation` and returns a `Described`, and running the adapter
  is `Described::run`. New: `publish::publish_guarded` publishes to a path that may not
  overlap any of a list of protected directories and replaces only an earlier output with a
  given `format` marker (`publish::publish` is unchanged); `diff::compare` takes two
  snapshot directories, refuses either by its manifest's schema before any table is read, and
  then validates and compares them; `write::write_diff` and `write::serialise_diff` write the
  result.

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
