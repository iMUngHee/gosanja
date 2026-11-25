# Rust-analyzer reference variance

Local measurement on 2026-11-26 on Linux x86-64 with rust-analyzer 0.3.3049-standalone
(release 2026-09-14), rustc 1.99.1 and Cargo 1.99.1. The input was an archive of the
gosanja workspace at `482df35ac16e9f1035738e39a355bd982094cb0b`, with its lockfile.

## Repeat runs

Build `gosanja` and `gosanja-adapter-rust` from the archive with
`cargo build --locked -p gosanja -p gosanja-adapter-rust`. With both pinned tools on
`PATH` and the adapter discoverable, run the following for each new output directory.
The measurement used a rust-analyzer wrapper that enabled offline Cargo and captured
the raw SCIP file. The default Cargo job setting and the adapter's SCIP arguments
were left as supplied by the tools.

```sh
gosanja index --root "$root" --layers base,syntax --out "$out"
gosanja validate "$out" --root "$root"
```

With fresh build directories on each of eight runs, the normalized non-local SCIP
occurrences (sorted by document, range, role bits and symbol) numbered 43,315 twice and
43,307 six times. The eight occurrences present only in the larger result reference
`proc-macro2` symbols for `Span::start`, `Span::end`, `LineColumn::line`,
`LineColumn::column` and `extra::invalidate_current_thread_spans`. The larger input
produced six additional `ref` rows, four additional `declared` calls, five additional
external symbol rows and four fewer `open-call` gaps, from 297 to 293. Each snapshot
validated. Feeding each captured SCIP file through the adapter twice by substituting
the file at `scip --output` gave byte-identical snapshots for each input.

## Replaying the two Cargo results

The workspace's Cargo resolver builds `proc-macro2` 1.0.107 in two feature contexts.
The direct dependency enables `span-locations`. The unit used by `serde_derive` enables
`default,proc-macro`. With the pinned toolchain and build directories outside the archive,
capture the counterpart of the indexer's build-script command.

```sh
__CARGO_TEST_CHANNEL_OVERRIDE_DO_NOT_USE_THIS=nightly cargo check --quiet --locked \
  --workspace --message-format=json --keep-going --compile-time-deps --all-targets \
  -Zunstable-options > "$scratch/cargo.jsonl"
```

The stream contained two `build-script-executed` records with the same `package_id`
and different `out_dir`s. One record's `cfgs` was `["span_locations"]` and the other's
was `["wrap_proc_macro","proc_macro_span_location","proc_macro_span_file"]`.
Save two copies of the stream with only these records exchanged. For each copy, set
`cargo.buildScripts.overrideCommand` in the JSON config to `["cat", "<absolute stream path>"]`
and run `rust-analyzer scip . --config-path "$config" --output "$scratch/index.scip"`.

Putting the `span_locations` record last gave 43,315 occurrences in all three replays.
Putting the other record last gave 43,307 in all three. The result also held with one
cache-priming thread. The pinned SCIP command waits for the build-script command before
indexing. The loader looks up each result by Cargo package ID and replaces the `cfg`s
and `out_dir` in that package's record. The second record therefore determines which
`cfg`s reach the indexer.

Eight additional adapter runs with two Cargo jobs all produced the larger result on
this host. The controlled replay shows the overwrite still applies. The observed
frequencies describe one workspace and one host.

Pinned sources for the SCIP load, build-script wait, package lookup, assignment and
override setting

- https://github.com/rust-lang/rust-analyzer/blob/2026-09-14/crates/rust-analyzer/src/cli/scip.rs#L45-L68
- https://github.com/rust-lang/rust-analyzer/blob/2026-09-14/crates/load-cargo/src/lib.rs#L50-L72
- https://github.com/rust-lang/rust-analyzer/blob/2026-09-14/crates/project-model/src/build_dependencies.rs#L266-L305
- https://github.com/rust-lang/rust-analyzer/blob/2026-09-14/crates/project-model/src/build_dependencies.rs#L322-L360
- https://github.com/rust-lang/rust-analyzer/blob/2026-09-14/crates/rust-analyzer/src/config.rs#L725-L747
- https://docs.rs/crate/proc-macro2/1.0.107/source/build.rs
