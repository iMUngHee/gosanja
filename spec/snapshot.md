# Snapshot, schema 0.6

A snapshot is what `gosanja index` writes: the facts established about one source tree at one
moment. It is a directory of eight files. The same input always produces the same bytes.

| File | Holds | Schema |
| --- | --- | --- |
| `manifest.json` | what was indexed, by which tools, and row counts | `snapshot-manifest.schema.json` |
| `files.jsonl` | the source files the facts refer to | `file.schema.json` |
| `entities.jsonl` | definitions, containers and collision groups | `entity.schema.json` |
| `externals.jsonl` | referenced symbols that are defined outside the indexed sources | `external.schema.json` |
| `relations.jsonl` | references, calls and implementation relations between rows | `relation.schema.json` |
| `syntax.jsonl` | syntactic facts about entities | `syntax.schema.json` |
| `targets.jsonl` | build targets and the entities that are their roots | `target.schema.json` |
| `gaps.jsonl` | what could not be established | `gap.schema.json` |

Every file is always present. The conventions for paths, ranges and versions are in
[README.md](README.md). A snapshot holds no timestamp, no absolute path and no source text.

## Guarantees

- **Determinism.** Rows are sorted as stated per file; ties are broken by the bytes of the
  serialised row. Fields are written in the order listed here. Numbers are integers.
- **Closed references.** Every `to`, `parent`, `member_of` and `key` used in another file names
  a row of `entities.jsonl` or, for `to` only, of `externals.jsonl`. Every target key in a
  `files.jsonl` row's `targets` or a gap's `target` names a row of `targets.jsonl`, and a
  target's `entity` names a row of `entities.jsonl`.
- **A still tree.** A snapshot describes a tree that did not change while it was read. Each
  file is resolved, checked to lie inside the root and opened by its resolved name. A tree that
  is modified during a run, for example a link swapped between the check and the open, is
  outside what a snapshot guarantees.
- **No guesses.** A fact that an input did not establish is absent, and when the engine can tell
  that something is missing it says so in `gaps.jsonl`. The engine cannot notice an item that no
  input mentions.
- **Provenance.** Every row of `entities.jsonl`, `externals.jsonl`, `relations.jsonl`,
  `syntax.jsonl`, `targets.jsonl` and `gaps.jsonl` has `src`, one of:
  - `scip:<tool name>`, taken from an index produced by that tool;
  - `adapter:<adapter name>/<layer>`, taken from an adapter's `syntax`, `calls` or `build` layer; a
    `skipped-file` gap, which records what an adapter reported about one of its layers, may
    also name `base`;
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

### Entities declared at several sites

When the adapter lists a symbol in `merged` (`adapter-contract.md`, result.json) and the
symbol's kept definitions are two or more sites without bodies and without syntax rows, the
symbol is one entity with `decls` instead of a collision group:

- its key is the symbol's key, with no `~n`; it has `symbol`, `role` and, when a document gives
  one, `kind`, by the same rule as any definition;
- `decls` lists every site `{file, name}`, sorted by (file, name range), each once; there is no
  `def`, `group`, `member_of` or `virtual`;
- `src` is the smallest provenance in byte order among its sites;
- anything that names the symbol points at the entity: a relation's `to`, and the `parent` of an
  entity in any file;
- an `implements` row from it has its key as `from` and, as `sites`, the name ranges of its
  declarations in the document that states the relationship; a document with no declaration of
  it states none;
- it has no body, so it owns no references and takes part in no `overlapping-bodies` gap;
- no `symbol-collision` gap is recorded for it.

An adapter anchor resolves against declaration sites: a definition's, or each site of such an
entity. It resolves only when exactly one site matches; a site of a folded entity gives that
entity's key, and two sites that match one anchor leave it unresolved, whether they belong to two
entities or to one.

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
| `schema` | version | snapshot schema version, `0.6` |
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
| `layers` | `{base, syntax, calls, build}` | each `present`, `absent`, `failed` or, for `syntax`, `calls` and `build`, `not-requested`; see below |
| `builds` | array | build contexts the adapter reported in a run that asked it for the calls layer, sorted by `id` |

