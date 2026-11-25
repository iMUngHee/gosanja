# The Go adapter

`gosanja-adapter-go` gives the engine two layers for a Go module:

- the base layer, an index made by `scip-go`;
- the syntax layer, read from the source with `go/packages` and `go/types`.

It has no calls layer. A snapshot made without `--layers`, or with `calls` among them,
records the calls layer `absent` with one `missing-layer` gap; one made with
`--layers base,syntax` records it `not-requested`, with no gap. A call of the syntax layer
becomes a `declared` call when the base layer holds a reference on its callee's name: the call
is bound to that declaration, and nothing is claimed about what runs. A call without such a
reference is an `open-call` gap.

## What it does to your project

The adapter takes these measures to keep itself and the tools it runs from writing in the
project, and each is tested:

- Its tools do not inherit your environment. They get a fixed list of variables: `PATH`, the
  proxy and certificate variables (`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` in
  either case, `SSL_CERT_FILE`, `SSL_CERT_DIR`), the eight Go settings below, and the
  adapter's own values. A `GOFLAGS`, `GOCACHE`, `TMPDIR`, `GOWORK`, `GOTOOLCHAIN` or
  `GOPACKAGESDRIVER` that you set, or that points into the project, has no effect, and a
  `gopackagesdriver` on `PATH` is never run.
- Your Go configuration is read once. The adapter's first Go command is
  `go env -json GOMODCACHE GOPATH GOPROXY GOPRIVATE GONOPROXY GONOSUMDB GOSUMDB GOINSECURE`,
  run in the adapter's output directory with your env file (the one `go env -w` writes, or
  your `GOENV`, a relative one taken from where you started the adapter) and with any of the
  eight you exported. Every later Go command gets those eight values and `GOENV=off`, so no
  env file is read again and nothing else of it is in force.
- Go's build cache, its temporary files, and the `HOME` and `XDG_CONFIG_HOME` its tools see
  go to `work/` in the adapter's output directory, and Go telemetry is off there: the
  adapter's Go processes write no counters and start no telemetry process, and your own
  telemetry settings and files are not touched. When `GOPATH` and
  `GOMODCACHE` are set nowhere, the tools still get Go's defaults for you, `$HOME/go` and
  `$HOME/go/pkg/mod`.
- It refuses to run when a directory the go command may write to is the project or lies
  inside it: any `GOPATH` entry, the module cache, its `cache/download` and `cache/vcs`, and
  the checksum database's `pkg/sumdb` under the first `GOPATH` entry. It decides this by file
  identity with links followed, so a case alias of the project on a case-insensitive volume
  counts as the project, and gives the tools exactly the paths it checked.
- Every go command runs with `-mod=readonly`, or `-mod=vendor` when `vendor/modules.txt` is a
  file, so `go.mod` and `go.sum` are never rewritten; a missing `go.sum` line fails the run
  instead.
- Every go command runs with `-buildvcs=false`, so none runs `git status`, which rewrites a
  stale `.git/index`.
- It never selects or downloads a toolchain (`GOTOOLCHAIN=local`), never reads a `go.work`
  (`GOWORK=off`), and builds without cgo (`CGO_ENABLED=0`).

Both layers are tested on a copy of a module whose every file and directory is read-only.

These stay in force, outside the measures, and you should know them:

- **The tools compile your packages.** scip-go and `go/packages` parse and type-check them.
  No code of the project runs: Go has no build scripts, cgo is off, and `go generate` is never
  run.
- **A relative `replace` to a directory outside the project is read**, as the go command reads
  it.
- **scip-go asks git about the project**, read-only: `git rev-parse --show-toplevel` and other
  queries, run in the project.
- **A link you placed inside the module cache**, pointing into the project, is followed. The
  adapter checks the cache's directories, not every path beneath them.
- **Your `GOPROXY`, `GOPRIVATE` and `GONOPROXY` are used.** A dependency missing from the
  module cache is downloaded into the cache, which lies outside the project. `GOPROXY=off`
  alone does not keep a run offline: the go command fetches a module that `GONOPROXY` (by
  default `GOPRIVATE`) matches straight from its version control, whatever `GOPROXY` says. For
  an offline run, set `GOPROXY=off` and leave `GOPRIVATE` and `GONOPROXY` empty, in your
  environment and in your env file; a missing dependency then fails the run with
  `dependencies-missing`.

## What it needs

`go` and `scip-go` on `PATH`; `gosanja adapter list` shows whether they are found. The Go
toolchain must be at least the version the module's `go` line names, since the adapter never
fetches another. The root must hold the module's `go.mod`; a `go.work` workspace is not
supported.

