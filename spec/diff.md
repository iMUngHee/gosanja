# Diff, schema 0.4

A row of a diff says that one snapshot holds a statement and the other does not. It never says
that the code changed. A snapshot that observed less, because a file was skipped, an anchor did
not resolve or a position could not be converted, lacks statements for that reason, and its gaps
say so; the diff shows the same absence a reader of the two snapshots would see. The manifest
gives each side's gap counts per kind and its layer states, so that a reader can weigh a row.

A diff is what `gosanja diff <old> <new>` writes: the differences between two snapshots of one
project, schema 0.5 (see [snapshot.md](snapshot.md)). It is a directory of five files. The same
two inputs always produce the same bytes.

| File | Holds | Schema |
| --- | --- | --- |
| `manifest.json` | the two inputs, the weights, and row counts | `diff-manifest.schema.json` |
| `entities.jsonl` | entities added, removed, changed or rekeyed | `diff-entity.schema.json` |
| `externals.jsonl` | externals added, removed or rekeyed | `diff-external.schema.json` |
| `relations.jsonl` | relations added or removed | `diff-relation.schema.json` |
| `candidates.jsonl` | removed and added entities that may be the same thing | `diff-candidate.schema.json` |

Every file is always present. The conventions for paths, ranges and versions are in
[README.md](README.md).

## Guarantees

- **Determinism.** Rows are sorted as stated per file; ties are broken by the bytes of the
  serialised row. Fields are written in the order listed here. Numbers are integers.
- **Closed references.** An `old` names a row of the old snapshot and a `new` a row of the new
  one:
  - in `entities.jsonl`, a row of that snapshot's `entities.jsonl`, and a `parent` or
    `member_of` a row of the same side as the row that carries it;
  - in `externals.jsonl`, a row of that snapshot's `externals.jsonl`;
  - in `relations.jsonl`, `from` names a row of `entities.jsonl`, and `to` a row of
    `entities.jsonl`, a group included, or of `externals.jsonl`;
  - every `old` of `candidates.jsonl` is the `old` of a `removed` row of `entities.jsonl`, and
    every `new` the `new` of an `added` row.
- **Nothing from the machine.** The engine adds no timestamp and no path of the machine it runs
  on, and reads no source file. Every path field is the root-relative path the snapshots hold.
  Property values are copied from the snapshots as recorded, so an attribute is written with the
  spelling the adapter gave it, whatever it contains.
- **One project.** The diff treats as the same thing only what the pairing rules below pair.
  Equal key text alone is not identity. Pairing assumes that both snapshots describe one
  project, and the engine cannot check that: `project.name` is a display name.
- **Still inputs.** Both snapshots must stay unchanged while the diff is made. A diff of an input
  that another run replaces meanwhile is outside what a diff guarantees.

## Inputs

Each input is a snapshot directory. It is read under the reader rules of
[README.md](README.md) and checked with the validator without sources. A snapshot of another
schema version, one with no scope or several, or one with any diagnostic is refused before
anything is written. That includes the snapshots kept frozen in `fixtures/diff-public`, which
stay at schema 0.2, the schema they were measured at. A snapshot reader of 0.4 refuses them. Only
scratch copies whose `schema` is re-stamped are compared in the repository's tests.

A diff reader of schema 0.3 refuses a diff written with schema 0.2.

## Identity across the two snapshots

Pairing gives every entity and every external of both snapshots at most one partner. A pair is
made only where the snapshots justify exactly one partner. A row that the rules do not pair is
reported as removed or added, which is noise and never a false statement.

A symbol's identity is the symbol without its package version: scheme, manager, package name and
descriptors. Key text plays no part, because one `@` suffix can be a version in one snapshot and
a scheme in the other. The steps, in order, over entities and externals together:

1. **Exact symbol.** For every full symbol that occurs on both sides in rows other than members:
   if it is one row on each side, in the same table and of the same variant, the two pair.
   Otherwise they do not pair, and neither may be used in step 2.
2. **Version moved on.** Among rows that step 1 left untouched, other than members, grouped by
   identity, table and variant: when exactly one row is left on each side, the two pair, but
   only if the package of that identity has exactly one version on each side, counted over all
   rows of both tables. Where two versions of a package are seen on a side, a row that changed
   its version is a change of dependency and stays unpaired.
3. **Members**, only of two groups that are paired. A member is identified within its group by
   its file. If the file's `sha256` is the same digest on both sides, compared as a value so
   that upper- and lowercase hex are equal, members of equal kind pair when their name ranges
   are equal. If the file changed, its members stay unpaired, whatever their ranges.
