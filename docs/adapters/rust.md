# The Rust adapter

`gosanja-adapter-rust` gives the engine three layers for a Cargo workspace:

- the base layer, an index made by `rust-analyzer scip`;
- the syntax layer, read from the source with `syn`;
- the calls layer, read from the LLVM IR the compiler emits when the adapter builds the
  project's library and binary targets.

A call relation of the calls layer says this: in one build, the compiler emitted at a line of
the project's source a direct call to a function, and on that line the base layer holds a
reference, written as a call, to the symbol of the definition at the position the compiler
names. The relation gives the compiler's site and the two definitions: the one whose body
holds the call and the one the compiler named. It is not a mark on a reference, and it does
not say that the base layer resolved the call to that definition. Where two definitions share
a symbol, the reference shows only that the source calls something of that symbol there; the
compiler's position decides which of them is the callee.

Every call the syntax layer finds is also a `declared` call, as before: the callee name is
bound to that declaration, and nothing is claimed about what runs.

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

The calls layer compiles the project. Its build output, its IR files and its temporary files
go under the adapter's output directory, like the indexer's, and nothing is written under
the project: the layer is tested on a copy of a workspace whose every file and directory is
read-only.

Two things are outside these measures, as the adapter contract allows, and you should know
them:

- **Your build scripts and procedural macros run.** `rust-analyzer scip` builds and runs them
  to see the code they generate, as `cargo build` and your editor do, and the calls layer's
  build runs them again for the targets it compiles. A build script that writes into its own
  source directory will do so.
- **Your cargo configuration stays in force**, the project's and the one in your cargo home,
  apart from the two build directories. A
  compiler flag set there, for example an `--emit` path in `build.rustflags`, is applied as
  written.

## What it needs

`rust-analyzer` and `cargo` on `PATH`; `gosanja adapter list` shows whether they are found.
The calls layer also needs the compiler that `cargo` runs and a linker on `PATH`, since it
links the binaries it builds. The project must be a workspace root with a `Cargo.lock` that
matches its manifests.

The adapter was developed and tested with rust-analyzer 0.3.3049 and with rustc and cargo
1.99.1, on `x86_64-unknown-linux-gnu`. The snapshots checked in under `fixtures/` are those of
that toolchain. Other platforms and toolchains have not been verified for the calls layer.
The IR is read as text, so another LLVM version may write lines the reader does not know.
The reader then fails the layer rather than guess, and `result.json` names the compiler that
wrote each module.

Indexing now compiles every selected target once with debug information, so it needs a
linker and takes the time and disk of a debug build. On ryu 1.0.23, a crate of 180 kB of
source, a whole run of the adapter took 7 to 8 seconds on a 16-thread x86-64 host and left
92 MiB under the output directory, nearly all of it Cargo's build directory; the IR read was
0.75 MB. The calls build runs before the base layer's indexer, so a project whose index then
fails has been compiled for nothing.

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

## What the calls layer builds

Of each workspace member, the adapter builds:

- its library target, whatever its crate types, once;
- each binary whose `required-features` are all enabled by the package's `default`
  features, directly or through other features.

A binary that needs more is outside the layer, and so are tests, examples and benches. Build
scripts and procedural macros that a built target depends on are compiled and run as
prerequisites.

Each target is compiled by itself, in the order package, kind and name, with

```text
cargo rustc --locked --offline --message-format=json -p <package>@<version> (--lib | --bin <name>) --
    --emit=llvm-ir=<path> -C codegen-units=1 -C debuginfo=2 -C opt-level=0
```

in the same restricted environment as the adapter's other tools. A row is therefore what one
`dev` build shows: a build for which the adapter asked for optimisation level 0 and whose
module reports itself as not optimised. The project's and your Cargo configuration stay in
force, as for the base layer. A `[profile.dev]` optimisation level gives way to the adapter's
`-C opt-level=0`. Configured `rustflags` that raise the level after it give a module that
reports itself as optimised, and the layer then fails with `build-unsupported`. That flag of
the module is what the layer checks; it does not show that no optimisation pass ran.

The adapter does not choose the compiler. rustup's own rules choose it (an override in the
environment or for the directory, the project's toolchain file, the default toolchain), and
so does your Cargo configuration, which may name another compiler or a wrapper. The build
context of each target names the compiler that produced its module, as the module records
it.

### Build contexts

Every relation of the calls layer names the build it was observed in. The manifest lists one
build context per compiled target:

| Field | Value |
| --- | --- |
| `id` | `<package>-<kind>-<target>`, `kind` being `lib` or `bin`, every character outside `[A-Za-z0-9._-]` replaced by `_` |
| `toolchain` | `rustc` and the version the module's compile unit records, as in `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| `target` | the module's target triple |
| `profile` | `dev` |
| `features` | the features Cargo reports for the target, sorted |

