# Snapshot, schema 0.1

A snapshot is what `gosanja index` writes: the facts established about one source tree at one
moment. It is a directory of seven files. The same input always produces the same bytes.

| File | Holds | Schema |
| --- | --- | --- |
| `manifest.json` | what was indexed, by which tools, and row counts | `snapshot-manifest.schema.json` |
| `files.jsonl` | the source files the facts refer to | `file.schema.json` |
| `entities.jsonl` | definitions, containers and collision groups | `entity.schema.json` |
| `externals.jsonl` | referenced symbols that are defined outside the indexed sources | `external.schema.json` |
| `relations.jsonl` | references, calls and implementation relations between rows | `relation.schema.json` |
| `syntax.jsonl` | syntactic facts about entities | `syntax.schema.json` |
| `gaps.jsonl` | what could not be established | `gap.schema.json` |

Every file is always present. The conventions for paths, ranges and versions are in
[README.md](README.md). A snapshot holds no timestamp, no absolute path and no source text.

## Guarantees

- **Determinism.** Rows are sorted as stated per file; ties are broken by the bytes of the
  serialised row. Fields are written in the order listed here. Numbers are integers.
- **Closed references.** Every `to`, `parent`, `member_of` and `key` used in another file names
  a row of `entities.jsonl` or, for `to` only, of `externals.jsonl`.
- **A still tree.** A snapshot describes a tree that did not change while it was read. Each
  file is resolved, checked to lie inside the root and opened by its resolved name. A tree that
  is modified during a run, for example a link swapped between the check and the open, is
  outside what a snapshot guarantees.
- **No guesses.** A fact that an input did not establish is absent, and when the engine can tell
  that something is missing it says so in `gaps.jsonl`. The engine cannot notice an item that no
  input mentions.
- **Provenance.** Every row of `entities.jsonl`, `externals.jsonl`, `relations.jsonl`,
  `syntax.jsonl` and `gaps.jsonl` has `src`, one of:
  - `scip:<tool name>`, taken from an index produced by that tool;
  - `adapter:<adapter name>/<layer>`, taken from an adapter's `syntax` or `calls` layer;
  - `core:<pass>`, derived by the engine. The passes are `containment`, `identity` and
    `reference-in-body`.

## Keys

A key identifies a row of `entities.jsonl` or `externals.jsonl` within one snapshot. Keys are
strings; there are no numeric identifiers.

### Base key

A SCIP symbol is `<scheme> <manager> <package> <version> <descriptors>`. Its base key is

```text
<manager>:<package>:<descriptors>
```

- `manager` and `package` are escaped: `%` becomes `%25`, `:` becomes `%3A`, `@` becomes `%40`,
  `~` becomes `%7E`.
- `descriptors` is the descriptor part exactly as SCIP writes it, backtick escaping included,
  and then `%`, `@` and `~` are escaped the same way. A colon is left as it is, because it can
  only belong to the descriptors once the first two colons have been read.
- A SCIP placeholder `.` for an absent manager, package or version is kept as `.`.

Local symbols (`local <id>`) have no key and are not rows. A symbol whose last descriptor is a
type parameter or a parameter is a row only when it is a container of an indexed entity, which
is how rust-analyzer names `impl` blocks; otherwise it is treated like a local.

### Qualifiers

Symbols are grouped by base key, indexed and external ones together. A group of one uses the
base key. In a larger group every symbol is qualified by the parts that differ within the
group, in this order:

1. `@<version>` when the versions differ;
2. `@<scheme>` when the schemes differ.

Version and scheme are escaped like `manager`. Two different symbols therefore never share a
key, and a reference always names the symbol its index entry named.

| Symbols sharing a base key `K` | Keys |
| --- | --- |
| one symbol | `K` |
| versions `1` and `2`, same scheme | `K@1`, `K@2` |
| same version, schemes `a` and `b` | `K@a`, `K@b` |
| version `1` scheme `a`, version `2` scheme `b` | `K@1@a`, `K@2@b` |
| version `1@x` scheme `y`, version `1` scheme `x@y` | `K@1%40x@y`, `K@1@x%40y` |

### Collision groups

When an index holds several distinct definitions of one symbol, the symbol's key names a
**collision group** and each definition is a **member**:

- the group row has `group: true` and no `def`;
- member keys are the group key followed by `~1`, `~2`, …, in order of (file path, name range);
- each member has `member_of` set to the group key;
- anything that names the symbol points at the group: a relation's `to`, and the `parent` of an
  entity whose containing symbol collided;
- one `symbol-collision` gap is recorded.

Definitions with the same symbol, file and name range are one observation, not a collision.

Member keys depend on source positions. They identify a row within one snapshot only and must
not be used to match rows between snapshots.

