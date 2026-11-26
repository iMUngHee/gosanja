# Release self-index history

For each workspace release, build Gosanja and the Rust adapter from the newer release.
Index both source revisions with those binaries, the same compiler and indexer, and
`--layers base,syntax`. Validate both snapshots and compare them with the newer
release's `gosanja diff`. Record the source revisions, measurement tool versions,
counts, and any gaps that affect the comparison before creating the new release tag.
Use the previous release tag as the old source ref. When reconstructing an untagged
checkpoint, record its commit and how it was selected.

Use separate source and output directories. Build the binaries from the newer
source with `cargo build --locked -p gosanja -p gosanja-adapter-rust`. With `gosanja`
and `adapter` set to their absolute paths, archive each source ref into a directory
named `source` beneath its own scratch directory:

```sh
mkdir -p "$scratch/source"
git archive "$source_ref" | tar -x -C "$scratch/source"
"$gosanja" index --adapter "$adapter" --root "$scratch/source" \
  --out "$scratch/snapshot" --layers base,syntax
"$gosanja" validate "$scratch/snapshot" --root "$scratch/source"
# After repeating the above for both source refs:
"$gosanja" diff "$old_snapshot" "$new_snapshot" --out "$diff_output"
```

Read the index and diff manifests for counts and layer coverage. Use the module,
package, and unreferenced-definition checks described in the
[self-check conventions](CONVENTIONS.md#the-engines-map-of-its-own-code).
The numbers describe observed structure and evidence coverage. Read changes in
counts alongside the gap kinds and the scope of the two inputs.

## diff (0.1.0 -> 0.2.0, tool-version: 0.2.0)

Measured on 2026-11-27 on Linux x86-64. Both sources were untagged. The old
checkpoint is `f3294e4a0fa6e5ac1825642d80b80872b452d577`, the last commit on
`main` with workspace version 0.1.0. The new checkpoint is
`fb521ab281dd9d6300b019cae837ddd1766ec7de`, the current 0.2.0 commit at the time
of measurement and the source of both measurement binaries.

Cargo and rustc were 1.99.1, and rust-analyzer was `0.3.3049-standalone`.
Both snapshots validated with schema 0.6 and had the base and syntax layers
present. The comparison covers the Rust workspace. The diff manifest lists
`calls` and `test` under `not_compared`.

| Measure | 0.1.0 | 0.2.0 |
| --- | ---: | ---: |
| Indexed files | 26 | 77 |
| Entities | 946 | 3,036 |
| Relations | 10,615 | 42,672 |
| Source module pairs | 57 | 126 |
| Package layering violations | 0 | 0 |
| Unreferenced definition candidates | 3 | 6 |
| Gaps | 813 | 7,195 |
| Open calls | 61 | 361 |

The diff added 2,114 entities, removed 24, changed 90, and rekeyed one. It added
33,195 relations and removed 1,138. The largest new gap kind was
`overlapping-bodies` with 3,933 rows. The larger source tree includes the Rust
adapter, which was absent in the 0.1.0 checkpoint.