The layers of a scope made by an adapter follow from what was requested and what the
adapter's manifest offers. A request is a set of layer names that always holds `base`; a run
that names no layers requests all four. `gosanja index --layers <names>` writes it as a
comma-separated list of `base`, `syntax`, `calls` and `build`, each at most once, holding `base`, with
no space and no empty element; any other list is a usage error (exit 2) and no adapter runs.
`--layers` together with `--scip` is a usage error (exit 2) as well, and no adapter runs. The
engine passes the adapter the request intersected with the layers its manifest lists (see
[adapter-contract.md](adapter-contract.md#index)). For `syntax`, `calls` and `build`:

| Requested | Offered by the adapter's manifest | Passed to the adapter | State | Gap |
| --- | --- | --- | --- | --- |
| no | either | no | `not-requested` | none |
| yes | no | no | `absent` | `missing-layer` |
| yes | yes | yes | `present`, or `failed` by the adapter's result | as [adapter-contract.md](adapter-contract.md) says for its result |

`not-requested` records a choice, not a limit: it says nothing about whether the adapter offers
the layer, and a reader must not take it for `absent`. `base` is always requested and is never
`not-requested`. A scope made from an index given directly has no adapter and records every
optional layer `absent`.

A build context is `{id, toolchain?, target?, profile?, features?}` and nothing else. `id` is
unique within the scope and is what call evidence refers to. The values are names, not paths:
none is an absolute path, and `id`, `target` and `profile` contain no `/` or `\`.

## files.jsonl

One row per source file that a fact refers to: every indexed document, and any other file named
by call evidence that is in the snapshot. Sorted by `path`. A file that only a gap without a
range names, such as a `skipped-file` or `source-unavailable` gap, has no row: the engine
never read it.

| Field | Type | Meaning |
| --- | --- | --- |
| `path` | path | relative to the project root |
| `language` | string, optional | as the index names it, lower case; absent for a file no document describes |
| `bytes` | integer | length of the file |
| `lines` | integer | number of lines |
| `sha256` | 64 hex digits | digest of the file's bytes |
| `targets` | array of target keys, optional | the targets the build layer proved include the file, sorted, distinct, never empty |
| `test` | `cfg`, `target` or `file`, optional | the file is test code; see Test code |
| `constraint` | string, optional | the file's build constraint as written; the engine never evaluates it |

`targets` claims inclusion and no exclusion: a `skipped-file` gap of layer `build` with code
`targets-incomplete` says that another target may include the file too. `targets` absent means
the build layer is not present, or the adapter established no target for the file, which a
`skipped-file` gap of layer `build` with code `target-unknown` then records. A reader may infer
only that no inclusion was proven, never that the file belongs to no target. Build facts and
these skips exist only for documents the base layer admitted; a row that call evidence adds
has none.

## targets.jsonl

One row per build target of a scope whose build layer is `present`; empty otherwise. Sorted by
`key`.

| Field | Type | Meaning |
| --- | --- | --- |
| `scope` | path or `"."` | the `path` of the scope entry whose build layer wrote the row |
| `key` | target key | unique in the snapshot; a reader must not parse it |
| `kind` | string | lower-case word from an open set, for example `lib`, `bin`, `test`, `bench`, `example`, `build`, `proc-macro`, `package`, `command` |
| `name` | string | the adapter's name for the target |
| `root` | `{symbol, anchor?}`, optional | the target's root symbol, with its anchor converted to UTF-8 byte columns; the anchor is absent when the adapter gave none or it could not be converted |
| `entity` | key, optional | the entity the root resolved to |
| `required_features` | array of strings, optional | as the adapter reported them |
| `test` | `true`, optional | the target is built only for tests |
| `src` | provenance | `adapter:<name>/build` |

A target whose root did not resolve has no `entity` and an `unresolved-root` gap; one without
`root` has neither. Two targets may resolve to one entity.

## Test code

A file `test` (`cfg`, `target`, `file`) means that every compilation the adapter observed
reading the file, as code or as text, was a test compilation; for Go, a `_test.go` file. The
value says on what evidence: `target` when every such compilation belongs to a test or bench
target, `cfg` when a test configuration of another target compiled it, `file` when the file's
name makes it test code.

An entity `test` holds `attribute`, `cfg`, `target` or `file`, and has one of two grounds; the
field does not say which:

- from the definition itself, through a test row of the syntax layer: for `cfg`, its own gate
  excludes it from every non-test compilation and a test compilation read its file; for
  `attribute`, the adapter proved that it is the language's built-in test or benchmark at its
  expansion site, read by a test-harness compilation, in a file that no non-test compilation the
  adapter observed read;
- from its file: the entity takes its file row's `test`, and every compilation that read the
  file was a test compilation, while the definition itself may carry no gate at all.

A reader cannot infer a gate from an entity's `cfg` alone, and neither ground claims more, for
example that the code ran. The engine takes the entity's own evidence when there is some and the
`test` of its `def.file` row otherwise. An entity declared at several sites takes the common
`test` of its declaration files when they all have the same one, and never its own evidence. A
collision group and a virtual container have none; a group's members have their own.

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
| `def` | object, optional | where it is defined; absent on virtual and group rows and on an entity declared at several sites |
| `decls` | array of `{file, name}`, optional | the sites of an entity declared at several sites, written where `def` stands; see Entities declared at several sites |
| `test` | `attribute`, `cfg`, `target` or `file`, optional | the entity is test code; see Test code |
| `src` | provenance | |

`def`:

| Field | Type | Meaning |
| --- | --- | --- |
| `file` | path | a `files.jsonl` path |
| `name` | range | the range the index gives for the definition; usually the name token, though some indexers give more (rust-analyzer gives the whole file for a module) |
| `body` | range, optional | the whole definition, used to decide which entity a reference belongs to |
| `body_src` | provenance, optional | where `body` came from; present whenever `body` is |

`decls`: `file` a `files.jsonl` path and `name` the range the index gives for that site, as in
`def`; two or more, sorted by (`file`, `name`), distinct.

A row is exactly one of five variants:

| Variant | Marked by | Has | Has not |
| --- | --- | --- | --- |
| definition | none of the marks below | `symbol`, `role`, `def` | `decls` |
| declared at several sites | `decls` | `symbol`, `role`, `decls` | `def`, `group`, `member_of`, `virtual` |
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
| `rel` | enum | `ref`, `call`, `implements`, `impl-self-type` or `impl-trait` |
| `roles` | array, `ref` only | subset of `import`, `write`, `read`, `generated`, `test`, `forward-definition`, sorted by name |
| `mechanism` | enum, `call` only | see below |
| `resolution` | enum, `call` only | see below |
| `sites` | array, required except on `call` | `[file, startLine, startCol, endLine, endCol]`, sorted, each once |
| `syntax_import_sites` | array, optional on `ref` | nonempty, sorted subset of `sites` where syntax supplied the `import` role |
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

There is one `ref` row per (`from`, `to`, `roles`). Taking the address of a function is
one example of a reference. A present syntax layer can add the `import` role when an
eligible non-definition occurrence exactly matches a written import token. The row retains
`core:reference-in-body` provenance, and its `syntax_import_sites` records only the sites
where syntax supplied that role. Each such site is a nonempty single-line range.
Base import evidence at the same file, range, target and otherwise identical roles
preserves both the native import and any corresponding ordinary base occurrence.

### Impl header links

`impl-self-type` and `impl-trait` link a virtual non-group source entity to the entity,
group or external resolved at the corresponding header token. Their provenance is
`adapter:<name>/syntax` and their sites are nonempty single-line token ranges. Syntax
establishes the block's source when the resolved definition sites of direct members each
identify one declaration with the same virtual non-group parent. Each header resolves
independently from exact base non-definition occurrences. Repeat observations of one
endpoint contribute their distinct sites to one row per (`from`, `rel`).

A conflict in the parent of a block's members gives an `ambiguous-impl` gap for that
block. Conflicting targets for an impl container and relation kind suppress that relation
and give an `ambiguous-impl` gap for each block contributing a target for that kind.
There is at most one such gap per file and block extent. An unresolved token gives no
relation. The parent chain and base `implements` relations still describe their own
containment and definition-site relationships.

Every impl site and every site of a reference carrying `syntax_import_sites` names a
snapshot file and resolves to the same scope with present syntax. The scope is the
deepest matching component prefix of the path, with `.` matching the whole root. A missing
or tied match is invalid. Impl relations and impl gaps name that scope's syntax adapter.
The syntax `unknown-file` gap uses the same scope lookup and can name a path admitted to
the snapshot later by calls.

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
member defined in that document; a member whose document says nothing gets no row. When that
document defines the symbol more than once, the statement cannot be tied to one member: `from`
is the group, and `sites` holds the name ranges of its members in that document.

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
| `callee` | range, optional, `call` only | the callee name token; absent when the callee is not a name |
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
| `call` | a call expression, with `callee` when the callee is a name (`f(x)`, `x.m()`) and without it otherwise (`make()(x)`, `(self.f)(x)`) |
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
| `layer` | `base`, `syntax`, `calls` or `build`, optional | |
| `scope` | path or `"."`, optional | the scope entry a build-layer outcome gap belongs to |
| `target` | target key, optional | the target a gap concerns |
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
| `missing-layer` | the adapter does not offer an optional layer that was requested | `layer`, and `scope` when the layer is `build` |
| `failed-layer` | an optional layer failed, was reported without a status, or was malformed | `layer`, `code`, `detail`, and `scope` when the layer is `build` |
| `unanchored` | an anchor matched no definition or several | `layer`, `anchor` |
| `open-call` | a reported call's callee token matches no reference, or the call has no callee token | `file`, `range`: the callee token in the first case, the whole call node in the second |
| `opaque-region` | the syntax layer marked a region as unreadable | `file`, `range`, `keys`: the entity |
| `skipped-file` | an adapter layer reported a file it was meant to cover and could not process, in whole or in part | `file`, `layer`, `code`; one row per distinct file and code of a layer |
| `unknown-build-context` | call evidence names a build context the adapter did not report | `layer`, `count` |
| `unknown-value` | rows were skipped under the compatibility rules | `layer`, `count` |
| `unresolved-root` | a target's root did not resolve to an entity | `layer`, `target`, and `anchor` in snapshot coordinates when the root kept one |
| `unknown-file` | build facts, a build skip or syntax token facts named a file omitted from the base index | `layer`, `file`; one row per file and layer |
| `ambiguous-impl` | impl member sites disagreed on the virtual source or a header kind had conflicting target evidence | `layer: "syntax"`, `file`, nonempty `range` of the block |

The `src` of a gap names what the gap is about:

| `src` | Gaps |
| --- | --- |
| `adapter:<name>/<layer>` | `missing-layer`, `failed-layer`, `unanchored`, `open-call`, `opaque-region`, `skipped-file`, `unknown-build-context`, `unknown-value`, `unresolved-root`, `unknown-file`, `ambiguous-impl`, and `position-out-of-range` for a side-car row |
| `scip:<tool>` | `undecodable-document`, `document-outside-root`, `source-unavailable`, and `position-out-of-range` for an occurrence |
| `core:identity` | `symbol-collision` |
| `core:reference-in-body` | `unattributed-references`, `overlapping-bodies` |

A document goes through three checks in a fixed order and the first that fails decides its
single gap: location (`document-outside-root`), availability (`source-unavailable`), encoding
(`undecodable-document`).

`detail` for `failed-layer` is one of `reported-failed`, `no-status`, `file-missing`,
`row-invalid`.

A `skipped-file` gap has `file`, `layer`, `code` and `src`, and none of `range`, `count`,
`keys`, `anchor` and `detail`. The file need not be listed in `files.jsonl`: the engine records
what the adapter reported and does not open the file. The layer named in `src` is the field
`layer`, and it may be `base`: this is the only row whose provenance names the base layer of
an adapter. Every other `adapter:` provenance names `syntax`, `calls` or `build`.

## Build facts in a valid snapshot

A snapshot holds only states the engine can write. A validator checks, per scope:

- **Association.** A `targets.jsonl` row belongs to its `scope`; a `missing-layer` or
  `failed-layer` gap of layer `build` to its `scope`; an `unresolved-root` gap to its target's
  scope; a `files.jsonl` row and a `skipped-file` or `unknown-file` gap of layer `build` to the
  deepest scope whose `path` contains its file.
- **Layer state.** Where `build` is not `present`, the scope has no target row, and no file row
  carries `targets`, `test` or `constraint`. Its gaps of layer `build` are: none when
  `not-requested`; exactly one `missing-layer` when `absent` in a scope with an `adapter`, and
  none when `absent` in a scope without one; exactly one `failed-layer` when `failed`. A
  `present` build layer has neither of these two gaps.
- **Membership.** A file row with `targets` has no `target-unknown` skip; a file row without
  `targets` that has a `test` or `constraint` has one; a `targets-incomplete` skip needs a row
  with `targets`.
- **Entity marks.** An entity `target` or `file` needs the same `test` on its `def.file` row; an
  entity declared at several sites needs its `test` on every declaration file's row; a group
  and a virtual container have none. An entity's own `cfg` or `attribute`, which comes from a
  test row the snapshot does not hold, is accepted.
- **Root links.** A target's `entity` has the root's symbol and, when the root has an
  `anchor`, a kept declaration exactly at it. A target with `entity` has no `unresolved-root`
  gap, and one with `root` and without `entity` has exactly one, whose `anchor` is the root's.
  A root whose stored anchor resolves to a kept declaration of its symbol must have the
  link.

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