### Containers

Every proper prefix of a symbol's descriptors that ends at a descriptor boundary is itself a
symbol, the container. Prefixes are taken per symbol, so a container belongs to the same scheme,
manager, package and version as what it contains and is qualified by the same rule.

- A container the index defines is an ordinary entity.
- A container the index does not define is written with `virtual: true`. If something refers to
  such a symbol, the reference points at this virtual entity and no external row is written.
- `parent` is the longest proper prefix. A symbol with a single descriptor has no `parent`.

## manifest.json

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `"gosanja-snapshot"` | marks the directory as a snapshot; see Publication |
| `schema` | version | snapshot schema version, `0.1` |
| `tool` | `{name, version}` | the engine that wrote the snapshot |
| `project` | `{name}` | a display name; it carries no identity |
| `scopes` | array | one entry per indexed scope, in path order |
| `counts` | object | number of rows per file, plus `dropped_local_references` |

A scope entry:

| Field | Type | Meaning |
| --- | --- | --- |
| `path` | path or `"."` | scope root relative to the project root |
| `adapter` | `{name, version, contract}`, optional | the adapter that produced the inputs; absent when an index was given directly |
| `indexers` | array of `{name, version}` | tools named in the metadata of the SCIP input, sorted |
| `layers` | `{base, syntax, calls}` | each `present`, `absent` or `failed` |
| `builds` | array | build contexts reported by the adapter, sorted by `id` |

