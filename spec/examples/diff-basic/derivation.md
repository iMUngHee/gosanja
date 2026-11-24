# diff-basic: how each row was derived

This note is not part of the example and is not compared. The five files beside it were derived
row by row, by hand, from the two sources and the two snapshots of `fixtures/diff-basic`
(`old/expected`, `new/expected`), under the rules of [../../diff.md](../../diff.md). They are
written in the field order that document lists. `fixtures/diff-basic/expected` is the same five
files, byte for byte, and is what the engine is tested against. The cases are listed in
`fixtures/diff-basic/NOTE.md`.

Keys are abbreviated: `L:x` is `cargo:diff-basic:x`. `Add`, `Mul` and `Sub` are the core
externals `cargo:core:ops/arith/impl#[u8][`Add<Self>`]add().`, and so on.

## Pairing

Step 1, exact symbol, one non-member row per side, same table and variant:

- Entities: `both_g`, `callee`, `caller`, `crate/` (group with group), `grows`, `hidden`,
  `main()` (group with group), `uses_dep`, `uses_f`, `uses_moved`, `util/`, `util/stays`.
  `mover()` and `util/mover()` are different symbols and are not paired.
- Externals: `Add`, `Mul`, `Sub`, `pair crate/` 1.0.0 and 2.0.0, `pair g()` 1.0.0 and 2.0.0 with
  equal keys; `dep crate/` and `dep helper()` 0.1.0 with keys `cargo:dep:crate/` ->
  `cargo:dep:crate/@0.1.0` and `cargo:dep:helper().` -> `cargo:dep:helper().@0.1.0`.

Step 2, version moved on: the only identity left with one row on each side is `pair f()`
(old 1.0.0, new 2.0.0, both keyed `cargo:pair:f().`). Package `pair` has versions 1.0.0 and 2.0.0
on each side, so it is not paired. `dep` 0.2.0's rows have no old row left. Nothing pairs.

Step 3, members of the paired groups. File digests: tool.rs `8b1c...` -> `a34e...` (changed),
lib.rs `6bf6...` -> `93ee...` (changed), main.rs `47935a88...` on both sides. extra.rs is new.

| Group | old member | new member | Result |
| --- | --- | --- | --- |
| `crate/` | `~1` tool.rs | `~2` tool.rs | file changed: both unpaired |
| `crate/` | `~2` lib.rs | `~3` lib.rs | file changed: both unpaired, though both span the file |
| `crate/` | `~3` main.rs `[0,0,3,0]` | `~4` main.rs `[0,0,3,0]` | same digest, kind, range: pair |
| `crate/` | - | `~1` extra.rs | unpaired |
| `main()` | `~1` tool.rs `[0,3,0,7]` | `~2` tool.rs `[1,3,1,7]` | file changed: both unpaired |
| `main()` | `~2` main.rs | `~3` main.rs | same digest and range: pair |
| `main()` | - | `~1` extra.rs | unpaired |

Step 4: no row without `symbol`, so no unnamed container to pair.

Totals: 14 entity pairs; old 21 = 14 + 7 unpaired, new 26 = 14 + 12. 9 external pairs; old
10 = 9 + 1, new 15 = 9 + 6.

## entities.jsonl (25 rows)

- removed (7): `mover`, `old_name`, `removed`, `wander` (rust:function, callable, src/lib.rs); the
  `crate/` members `~1` (tool.rs) and `~2` (lib.rs); `main().~1` (tool.rs).
- added (12): `added`, `new_name`, `uses_dep2`, `width` (src/lib.rs); `inner/` (rust:module,
  container, src/lib.rs); `inner/meander` (parent `L:inner/`); `util/mover` (parent `L:util/`,
  src/util.rs); `crate/~1` (extra.rs), `crate/~2` (tool.rs), `crate/~3` (lib.rs); `main().~1`
  (extra.rs), `main().~2` (tool.rs).
- changed (4), the only differing properties of the pairs:
  - `caller`: outline `{"call":1}` -> `{"call":2}`.
  - `grows`: lines 3 -> 5 (`[22,0,24,1]`: 24 - 22 + 1; `[20,0,24,1]`: 24 - 20 + 1).
  - `hidden`: visibility absent -> `pub`.
  - `util/`: lines 3 -> 7 (`[0,0,3,0]`: 3 - 0 + 1 - 1, end column 0; `[0,0,7,0]`: 7).
- rekeyed (2): the main.rs members `crate/~3` -> `crate/~4` and `main().~2` -> `main().~3`
  (no differing property: lines 3 and 3, the `main` syntax rows equal).
- No row: `both_g`, `callee`, `uses_dep`, `uses_f`, `uses_moved`, `util/stays` and both groups,
  although most of them moved lines; their lines, visibility and outline counts are equal.

## externals.jsonl (9 rows)

