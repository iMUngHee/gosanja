# diff-public: a frozen pair of public snapshots

`v0.20.0/` and `v0.21.0/` are the snapshots of the `base64` crate at two release tags, taken
for the measurement in
[`docs/research/snapshot-diff-prestudy.md`](../../docs/research/snapshot-diff-prestudy.md).
They hold facts about the public source: keys, names, root-relative paths, positions, file
digests and relations. They hold no source text. `fixtures/regenerate.sh` does not rebuild
them, and there is no index beside them: they are kept as frozen inputs, so that a test of the
matcher reads the same snapshots and labels that the measurement read.

| | Old | New |
| --- | --- | --- |
| Repository | https://github.com/marshallpierce/rust-base64 | the same |
| Tag | `v0.20.0` | `v0.21.0` |
| Commit | `9fcde482cd6aace0716c82b6472398d737c5b4d5` | `d7fb31c4ada4ca45df5ae80ec691fa3a050d9c3e` |
| Snapshot | `v0.20.0/` | `v0.21.0/` |

Each snapshot was made by gosanja 0.2.0 (`566cc6b4430ef167399820f2b1151931fb84a649`), schema
0.2, from an index of rust-analyzer 0.3.3049-standalone with rustc 1.99.1, as the research
record states. Each `sha256` of its `files.jsonl` equals the digest of that file at its tag.

## Licence

`base64` is licensed `MIT OR Apache-2.0`, as its `Cargo.toml` states at both tags. The MIT
licence's notice is "Copyright (c) 2015 Alice Maz". The licence texts are in the repository's
`LICENSE-MIT` and `LICENSE-APACHE` at each tag. The upstream repository has no `NOTICE` file.

## truth.tsv

The list of expected pairs, written before any matcher was run and kept byte for byte as it
was frozen. Its SHA-256 is
`be21eb3729be71bcb2248ae5e32bed5ed06cf5b8e07e94ffb72751afeb460d25`. One row per vanished key,
tab-separated: project, old key, new key or `-`, `yes` / `no` / `ambiguous`, and the evidence
it was traced from. The base64 rows are the 20 renames and moves (`yes`) and the 3 deletions
(`no`) that the tests use. The file also holds the 11 rows of the second crate of the
measurement, itoa, whose snapshots are not kept here. They stay because removing them would
change the recorded digest.