A build context is `{id, toolchain?, target?, profile?, features?}` and nothing else. `id` is
unique within the scope and is what call evidence refers to. The values are names, not paths:
none is an absolute path, and `id`, `target` and `profile` contain no `/` or `\`.

## files.jsonl

One row per source file that a fact refers to: every indexed document, and any other file named
by call evidence that is in the snapshot. Sorted by `path`.

| Field | Type | Meaning |
| --- | --- | --- |
| `path` | path | relative to the project root |
| `language` | string, optional | as the index names it, lower case; absent for a file no document describes |
| `bytes` | integer | length of the file |
| `lines` | integer | number of lines |
| `sha256` | 64 hex digits | digest of the file's bytes |

## entities.jsonl

Sorted by `key`.

| Field | Type | Meaning |
| --- | --- | --- |
| `key` | key | see Keys |
| `symbol` | string, optional | the full SCIP symbol; absent on a virtual container made from a prefix that no input names |
| `kind` | string, optional | `<language>:<kind>`, lower case, from the index; open set; absent when the index gives none |
| `role` | enum | `container`, `type`, `callable`, `value` or `member`; on a group row, present only when all members agree |
| `name` | string | the last descriptor's name, unescaped |
| `parent` | key, optional | the containing entity or group |
| `virtual` | `true`, optional | a container with no definition in the inputs |
| `group` | `true`, optional | a collision group |
| `member_of` | key, optional | the group this definition belongs to |
| `def` | object, optional | where it is defined; absent on virtual and group rows |
| `src` | provenance | |

`def`:

| Field | Type | Meaning |
| --- | --- | --- |
| `file` | path | a `files.jsonl` path |
| `name` | range | the range the index gives for the definition; usually the name token, though some indexers give more (rust-analyzer gives the whole file for a module) |
| `body` | range, optional | the whole definition, used to decide which entity a reference belongs to |
| `body_src` | provenance, optional | where `body` came from; present whenever `body` is |

A row is exactly one of four variants:

| Variant | Marked by | Has | Has not |
| --- | --- | --- | --- |
| definition | none of the marks below | `symbol`, `role`, `def` | |
| member of a collision group | `member_of` | `symbol`, `role`, `def` | |
| collision group | `group: true` | `symbol` | `kind`, `def`, `member_of`, `virtual` |
| virtual container | `virtual: true` | `role` | `def`, `member_of`, `group` |

`role` follows the last descriptor of the symbol:

| Descriptor | Role |
| --- | --- |
| namespace | `container` |
| type | `type` |
| method, macro | `callable` |
| term, meta | `member` when the parent is a `type`, otherwise `value` |
| type parameter, parameter | `container`; such a symbol is a row only as a container |

From an index, `body` is the enclosing range of the definition occurrence. An enclosing range
on any other occurrence is ignored: rust-analyzer puts the body of the referenced definition
there.

## externals.jsonl

One row per referenced symbol that has no definition in the indexed sources and is not a
container of an indexed entity. Sorted by `key`.

| Field | Type | Meaning |
| --- | --- | --- |
| `key` | key | see Keys |
| `symbol` | string | the full SCIP symbol |
| `package` | `{manager, name, version}` | as the symbol states them |
| `src` | provenance | |

## relations.jsonl

Only forward relations are stored. Sorted by (`from`, `rel`, `to`, `mechanism`, `resolution`,
`src`), absent fields first.

| Field | Type | Meaning |
| --- | --- | --- |
| `from` | key | an entity |
| `to` | key | an entity, a group or an external |
| `rel` | enum | `ref`, `call` or `implements` |
| `roles` | array, `ref` only | subset of `import`, `write`, `read`, `generated`, `test`, `forward-definition`, sorted by name |
| `mechanism` | enum, `call` only | see below |
| `resolution` | enum, `call` only | see below |
| `sites` | array, `ref` and `implements` | `[file, startLine, startCol, endLine, endCol]`, sorted, each once |
| `evidence` | array, `call` only | one entry per observation, sorted |
| `src` | provenance | |

### ref

A `ref` row says that the body of `from` contains a reference to `to`. Which body a reference
belongs to is decided by containment: the innermost `def.body` that contains the reference,
that is, the containing body that every other containing body strictly contains. Two bodies of
different entities that overlap without one strictly containing the other, identical extents
included, are both left out of this decision and reported once as `overlapping-bodies`; a
reference inside them then belongs to the innermost remaining body. A reference that no
remaining body contains produces no row (see `unattributed-references`).

There is one `ref` row per (`from`, `to`, `roles`). A reference is not a call: taking the
address of a function is a `ref` and nothing more.

### call

A `call` row exists only with evidence that a call happens. A row is identified by `from`, `to`,
`mechanism`, `resolution` and `src` together, so one caller and callee observed with two
different classifications are two rows. The `ref` row a call was derived from is kept.

`resolution` says how far the target was established, from weakest claim to strongest:

| Value | Claim | May be asserted by |
| --- | --- | --- |
| `declared` | the callee name is bound to this declaration, which may be an interface or trait method; nothing is claimed about what runs | syntax layer |
| `name-only` | a target with this name exists; nothing else matched | calls layer |
| `hierarchy` | `to` is one of the implementations of the declared target | calls layer |
| `coercion-filtered` | as `hierarchy`, and the implementing type is converted to the interface or trait object somewhere in the build | calls layer |
| `mono-instantiated` | `to` is the target after generic instantiation | calls layer |
| `direct` | exactly this definition is called | calls layer |

`mechanism` says how control reaches the callee: `unknown`, `static`, `dynamic`,
`function-value`, `closure`, `drop`, `spawn`, `await` or `generated`. When a call is both
scheduled and dispatched, as in Go's `go x.M()`, `mechanism` states the dispatch (`dynamic`
here); the scheduling is recorded in the caller's outline as a `spawn` node around the call.
`spawn` and `await` are for evidence that shows only the scheduling link, such as an executor
polling a future.

An evidence entry:

| Field | Type | Meaning |
| --- | --- | --- |
| `file` | path, optional | where the call was observed |
| `line` | integer, optional | zero-based; present whenever `file` is |
| `col` | integer, optional | start column |
| `end` | `[endLine, endCol]`, optional | present when the producer gave a range |
| `build` | string, optional | `id` of a build context of the scope; present on every calls-layer entry |

Evidence may have no location: compiler-generated calls such as drop glue have none.

Entries are sorted by (`file`, `line`, `col`, `end`, `build`). Numbers compare as numbers,
strings by their bytes, and an absent field comes before any value. Two observations that are
equal in every field are both kept. `sites` are sorted the same way, by file and then the four
coordinates, and each site appears once.

### implements

`from` implements or overrides `to`, as the index's relationships state it. `from` is the
definition in the document whose symbol information states the relationship, and `sites` holds
the name range of that definition only. When the symbol is a collision group, `from` is the
member defined in that document; a member whose document says nothing gets no row.

## syntax.jsonl

One row per entity that an adapter's syntax layer described. Sorted by `key`.

| Field | Type | Meaning |
| --- | --- | --- |
| `key` | key | an entity |
| `visibility` | string, optional | as written in the source, for example `pub(crate)` or `export`; open set |
| `qualifiers` | array of strings, optional | keywords on the definition, for example `async`; open set, source order |
| `attributes` | array of strings, optional | attributes, annotations or decorators directly on the definition; open set, source order |
| `outline` | array of nodes, optional | nested control-flow outline of the body; absent means not analysed, empty means nothing to show |
| `src` | provenance | |

An outline node:

| Field | Type | Meaning |
| --- | --- | --- |
| `k` | enum | node kind, below |
| `detail` | string, optional | the construct in the language's own words, for example `match`, `for-range`, `?` |
| `range` | range | extent of the construct |
| `callee` | range, `call` only | the callee name token |
| `children` | array of nodes, optional | nested nodes in source order |

| `k` | Covers |
| --- | --- |
| `branch` | a construct that chooses between alternatives: `if`, `switch`, `match`, `select`, `try` |
| `arm` | one alternative of a branch: `else`, `case`, a match arm, `catch` |
| `loop` | any loop |
| `exit` | leaving the current flow: `return`, `break`, `continue`, `throw`, `goto`, `?` |
| `await` | a suspension point |
| `spawn` | starting concurrent or deferred work: `go`, `defer`, a spawned task |
| `closure` | a nested function body |
| `call` | a call expression |
| `opaque` | a region the adapter could not read, such as an unexpanded macro body |

An outline is a description of syntax. It states neither execution order nor reachability. An
`opaque` node means "unknown here", never "nothing here"; each one also produces an
`opaque-region` gap.

## gaps.jsonl

Sorted by (`gap`, `file`, `range`), absent fields first.

| Field | Type | Meaning |
| --- | --- | --- |
| `gap` | enum | below |
| `file` | path, optional | |
| `range` | range, optional | |
| `count` | integer, optional | how many items the gap stands for |
| `keys` | array of keys, optional | rows the gap concerns, sorted |
| `layer` | `base`, `syntax` or `calls`, optional | |
| `anchor` | anchor, optional | as the adapter gave it |
| `code` | string, optional | an adapter's diagnostic code |
| `detail` | string, optional | a fixed phrase from the table below, never free text |
| `src` | provenance | |

| `gap` | Recorded when | Fields |
| --- | --- | --- |
| `symbol-collision` | several distinct definitions share one symbol | `keys`: the group, then its members |
| `undecodable-document` | a document's position encoding is unspecified | `file` |
| `document-outside-root` | a document's path is absolute or resolves outside the root; the file is never opened | `file` when the index gave a valid relative path that escapes through a link; otherwise one row for all such documents with `count` |
| `source-unavailable` | a document's file is missing or unreadable | `file` |
| `position-out-of-range` | positions fall outside their line or inside a multi-byte character, or a call site names a file that cannot be read inside the root; the occurrences, or the side-car rows that hold them, are dropped | `file`, `count`, and `layer` for a side-car |
| `unattributed-references` | references in a file lie in no body | `file`, `count` |
| `overlapping-bodies` | two bodies overlap without nesting; neither owns references; one row per pair | `file`, `keys`: the two entities |
| `missing-layer` | the adapter does not offer an optional layer | `layer` |
| `failed-layer` | an optional layer failed, was reported without a status, or was malformed | `layer`, `code`, `detail` |
| `unanchored` | an anchor matched no definition or several | `layer`, `anchor` |
| `open-call` | a reported call's callee token matches no reference | `file`, `range` |
| `opaque-region` | the syntax layer marked a region as unreadable | `file`, `range`, `keys`: the entity |
| `unknown-build-context` | call evidence names a build context the adapter did not report | `layer`, `count` |
| `unknown-value` | rows were skipped under the compatibility rules | `layer`, `count` |

The `src` of a gap names what the gap is about:

| `src` | Gaps |
| --- | --- |
| `adapter:<name>/<layer>` | `missing-layer`, `failed-layer`, `unanchored`, `open-call`, `opaque-region`, `unknown-build-context`, `unknown-value`, and `position-out-of-range` for a side-car row |
| `scip:<tool>` | `undecodable-document`, `document-outside-root`, `source-unavailable`, and `position-out-of-range` for an occurrence |
| `core:identity` | `symbol-collision` |
| `core:reference-in-body` | `unattributed-references`, `overlapping-bodies` |

A document goes through three checks in a fixed order and the first that fails decides its
single gap: location (`document-outside-root`), availability (`source-unavailable`), encoding
(`undecodable-document`).

`detail` for `failed-layer` is one of `reported-failed`, `no-status`, `file-missing`,
`row-invalid`.

## Publication

These rules bind a program that writes a snapshot to a path.

- The decision is made on resolved paths: links and `..` are resolved first, so `src/../out`
  is judged as `out`.
- The output path is refused when it is the indexed root, lies inside it or contains it.
- The parent directory of the output path must exist; otherwise the path is refused. Nothing
  is created above the snapshot directory itself.
- An output path that does not exist, or is an empty directory, is used.
- An existing non-empty directory is replaced only when it is a snapshot: its `manifest.json`
  parses and has `format` equal to `gosanja-snapshot`. Any other non-empty directory is refused
  and left unchanged, whatever files it holds.
- A snapshot is written elsewhere first and moved into place whole. When a run fails at any
  point, including during that move, what was at the output path before is there afterwards,
  byte for byte.
