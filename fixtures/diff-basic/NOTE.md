# diff-basic

Two versions of one small Rust crate, `old/` and `new/`, written for this repository. Each side
has its index (`index.scip`) and its snapshot (`expected/`), both made by
`fixtures/regenerate.sh diff-basic` at the fixed paths `/tmp/gosanja-fixtures/diff-basic/old` and
`/tmp/gosanja-fixtures/diff-basic/new`. The snapshot is written by the engine with the Rust adapter,
whose syntax layer gives the visibility change and the declared calls. `deps/` holds the path
dependencies of both sides. `expected/` at the top is the diff of the two snapshots, identical
file for file to `spec/examples/diff-basic`, where the derivation of every row is written out.

## The cases

| Case | old | new |
| --- | --- | --- |
| added | - | `added` |
| removed, no counterpart | `removed` | - |
| renamed in place | `old_name` (`x + 1`) | `new_name`, same body |
| moved to another module unchanged | `mover` (crate root, `x * 2`) | `util::mover` in `src/util.rs` |
| moved and renamed | `wander` (crate root, `x - 3`) | `inner::meander`, an inline module in `src/lib.rs`, same body |
| caller of the two moved functions | `uses_moved`: `mover(1) + wander(9)` | `util::mover(1) + inner::meander(9)` |
| grown | `grows`, 3 lines | 5 lines |
| visibility | `fn hidden` | `pub fn hidden` |
| call added between existing functions | `caller` calls `hidden` | `hidden() + callee()` |
| new external package | - | `width` uses `std::mem::size_of` |
| second version of a dependency that requalifies a key | `dep` 0.1.0 | `dep` 0.1.0 and, as `dep2`, 0.2.0 (`uses_dep2`); the 0.1.0 keys gain `@0.1.0` |
| one function seen in two versions of a package | `uses_f` calls `pair1::f`; `both_g` calls the `g` of both | `uses_f` calls `pair2::f`; `both_g` unchanged |
| a member in a changed file, at A on one side and B on the other | `src/bin/tool.rs` `main` on line 0 | a comment line above it moves `main` to line 1 |
| a member spanning a changed file | the crate root of `src/lib.rs`, with `ref util/` | the same, grown |
| a collision group gains a member in another file | `crate/` with 3 members, `main()` with 2 | `src/bin/extra.rs` adds one to each; `src/main.rs` is unchanged |

## The three candidates

The renamed, moved, and moved-and-renamed functions each score at or above the threshold of
440 000 against their counterpart and above every other added function. Worked by hand from
the two snapshots with the features and weights of the plan (D5):

- `file`: whether `def.file` is equal.
- `parents`: the names along the `parent` chain. A crate-root item has no `parent`, so its
  sequence is empty.
- `size`: every body involved spans 3 lines with the same `body_src`.
- `outgoing` and `incoming`: only relations whose other end is paired count. `uses_moved` is
  paired, so it is the caller each side shows.

| old | new | name | file | parents | size | outgoing | incoming | score |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `old_name` | `new_name` | 625000 | 1000000 | 1000000 | 1000000 | 1000000 | 0 | 797500 |
| `mover` | `util/mover` | 1000000 | 0 | 0 | 1000000 | 1000000 | 1000000 | 640000 |
| `wander` | `inner/meander` | 714285 | 1000000 | 0 | 1000000 | 1000000 | 1000000 | 734285 |

- `old_name` / `new_name`: the Levenshtein distance is 3 (o→n, l→e, d→w) over 8 characters,
  giving name 625000. Both are at the root of `src/lib.rs`, and both reference `Add`. Score:
  (30 × 625000 + 18 × 10⁶ + 18 × 10⁶ + 12 × 10⁶ + 13 × 10⁶) / 100 = 797500. The best other added
  function, `added` or `width`, scores 517500.
- `mover` / `util/mover`: the names are equal. The files differ, and the parent chains `[]` and
  `[util]` give 0. Both reference `Mul` and are called and referenced by `uses_moved`. Score:
  (30 × 10⁶ + 12 × 10⁶ + 13 × 10⁶ + 9 × 10⁶) / 100 = 640000. Next: `added` 540000 (name 200000),
  `inner/meander` 518571.
- `wander` / `inner/meander`: the distance is 2 (w→m, an inserted e) over 7 characters, giving
  name 714285. Both are in `src/lib.rs`, with parent chains `[]` and `[inner]`. Both reference
  `Sub` and have the same callers. Score: (30 × 714285 + 18 × 10⁶ + 12 × 10⁶ + 13 × 10⁶ +
  9 × 10⁶) / 100 = 734285. Next: `added` 630000 (name 500000), `width` 579999.

The function with no counterpart, `removed`, still has three candidates (565714, 546666 and
517500). Two 3-line functions at the root of one file score 480000 on `file`, `parents` and
`size` alone, which is above the threshold.
