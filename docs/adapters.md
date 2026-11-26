# Adapters

gosanja's engine knows no programming language. Everything it learns about a project comes
from an adapter: a separate program that knows one language ecosystem and speaks the
[adapter contract](../spec/adapter-contract.md). Adapters are installed next to the engine,
not built into it, so a language can be added without rebuilding the engine.

Two adapters are in this repository. [The Rust adapter](adapters/rust.md) gives all three
layers of the contract: the base layer from rust-analyzer's index, the syntax layer from the
source, and the calls layer from the LLVM IR of a build of the project. [The Go
adapter](adapters/go.md), a Go program under `adapters/go`, gives the base layer from
scip-go's index and the syntax layer from `go/packages`; it has no calls layer yet. An
adapter for TypeScript is planned. This page describes how the engine finds and uses
whatever is installed.

## What an installed adapter is

An executable file named `gosanja-adapter-<name>`, where `<name>` is made of lower-case
letters, digits and `-`. On Windows the file name ends in `.exe`. A file such as
`gosanja-adapter-rust.sh` is not an adapter, because a dot is not part of a name.

The engine looks in these directories, in this order, and the first file found for a name is
that adapter:

1. each directory in `GOSANJA_ADAPTER_PATH` (a path list, like `PATH`);
2. the directory that holds the `gosanja` executable;
3. each directory in `PATH`.

Empty and relative entries are ignored. A later file of the same name is never looked at,
even when the first one turns out not to work. If a directory on the list cannot be read, or
a file with an adapter's name cannot be inspected, the engine stops with exit code 3 instead
of guessing what was there.

Installing an adapter is putting its executable in one of those directories. The engine never
downloads or installs an adapter, or a tool an adapter needs.

## Seeing what is installed

```sh
gosanja adapter list
```

```text
old path=/usr/local/bin/gosanja-adapter-old unusable: written with version 0.5, this reader implements 0.6; 0.x versions are only compatible with themselves
rust 1.2.3 contract=0.6 languages=rust layers=base,syntax,calls,build markers=Cargo.toml path=/usr/local/bin/gosanja-adapter-rust
  requires rust-analyzer: found /usr/bin/rust-analyzer
  requires cargo: missing
```

Adapters are listed in name order, usable or not. Each adapter says which external tools it
needs. `found` means an executable file of that name is in an absolute directory of `PATH`;
`missing` means it is not. The engine does nothing more with this: it still runs an adapter
whose tool is missing, and what happens then is up to the adapter. A tool is looked up only
when its name is made of letters, digits, `.`, `_` and `-`; any other value, a path or a
name such as `g++`, is shown as `not checked`.

To list adapters, the engine runs `describe` on every one it finds, which means it executes
those programs. An adapter that does not return from `describe` holds the command up; there
is no time limit yet.

The text of `adapter list` is meant for people. It may change between releases, so do not
parse it; `gosanja adapter describe <name>` prints an adapter's manifest as JSON.

## Indexing a project

```sh
gosanja index --root <project> --out <dir>
```

The engine chooses the adapter by marker files. An adapter's manifest names the files that
mark its ecosystem, for example `Cargo.toml`; an adapter is a candidate when one of its
markers is a regular file directly in `<project>`.

| Candidates | What happens |
| --- | --- |
| exactly one | it is run |
| none | exit code 2, with the installed adapters and their markers |
| several | exit code 2, naming them; choose one with `--adapter` |

When the engine refuses, nothing is written: a snapshot already at `--out` stays as it was.

To choose yourself:

```sh
gosanja index --adapter rust --root <project> --out <dir>         # an installed adapter, by name
gosanja index --adapter ./my-adapter --root <project> --out <dir>  # a program, by path
```

An argument that is absolute or contains a path separator is a program; anything else is the
name of an installed adapter. A bare word is never looked up as a program in `PATH`: to run
one, give its path.

### Choosing layers

```sh
gosanja index --root <project> --out <dir> --layers base,syntax
```

`--layers` names the layers to ask the adapter for: `base`, `syntax`, `calls` and `build`, separated by
commas, each at most once, with no spaces, always including `base`. Without it all four are
asked for. Build contexts reported by an adapter whose manifest does not offer the calls layer
are not copied, and an entry the adapter wrote for a layer it does not offer is discarded
before the result is validated, so a malformed one does not fail the run by itself. A list that breaks
these rules exits with code 2 and says what is wrong with it, and no adapter runs. So does
`--layers` together with `--scip`: a snapshot made from an index has no adapter to ask.

The adapter is chosen first, by name, by path or by markers, exactly as above; the list plays
no part in that choice. The engine then passes the adapter `--layers` with the layers you
asked for that its manifest offers, always in the order `base`, `syntax`, `calls`, `build`. The
snapshot records each optional layer:

| You asked for it | The adapter offers it | State in the snapshot | Gap |
| --- | --- | --- | --- |
| no | either | `not-requested` | none |
| yes | no | `absent` | `missing-layer` |
| yes | yes | `present` or `failed` | as for any run |