The adapter runs on Linux and macOS; on Windows it fails at once with
`platform-unsupported`, because Go there takes its configuration and default `GOPATH` from
variables the adapter does not pass. It was developed and tested with Go 1.27.3 and scip-go
0.2.10 on `linux/amd64`; macOS has not been run. The snapshot checked in under
`fixtures/go-flow/expected` is that of this toolchain.

## Using it

Build the adapter, put it where the engine finds adapters (see [adapters.md](../adapters.md)),
then:

```sh
gosanja index --root <module> --out <dir>
```

The engine chooses the adapter because the root has a `go.mod`. From a checkout of this
repository, with Go and scip-go installed:

```sh
cargo build -p gosanja
(cd adapters/go && go build -o ../../target/go-adapter/gosanja-adapter-go ./cmd/gosanja-adapter-go)
export GOSANJA_ADAPTER_PATH=$PWD/target/go-adapter
target/debug/gosanja adapter list
out=$(mktemp -d)
target/debug/gosanja index --root fixtures/go-flow --out "$out/go-flow"
target/debug/gosanja validate "$out/go-flow" --root fixtures/go-flow
```

With `--layers base` the adapter loads no packages for the syntax layer and writes no
`syntax.jsonl`; scip-go runs once either way. The base layer's status does not depend on the
syntax layer: a syntax layer that fails leaves the base layer and its snapshot in place.

## The build context

Both layers see the module in one build context: the host's `GOOS` and `GOARCH`, cgo
disabled, and the tests included, internal and external. A Go file of the module that this
context leaves out is reported under the base layer as a `skipped-file` gap with the code
`build-constraint`: a file whose `//go:build` line excludes it, a cgo file, and the files of
a directory where every file is excluded. A cgo file is left out rather than indexed, because
with cgo enabled scip-go indexes cgo's generated copies under the build cache, and the
snapshot would lose the file's own definitions.

Files the go command never considers part of the module are not reported: those under
`testdata` and `vendor`, in a directory whose name starts with `.` or `_`, in a nested module,
or in a directory that a `go.mod` `ignore` directive names.

## When it cannot index

Once the adapter runs, a problem with the module is reported as a failed base layer with a
code, and the engine exits 3 and names the code. What a failing tool printed on its standard
error is passed on. (When you name no adapter and the root has no `go.mod`, the adapter is
never chosen: the engine exits 2 and says that no installed adapter has a marker file there.)
The checks run in the order of the table, and the first that fails decides the code.

| Code | Meaning |
| --- | --- |
| `platform-unsupported` | the adapter runs on Windows |
| `manifest-missing` | no `go.mod` in the root (seen only when the adapter was named with `--adapter`) |
| `toolchain-missing` | the first `go env`, `scip-go --version` or `go version` could not be started, failed, or printed something else |
| `tool-home-inside-root` | a `GOPATH` entry, the module cache or one of the directories above is the project or lies in it, or its path holds a `.` or `..` in a part that does not exist yet and so cannot be placed |
| `tool-home-unresolvable` | such a directory could not be resolved, for example through a regular file |
| `toolchain-too-old` | the module needs a newer Go than the one on `PATH` |
| `vendor-inconsistent` | `vendor/modules.txt` does not match `go.mod` |
| `manifest-invalid` | the go command rejects `go.mod` |
| `replace-absolute` | `go.mod` replaces a module with an absolute directory, which scip-go would put into every key of that module as a machine path |
| `dependencies-unreadable` | the go command's package listing failed or could not be read |
| `dependencies-missing` | a module the packages import, also one imported only by a test, is missing from the module cache or from `go.sum` |
| `indexer-failed` | `scip-go index` failed or wrote no index |
| `index-unreadable` | the index could not be decoded |
| `index-incomplete` | the index lists no file, lacks a Go file of the module that the build context builds and that the adapter could read and parse, or the package listing missed such a file |

A failure in a package of the module itself, such as a syntax or type error, does not fail
the base layer; see below for how the files are reported. Run directly, the adapter also
refuses an invocation that is itself wrong (a root that is not a directory, an output
directory that is missing, not empty, or inside the root): it exits 2 and writes nothing.

## What the base layer says

The adapter rewrites scip-go's index only to drop a document for a file outside the package
listing's Go files, such as one the build context excludes; the records it keeps are written
back byte for byte, in their order. So the relations reach the snapshot as scip-go states
them. Three of them read differently from what a Go programmer may expect:

- **`implements` has two forms.** A row from a named type to an interface, and one from a
  method to the interface's method. `T implements I` is written also when only `*T` has the
  methods: in `fixtures/go-flow`, `Gauge implements Loader` although `Load` has a pointer
  receiver. Read it as "`T` or `*T` implements `I`".
- **The names of one declaration share an enclosing range.** scip-go gives `var A, B = f(), g()`
  one extent for both names, so the engine reports an `overlapping-bodies` gap for each such
  pair and leaves both extents out of reference ownership: a reference inside the initializers
  belongs to neither name, and is counted with the file's `unattributed-references`.
