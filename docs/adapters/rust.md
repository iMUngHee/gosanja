# The Rust adapter

`gosanja-adapter-rust` gives the engine two layers for a Cargo workspace: the base layer, an
index made by `rust-analyzer scip`, and the syntax layer, read from the source with `syn`. It
has no calls layer yet, so a snapshot records a `missing-layer` gap for it and every call it
holds is a `declared` call: the callee name is bound to that declaration, and nothing is
claimed about what runs.

## What it does to your project

The adapter takes these measures to keep itself and the tools it runs from writing in the
project, and each is tested:

- Its tools do not inherit your environment. They get a fixed list of variables (`PATH`,
  `HOME`, the rustup home, toolchain and mirror settings, proxy and certificate settings), so a
  variable such as `RA_LOG_FILE`, `TMPDIR`, `RUSTFLAGS` or `CARGO_TARGET_DIR` that points into
  the project has no effect.
- Build output, intermediate build output and temporary files go to the adapter's own output
  directory, whatever the cargo configuration of the project or of your cargo home says about
  `build.target-dir` and `build.build-dir`.
- It refuses to run when cargo's home, its `registry` or `git` directory, or rustup's home lies
  inside the project.
- It never lets rustup install a toolchain.
- It checks with `cargo metadata --locked` that `Cargo.lock` exists and is complete before it
  runs anything that would write one.
- It resolves cargo's and rustup's homes to absolute paths before it starts a tool, and gives
  the tools exactly the paths it checked.

Two things are outside these measures, as the adapter contract allows, and you should know
them:

- **Your build scripts and procedural macros run.** `rust-analyzer scip` builds and runs them
  to see the code they generate, as `cargo build` and your editor do. A build script that
  writes into its own source directory will do so.
- **Your cargo configuration stays in force**, the project's and the one in your cargo home,
  apart from the two build directories. A
  compiler flag set there, for example an `--emit` path in `build.rustflags`, is applied as
  written.

## What it needs

`rust-analyzer` and `cargo` on `PATH`; `gosanja adapter list` shows whether they are found. The
project must be a workspace root with a `Cargo.lock` that matches its manifests. The adapter
was developed and tested with rust-analyzer 0.3.3049 and cargo 1.99.1 on Linux.

## Using it

Put `gosanja-adapter-rust` where the engine finds adapters (see [adapters.md](../adapters.md)),
then:

```sh
gosanja index --root <workspace> --out <dir>
```

The engine chooses the adapter because the workspace has a `Cargo.toml`. From a checkout of
this repository, with the real tools installed:

```sh
cargo build -p gosanja -p gosanja-adapter-rust
export GOSANJA_ADAPTER_PATH=$PWD/target/debug
target/debug/gosanja adapter list
target/debug/gosanja index --root fixtures/rust-flow --out /tmp/gosanja-rust-flow
target/debug/gosanja validate /tmp/gosanja-rust-flow --root fixtures/rust-flow
```

## When it cannot index

Once the adapter runs, a problem with the project is reported as a failed base layer with a
code, and the engine exits 3 with `base layer failed: <code>`. What a failing tool printed on
its standard error is passed on. (When you name no adapter and the root has no `Cargo.toml`,
the adapter is never chosen: the engine exits 2 and says that no installed adapter has a
marker file there.)

| Code | Meaning |
| --- | --- |
| `manifest-missing` | no `Cargo.toml` in the root (seen only when the adapter was named with `--adapter`) |
| `tool-home-inside-root` | cargo's or rustup's home, or cargo's `registry` or `git`, is in the project, or its path holds a `.` or `..` in a part that does not exist yet and so cannot be placed |
| `tool-home-unresolvable` | such a home could not be resolved |
| `toolchain-missing` | `cargo --version` or `rust-analyzer --version` could not be started or failed |
| `manifest-invalid` | `cargo metadata` rejects the manifests |
| `not-workspace-root` | the root is not the root of its workspace |
| `lockfile-missing` | no `Cargo.lock`; create one with `cargo generate-lockfile` |
| `locked-metadata-failed` | the lockfile is out of date, or a dependency cannot be fetched |
| `indexer-failed` | `rust-analyzer scip` failed or wrote no index |
| `index-unreadable` | the index could not be decoded |
| `index-incomplete` | the index lists no file, or lacks the root file of a library, procedural macro or binary of the workspace (a binary with `required-features` may be absent) |

These codes are what you see through `gosanja index`, which creates the output directory
itself. Run directly, the adapter also refuses an invocation that is itself wrong (a root
that is not a directory, an output directory that is missing, not empty, or inside the root):
it exits 2 and writes nothing.

## What the syntax layer reports

One row for each function, method, struct, enum, union, trait, type alias, associated type,
constant, static, inline module and `macro_rules!` definition written in a file the index
lists: its visibility, its qualifiers (`const`, `async`, `unsafe`, `extern "C"`, `default`,
`mut`, `auto`) and its attributes as written, without doc comments. For a function with a
body it adds an outline: branches and their arms, loops, exits (`return`, `break`, `continue`,
`?`), `.await`, closures and `async` blocks, and calls.

A call names its callee by the callee's name token. The engine turns it into a `declared`
call when the index has a reference exactly there. A call whose callee is not a name, such as
`make()(x)`, is reported without one and becomes an `open-call` gap.

## What it cannot see

- **Macro invocations.** The adapter does not expand macros and does not guess what their
  tokens mean. An invocation inside a function body, or written as an item in a trait, a
  module or a file, is an `opaque` node and an `opaque-region` gap. One in a place that has
  no outline is not reported at all: a field type, a function's parameter or return type, the
  initializer of a constant or static. On test-heavy code this is most of the gaps: `assert_eq!` alone accounts for many. The
  references inside the invocation are still in the base layer.
- **Items that exist only after expansion** have no syntax row.
- **Items under a `#[cfg]` that is not active.** In a file the index lists, the adapter sees
  them, the index does not, and each becomes an `unanchored` gap. A file that the index does
  not list at all, such as the file of an inactive `mod`, is never read.
- **Enum variants, fields and items in `extern` blocks** have no syntax row, and constants and
  statics have no outline.
- **A file that does not parse, is not UTF-8, cannot be read or starts with a byte-order
  mark** is reported as a `skipped-file` gap; the other files are unaffected. `syn` lags the
  compiler for the newest syntax, so a file using it may be skipped.
- **An outline that would mislead** is withheld, with a `skipped-file` gap for its file: one
  whose nodes fall outside the extent the index gives the definition
  (`outline-outside-definition`), and one nested more than 32 levels deep, which a long chain
  of method calls can reach (`outline-too-deep`).
- Constant items named `_` have no row: the index does not define them.

Measurements behind these statements, and how often each case occurs on real crates, are in
[research/rust-adapter-measurements.md](../research/rust-adapter-measurements.md).