4. **Unnamed virtual containers** (no `symbol`). Two with equal keys pair when at least one
   child of the one is paired with a child of the other, by any step, members included. This is
   repeated until nothing changes, so that a chain of unnamed containers pairs from its paired
   descendants upwards.

Entities with one definition (`def`) and entities with merged declarations (`decls`) share the
definition variant. The same package can therefore pair when its declaration count changes.

A definition whose symbol becomes a group because a second definition appeared is therefore
removed, and the group and its members are added. This is what a new collision looks like in a
diff.

A **mapped key** is the pair of keys of a paired row, or the single key of an unpaired row
together with its side. It is written as an object `{old?, new?}`: both keys for a paired row,
`old` alone for a row of the old snapshot only, `new` alone for one of the new snapshot only.
An unpaired row never equals a row of the other side that has the same key text.

## manifest.json

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `"gosanja-diff"` | marks the directory as a diff; see Publication |
| `schema` | version | diff schema version, `0.4` |
| `tool` | `{name, version}` | the engine that wrote the diff |
| `weights` | object | the weights and the threshold of candidates, below |
| `old` | side | the old snapshot |
| `new` | side | the new snapshot |
| `counts` | object | number of rows per file, below |
| `packages_added` | array of packages | packages that only the new side's relations reach, sorted |
| `packages_removed` | array of packages | packages that only the old side's relations reach, sorted |
| `not_compared` | array | `build`, `calls`, `syntax`, `test`: the comparisons left out (see What is not compared), sorted |

`weights` has `name`, `file`, `parents`, `size`, `outgoing`, `incoming` and `threshold`, each an
integer: the weight of the candidate feature of that name, and the threshold of the score.

A side:

| Field | Type | Meaning |
| --- | --- | --- |
| `schema` | version | the snapshot's `schema`, `0.5` |
| `project` | `{name}` | the snapshot's `project` |
| `adapter` | `{name, version, contract}`, optional | the scope's `adapter`, absent when the snapshot's scope has none |
| `layers` | `{base, syntax, calls, build}` | the scope's `layers` |
| `builds` | array | the scope's `builds` |
| `counts` | object | the snapshot's `counts` |
| `gaps` | object | the number of rows of the snapshot's `gaps.jsonl` per `gap`, one member per kind that has rows, in byte order of the kind |

`counts`:

| Field | Type | Meaning |
| --- | --- | --- |
| `entities` | `{added, removed, changed, rekeyed}` | number of rows of `entities.jsonl` per `change`, every member present |
| `externals` | `{added, removed, rekeyed}` | the same for `externals.jsonl` |
| `relations` | `{added, removed}` | the same for `relations.jsonl` |
| `candidates` | integer | number of rows of `candidates.jsonl` |

A package is `<manager>:<package>`, the first two parts of an external's base key, escaped as
there.

## entities.jsonl

One row per entity that is added, removed, changed or rekeyed. Sorted by `old`, then `new`; an
absent field sorts after any value.

| Field | Type | Meaning |
| --- | --- | --- |
| `change` | enum | `added`, `removed`, `changed` or `rekeyed` |
| `old` | key, optional | the entity's key in the old snapshot |
| `new` | key, optional | the entity's key in the new snapshot |
| `kind` | string, optional | the snapshot row's `kind` |
| `role` | enum, optional | the snapshot row's `role` |
| `name` | string, optional | the snapshot row's `name` |
| `parent` | key, optional | the snapshot row's `parent`, a key of the same side |
| `virtual` | `true`, optional | the snapshot row's `virtual` |
| `group` | `true`, optional | the snapshot row's `group` |
| `member_of` | key, optional | the snapshot row's `member_of`, a key of the same side |
| `file` | path, optional | the snapshot row's `def.file` |
| `properties` | object, `changed` only | one member per property that differs |

| `change` | Row of | Has | Has not |
| --- | --- | --- | --- |
| `added` | an unpaired row of the new snapshot | `new`, `name`, and of `kind`, `role`, `parent`, `virtual`, `group`, `member_of`, `file` those the snapshot row has | `old`, `properties` |
| `removed` | an unpaired row of the old snapshot | `old`, `name`, and the same as `added` from the old row | `new`, `properties` |
| `changed` | a pair with at least one differing property | `old`, `new`, `properties` | the snapshot row's fields |
| `rekeyed` | a pair whose keys differ and that has no differing property | `old`, `new` | `properties` and the snapshot row's fields |

A pair with equal keys and no differing property has no row. A position alone is never a change.
An added or removed merged entity has no `file`. A collision member always carries its
`def.file`.

`properties` lists the differing properties in the order of the table below. Each member is
`{old, new}`, the property's value on each side; a side's member is absent when the snapshot has
no value there, and at least one side is present. `file`, `lines` and `outline` are compared
only when both sides have them, so they always have both.