- **A package is one entity, declared in each of its files.** scip-go defines the package's
  symbol at the `package` clause of every file. The adapter lists, in `result.json`'s `merged`,
  a namespace symbol that the index it publishes defines in two or more files only when it has
  verified every retained definition of that symbol as the package clause of a file it read,
  and all those clauses name the same package. The engine then writes the package as one
  entity with a `decls` entry per file and no `def`, instead of a collision group. One
  definition it cannot verify (one away from the package clause, one in a document whose
  positions are not UTF-8 byte offsets, one in a file whose clause it could not read) or a
  clause naming another package keeps the symbol out of the list, and it stays a collision
  group. In `fixtures/go-flow`, `example.com/goflow` has `decls` for flow.go, flow_test.go,
  tagged.go and zz_generated.go; the one-file `meter` package keeps its `def`. Two `init` functions of one package are distinct functions, are not
  listed, and stay a `symbol-collision` group.

References that lie in no definition's extent, such as imports, are counted in
`unattributed-references` gaps. Local variables are `local` symbols, which the engine drops,
so a call of a local function value has no reference to join and is an `open-call` gap.

## What the syntax layer reports

One row for each function, method and interface method, each type (named or alias, generic
included), each name of a package-level `const` and `var`, and the package clause of a file
that carries a `//go:build` line when its package has that one file; the line is that row's
attribute. A package declared in several files has no package-clause row, since a syntax row at
one of its sites would keep it a collision group (`spec/adapter-contract.md`, `merged`), so the
`//go:build` lines of its files are not recorded. The adapter decides this from its own count
of the files it read; should the engine still drop one of two sites, for a reason the adapter
does not check, such as a range it rejects, the package keeps `def` and has no row for its
tag. Its attributes are the `//go:`
directive lines of the declaration's doc comment, as written. A row has no `visibility`, since
Go writes none (exportedness is the case of the first letter), no `qualifiers`, and no `body`:
the base layer gives the extents. Nothing named `_` has a row, since scip-go never defines it,
and neither do fields or declarations inside a function body.

Every function and method with a body has an outline:

| Go | Node |
| --- | --- |
| `if`, with an `else` | `branch` `if`, its `else` part an `arm` `else`; an `else if` is a `branch` inside that arm |
| `switch`, type switch, `select` | `branch` `switch`, `type-switch` or `select`, each `case` an `arm` |
| `for`, `for … range` | `loop` `for` or `range` |
| `return`, `break`, `continue`, `goto`, `fallthrough` | `exit`, the keyword as detail |
| `go`, `defer` | `spawn` `go` or `defer` |
| a function literal | `closure` |
| a call of a function, method or function value named by an identifier or a selector, also with type arguments (`f(x)`, `x.m()`, `pkg.F[T](x)`) | `call`, its callee the name |
| a call of any other expression (`make2()(1)`, `fs[0]()`), or one whose callee did not type-check | `call` without a callee, an `open-call` gap |
| a conversion (`int(c)`) or a call of a builtin (`len`, `append`) | no node |

A conversion and a builtin call are written like calls in Go, but neither calls a declaration
and scip-go records no occurrence on either, so a `call` node would leave an `open-call` gap
that no index could close. The calls inside their arguments are still reported; see the worked
example "A conversion in Go" in [the adapter contract](../../spec/adapter-contract.md). Go has
no macros and no `await`, so `opaque` and `await` nodes do not occur. A function literal at
package level, such as the initializer of a `var`, has no outline.

Positions are physical: a `//line` directive moves no anchor or range, and columns count
UTF-8 bytes.

## What it cannot see

Each of these is a `skipped-file` gap with the code named, and the other files are unaffected:

- **A file that cannot be read, is not UTF-8, or does not parse** (`unreadable`,
  `invalid-utf8`, `parse-failed`) has no syntax row, and the code is reported under both
  layers.
- **A file with a type error** (`type-error`) keeps its rows. A call whose callee did not
  type-check has no callee.
- **A package diagnostic the adapter cannot place** (`package-error`), such as a `//go:embed`
  error in a package where a `//line` directive makes the reported position unreliable: every
  file of that package has no row, the files the package shares with its test variant
  included. A
  diagnostic at a position in one of the package's files, such as a `//go:embed` pattern that
  matches nothing, costs only that file the gap, and it keeps its rows.
- **An outline that would mislead** is withheld: one whose nodes fall outside the extent the
  index gives the definition (`outline-outside-definition`), and one nested more than 32
  levels deep (`outline-too-deep`).

When `go/packages` fails as a whole or loads no package, the syntax layer fails with
`load-failed`, which the snapshot records as a `failed-layer` gap; the base layer stands.