The `id` holds no version, so two releases of a package that differ only in their version
have equal build contexts, and `gosanja diff` compares their calls. No path of the machine
enters a build context.

A build context does not capture configured flags or profile settings other than these, and
its `target` is the module's triple, which two different Rust targets can share:
`riscv32imc-unknown-none-elf` and `riscv32imac-unknown-none-elf` both give `riscv32`, and
differ in a `target_feature` that source can test. Equal build contexts therefore do not show
equal builds.

## From an instruction to a row

For each direct call instruction of a module, the adapter makes five checks in this order.
The first that fails ends the instruction, and the omission is recorded as the table under
"What the calls layer leaves out" says.

1. **A site in the project.** The instruction's debug location lies at a line above 0 of a
   file of the project, with the content the compiler read (see "Files and the state of the
   project").
2. **A callee in the project.** The module itself defines the called function, with debug
   information that places it in a file of the project. A function the module only declares
   gives no row (see "Calls into another target"). The callee's position is that file, the
   line the compiler records for the function and its name, a trailing generic argument list
   removed.
3. **The base layer's callee.** In that file, the base index holds exactly one definition of
   that name whose name token is on that line. A definition is counted once however often the
   index repeats it.
4. **The base layer's reference.** On the site's line, the index holds references to that
   definition's symbol that are written as calls: the token is followed, after optional white
   space and an optional `::<...>`, by `(`, or it is an operator. A reference that only names
   the function, as `let f = leaf;` or a path passed as an argument, does not count. Each
   counted reference has an owner: the innermost function or method definition of the index
   whose range holds it. The definitions whose ranges hold a reference must nest strictly, one
   inside the next; two with equal ranges, or two that overlap without one holding the other,
   leave the reference without an owner. All counted references must have the same owner,
   the **caller**, so references in a nested function and in the function around it give no
   row. The line must hold at least as many counted references as any one emitted function
   holds instructions at that line calling the callee; a generic function is emitted once per
   instantiation, each with its own copy of the call.
5. **Written items, and the caller's own position.** Both ends must be definitions written
   in the source, at positions nothing else can take:
   - neither end may be under an attribute macro or a derive (below);
   - no line its signature touches, from `fn` to its name, may be taken, or hold another
     `fn` signature or a macro invocation;
   - no other definition of the index may start at its name token;
   - the instruction's site may not be on a taken line, and the innermost `fn`, `const` or
     `static` item written around the site must be the caller;
   - the compiler must record the instruction in the caller itself, or in a closure, an
     `async` block, an `async` closure or the body of an `async fn` that the caller owns
     alone: written on a line that lies in the caller, that no other written `fn`, `const`
     or `static` touches unless it encloses the caller, and that is neither taken nor shared
     with a macro invocation.

   A line is **taken** when it holds any part of a `macro_rules!` definition, or of an item
   that carries an attribute outside the list below (a `derive` included), from its first
   attribute to its end. A macro can give the code it generates the position of a token of
   its input or of its own invocation, so a position on a taken line may be generated code.

A row then names the caller and the callee by the file, line and column of their name
tokens, with the column in characters, and records `mechanism: static`, `resolution: direct`,
the site and the build context. Equal rows are kept: one row per instruction.

### Attributes

A function or method is no end of a row when it, or an item around it (an `impl`, a `trait`,
a `mod`), carries an attribute other than the inert attributes the compiler owns and no
import can replace:

`cfg`, `doc`, `inline`, `cold`, `must_use`, `deprecated`, `allow`, `warn`, `deny`, `forbid`,
`expect`, `track_caller`, `no_mangle`, `export_name`, `link_section`, `target_feature`,
`repr`, `path`, `automatically_derived`, `ignore`, `should_panic`.

`unsafe(...)` counts as the attribute inside it. `cfg_attr(predicate, attributes)` counts as
its attributes whatever the predicate, because the adapter cannot establish the `cfg` of a
definition's own build: `#[cfg_attr(feature = "fast", inline)]` leaves a function an end, and
`#[cfg_attr(feature = "off", some_macro)]` does not, even with the feature off.

So functions and methods under `#[async_trait]`, `#[tokio::main]`, `#[instrument]`, `#[test]`
or `#[rustfmt::skip]` have no rows. `test`, `bench` and `derive` are not on the list, and
neither is any tool attribute written with a path, as `rustfmt::skip` or `clippy::...`: a
project can bring its own macro under each of those names. Code built around attribute
macros, as async services often are, gets correspondingly fewer rows.

### What a row does and does not show

- **The callee** is the definition at the position the compiler names. The base layer's
  reference on the site's line shows that the source calls something of that symbol there.
  When two definitions share the symbol, as a helper of one name in each of two binaries of a
  package does, the snapshot's `ref` points to the group of both, and the call relation to the
  one the compiler named.
- **The caller** is the definition whose body holds the reference and in which the compiler
  compiled the instruction. For a call written in a closure, an `async` block, an `async`
  closure or the body of an `async fn`, that is the function it is written in. The row then says that the call is
  in that function's code, not that calling the function runs it.
- **A call to a closure** has no row: a closure's body has no definition.
- **Inlining.** For an instruction the compiler inlined from `g` into `f`, the site lies in
  `g`'s source and the caller is `g`.
- **Generics.** `direct` holds for a generic callee at any instantiation; the layer never
  writes `mono-instantiated`.
- **An end is a source item.** A file compiled under two module names, or in two crates,
  gives its rows on the one item, also when the index holds it under the module or crate the
  build did not use. When the index itself holds two definitions at one position, no row is
  written: if they have one name, step 3 finds no single one; if their names differ, step 5
  refuses the position as an end, since another definition starts at its name token.
- **The base layer's reference decides.** No row is written for a call the base layer holds
  no reference for at its line: a call made by generated code, a call through a macro that
  supplies the parentheses, or a call written over two lines when the compiler's line is not
  the reference's. An overloaded operator is treated like any call and gives a row only where
  the index references its implementation at the operator. rust-analyzer 0.3.3049 does: for
  `a + b` it gives three one-character references, of which only the `+` counts.
- **`cfg(test)`.** The indexer and the build can differ, as rust-analyzer indexes with
  `cfg(test)` on and the build compiles without it. A row needs both records, so a
  difference costs rows and never adds one.
- **A procedural macro's entry function** has no rows: the index holds it as a macro, not as
  a function, so calls written in it have no caller.

Two things remain possible and are not detected:

- **An equal file in place of the indexed one.** A debug path remapped onto another file of
  the project with the same content and the same definitions at the same positions, whether
  of another release of a crate, of an alternative the indexer's `cfg`s did not select, or a
  second file read by `include!` under a module of the same name. The row then stands on the
  indexed file for a call the other file's code made; the two are the same text.
- **A redirected call to a function the line also calls.** A configured pass that merges
  functions can make an instruction call `leaf` where the source calls `twin`, on a line
  that also calls `leaf` in a call the compiler removed.

Both need an unusual configuration and a coincidence.

### Calls into another target

A target is compiled against the other targets of the workspace as Cargo built them for that
compilation. The adapter does not read those builds, and the IR of the other target's own
build may differ from them, for example by a `cfg`. A callee that the calling target's IR
only declares therefore gives no row, as most calls from a binary into its library do.

The rule is about what the calling target's IR defines, not about where the callee was
written. A function of a library that the calling target's IR defines itself gives rows like
any other: an instance of a generic function, or an `#[inline]` function when the compiler
emits a copy there. Of two calls to one generic function at two types, one may have a row
and the other not. Measured: a binary that calls a library's `generic` at `u32`, which the
library itself instantiates, and at `u64` only declares `generic::<u32>` in its IR and
defines `generic::<u64>`, so only the `u64` call can give a row.

A call whose callee is only declared marks the calling file `callee-unresolved` when the
callee's symbol is a `v0` symbol whose path begins in a crate with a workspace member's name:
the crate that begins the demangled path, or, for a method of an `impl`, the crate of its type
or of its trait. This is a test of a crate's name and not of where the item was written:
crate names inside generic arguments are not looked at, and a crate outside the project that
shares a member's name is reported too. It neither catches every item written in a member
nor only those.

### Not covered

Destructor calls, which have no reference in the source to be corroborated by; dynamic
dispatch; and calls to a function the calling target's IR only declares.

## Files and the state of the project

A debug location names its file by a directory and a file name. The adapter takes the file as
a file of the project only when:

- the two make an absolute path;
- that path resolves inside the project;
- the file's bytes give the MD5 the compiler recorded for it.

So a build whose debug paths name no file of the project, as one that remaps its source
directory to `.`, gives a layer with no rows and no `skipped` entry. An empty calls layer
without `skipped` entries therefore does not show that the compiler saw no calls.

The calls build runs first, then the indexer, then the adapter compares each file the IR
names with what the compiler recorded. The project must stay unchanged while it is indexed.
A file whose bytes at the end of indexing differ from what the compiler read gives no rows,
which catches an edit that lasts. It does not catch an edit made during indexing and undone
before the adapter reads the file.

Columns are converted from the compiler's UTF-8 bytes to the snapshot's characters against
the text of the line. A column that is 0 or falls inside a character gives a site without a
column, and step 5 then gives no row.

A file that the syntax module's parser cannot read, or that begins with a byte order mark,
gives no site and no end, and is marked `parse-failed`.

## When the calls layer fails

The calls layer is optional. When it fails and the base layer succeeded, the base and syntax
layers are published as usual, the snapshot records a `failed-layer` gap for calls, and no
row and no build context of the run is kept. When the base layer fails, the engine writes no
snapshot at all (see "When it cannot index"), and the calls layer's `base-failed` is seen only
in the adapter's own `result.json`.

The situations are checked in the order of this table. The first that holds decides the code,
for the first target in build order it holds for; a later situation is not looked at, even
for an earlier target. So an output path with a comma in a project whose two targets share a
build context id gives `build-unsupported`, while two such targets whose modules say they
were optimised give `build-context-ambiguous`. The comma, the metadata, an empty selection
and a shared id are decided before anything is compiled; a base layer that fails gives
`base-failed` even when the calls build has already run.

| Situation, in the order it is checked | Code |
| --- | --- |
| the base layer failed, so no row can be made | `base-failed` |
| the adapter's output directory has a comma in its path, which the compiler's `--emit` option cannot name | `build-unsupported` |
| Cargo's metadata lacks what the selection of targets needs | `build-failed` |
| no target is selected | none: the layer succeeds, empty |
| two selected targets give one build context id | `build-context-ambiguous` |
| a selected target does not compile | `build-failed` |
| Cargo reports more than one artifact for a target, as a `build.target` naming several platforms gives | `build-unsupported` |
| Cargo reports no artifact for a target, or no IR file is at the path the adapter gave | `ir-missing` |
| the reader cannot follow an IR file, or the file is not a module of rustc for one target | `ir-unreadable` |
| a module reports itself as optimised | `build-unsupported` |

**A comma in the output path.** For `gosanja index` the directory the adapter writes to is
the engine's cache directory, not the snapshot's destination; for an adapter run by hand it
is its `--out`. A comma there disables the calls layer.

**A dependency missing from the local cache** is met in one of two places:

- The base layer's locked metadata command comes first. Whether it may use the network is
  your Cargo configuration: it may fetch the dependency and go on, and if it cannot, the base
  layer fails with `locked-metadata-failed`, and with it the run.
- The calls build runs offline. When it then lacks a dependency, only the calls layer fails,
  with `build-failed`, and its message says that `cargo fetch`, which needs the network,
  restores registry and git dependencies, and that a path dependency has to be there.

A project with no target to build gives an empty calls layer without failure.

## What the calls layer leaves out

What the layer omits it names per file in `skipped`: one entry per file and code, so a file
can carry several codes, once each. A snapshot shows each entry as a `skipped-file` gap.

| Code | The file named is | When |
| --- | --- | --- |
| `indirect-call` | that of the call's site | a call there goes through a function pointer or a vtable |
| `no-debug-location` | that of the function holding the call | a direct call has no debug location, or one at line 0 |
| `callee-unresolved` | that of the site | the callee is only declared in the calling module and its symbol passes the test of names above |
| `parse-failed` | the unreadable file | a site or an end lies in a file the parser cannot read, or one beginning with a byte order mark |
| `uncorroborated` | that of the site | the callee is a function of the project and step 3, 4 or 5 does not hold |

An omission whose file is not a file of the project is not reported, and neither is an
indirect call without a site there. Calls to code outside the project are outside the layer
and are not `skipped`, except as `callee-unresolved` says.

`uncorroborated` marks a file in which the compiler made at least one call between
definitions of the project that the layer did not publish. It is a mark per file, not a
count. A call to a closure is among those calls. On ryu 1.0.23 the layer published 49 rows.
A scratch reader of the same rules, measured before the layer was finished, gave the same 49
of the 65 direct calls whose site is in the project and whose callee the calling module
defines there, and 1 736 of 1 969 on regex-syntax 0.8.12.

## What the calls layer trusts

The toolchain that rustup or your configuration selects, Cargo, and the project's build
scripts and procedural macros, which the base layer already runs. The layer is built to be
right for projects as people configure them; it does not claim to withstand a project
arranged to deceive it.


Measurements behind the base and syntax layers, and how often each case occurs on real
crates, are in [research/rust-adapter-measurements.md](../research/rust-adapter-measurements.md).
Those behind the calls layer are in
[research/rust-call-evidence-measurements.md](../research/rust-call-evidence-measurements.md).