| Property | Compared values |
| --- | --- |
| `parent` | the parent's key; equal when both are absent or the two parents are paired with each other |
| `kind`, `role` | as in the snapshot |
| `file` | `def.file`, compared only when both sides have `def` |
| `lines` | lines of `def.body`: end line − start line + 1, minus 1 when the end column is 0 and the end line is after the start line; compared only when both sides have a body with the same `body_src` |
| `visibility`, `qualifiers`, `attributes` | from the entity's `syntax.jsonl` row; compared only when both sides have a row and the syntax layer is compared |
| `outline` | the number of outline nodes per `k`, counting nested nodes, as an object with one member per kind that occurs, in byte order of the kind; compared under the same condition and only when both sides have an `outline` |

## externals.jsonl

One row per external that is added, removed or rekeyed. Sorted as `entities.jsonl`.

| Field | Type | Meaning |
| --- | --- | --- |
| `change` | enum | `added`, `removed` or `rekeyed` |
| `old` | key, optional | the external's key in the old snapshot |
| `new` | key, optional | the external's key in the new snapshot |
| `package` | package, `added` and `removed` only | the external's manager and package name |

`added` and `removed` are an unpaired row of the new and the old snapshot. `rekeyed` is a pair
whose two keys differ. An external has no properties, so it is never `changed`, and a pair with
equal keys has no row.

## relations.jsonl

One row per relation that is added or removed. Sorted by `from.old`, `from.new`, `to.old`,
`to.new`, `rel`, then the bytes of the row; an absent field sorts after any value.

A relation's identity is its mapped `from`, its mapped `to`, `rel`, and then: for `ref`, its
`roles`; for `call`, `mechanism`, `resolution` and `src`. Each side's relations are reduced to
the set of their identities, and a row is written for each identity that is in one set only:
`removed` for the old set, `added` for the new. Sites and evidence are not compared.

| Field | Type | Meaning |
| --- | --- | --- |
| `change` | enum | `added` or `removed` |
| `from` | `{old?, new?}` | the mapped key of the source entity |
| `to` | `{old?, new?}` | the mapped key of the target: an entity, a group or an external |
| `endpoint` | enum, optional | `from`, `to` or `both`: which ends are unpaired; absent when both ends are paired |
| `rel` | enum | `ref`, `call` or `implements` |
| `roles` | array, `ref` only | the snapshot row's `roles` |
| `mechanism` | enum, `call` only | the snapshot row's `mechanism` |
| `resolution` | enum, `call` only | the snapshot row's `resolution` |
| `src` | provenance, `call` only | the snapshot row's `src`: the layer that recorded the call |

A row carries every part of its identity and nothing else of the snapshot row. An end of a
`removed` row has `old`, and an end of an `added` row has `new`. `endpoint` tells "the new
snapshot records a call from this function to that one" (no `endpoint`) from "a function is in
one snapshot only, with its calls" (`endpoint: from`). Either is a difference between recorded
relations.

**Packages.** `packages_added` lists the packages that are the target of at least one relation
identity of the new side and of none of the old side, counting only relations that are
compared. `packages_removed` lists the reverse. A second version of a package already reached
adds nothing.

## candidates.jsonl

A candidate row says that a removed and an added entity may be the same thing, with the evidence
it was ranked by. It is not a claim that they are. Sorted by `old`, then `rank`.

An eligible `removed` or `added` row comes from an entity with `def` and without `member_of`.
The two rows must have equal, present `kind`. For every removed row the eligible added rows
are scored. Those with a score of at least `weights.threshold` are ranked by score, highest
first, then by `new` in byte order. The first three are written with `rank` 1 to 3. An added
row may be a candidate of several removed rows.

| Field | Type | Meaning |
| --- | --- | --- |
| `old` | key | the removed entity |
| `new` | key | the added entity |
| `rank` | integer | 1 to 3 |
| `score` | measure | below |
| `name` | measure | feature |
| `file` | measure | feature |
| `parents` | measure | feature |
| `size` | measure | feature |
| `outgoing` | measure | feature |
| `incoming` | measure | feature |

A measure is an integer from 0 to 1 000 000. A feature is floor(numerator × 1 000 000 /
denominator):