- rekeyed: `cargo:dep:crate/` -> `cargo:dep:crate/@0.1.0`, `cargo:dep:helper().` ->
  `cargo:dep:helper().@0.1.0`.
- removed: old `cargo:pair:f().` (1.0.0), package `cargo:pair`.
- added: new `cargo:pair:f().` (2.0.0), `cargo:core:mem/`, `cargo:core:mem/size_of().`,
  `cargo:dep:crate/@0.2.0`, `cargo:dep:helper().@0.2.0`, `cargo:std:crate/`.

## relations.jsonl (46 rows)

Old has 30 relations and new 48. With the mapping above, 16 identities are in both: the seven of
`both_g` (the `g` and `pair crate/` targets pair by exact symbol), `caller -> hidden` (ref, call),
the main.rs `main()` member's three (`-> caller` ref and call, `-> crate/` ref), `uses_dep`'s
three (the `dep` 0.1.0 targets are paired, so the new `@0.1.0` keys change nothing), and
`uses_moved -> Add`. Each remaining relation is one row.

removed (14):

- `endpoint: from`: lib.rs crate member `L:crate/~2 -> util/` (the whole-file case); tool.rs
  `main().~1 -> callee` (call, ref) and `-> crate/`; `mover -> Mul`; `old_name -> Add`;
  `wander -> Sub`.
- `endpoint: to`: `uses_f -> f()` 1.0.0 (call, ref); `uses_moved -> mover` and `-> wander`
  (call, ref each).
- no endpoint: `uses_f -> pair crate/@1.0.0`.

added (32):

- no endpoint: `caller -> callee` (call, ref), `caller -> Add`; `grows -> Add`, `-> Mul`,
  `-> Sub`; `uses_f -> pair crate/@2.0.0`; `uses_moved -> util/` (paired module).
- `endpoint: from`: lib.rs `L:crate/~3 -> util/`; tool.rs `main().~2 -> callee` (call, ref) and
  `-> crate/`; extra.rs `main().~1 -> crate/`; `inner/meander -> Sub`; `new_name -> Add`;
  `util/mover -> Mul`.
- `endpoint: to`: `uses_f -> f()` 2.0.0 (call, ref); `uses_moved -> inner/meander` (call, ref),
  `-> util/mover` (call, ref), `-> inner/`.
- `endpoint: both`: extra.rs `main().~1 -> width` (call, ref); `uses_dep2 -> helper()@0.2.0` (call,
  ref) and `-> dep crate/@0.2.0`; `width -> size_of` (call, ref), `-> mem/`, `-> std crate/`.

The `f` case: the two `f` rows stay unpaired, and `uses_f`'s call and ref are
removed to the 1.0.0 row and added to the 2.0.0 row, which have the same key text. The
whole-file member and the tool.rs members are written as removed and added with their outgoing
relations, and none cancels.

## Manifest

`not_compared` is `["calls"]`. Syntax is `present` on both sides and is compared. Calls is
`present` on both sides too, but the build contexts are not equal as sets of whole rows: the
old side has three, `diff-basic-bin-diff-basic`, `diff-basic-bin-tool` and
`diff-basic-lib-diff_basic`, and the new side has a fourth, `diff-basic-bin-extra`, for its
new binary `src/bin/extra.rs`. So the relations whose `src` is `adapter:rust/calls` are not
compared, and they are left out of the candidate features and of the packages. They are the
only relations the calls layer added to either side: on the old side `caller -> hidden`,
`uses_moved -> mover` and `-> wander`; on the new side `caller -> hidden` and `-> callee`,
`uses_moved -> util/mover` and `-> inner/meander`. Every other row of both sides' relations
is as before, so `entities.jsonl`, `externals.jsonl`, `relations.jsonl` and
`candidates.jsonl` are unchanged, and so are the row counts.

Each side gives its `layers`, its `builds` and its `counts` as its snapshot records them: old
33 relations and 4 gaps, new 52 relations and 5 gaps. Gap counts per kind: old
`skipped-file` 2 (`callee-unresolved` for `src/bin/tool.rs` and `src/main.rs`, whose calls
into the library the binaries only declare) and `symbol-collision` 2; new `skipped-file` 3
(`src/bin/extra.rs` too) and `symbol-collision` 2. No `missing-layer` gap remains.

`packages_added` is `["cargo:std"]`: the new side reaches externals of core, dep, pair and std,
the old side of core, dep and pair. `dep` 0.2.0 adds nothing (see Packages in diff.md).
`packages_removed` is `[]`.

## candidates.jsonl (12 rows)

Eligible removed rows: `mover`, `old_name`, `removed`, `wander` (members excluded). Eligible added
rows of their kind (rust:function): `added`, `inner/meander`, `new_name`, `uses_dep2`,
`util/mover`, `width`. Facts used:

- file: every removed row is in src/lib.rs, and every added row except `util/mover` (src/util.rs).
- parents, from the `parent` chain: `[]` for every crate-root item, `["inner"]` for `meander`,
  `["util"]` for `util/mover`. `[]` and `[]` give 1 000 000; `[]` and `[x]` give (1 - 1) / 1 = 0.
- size: every body of the ten rows spans 3 lines (for example `[14,0,16,1]` and
  `meander` `[3,4,5,5]`), all `scip:rust-analyzer`, so 1 000 000 everywhere.
- outgoing, paired targets only: `mover {(ref,Mul)}`, `old_name {(ref,Add)}`, `removed {}`,
  `wander {(ref,Sub)}`; `added {}`, `meander {(ref,Sub)}`, `new_name {(ref,Add)}`, `util/mover
  {(ref,Mul)}`, `uses_dep2 {}` and `width {}` (their targets are unpaired).
- incoming, paired sources only: `mover`, `wander`, `meander` and `util/mover` each
  `{(call,uses_moved),(ref,uses_moved)}`; the others nothing (`width`'s caller is the unpaired
  extra.rs member).
- name: Levenshtein over the entity names. The distances were computed with a small DP written
  from the definition in diff.md and checked by hand for the rank-1 pairs: `mover`/`mover` 0,
  `old_name`/`new_name` 3 (o->n, l->e, d->w), `wander`/`meander` 2 (w->m, insert e).

score = floor((30 name + 18 file + 18 parents + 12 size + 13 outgoing + 9 incoming) / 100).

| old | new | rank | score | name | file | parents | size | outgoing | incoming |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| mover | util/mover | 1 | 640000 | 1000000 | 0 | 0 | 1000000 | 1000000 | 1000000 |
| mover | added | 2 | 540000 | 200000 | 1000000 | 1000000 | 1000000 | 0 | 0 |
| mover | inner/meander | 3 | 518571 | 428571 | 1000000 | 0 | 1000000 | 0 | 1000000 |
| old_name | new_name | 1 | 797500 | 625000 | 1000000 | 1000000 | 1000000 | 1000000 | 0 |
| old_name | added | 2 | 517500 | 125000 | 1000000 | 1000000 | 1000000 | 0 | 0 |
| old_name | width | 3 | 517500 | 125000 | 1000000 | 1000000 | 1000000 | 0 | 0 |
| removed | added | 1 | 565714 | 285714 | 1000000 | 1000000 | 1000000 | 0 | 0 |
| removed | uses_dep2 | 2 | 546666 | 222222 | 1000000 | 1000000 | 1000000 | 0 | 0 |
| removed | new_name | 3 | 517500 | 125000 | 1000000 | 1000000 | 1000000 | 0 | 0 |
| wander | inner/meander | 1 | 734285 | 714285 | 1000000 | 0 | 1000000 | 1000000 | 1000000 |
| wander | added | 2 | 630000 | 500000 | 1000000 | 1000000 | 1000000 | 0 | 0 |
| wander | width | 3 | 579999 | 333333 | 1000000 | 1000000 | 1000000 | 0 | 0 |

Worked through for the three that step 2 names:

- `old_name -> new_name`: name (8 - 3) / 8 = 625000; (30 x 625000 + 18e6 + 18e6 + 12e6 + 13e6) /
  100 = 797500. The best other added function, `added` or `width`, scores 517500.
- `mover -> util/mover`: (30e6 + 0 + 0 + 12e6 + 13e6 + 9e6) / 100 = 640000. Next: `added` 540000
  (name (5 - 4) / 5 = 200000; 6e6 + 48e6 = 54e6), `meander` 518571, `uses_dep2` 513333, `new_name`
  and `width` 480000.
- `wander -> inner/meander`: name (7 - 2) / 7 = 714285; (21428550 + 18e6 + 0 + 12e6 + 13e6 + 9e6)
  / 100 = 734285. Next: `added` 630000 (name (6 - 3) / 6 = 500000), `width` 579999,
  `new_name` 555000, `uses_dep2` 546666, `util/mover` 309999.

Rows not written: `old_name` with `meander` 337500 and `util/mover` 120000; `removed` with
`meander` 385714 and `util/mover` 291428; `wander` with `util/mover` 309999; and, below rank 3,
`mover` with `uses_dep2`, `new_name`, `width`, `old_name` with `uses_dep2`, `removed` with `width`,
and `wander` with `new_name` and `uses_dep2`. Ties: `old_name` with `added` and with `width` are
both 517500 and are ranked by `new` in byte order (`added` first).

## Checks made on the derivation

- Every key in the rows exists in the snapshot of its side, and so does every `parent` and
  `member_of`.
- A second pass took only the pairing table above. It reduced each side's relations to
  identities and compared the sets, giving 46 rows, equal to the rows here with nothing
  found by one pass only. Every `endpoint` matches the unpaired ends.
- The five files pass the five diff schemas.
