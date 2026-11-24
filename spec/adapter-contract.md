# Adapter contract, version 0.3

An adapter is a program that knows one language ecosystem. The engine knows none: it runs an
adapter, reads the files the adapter wrote, and builds a [snapshot](snapshot.md) from them. An
adapter may be written in any language and may use any parser or compiler interface.

The conventions for paths, ranges and versions, and the rules a reader follows, are in
[README.md](README.md).

## Layers

An adapter produces up to three layers for one scope, a directory tree in one language
ecosystem. A run produces only the layers it is asked for (see [index](#index)).

| Layer | Content | File | Required |
| --- | --- | --- | --- |
| `base` | definitions, references and relationships | `index.scip`, a [SCIP](https://github.com/scip-code/scip) index | yes |
| `syntax` | bodies, visibility, qualifiers, attributes and a control-flow outline | `syntax.jsonl` | no |
| `calls` | call evidence from a compiler or an equivalent whole-program analysis | `calls.jsonl` | no |

The base layer may come from an existing SCIP indexer that the adapter runs. Its document
paths must be relative to `--root`. Indexers also record the absolute project root in the index
metadata; the engine ignores it and copies no absolute path into a snapshot. An index may be
several `Index` messages written one after another, which is what scip-go and scip-typescript
produce; the engine reads them as one.

When an occurrence carries both forms of a range, the engine uses `typed_range` and ignores the
deprecated `range`; the same holds for `typed_enclosing_range` and `enclosing_range`.

## Commands

### describe

```text
<adapter> describe
```

Prints the adapter manifest as one JSON object on standard output and exits 0. It does no other
work and needs no project.

| Field | Type | Meaning |
| --- | --- | --- |
| `contract` | version | the contract version the adapter speaks |
| `name` | string | lower case letters, digits and `-` |
| `version` | string | the adapter's own version |
| `languages` | array of strings | languages it handles, lower case |
| `position_encoding` | `utf-8`, `utf-16` or `utf-32` | the unit of every column in `syntax.jsonl` and `calls.jsonl` |
| `layers` | array | the layers it can produce; always includes `base` |
| `requires` | array of `{tool, version?}`, optional | external tools it needs at run time |
| `markers` | array of strings, optional | file names whose presence in a project root marks the ecosystem, for example `Cargo.toml` |

A marker is one portable file name: it matches `^[A-Za-z0-9._-]+$` and is not `.` or `..`. It
is present when `<root>/<marker>`, with links followed, is a regular file. Several markers
are alternatives. The engine uses markers to choose among installed adapters when the user
names none; an adapter without markers is used only when it is named.

How the command line finds installed adapters is described in
[docs/adapters.md](../docs/adapters.md); these rules bind it:

- It looks in each directory of `GOSANJA_ADAPTER_PATH`, then in the directory of the engine's
  executable, then in each directory of `PATH`. Of several executables with one name, the
  first found is the adapter of that name, and a later one is never looked at, even when the
  first is unusable.
- An adapter whose manifest names a contract the engine does not accept under the reader rules
  is reported and set aside before markers are matched. So when adapters of two names both
  match a root and only one of them speaks an accepted contract, that one runs.
- Two usable adapters whose markers both match a root are refused as ambiguous.
- `--adapter <name>` or `--adapter <path>` names the adapter directly; a path selects that
  executable whatever the search order would find.
- The layers asked for play no part in any of this.

### index

```text
<adapter> index --root <dir> --out <dir> [--layers <names>]
```

Options may come in any order.

- `--root` is the scope to analyse, an absolute path. The adapter must not create, change or
  delete anything under it, and must make the tools it runs do the same, for example by
  redirecting a build directory. This rule covers what the adapter and its tools do of their
  own accord. Two things are outside it: code of the analysed project that its own build
  runs, such as a build script or a compile-time plug-in, and a destination that the
  project's or the user's own configuration gives a tool. An adapter may run such code and
  leave such configuration in force, and its documentation says which.
- `--out` is an existing empty directory, an absolute path outside `--root`. The adapter writes
  its files there and nowhere else. It may create a subdirectory for its own temporary files.
- `--layers` names the layers the run is asked for. `<names>` is a comma-separated list of
  layer names, each at most once, with no space and no empty element; it holds `base` and only
  layers the manifest lists, in any order. Without `--layers` the run is asked for every layer
  the manifest lists. The adapter does no work for a layer it was not asked for and writes no
  file of it. An adapter given a list it cannot accept exits 2 and writes nothing.
- The engine always passes `--layers`, so an adapter's default never decides what was asked.
  What it passes, the *argument*, is the *request* (the layers the user asked for, `base`
  always among them, all three when the user names none; see
  [snapshot.md](snapshot.md#manifestjson)) intersected with the layers the manifest lists, in
  the order `base`, `syntax`, `calls`. Which adapter runs is decided before, and the request
  plays no part in it (see [describe](#describe)).
- One run handles the whole scope. The engine does not run an adapter once per file.
- Standard output and standard error are free text for a human. The engine shows them and never
  parses them.

## Output directory

| File | Written when |
| --- | --- |
| `result.json` | always, when `index` exits 0 |
| `index.scip` | the base layer succeeded |
| `syntax.jsonl` | the run was asked for the syntax layer and it succeeded |
| `calls.jsonl` | the run was asked for the calls layer and it succeeded |

### result.json

| Field | Type | Meaning |
| --- | --- | --- |
| `contract` | version | as in the manifest |
| `layers` | object | one entry for each layer the run was asked for, and none for another |
| `builds` | array, optional | the build contexts actually used; reported only when the run was asked for `calls` |
| `tools` | array of `{name, version}`, optional | tools that were run |

A layer entry:

| Field | Type | Meaning |
| --- | --- | --- |
| `status` | `ok` or `failed` | |
| `diagnostics` | array of `{code, message?}`, optional | what went wrong |
| `skipped` | array of `{file, code}`, optional | files the layer was meant to cover and could not process, in whole or in part |

`code` matches `^[a-z][a-z0-9-]{0,63}$` and names a kind of problem, for example
`toolchain-missing`. `message` is free text for a human and may hold paths, process ids or
timings. Only the first `code` of a failed layer is stored in the snapshot; a `message` never
is, so two equivalent failures give identical snapshots.

`skipped` lets a layer succeed without hiding what it left out: a source file that does not
parse, a file that is not valid text, or a file for which the layer reports some facts and had
to leave others out; the `code` says which. `file` is a path relative to `--root` and `code`
has the form of a diagnostic code. What remains of `result.json` once the engine has removed
what it did not ask for (see [How the engine reads
result.json](#how-the-engine-reads-resultjson)) is checked against its schema, so a malformed
entry of a layer the engine asked for is an adapter failure whatever the layer's status. For a
layer whose status is `ok`, the engine records one `skipped-file` gap for each distinct file
and code of that layer, without opening the file. A layer that is missing, failed, discarded or
not requested contributes none.

A build context:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | string | unique in this result; call evidence refers to it |
| `toolchain` | string, optional | compiler name and version |
| `target` | string, optional | target platform |
| `profile` | string, optional | for example `debug` |
| `features` | array of strings, optional | enabled features, tags or flags, sorted |

A build context states what the evidence is true for: a call observed with one feature set may
not exist with another. Only the fields above are copied into the snapshot manifest; any other
field is ignored. These values are names, not paths. `id`, `target` and `profile` may not
contain `/` or `\`. `toolchain` and each feature may contain `/`, as in Cargo's
`package/feature`, but may not be an absolute path: they may not start with `/`, `\`, `~` or a
drive prefix such as `C:\`. A build that the engine keeps (see
[How the engine reads result.json](#how-the-engine-reads-resultjson)) and that breaks this
fails the schema.

When the run is asked for `calls`, an adapter may report the build contexts it used whether the
calls layer succeeded or failed, and the engine copies them. When it is not, the adapter
reports none.

### How the engine reads result.json

The engine reads `result.json` in this order:

1. It decodes the file as JSON. A file that does not decode fails the run.
2. It compares `contract` with the contract of the manifest it accepted, and refuses a
   missing or different value before it looks at anything else. An old result is therefore refused as
   old, never for a schema error of the new reader.
3. In terms of the argument it passed, not the request: it removes every layer entry
   outside the argument, and removes `builds` unless `calls` is in the argument, since only
   call evidence refers to them. When `calls` is in the argument, `builds` are kept whether
   the calls layer succeeded or failed.
4. It checks what remains against the schema, then the build ids for duplicates, then reads
   the diagnostics.

So once `result.json` decodes as JSON, an entry or a build the engine did not ask for, however
malformed or duplicated, can neither fail the run nor add a gap, and its diagnostics are never
shown. The engine never opens the file of a layer outside the argument. `tools` are kept,
since they describe the run. The adapter's standard output and standard error stay free text
that the engine shows, whatever they mention.

Whether a layer of the argument has an entry is not a rule of the schema but of this reading:
an optional layer passed to the adapter without an entry is recorded as failed, with detail
`no-status`, and the valid builds are kept when `calls` was passed.

The guarantee is about what the engine reads. Work that an adapter does for a layer it was not
asked for is outside it: it costs time, it can fail the run, and code of the project that
such work runs can change what the requested layers observe.

## Exit status and outcomes

A problem with the project is data, not a crash: the adapter reports a failed layer in
`result.json` and exits 0. A non-zero exit means the adapter itself could not do its job.

| Situation | Engine result |
| --- | --- |
| Manifest `contract` not accepted under the reader rules | refused before `index` is run; exit 2; nothing written |
| `index` exits non-zero | adapter failure; exit 3; no snapshot published |
| `index` exits 0 and `result.json` is absent or is not valid JSON | adapter failure; exit 3; no snapshot published |
| `result.json`'s `contract` differs from the manifest's | adapter failure; exit 3; no snapshot published; refused before anything else in the file is read |
| What remains of `result.json` once the entries and builds the engine did not ask for are removed fails its schema, or repeats a build id | adapter failure; exit 3; no snapshot published |
| Base layer `failed`, without a status, or `index.scip` absent or unreadable | scope failed; exit 3; no snapshot published |
| Optional layer not requested | snapshot published; layer `not-requested`; no gap; the adapter is not asked for it |
| Optional layer requested but not named in the manifest | snapshot published; layer `absent`; `missing-layer` gap |
| Optional layer `failed` | snapshot published; `failed-layer` gap, detail `reported-failed`, with the first diagnostic code |
| Optional layer passed to the adapter but without an entry in `result.json` | snapshot published; `failed-layer` gap, detail `no-status` |
| Optional layer `ok` but its file is absent | snapshot published; `failed-layer` gap, detail `file-missing` |
| Optional layer `ok` but a row fails its schema | the whole layer is discarded; `failed-layer` gap, detail `row-invalid` |

When no snapshot is published, a snapshot already at the engine's output path stays as it was.

## Anchors

Rows of `syntax.jsonl` and `calls.jsonl` point at definitions by position, not by symbol, so
that an adapter does not have to reproduce an indexer's symbol strings.

| Field | Type | Meaning |
| --- | --- | --- |
| `file` | path | relative to `--root` |
| `line` | integer | zero-based line of the definition's name token |
| `col` | integer, optional | start column of the name token, in the manifest's `position_encoding` |
| `name` | string, optional | the definition's name |

At least one of `col` and `name` is present. The engine matches an anchor against the
definitions of the base layer:

- with `col`: the definition whose name token starts exactly at `line`, `col`;
- without `col`: the only definition named `name` whose name token is on `line`.

No match, or several, makes the row unusable: it becomes an `unanchored` gap and nothing is
attached. The engine never matches by nearness. An adapter that has columns should give them;
`name` alone exists for evidence that carries lines only, such as compiler debug information.

## Positions that cannot be converted

A column is converted from the adapter's unit to UTF-8 bytes against the text of its line. A
row of `syntax.jsonl` or `calls.jsonl` that holds a `body`, an outline range, a `callee` or a
`site` whose position lies outside its line or inside a character is dropped as a whole and
counted in a `position-out-of-range` gap for its file; the rest of the layer is kept, as with
an occurrence of the base layer. An anchor whose column cannot be converted matches nothing and
is `unanchored`.

## syntax.jsonl

One row per definition the adapter describes, and at most one row for any definition.

| Field | Type | Meaning |
| --- | --- | --- |
| `anchor` | anchor | the definition |
| `body` | range, optional | the whole definition, from its first token to its last |
| `visibility` | string, optional | as written in the source |
| `qualifiers` | array of strings, optional | keywords on the definition, source order |
| `attributes` | array of strings, optional | attributes, annotations or decorators directly on it, source order |
| `outline` | array of nodes, optional | control-flow outline of the body |

`body` matters when the base layer gives no extent for definitions: the engine uses it to
decide which definition a reference belongs to.

An outline node has `k`, `detail?`, `range`, `callee?` and `children?`, with the kinds and
meanings given in [snapshot.md](snapshot.md#syntaxjsonl). These rules bind the adapter:

- A `call` node gives `callee`, the range of the callee's name token alone: `foo` in
  `foo(bar)`, `m` in `x.m()`. The engine creates a call only when a reference of the base layer
  sits exactly on that range, and it labels the call `resolution: declared`,
  `mechanism: unknown`. The syntax layer says nothing about dispatch.
- When the callee is not a name, as in `make()(x)` or `(self.f)(x)`, the `call` node has no
  `callee`. An adapter never widens `callee` to an expression. The engine derives no call from
  such a node and records an `open-call` gap with the node's range.
- What the adapter cannot read, it marks with an `opaque` node covering the region. Leaving the
  region out would claim that it holds nothing.
- Nodes are nested by containment and listed in source order.
- Ranges are well-formed: a range does not end before it starts, a child node lies within its
  parent, and a `callee` lies within the `range` of its node.
- An outline covers the body of its own definition, and none of its nodes lies outside that
  body. It does not descend into a nested
  definition that the base layer knows as a definition of its own, because references there
  belong to that definition; a nested closure, which has no definition, is part of the outline.

A layer in which two rows resolve to the same definition, or in which a row breaks the range
rules above, is malformed. It is discarded as a whole, like a layer with a row that fails its
schema (`failed-layer`, detail `row-invalid`), so that the result cannot depend on the order of
the rows. The order in which these checks apply is given under
[How a layer is read](#how-a-layer-is-read).

## calls.jsonl

One row per observation of a call.

| Field | Type | Meaning |
| --- | --- | --- |
| `caller` | anchor | the calling definition |
| `callee` | anchor | the called definition |
| `mechanism` | enum | as in [snapshot.md](snapshot.md#call) |
| `resolution` | enum | any value except `declared` |
| `site` | `{file, line, col?}`, optional | where the call was observed |
| `build` | string | `id` of a build context in `result.json` |

- A row needs no matching reference and no site: compiler-generated calls have neither.
- A `site` may be in any file under `--root`, including one the base layer does not mention,
  such as generated code. The engine reads that file through the same boundary as every other
  source, to check and convert the position. The file is listed in `files.jsonl` only when the
  row ends up in the snapshot.
- A site's `file` is a path like any other. An absolute path, or one with a `..` segment, fails
  the row's schema, which makes the layer malformed. A well-formed path that leads outside the
  root through a link, or names a missing or unreadable file, makes only that row unusable: it
  is dropped and counted in `position-out-of-range`.
- Each row stays one observation. The engine groups rows into one relation only when caller,
  callee, mechanism and resolution are all equal, and keeps one evidence entry per row.
- The engine handles a row in this order and stops at the first problem: `build` not listed in
  `result.json` (the row is dropped and counted in `unknown-build-context`); `caller` or
  `callee` not resolved (`unanchored`). A row with both problems is counted once, under
  `unknown-build-context`.

## How a layer is read

The engine reads an optional layer in phases, each over all rows, so that the outcome does not
depend on the order of the rows.

1. **Shape.** Every row must pass its schema and, in `syntax.jsonl`, the range rules. One
   failing row discards the layer (`row-invalid`).
2. **Build contexts**, `calls.jsonl` only. A row naming a build context that `result.json`
   does not list is dropped and counted in `unknown-build-context`.
3. **Anchors.** A row with an anchor that does not resolve is dropped and recorded as
   `unanchored`.
4. **One row per definition**, `syntax.jsonl` only. If two of the remaining rows resolve to
   the same definition, the layer is discarded (`row-invalid`).
5. **Positions.** A remaining row with a position that cannot be converted, or a call site
   whose file cannot be read inside the root, is dropped and counted in
   `position-out-of-range`.
6. **Extent**, `syntax.jsonl` only. Every node of a remaining row's outline must lie within
   the body of the row's definition as the snapshot records it: the body the base layer gives,
   and the row's own `body` only when the base layer gives none. A row with a node outside it
   would attach facts to the wrong definition, so the layer is discarded (`row-invalid`). A
   definition with no body from either source is not checked.

A discarded layer contributes nothing but its one `failed-layer` gap: no rows, no bodies, no
file rows, none of the gaps that an earlier phase recorded for it, and no `skipped-file` gap.

## Worked examples

**A call with arguments.** For `render(empty(x), y)` inside `fn page`, the syntax layer reports
two `call` nodes, one with `callee` on `render` and a child with `callee` on `empty`. `x` and
`y` are not callees. The snapshot gets `call` relations from `page` to `render` and to `empty`,
both `declared`; references to `x` and `y`, if they are not locals, stay `ref`.

**A call through a call.** For `make()(x)` inside `fn page`, the syntax layer reports an outer
`call` node without `callee` and, nested in it, a `call` node with `callee` on `make`. The
snapshot gets a `declared` call from `page` to `make` and an `open-call` gap over the outer
call. If the calls layer knows what the outer call reaches, it reports that as a row of its
own, and the relation and the gap stand side by side.

**A method on an interface.** For `s.Write(p)` where `s` has interface type, the base layer
binds `Write` to the interface's method. The syntax layer yields a `declared` call to that
method. If the calls layer knows that `File.Write` is what can run because `File` is converted
to the interface somewhere in the build, it adds a row to `File.Write` with
`mechanism: dynamic`, `resolution: coercion-filtered`. Both relations are kept.

**A conversion in Go.** For `score := int(meter.Double(int32(n)))` inside `func Flow`, the Go
adapter reports one `call` node, with `callee` on `Double`. `int(…)` and `int32(…)` are
conversions: Go writes them like calls, but they call no declaration, and the index records no
occurrence on the type name that could close the call. A builtin such as `len(s)` or
`append(s, v)` is the same. The Go adapter reports no node for either, so the snapshot gets a
`declared` call from `Flow` to `Double` and no `open-call` gap at the conversions. This is the
Go adapter's reporting policy for what a call node is in Go's grammar; `call` keeps its meaning
for every adapter.

**Scheduling and dispatch together.** For Go's `go x.M()` the calls layer reports the dispatch,
`mechanism: dynamic`. The scheduling is syntax: the outline holds a `spawn` node with the `call`
node inside it.

**No call token.** A value dropped at the end of a Rust function calls its `Drop`
implementation. The calls layer reports caller, callee, `mechanism: drop`,
`resolution: direct`, a `build`, and no `site`.

**Two builds.** The same call observed with and without a feature is two rows with different
`build` values. The snapshot holds one relation with two evidence entries.

**A file that does not parse.** A syntax layer that cannot parse `src/broken.rs` still reports
every other file, sets its status to `ok` and lists
`{"file": "src/broken.rs", "code": "parse-failed"}` under `skipped`. The snapshot records a
`skipped-file` gap for that file and layer.

**An unreadable macro.** A Rust adapter that does not expand macros reports an `opaque` node
over the body of `select! { … }`. The snapshot records an `opaque-region` gap there.