| Feature | Weight | Numerator / denominator |
| --- | --- | --- |
| `name` | 30 | (L − d) / L, with d the Levenshtein distance between the two `name`s over Unicode scalar values and L the length of the longer; 1 000 000 when both are empty |
| `file` | 18 | 1 000 000 when `def.file` is equal, else 0 |
| `parents` | 18 | (L − d) / L over the two sequences of the names of the `parent` chain, from the outermost down, one name being one element; an entity without `parent` has the empty sequence; 1 000 000 when both are empty |
| `size` | 12 | the `lines` of the shorter body / those of the longer; 0 when either has no body or the two bodies have different `body_src` |
| `outgoing` | 13 | Σ min / Σ max over the two multisets of the relations leaving the entity, an element being (`rel`, mapped target) with paired targets only, counted once per relation row; 0 when Σ max is 0 |
| `incoming` | 9 | the same over the relations arriving at the entity, an element being (`rel`, mapped source) |

`score` = floor(Σ weight × feature / 100). The weights are those of `weights` in the manifest.
Every quantity is an integer, so every platform writes the same bytes. Nothing else in a diff
depends on candidates.

A candidate is a suggestion. Two short definitions in one file can pass the threshold on
`file`, `parents` and `size` alone, so a removed entity with no counterpart may still have
candidates.

## What is not compared

Two comparisons can be left out: one that would compare a layer with nothing or with a layer
that was not asked for, and, for calls, one between different builds. Whether `syntax` and
`calls` are each compared follows from the layer's state on the two sides:

| Old state | New state | Compared |
| --- | --- | --- |
| `present` | `present` | yes; for `calls`, only when the two sides' build contexts are equal as sets of whole rows |
| `not-requested` | any | no |
| any | `not-requested` | no |
| `present` | `absent` or `failed` | no |
| `absent` or `failed` | `present` | no |
| `absent` or `failed` | `absent` or `failed` | yes: there is nothing to compare, and the word is not added |

A comparison left out adds its word to `not_compared`. Call evidence holds for a build, so
when the calls layer is `present` on both sides, calls are compared only between equal build
contexts; in every other pair the build contexts are not looked at. A reader can tell from the two sides'
`layers` in the manifest which row of the table applied.

What a comparison left out covers:

- `syntax`: syntax properties, and relations whose `src` is the syntax layer;
- `calls`: relations whose `src` is the calls layer.

The same relations are left out of the candidates' relation features and of the packages.
Gaps themselves are not diffed. Their counts per kind are in the manifest.

One effect of the syntax layer lies outside this rule. Where the index gives a definition no
extent, an adapter's syntax layer can supply its body, and bodies decide which definition owns
a reference (`core:reference-in-body` relations, see [snapshot.md](snapshot.md)). Those
relations come from the base layer and are compared whatever the state of the syntax layer.
Two snapshots of one project, one with such a syntax layer and one without it, whether it was
not requested, absent or failed, can therefore differ although the code did not change:

- a reference can have another owner, or none: where the syntax layer supplied the only body
  that holds it, the reference is left unattributed (an `unattributed-references` gap) and its
  relation is missing, with its part in the candidates' `outgoing` and `incoming` features and
  in the packages;
- where a body of the syntax layer crossed one of the index, both were set aside as
  `overlapping-bodies`; without the syntax layer that overlap is gone, so the index's body can
  own references again, references outside the omitted body included, unless another overlap
  sets it aside or an inner body owns them;
- a candidate's `size` is 0 when either definition lacks a body, so a missing body can change
  a score or drop it below the threshold, whatever the relation features.

A published syntax row can also change whether a symbol listed in `merged` is folded. If the
symbol otherwise meets the folding conditions in [adapter-contract.md](adapter-contract.md), a
snapshot with a published syntax row at one of its sites holds a collision group and its
members, and a snapshot of the same base input without any such row holds one entity with
`decls`. A collision group and a merged entity have different pairing variants, so the diff
reports their unpaired rows as `removed` or `added`.

A reader comparing snapshots made with different layer selections should expect these rows.

Build facts are never compared: `targets.jsonl`, a file row's `targets`, `test` and
`constraint`, and an entity's `test` give no row, whatever changed. Two words say so:

- `build`, when either side's build layer is `present`;
- `test`, when either side has any entity or file `test`, whatever layer gave it, so that a
  mark from the syntax layer alone is disclosed too.

Pairing and every other comparison are unchanged, so a changed target, file fact or test mark
produces no row while a changed syntax property still does. Test evidence never reaches
`syntax.jsonl` and never decides whether a symbol is folded, so valid test evidence changes
neither a compared syntax property nor an entity's identity.

## Publication

The rules of a snapshot's publication apply, with these differences:

- `--out` may not be, contain or lie inside either input, after links are resolved.
- An existing non-empty directory is replaced only when it is a diff: its `manifest.json`
  parses and has `format` equal to `gosanja-diff`. A snapshot, like any other non-empty
  directory, is refused and left unchanged. A snapshot's output path holding a diff is
  refused in the same way.
