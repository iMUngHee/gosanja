# Adapter contract, version 0.1

An adapter is a program that knows one language ecosystem. The engine knows none: it runs an
adapter, reads the files the adapter wrote, and builds a [snapshot](snapshot.md) from them. An
adapter may be written in any language and may use any parser or compiler interface.

The conventions for paths, ranges and versions, and the rules a reader follows, are in
[README.md](README.md).

## Layers

An adapter produces up to three layers for one scope, a directory tree in one language
ecosystem.

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

### index

```text
<adapter> index --root <dir> --out <dir>
```

- `--root` is the scope to analyse, an absolute path. The adapter must not create, change or
  delete anything under it, and must make the tools it runs do the same, for example by
  redirecting a build directory.
- `--out` is an existing empty directory, an absolute path outside `--root`. The adapter writes
  its files there and nowhere else. It may create a subdirectory for its own temporary files.
- One run handles the whole scope. The engine does not run an adapter once per file.
- Standard output and standard error are free text for a human. The engine shows them and never
  parses them.

## Output directory

| File | Written when |
| --- | --- |
| `result.json` | always |
| `index.scip` | the base layer succeeded |
| `syntax.jsonl` | the syntax layer succeeded |
| `calls.jsonl` | the calls layer succeeded |

### result.json

| Field | Type | Meaning |
| --- | --- | --- |
| `contract` | version | as in the manifest |
| `layers` | object | one entry for each layer named in the manifest |
| `builds` | array, optional | the build contexts actually used |
| `tools` | array of `{name, version}`, optional | tools that were run |

A layer entry:

| Field | Type | Meaning |
| --- | --- | --- |
| `status` | `ok` or `failed` | |
| `diagnostics` | array of `{code, message?}`, optional | what went wrong, or what was skipped |

`code` matches `^[a-z][a-z0-9-]{0,63}$` and names a kind of problem, for example
`toolchain-missing`. `message` is free text for a human and may hold paths, process ids or
timings. Only the first `code` of a failed layer is stored in the snapshot; a `message` never
is, so two equivalent failures give identical snapshots.

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
drive prefix such as `C:\`. A `result.json` that breaks this fails its schema.

## Exit status and outcomes

A problem with the project is data, not a crash: the adapter reports a failed layer in
`result.json` and exits 0. A non-zero exit means the adapter itself could not do its job.

| Situation | Engine result |
| --- | --- |
| Manifest `contract` not accepted under the reader rules | refused before `index` is run; exit 2; nothing written |
| `index` exits non-zero | adapter failure; exit 3; no snapshot published |
| `index` exits 0 and `result.json` is absent, is not valid JSON or fails its schema | adapter failure; exit 3; no snapshot published |
| Base layer `failed`, without a status, or `index.scip` absent or unreadable | scope failed; exit 3; no snapshot published |
| Optional layer not named in the manifest | snapshot published; `missing-layer` gap |
| Optional layer `failed` | snapshot published; `failed-layer` gap, detail `reported-failed`, with the first diagnostic code |
| Optional layer named in the manifest but without an entry in `result.json` | snapshot published; `failed-layer` gap, detail `no-status` |
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
meanings given in [snapshot.md](snapshot.md#syntaxjsonl). Three rules bind the adapter:

- A `call` node gives `callee`, the range of the callee's name token alone: `foo` in
  `foo(bar)`, `m` in `x.m()`. The engine creates a call only when a reference of the base layer
  sits exactly on that range, and it labels the call `resolution: declared`,
  `mechanism: unknown`. The syntax layer says nothing about dispatch.
- What the adapter cannot read, it marks with an `opaque` node covering the region. Leaving the
  region out would claim that it holds nothing.
- Nodes are nested by containment and listed in source order.
- Ranges are well-formed: a range does not end before it starts, a child node lies within its
  parent, and a `callee` lies within the `range` of its node.
- An outline covers the body of its own definition. It does not descend into a nested
  definition that the base layer knows as a definition of its own, because references there
  belong to that definition; a nested closure, which has no definition, is part of the outline.

A layer in which two rows resolve to the same definition, or in which a row breaks the range
rules above, is malformed. It is discarded as a whole, like a layer with a row that fails its
schema (`failed-layer`, detail `row-invalid`), so that the result cannot depend on the order of
the rows.

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
  source, to check and convert the position, and lists it in `files.jsonl`. A site whose file
  is outside the root, missing or unreadable makes the row unusable: it is dropped and counted
  in `position-out-of-range`.
- Each row stays one observation. The engine groups rows into one relation only when caller,
  callee, mechanism and resolution are all equal, and keeps one evidence entry per row.
- The engine handles a row in this order and stops at the first problem: `build` not listed in
  `result.json` (the row is dropped and counted in `unknown-build-context`); `caller` or
  `callee` not resolved (`unanchored`). A row with both problems is counted once, under
  `unknown-build-context`.

## Worked examples

**A call with arguments.** For `render(empty(x), y)` inside `fn page`, the syntax layer reports
two `call` nodes, one with `callee` on `render` and a child with `callee` on `empty`. `x` and
`y` are not callees. The snapshot gets `call` relations from `page` to `render` and to `empty`,
both `declared`; references to `x` and `y`, if they are not locals, stay `ref`.

**A method on an interface.** For `s.Write(p)` where `s` has interface type, the base layer
binds `Write` to the interface's method. The syntax layer yields a `declared` call to that
method. If the calls layer knows that `File.Write` is what can run because `File` is converted
to the interface somewhere in the build, it adds a row to `File.Write` with
`mechanism: dynamic`, `resolution: coercion-filtered`. Both relations are kept.

**Scheduling and dispatch together.** For Go's `go x.M()` the calls layer reports the dispatch,
`mechanism: dynamic`. The scheduling is syntax: the outline holds a `spawn` node with the `call`
node inside it.

**No call token.** A value dropped at the end of a Rust function calls its `Drop`
implementation. The calls layer reports caller, callee, `mechanism: drop`,
`resolution: direct`, a `build`, and no `site`.

**Two builds.** The same call observed with and without a feature is two rows with different
`build` values. The snapshot holds one relation with two evidence entries.

**An unreadable macro.** A Rust adapter that does not expand macros reports an `opaque` node
over the body of `select! { … }`. The snapshot records an `opaque-region` gap there.
