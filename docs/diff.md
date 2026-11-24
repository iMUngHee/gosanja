# Comparing two snapshots

A row of a diff says that one snapshot holds a statement and the other does not. It never says
that the code changed. A snapshot that observed less, because a file was skipped, an anchor did
not resolve or a position could not be converted, lacks statements for that reason and says so
in its gaps, so the manifest of a diff gives each side's gap counts per kind and its layer
states beside the rows. Read a row as a difference between two records of the code, and weigh
it with those counts.

The format is specified in [spec/diff.md](../spec/diff.md). This page says how to use it and
what it does not claim.

## The command

```sh
gosanja diff <old> <new> --out <dir>
```

`<old>` and `<new>` are snapshot directories, as `gosanja index` writes them. Both must be of
the engine's snapshot schema (0.2), each with exactly one scope, and each must pass
`gosanja validate`; the diff checks this itself, without the sources, before it writes
anything.

| Exit | When |
| --- | --- |
| 0 | the diff was written |
| 2 | a usage error; an input of another schema version, or with no scope or several; an output path that is not allowed (below) |
| 3 | an input that fails validation, with its first diagnostic; a file system failure |

## What it writes

| File | Holds |
| --- | --- |
| `manifest.json` | both inputs (schema, project name, adapter, layer states, builds, row counts, gap counts per kind), the candidate weights, row counts of the diff, the packages reached on one side only, and `not_compared` |
| `entities.jsonl` | entities `added`, `removed`, `changed` (with the properties that differ) or `rekeyed` |
| `externals.jsonl` | externals `added`, `removed` or `rekeyed` |
| `relations.jsonl` | relations `added` or `removed` |
| `candidates.jsonl` | removed and added entities that may be the same thing, ranked, with the evidence |

An entity or external is named by `old`, its key in the old snapshot, and `new`, its key in
the new one; one is absent when it is on one side only. The same two inputs always give the
same bytes.

## What counts as the same thing

Two rows are the same thing only when the pairing rules of [spec/diff.md](../spec/diff.md) pair
them: by their symbol, by their symbol under a package version that moved on, or, for members
of a collision group, by an unchanged file and an equal position. Equal key text is not
identity. A key such as `cargo:pair:f().` can name version 1.0.0 of a function in one snapshot
and version 2.0.0 in the other, and the diff reports a removed and an added row for it.

A row that the rules do not pair is reported as removed or added. That is noise, never a false
statement: the diff states what the two snapshots establish and nothing more.

**A new collision.** When a second definition of a symbol appears, the symbol's key names a
collision group in the new snapshot. The diff then shows the old definition as removed and the
group and its members as added. A reference to the symbol is removed as a reference to the
definition and added as one to the group, and a relation that left the definition is removed
from it and added from a member. Members of a group pair
only within a file whose digest is the same on both sides; in a changed file they are removed
and added with their relations.

**One project.** Pairing assumes that both snapshots describe one project, at two moments.
That is yours to know: the engine cannot check it, because `project.name` is only a display
name. A diff of two unrelated projects is written all the same, and its pairs mean nothing.

## Candidates

The diff never says that something was renamed or moved. A removed and an added definition
stay removed and added, and a row of `candidates.jsonl` says that they may be the same, with the
evidence they were ranked by: six features (name, file, parents, size, outgoing and incoming
relations), each a number from 0 to 1 000 000, and the weighted score. At most three candidates
are written for a removed definition, those at or above the threshold in the manifest.

Candidates are suggestions. A removed definition with nothing corresponding to it can still have
candidates: two short functions in one file score above the threshold on file, parents and size
alone. Check a candidate against the code before you rely on it.

## What is not compared

`not_compared` in the manifest lists the comparisons the diff did not make, because one side
had nothing to compare with:

- `syntax`: the syntax layer is present on one side only. Syntax properties (visibility,
  qualifiers, attributes, outline) and the calls the syntax layer declared are not compared.
- `calls`: the calls layer is present on one side only, or the two sides were built with
  different build contexts. Calls from that layer are not compared.

A comparison that is listed there produced no rows, so the absence of such rows says nothing.
What is not compared is also left out of the candidates and of the package lists.

## Inputs and the output

Both inputs must stay unchanged while the command runs. The diff reads each of them table by
table; a snapshot that another run replaces meanwhile is outside what a diff guarantees.

The output path follows the rules of a snapshot's (see [spec/snapshot.md](../spec/snapshot.md),
Publication), with these differences: it may not be, contain or lie inside either input, after
links are resolved; and an existing directory there is replaced only when it is an earlier diff.
A snapshot at the output path is refused, as a diff at a snapshot's output path is.

## An example

`fixtures/diff-basic` holds two versions of one small crate with one case of every kind of
change; [fixtures/diff-basic/NOTE.md](../fixtures/diff-basic/NOTE.md) lists them. From the root
of this repository:

```sh
cargo run -q -p gosanja -- diff fixtures/diff-basic/old/expected \
    fixtures/diff-basic/new/expected --out /tmp/gosanja-diff-demo
grep '"changed"' /tmp/gosanja-diff-demo/entities.jsonl
grep 'wander' /tmp/gosanja-diff-demo/candidates.jsonl
```

The four changed entities are a function that gained a call, one that grew, one whose
visibility changed and a module that grew:

```text
{"change":"changed","old":"cargo:diff-basic:caller().","new":"cargo:diff-basic:caller().","properties":{"outline":{"old":{"call":1},"new":{"call":2}}}}
{"change":"changed","old":"cargo:diff-basic:grows().","new":"cargo:diff-basic:grows().","properties":{"lines":{"old":3,"new":5}}}
{"change":"changed","old":"cargo:diff-basic:hidden().","new":"cargo:diff-basic:hidden().","properties":{"visibility":{"new":"pub"}}}
{"change":"changed","old":"cargo:diff-basic:util/","new":"cargo:diff-basic:util/","properties":{"lines":{"old":3,"new":7}}}
{"old":"cargo:diff-basic:wander().","new":"cargo:diff-basic:inner/meander().","rank":1,"score":734285,"name":714285,"file":1000000,"parents":0,"size":1000000,"outgoing":1000000,"incoming":1000000}
{"old":"cargo:diff-basic:wander().","new":"cargo:diff-basic:added().","rank":2,"score":630000,"name":500000,"file":1000000,"parents":1000000,"size":1000000,"outgoing":0,"incoming":0}
{"old":"cargo:diff-basic:wander().","new":"cargo:diff-basic:width().","rank":3,"score":579999,"name":333333,"file":1000000,"parents":1000000,"size":1000000,"outgoing":0,"incoming":0}
```

`wander` was moved into the module `inner` and renamed `meander`. The diff reports it as removed
and `inner/meander` as added, and ranks `meander` first among its candidates. Its name is close,
and it has the same caller and refers to the same `Sub` implementation, which is what
`incoming` and `outgoing` measure. The two unrelated new functions after it score on sharing
the file, the parents and the size. That ranking is evidence for a reader, not a decision.