`not-requested` is your choice, not a limit of the adapter, and a diff treats it so: a layer
that one side did not ask for is not compared (see [diff.md](diff.md#what-is-not-compared)).
Two snapshots made with different lists can also differ in rows the diff does compare: the
syntax layer can supply bodies, which decide which definition owns a reference, and accepted
call evidence can name files that the index does not list, such as generated code, which are
then added to `files.jsonl`. A run without the calls layer adds no such file, so its file rows
and counts can be smaller.

The engine reads only what it asked for: an entry, a build context or a file of a layer it did
not pass is dropped unread and can neither fail the run nor add a gap. What an adapter does
anyway is outside that. An adapter that ignores `--layers` still spends the time, can still
fail or crash the run, and the project code it runs, such as build scripts, can still change
what the requested layers see.

### Upgrading to adapter contract 0.3

An engine of contract 0.3 refuses an adapter of 0.2, and `gosanja adapter list` reports it as
unusable. The search order above decides which copy of a name counts: an older copy earlier in
`GOSANJA_ADAPTER_PATH`, the engine's directory or `PATH` hides a newer one later, and the
engine refuses the older one rather than look further. Remove or move the old copy, or name
the new executable with `--adapter <path>`. Of two adapters of different names whose markers
both match a project, an unusable one is set aside before markers are matched, so the usable
one runs; two usable ones are still refused as ambiguous.

### Upgrading to adapter contract 0.4

An engine of contract 0.4 refuses an adapter of 0.3, as above. An adapter of 0.4 may list in
its base entry's `merged` the symbols whose definitions at several sites declare one entity;
an adapter with nothing to list writes what it wrote under 0.3, with `0.4` as its contract.

### Upgrading to adapter contract 0.5

An engine of contract 0.5 requires `0.5` in the manifest and in `result.json`.
An adapter can offer the optional `build` layer to report targets and file membership in
`build.jsonl`. Syntax adapters can also write test evidence as separate test rows in
`syntax.jsonl`. The adapter contract specifies the row shapes and how each layer is read.

### Upgrading to adapter contract 0.6

A 0.6 engine requires `0.6` in the manifest and in `result.json`. In the syntax layer
an adapter can write tagged import and impl rows containing token ranges. An adapter
without those facts can keep writing the existing definition and test rows.

## Trying it without a real adapter

The repository has a fake adapter for its tests, a shell script that serves the checked-in
index of `fixtures/rust-basic`. From the repository root, on a Unix system:

```sh
mkdir -p /tmp/gosanja-adapters
cat > /tmp/gosanja-adapters/gosanja-adapter-demo <<EOF
#!/bin/sh
exec "$PWD/fixtures/fake-adapter/adapter.sh" --name demo --markers '["Cargo.toml"]' "\$@"
EOF
chmod +x /tmp/gosanja-adapters/gosanja-adapter-demo
export GOSANJA_ADAPTER_PATH=/tmp/gosanja-adapters

cargo run -q -p gosanja -- adapter list
cargo run -q -p gosanja -- index --root fixtures/rust-basic --out /tmp/gosanja-demo
cargo run -q -p gosanja -- validate /tmp/gosanja-demo --root fixtures/rust-basic
cargo run -q -p gosanja -- index --root fixtures/go-basic --out /tmp/gosanja-demo2
```

The first command lists `demo`. The second finds `Cargo.toml` in the fixture, runs `demo` and
writes a snapshot whose manifest names that adapter. The last one is refused with exit code
2, because the Go fixture has no `Cargo.toml` and no installed adapter knows `go.mod`; with
the Go adapter installed, the engine would choose it there:

```text
gosanja: no installed adapter has a marker file in fixtures/go-basic
  demo: markers Cargo.toml
name one with --adapter <name or path>, or index a SCIP file with --scip <file>
```

If you have also built the Rust adapter (`cargo build -p gosanja-adapter-rust`), it sits
beside the `gosanja` executable and recognises `Cargo.toml` as well. The list then shows
`demo` and `rust`, the second command is refused because two adapters match, and you choose
with `--adapter demo`.

## What is not there yet

- Discovery has been exercised on Unix only. The rule for `.exe` names on Windows is
  implemented and unit-tested, and nothing else was run there.
- One project root is indexed by one adapter. A root that several adapters recognise needs
  `--adapter`.
- The engine does not check the version of a required tool.

## Writing an adapter

The contract is [spec/adapter-contract.md](../spec/adapter-contract.md): two commands,
`describe` and `index`, and the files `index` writes. An adapter may be written in any
language. Name it `gosanja-adapter-<name>`, make `describe` print the same `name`, and list
your ecosystem's marker files under `markers` if the engine should choose it by itself.
`index` takes `--layers`, the layers it is asked for: produce only those, write a
`result.json` entry for each of them and none for another, and report build contexts only
when `calls` is among them.

When your language declares one thing at several sites without a body there, such as a Go
package in each of its files, list the symbol in the base entry's `merged` (global SCIP
symbols, sorted by bytes, each once): the engine then writes one entity with `decls` instead
of a collision group. Do not list a symbol whose definitions are distinct things, such as two
functions of one name; and do not send a syntax row at any of its sites, because a published
syntax row, or a body at a site, keeps the symbol a collision group. A malformed list fails the
run whatever the base layer's status.
