# Go adapter measurements

Local observations on 2026-11-24 and 2026-11-25, Linux x86_64. The probes used
go1.27.3 (`go version go1.27.3-X:nodwarf5 linux/amd64`) and scip-go 0.2.10. scip-go was built
from the release's module in the local module cache (`github.com/scip-code/scip-go@v0.2.10`,
https://github.com/scip-code/scip-go), because `go install …@v0.2.10` fails when
`GOPROXY=off` ("loading deprecation … module lookup disabled"). Call graphs used `cmd/callgraph`
of `golang.org/x/tools` v0.53.0, the version scip-go requires, built inside scip-go's module.
The snapshots were made by gosanja at snapshot schema 0.3. Nothing was downloaded: every Go
command ran with `GOPROXY=off GOTOOLCHAIN=local GOWORK=off` against the local module cache.

The probes were synthetic modules in a scratch directory W outside the repository:

- **`p1`** (`example.com/probe`, go 1.26, with a sub-package `inner`) holds:
  - exported and unexported declarations, an interface, an `iota` const group, two `init`
    functions, a generic type `Box[T]` with a method, and an embedded field;
  - `defer`, `go`, `select`, a type switch, labelled `continue` and `break`, `goto`, closures and
    a call through a returned func;
  - the non-ASCII identifiers `π` and `名前` and an emoji before a reference, and `//go:noinline`;
  - files with `//go:build linux`, `//go:build windows` and `//go:build ignore`, a cgo file
    (`import "C"`) and a test file.
- **`p2`** (`example.com/p2`, a `main` package and package `shapes`) holds:
  - interfaces `Shape` and `Named`, the latter embedding the former. `Square` implements both by
    value and `Circle` implements `Shape` by pointer;
  - a struct embedding `Base` and `*Circle`, with fields `X, Y int` on one line;
  - generics: a constraint `Number`, `Max[T Number]`, `Total[S Sizer]` and `Stack[T]` with
    methods, called at explicit and inferred instantiations;
  - calls through an interface, direct calls, a conversion, `append` and `len`;
  - a file with the `// Code generated … DO NOT EDIT.` header and a file with `//go:build probe`.
- **`p3`** holds `var A, B = 1, 2`, an `iota` const group and a struct with `X, Y int`.
- **`m1`** (`example.com/rev`) holds an internal test file, an external test file
  (`package rev_test`), a file with a `//line` directive, `var _ = helper()` and
  `var X, _ = 1, 2`, a sub-package, and a directory whose only file is `//go:build ignore`.
- **`m2`** (`example.com/repl`) requires `example.com/dep` through an absolute `replace`
  directive pointing at a sibling directory.
- **Error and selection probes.** Further small modules, each named where it is used: `kinds`
  (one file per failure kind), `blank` (`_` as every kind of name), `m3` (a broken package
  imported inside the module, a cgo-only directory and an `ignore` directive), `m4` (an import
  from an ignored directory, a missing embed target and a dangling link), and `m5` and `m6`
  (compiler diagnostics).
- **The `case-*` modules** are copies of `p1` broken in one way each: a `go 1.99` line, a
  `toolchain go1.99.1` line, an import of an absent module, a file with a syntax error, a file
  with a type error, and no `go.mod`. A module with `require golang.org/x/mod v0.43.0` and no
  `go.sum` was also probed.

Every scip-go run had this environment and these flags, unless a section says otherwise. The
runs on `m1` and `m2` also set `CGO_ENABLED=0`. Paths under the scratch directory are written
below as `$W`, also inside quoted output:

```sh
env -i PATH=/usr/bin:/bin HOME=$W/sbx/user XDG_CONFIG_HOME=$W/sbx/user/.config \
    GOCACHE=$W/sbx/cache TMPDIR=$W/sbx/tmp GOTMPDIR=$W/sbx/tmp GOPATH=$W/sbx/gopath \
    GOMODCACHE="$(go env GOMODCACHE)" GOPROXY=off GOTOOLCHAIN=local GOWORK=off GOFLAGS=-mod=readonly \
    scip-go index --module-version v1.0.0 --repository-remote example.com/probe --output "$OUT"
```

Indexes were read with a scratch decoder of about 70 lines over the SCIP Go bindings v0.10.0
(`github.com/scip-code/scip/bindings/go/scip`, https://github.com/scip-code/scip). It prints
every document symbol with its relationships, and every occurrence with its typed range and
enclosing range. Lines in decoded output are zero-based; lines in compiler and `go/types` output
are one-based.

## Module root, empty indexes and git-derived flags

scip-go indexes the `./...` packages of its working directory, which must hold a `go.mod`. A
directory without one gives an index with no document and exit status 0:

```text
case-no-gomod      exit 0  documents 0      WARN msg="No go.mod file found in current directory."
```

Without `--module-version` and `--repository-remote`, scip-go takes both from git. In a copy of
`p1` made into a git repository with one commit and the remote `https://example.invalid/probe.git`:

```text
$ scip-go index --help | grep -E -- '--(module-version|repository-remote)'
 --repository-remote="example.invalid/probe"
 --module-version="02c449927597"
$ git rev-parse --short=12 HEAD
02c449927597
```

Symbols carry the module version, as in `scip-go gomod example.com/p2 v1.0.0 …`. Without fixed
values, keys depend on git state. Standard-library symbols carry the `go` line of `go.mod`
(`scip-go gomod github.com/golang/go/src go1.26 fmt/`).

The repository remote does not reach the decoded index. scip-go takes the module path from a
readable `go.mod` (`internal/modules/modules.go` of the release). Indexing `p2` twice, with
`--repository-remote=example.invalid/one` and `=example.invalid/two`, gave decoded outputs that
differ in no line:

```text
decoded lines differing: 0
```

The value does appear once in the raw index bytes, outside the documents and symbols the decoder
prints.

## Writes, the build cache and the module

Linux inotify watched `p1` and the sandbox during one cold scip-go run (create, modify, delete
and move events, recursively). Counts per directory:

```text
scip-go exit 0
src/p1           0
sbx/cache        5212
sbx/tmp          16678
sbx/user         16
sbx/out.scip     9
GOCACHE after the run: 67 MB
```

scip-go loads packages with `golang.org/x/tools/go/packages`, which compiles. The events under
the home directory are Go's telemetry counters. Their names show which tools ran:

```text
telemetry/local/asm@   telemetry/local/cgo@   telemetry/local/compile@
telemetry/local/go@    telemetry/local/link@
```

The files under `TMPDIR` were the `go-build…` work directory, removed before exit. A run that
redirects `GOCACHE`, `TMPDIR`, `GOTMPDIR`, `HOME` and `XDG_CONFIG_HOME` therefore wrote nothing
in the module. Inotify reports visible file system events, not every system call, and writes
outside the watched directories were not measured.

`-mod=mod` writes into the module. In the module with a `require` and no `go.sum`, the dependency
being present in the module cache:

```text
-mod=mod exit 0 files before: go.mod main.go after: go.mod go.sum main.go
-mod=readonly exit 0 files before: go.mod main.go after: go.mod main.go
```

A user's `GOFLAGS`, or a `go env -w GOFLAGS=…` in `$XDG_CONFIG_HOME/go/env`, can set
`-mod=mod`. In the earlier run of 2026-11-24, a `go.work` in a parent directory was picked up
unless `GOWORK=off` was set: in a module `b` under a workspace root, `go list -m` listed the
workspace's module `a` as well.

## The toolchain and the package driver

`go env` selects a toolchain before it answers. In a copy of `case-toolchain`, whose `go.mod`
names `toolchain go1.99.1`, `go env -json GOPROXY GOMODCACHE` ran with `GOPROXY` pointing at a
local HTTP listener that records every request:

```text
go env in the toolchain case, GOTOOLCHAIN=auto: exit 1 requests 1 go: downloading go1.99.1 (linux/amd64)
go: download go1.99.1 for linux/amd64: toolchain not available
go env in the toolchain case, GOTOOLCHAIN=local: exit 0 requests 0
go env in a directory outside the module, GOTOOLCHAIN=auto: exit 0 requests 0
```

A program that reads the user's Go settings with `go env` inside a module can therefore start a
toolchain download. Under `GOTOOLCHAIN=local`, or outside the module, it does not.

`golang.org/x/tools/go/packages` v0.53.0 runs an external driver instead of `go list` when
`GOPACKAGESDRIVER` names one. When the variable is unset, it runs the first `gopackagesdriver`
on `PATH`, and `GOPACKAGESDRIVER=off` disables the lookup (`go/packages/external.go`). With a
`gopackagesdriver` script first on `PATH` that records its arguments and exits 1:

```text
GOPACKAGESDRIVER unset: scip-go exit 1 driver log: driver called: ./...
error: $W/rev/fakebin/gopackagesdriver: exit status 1:
GOPACKAGESDRIVER=off: scip-go exit 0 driver log:
```

A program found on `PATH` can thus decide which files scip-go loads.

## Exit status on broken modules

scip-go exits 0 on most problems a module can have:

```text
p1                 exit 0  documents 7
case-newer-go      exit 1  documents no index  go: go.mod requires go >= 1.99 (running go 1.27.3; GOTOOLCHAIN=local)
case-toolchain     exit 0  documents 7
case-missing-req   exit 0  documents 5  WARN msg="Could not find node"
case-syntax-error  exit 0  documents 8
case-type-error    exit 0  documents 8
case-no-gomod      exit 0  documents 0
```

`p1` has 7 documents: 4 project files and 3 generated cgo files (next section). In
`case-missing-req` the absent import also dropped two documents, and no error was printed.
`go list -e -deps -json .` reports it:

```text
"Err": "cannot find module providing package example.org/absent/pkg: import lookup disabled by -mod=readonly"
exit 0
```

and for the newer `go` line:

```text
go: go.mod requires go >= 1.99 (running go 1.27.3; GOTOOLCHAIN=local)
exit 1
```

`go list` reads files only up to their imports, so it reports neither the syntax error nor the
type error. A broken file stays in `GoFiles`, and `InvalidGoFiles` stays empty. For the syntax
error case, and for a copy of `m1` with a file holding the bytes `\xff\xfe` in a string
literal, `go list -e -json .` under `CGO_ENABLED=0` gave:

```text
{'GoFiles': ['a.go', 'bad.go', 'lined.go'], 'InvalidGoFiles': None, 'IgnoredGoFiles': None} None True
{'GoFiles': ['broken.go', 'linux_only.go', 'main.go'], 'InvalidGoFiles': None, 'IgnoredGoFiles': ['cgo.go', 'never.go', 'windows_only.go']} None True
```

The last two values are the package's `Error` and its `Module.Main`. With cgo disabled, the cgo
file is listed among `IgnoredGoFiles`. scip-go indexes the file with invalid UTF-8, exits 0, and
the compiler rejects it:

```text
scip-go exit 0 documents documents 6 bad.go indexed: 1
./bad.go:3:12: invalid UTF-8 encoding
```

A dependency imported only by a test is missed by `go list -e -deps` without `-test`. In a copy
of `m1` whose external test also imports `example.org/absent/testonly`:

```text
scip-go exit 0 documents documents 5
go list -e  -deps: lines naming the absent module: 1
go list -e -test -deps: lines naming the absent module: 11
```

scip-go printed no warning. The one line without `-test` is the import list of the external test,
not an error. In the earlier run, a vendored module indexed under `-mod=readonly` with an empty
module cache attributed the unresolved import to the standard library, as
``gomod:github.com/golang/go/src:`golang.org/x/mod/semver`/``, and exited 0. Under
`-mod=vendor` it gave the correct external `golang.org/x/mod`.

## Tests and package variants

scip-go loads the module's own tests (`Tests: !opts.SkipTests` in `internal/loader/loader.go`).
When a package and its test variant are both loaded, it keeps the one with more syntax files,
since the test variant holds the package's files too. Documents of `m1`:

```text
doc a.go
doc a_external_test.go
doc a_internal_test.go
doc lined.go
doc sub/sub.go
```

The external test package is defined at its own `package` clause under its own symbol,
`` `example.com/rev_test`/ ``. `go list -e -json ./...` already names every test file of a
package. With `-test` it adds the variants:

```text
go list -e  -json ./... :
   example.com/rev {'GoFiles': ['a.go', 'lined.go'], 'TestGoFiles': ['a_internal_test.go'], 'XTestGoFiles': ['a_external_test.go']}
   example.com/rev/sub {'GoFiles': ['sub.go']}
go list -e -test -json ./... :
   example.com/rev {'GoFiles': ['a.go', 'lined.go'], 'TestGoFiles': ['a_internal_test.go'], 'XTestGoFiles': ['a_external_test.go']}
   example.com/rev/sub {'GoFiles': ['sub.go']}
   example.com/rev.test {'GoFiles': ['$W/sbx/cache/f9/f9e34cc5…-d']}   (cache file name shortened here)
   example.com/rev [example.com/rev.test] {'GoFiles': ['a.go', 'lined.go', 'a_internal_test.go'], 'TestGoFiles': ['a_internal_test.go'], 'XTestGoFiles': ['a_external_test.go']}
   example.com/rev_test [example.com/rev.test] {'GoFiles': ['a_external_test.go']}
```

The generated `.test` main package has its one file in the build cache. The directory
`excluded/`, whose only file is `//go:build ignore`, appears in neither listing.

## Repeatability

Five runs of scip-go on `p1`, each in a fresh sandbox and snapshotted by the engine:

```text
run 1 index 4a911c777b7f snapshot 8dd853eb6bc2
run 2 index 9dc2ce693507 snapshot 8dd853eb6bc2
run 3 index 3bb9de0df4b3 snapshot 8dd853eb6bc2
run 4 index da7a529bc63c snapshot 8dd853eb6bc2
run 5 index 4bfa1d10e528 snapshot 8dd853eb6bc2
```

Index digests are SHA-256 of `index.scip`; snapshot digests are of the concatenated snapshot
files. The index bytes differ on every run, because the order of `Document.symbols` changes, as
`fixtures/README.md` records for `go-basic`; the snapshot does not. Tests and fixtures of Go
output therefore have to compare snapshots.

## Positions, ranges and enclosing ranges

Every document declares `UTF8CodeUnitOffsetFromLineStart`:

```text
doc main.go encoding=UTF8CodeUnitOffsetFromLineStart symbols=2 occurrences=27
doc shapes/shapes.go encoding=UTF8CodeUnitOffsetFromLineStart symbols=57 occurrences=124
doc shapes/zz_generated.go encoding=UTF8CodeUnitOffsetFromLineStart symbols=1 occurrences=3
external symbols 0
```

Ranges are typed: `single_line_range` for names, and `single_line_range` or `multi_line_range`
for enclosing ranges. The legacy `range` field is not filled. A first decoder that read only
`range` failed with "index out of range [0] with length 0" on the first occurrence it sorted.

Definitions in `p2/shapes/shapes.go`, as `range` then `enclosing`. `S:` stands for
``scip-go gomod example.com/p2 v1.0.0 `example.com/p2/shapes`/``:

```text
occ roles=1 range=[3 5 10] enclosing=[2 0 5 1] S:Shape#
occ roles=1 range=[12 5 11] enclosing=[12 0 30] S:Square#
occ roles=1 range=[12 20 24] enclosing=[12 20 28] S:Square#Side.
occ roles=1 range=[14 16 20] enclosing=[14 0 53] S:Square#Area().
occ roles=1 range=[28 1 5] enclosing=[28 1 5] S:Labeled#Base.
occ roles=8 range=[28 1 5] enclosing=[] S:Base#
occ roles=1 range=[30 1 2] enclosing=[30 1 9] S:Labeled#X.
occ roles=1 range=[30 4 5] enclosing=[30 1 9] S:Labeled#Y.
occ roles=1 range=[44 5 8] enclosing=[44 0 49 1] S:Max().
occ roles=1 range=[53 19 23] enclosing=[53 0 61] S:Stack#Push().
```

- Functions, methods, types, fields and consts have an enclosing range; a package has none
  (`occ roles=1 range=[0 8 10] enclosing=[]` in `p3`).
- A type's enclosing range starts at its doc comment: line 2 of `Shape` is
  `// Shape is implemented …`.
- An embedded field is a definition of the field and a reference to the embedded type, on the
  same range.
- References carry the role `ReadAccess` (8).

The names of one declaration with several identifiers share one enclosing range. In `p3`:

```text
occ roles=1 range=[2 4 5] enclosing=[2 0 15] A.
occ roles=1 range=[2 7 8] enclosing=[2 0 15] B.
occ roles=1 range=[5 1 3] enclosing=[5 1 10] C1.
occ roles=1 range=[6 1 3] enclosing=[6 1 3] C2.
occ roles=1 range=[10 1 2] enclosing=[10 1 9] Pair#X.
occ roles=1 range=[10 4 5] enclosing=[10 1 9] Pair#Y.
```

The engine reports both pairs, since two bodies with identical extents are left out of
reference ownership (`spec/snapshot.md`, "ref"):

```text
overlapping-bodies ['A.', 'B.']
overlapping-bodies ['Pair#X.', 'Pair#Y.']
```

Blank identifiers are not defined. In `m1`, `var _ = helper()` gives no definition, and
`var X, _ = 1, 2` gives one, for `X`:

```text
occ roles=1 range=[4 4 5] enclosing=[4 0 15] `example.com/rev`/X.
occ roles=1 range=[6 5 11] enclosing=[6 0 30] `example.com/rev`/helper().
occ roles=1 range=[8 5 6] enclosing=[8 0 32] `example.com/rev`/A().
```

Positions are physical: a `//line` directive does not move them. `m1/lined.go` is:

```go
package rev

//line grammar.y:10
func Lined() int { return helper() }
```

scip-go defines `Lined` on its physical line:

```text
occ roles=1 range=[3 5 10] enclosing=[3 0 36] `example.com/rev`/Lined().
```

`go/token` applies the directive in `FileSet.Position` and not in `FileSet.PositionFor(pos,
false)`. A scratch probe printed both for the function's name:

```text
Lined Position grammar.y:10:0 PositionFor(false) lined.go:4:6
```

Unverified: a reference in the initialiser of such a declaration, as in `var A, B = f(), g()`,
would then belong to no body and be counted in `unattributed-references`, since the package has
no enclosing range. This was not run.

## Packages, imports and the snapshot

The snapshot of `p1` passes `gosanja validate --root`:

```text
validate --root exit 0
counts {'files': 4, 'entities': 27, 'externals': 4, 'relations': 24, 'syntax': 0, 'gaps': 5, 'dropped_local_references': 25}
gaps {'document-outside-root': 1, 'symbol-collision': 2, 'unattributed-references': 2}
  document-outside-root - 3
  collision group of 3 members: `example.com/probe`/
  collision group of 2 members: `example.com/probe`/init().
  unattributed-references main.go 2
  unattributed-references main_test.go 1
withC in entities: False
```

- scip-go defines a package at every file's `package` clause, so a package of several files is a
  collision group, one member per file.
- The two `init` functions form a collision group of their own.
- Imports lie in no body, because the package has no enclosing range, so they are
  `unattributed-references`.
- In the earlier run, on scip-go's own tree, all 10 collision groups were packages, with up to
  7 members.

## Build constraints and cgo

Build constraints follow the build context. A directory whose files are all excluded is matched
by no package of `go list ./...` (above), so it is reported nowhere unless a reader walks the
directories itself. With `GOOS=linux`:

- `linux_only.go` was indexed;
- `windows_only.go` and the `//go:build ignore` file were not;
- in `p2`, `tagged.go` (`//go:build probe`) is not a document.

`go list` names the excluded files under `IgnoredGoFiles` and the cgo file under `CgoFiles`. scip-go
indexes cgo's generated copies of the cgo file, which lie in the build cache, outside the root.
The engine counts them as 3 `document-outside-root`, and `withC`, the cgo file's function, is
not an entity (above).

## References across generics, embedding and interfaces

| Source (one-based line in `shapes.go`) | Occurrence | Symbol |
| --- | --- | --- |
| `Max(1, 2) + Max[int](3, …)` (75) | `[74 8 11]`, `[74 20 23]` | `Max().`, the generic definition |
| `s.Push("a")` on `Stack[string]` (74) | `[73 3 7]` | `Stack#Push().` |
| `l.ID` (35) | `[34 7 9]` | `Base#ID.`, with no reference to the embedded field |
| `l.Area()`, promoted from `*Circle` (36) | `[35 7 11]` | `Circle#Area().` |
| `sh.Area()` on an interface value (78) | `[77 40 44]` | `Shape#Area.`, the method specification |
| `x.Size()` on a type parameter `S Sizer` (63) | `[62 9 13]` | `Sizer#Size.` |

The probe's occurrences above are verbatim from the decoder. An instantiation has no symbol of
its own, and a promoted selector names the declaration it reaches. On the lines that hold
`append` (zero-based 53), `len` (55) and the conversion `int(c)` (69), every non-local occurrence
is one of `Stack#`, `Stack#items.`, `Stack#Push().`, `Stack#Len().`, `Cell#` or `Cell#Size().`.
scip-go writes no occurrence on `append`, `len` or `int`.

## Implementation relationships

scip-go writes `is_implementation` relationships on document symbols. The rust-analyzer indexes
measured so far had none (`rust-call-evidence-measurements.md`). In `p2`:

```text
sym S:Cell# kind=Type            rel impl=true -> S:Sizer#
sym S:Cell#Size(). kind=Method   rel impl=true -> S:Sizer#Size.
sym S:Circle# kind=Struct        rel impl=true -> S:Shape#
sym S:Circle#Area(). kind=Method rel impl=true -> S:Shape#Area.
sym S:Labeled# kind=Struct       rel impl=true -> S:Shape#
sym S:Square# kind=Struct        rel impl=true -> S:Named#
                                 rel impl=true -> S:Shape#
sym S:Square#Area(). kind=Method rel impl=true -> S:Shape#Area.
sym S:Square#Name(). kind=Method rel impl=true -> S:Named#Name.
```

The engine turns them into `implements` rows:

```text
relations {('ref', 'core:reference-in-body'): 48, ('implements', 'scip:scip-go'): 9}
gaps {'overlapping-bodies': 1, 'symbol-collision': 1, 'unattributed-references': 1}
```

The relationships are written at the level of the named type. `go/types` (below) gives the
method sets:

```text
methodset Circle: [] implements []
methodset *Circle: [Area(path [0])] implements [Shape]
methodset Labeled: [Area(path [1 0]) Describe(path [0 0])] implements [Shape]
```

`Circle# → Shape#` therefore holds for `*Circle` only. `Labeled` implements `Shape` through its
embedded `*Circle`, and gets no method-level relationship. The constraint `Number` and the
generic type `Stack` have none.

## Syntax rows from go/parser

A scratch probe of about 200 lines over `go/parser`, `go/ast` and `go/build` wrote one
contract-shaped syntax row per definition of `p1`, with the anchor at the name identifier:

- line `Position.Line - 1`;
- column `Position.Column - 1`, a UTF-8 byte offset.

It applied the build constraints of the host context with `go/build.Context.MatchFile`. Its
rows were checked against the definition occurrences of the index:

```text
rows 27, parse errors 0
anchored 25 of 27 missing [('cgo.go', 0, 8, 'main'), ('cgo.go', 5, 5, 'withC')]
```

The two misses are the cgo file's package clause and function, which scip-go indexes in their
generated copies. `π`, 2 bytes wide, anchored at its byte column. The earlier run mapped Go's
constructs to the contract's outline kinds:

- `if`, `switch`, a type switch and `select` to `branch`, and their cases to `arm`;
- `for` and `for … range` to `loop`;
- `return`, `break`, `continue`, `goto` and `fallthrough` to `exit`;
- `go` and `defer` to `spawn`, and a function literal to `closure`;
- a call to `call`, with `callee` for `f(…)`, `x.m(…)` and `f[T](…)`.

Go has nothing for `await` or `opaque`.

## What go/types adds

A scratch probe type-checked `p2/shapes` from source (`go/types` with
`importer.ForCompiler(fset, "source", nil)`). For every call expression it printed what the type
checker says it is:

```text
generated file p2/shapes/zz_generated.go
call shapes.go:36:12 method value Area promoted-path=[1 0] target shapes.go:21:18
call shapes.go:37:19 method value Describe promoted-path=[0 0] target shapes.go:26:15
call shapes.go:54:48 builtin append
call shapes.go:56:42 builtin len
call shapes.go:63:14 method value Size promoted-path=[0] dynamic (interface receiver) target shapes.go:58:23
call shapes.go:70:38 conversion to int
call shapes.go:74:8 method value Push promoted-path=[0] target shapes.go:54:20
call shapes.go:75:12 static func shapes.Max target shapes.go:45:6
call shapes.go:75:29 static func shapes.Max target shapes.go:45:6
call shapes.go:75:38 method value Len promoted-path=[1] target shapes.go:56:20
call shapes.go:75:49 static func shapes.Total target shapes.go:60:6
call shapes.go:78:45 method value Area promoted-path=[0] dynamic (interface receiver) target shapes.go:5:2
call shapes.go:80:48 method value Area promoted-path=[0] target shapes.go:15:17
call shapes.go:80:57 static func shapes.leaf target shapes.go:82:6
call tagged.go:5:32 static func shapes.leaf target shapes.go:82:6
call zz_generated.go:5:35 static func shapes.leaf target shapes.go:82:6
instance shapes.go:73:8 Stack [string]
instance shapes.go:75:21 Max [int]
instance shapes.go:75:44 Total [shapes.Cell]
instance shapes.go:75:9 Max [int]
```

This probe parsed the directory with `parser.ParseDir` and applied no build constraint, which
is why `tagged.go` appears. The type checker gives what a parser cannot:

- a conversion (`int(c)`) and a builtin call (`append`, `len`), which `go/ast` writes as call
  expressions;
- the declaration a call resolves to, with a promoted method's path;
- whether the receiver is an interface or a type parameter;
- the type arguments of every instantiation.

`ast.IsGenerated` detects the generated file by its header.

A second probe of about 20 lines used `golang.org/x/tools/go/packages` v0.53.0 with syntax and
type information, `Tests: true` and `CGO_ENABLED=0`. It built offline from the module cache
with scip-go's `go.sum`. Per package, it prints the number of files, errors and resolved uses:

```text
example.com/p2/shapes 2 0 97
example.com/p2 1 0 23
```

`shapes` has 2 files under this context: `shapes.go` and the generated file. The
`//go:build probe` file is left out.

## Call instructions in the compiler's listing

`go doc cmd/compile` documents `-S` as "Print assembly listing to standard output (code
only)", `-N` as "Disable optimizations." and `-l` as "Disable inlining." It says nothing about
the listing's format (https://pkg.go.dev/cmd/compile). The listing was produced for the
module's packages only, with a cold build cache:

```sh
go build -o /dev/null -gcflags="example.com/p2/...=-S" ./...          # opt
go build -o /dev/null -gcflags="example.com/p2/...=-S -N -l" ./...    # noopt
```

```text
opt exit 0 wall 1.13 s CALL lines 37
noopt exit 0 wall 1.12 s CALL lines 74
direct calls between project functions left with the default optimisation:
(main.go:10) CALL example.com/p2/shapes.Generics(SB)
register calls from project lines with the default optimisation: 4
```

With the default optimisation, inlining removes every direct call between the module's functions
but one. With `-N -l`, each `CALL` line gives the absolute source file and a line, no column,
and the callee as a linker symbol. A scratch script of about 30 lines mapped each callee symbol
to its scip-go symbol by name, with type arguments removed, so that `(*Circle).Area` becomes
`Circle#Area().`. It then looked for a reference to that symbol on the call's line:

```text
corroborated shapes/shapes.go:36 shapes.(*Circle).Area -> Circle#Area().
corroborated shapes/shapes.go:37 shapes.Base.Describe -> Base#Describe().
corroborated shapes/shapes.go:74 shapes.(*Stack[go.shape.string]).Push -> Stack#Push().
corroborated shapes/shapes.go:75 shapes.Max[go.shape.int] -> Max().
corroborated shapes/shapes.go:75 shapes.(*Stack[go.shape.string]).Len -> Stack#Len().
corroborated shapes/shapes.go:75 shapes.Max[go.shape.int] -> Max().
corroborated shapes/shapes.go:75 shapes.Total[go.shape.int] -> Total().
corroborated shapes/shapes.go:80 shapes.Square.Area -> Square#Area().
corroborated shapes/shapes.go:80 shapes.leaf -> leaf().
corroborated shapes/zz_generated.go:5 shapes.leaf -> leaf().
no reference shapes/shapes.go:60 shapes.Total[go.shape.int] -> Total().
no reference shapes/shapes.go:45 shapes.Max[go.shape.int] -> Max().
no reference shapes/shapes.go:56 shapes.(*Stack[go.shape.string]).Len -> Stack#Len().
no reference shapes/shapes.go:54 shapes.(*Stack[go.shape.string]).Push -> Stack#Push().
corroborated main.go:10 shapes.Direct -> Direct().
corroborated main.go:10 shapes.Dispatch -> Dispatch().
corroborated main.go:10 shapes.Generics -> Generics().
corroborated main.go:11 shapes.UseEmbedded -> UseEmbedded().
corroborated main.go:11 shapes.Generated -> Generated().
15 of 19 calls to a module function from a module line are corroborated by a reference on the line
```

- **The four without a reference** are the compiler's instantiation wrappers. Their site is the
  generic definition's own line, where the index has a definition and no reference.
- **Calls between packages.** The calls from `main.go` into `shapes` are named symbols of
  another package, and their definitions are found by name.
- **Dynamic calls.** Calls through an interface or a type parameter are register calls
  (`CALL CX`).
- **Other compiler-generated functions** appear with the position `<autogenerated>:1`:
  pointer-receiver wrappers, promoted-method wrappers and interface method expressions.

A `//line` directive does not hide the real position. The listing writes the remapped
position, then the real one in brackets:

```text
(shapes/grammar.y:10[shapes/lined.go:4]) CALL example.com/p2/shapes.leaf(SB)
```

scip-go's own test of `//line` directives keeps the `.go` file's coordinates. The output
snapshot of `internal/testdata/snapshots/output/line_directives/generated.go`, in the release's
module, shows the definition `Generated()` on the line after `//line grammar.y:100`. Unverified:
a linked binary's line table, as `go tool objdump` reads it, holds only the remapped position.
This was not run.

## SSA call graphs

`callgraph -test -algo=<a> -format='{{.Caller}} {{.Callee}} {{.Description}}' .` on `p1`, with cgo
enabled. A project edge is one whose caller and callee are both functions of `example.com/probe`:

```text
static exit 0 edges 30428 project edges 29 wall 0.48 s
cha exit 0 edges 94553 project edges 37 wall 0.57 s
rta exit 0 edges 39064 project edges 31 wall 0.39 s
vta exit 0 edges 38637 project edges 33 wall 0.5 s
```

Two VTA edges that a syntax layer cannot give:

```text
example.com/probe.Exported (*example.com/probe.Box[int]).Area dynamic method call
example.com/probe.flow example.com/probe.make2$1 dynamic function call
```

The first callee anchors at the definition of `Area` on `Box`. The second is a closure, which has
no scip-go definition. SSA is built from `go/types` (https://pkg.go.dev/golang.org/x/tools/go/ssa,
https://pkg.go.dev/golang.org/x/tools/go/callgraph), not by the compiler:

- `static` edges are the calls the type checker resolves;
- `cha`, `rta` and `vta` edges are possible targets of dynamic calls under each algorithm's
  assumptions;
- in the earlier run, `callgraph` refused the module with `-test` when a test-only dependency was
  missing from the module cache ("packages contain errors"). scip-go tolerated the same missing
  package silently.

## Costs on a public tree

scip-go's own source at v0.2.10, identical file for file to the release's module in the module
cache: 37 Go files (7 of them tests), 5 066 lines. Each command ran under a scratch wrapper that
reports the child's wall time and peak resident memory (`getrusage` of the children):

```text
{"cmd": "callgraph -algo=static …", "exit": 0, "wall_s": 0.93, "maxrss_mb": 980, "stdout_lines": 73079}
{"cmd": "callgraph -algo=rta …", "exit": 0, "wall_s": 0.87, "maxrss_mb": 994, "stdout_lines": 145406}
{"cmd": "callgraph -algo=vta …", "exit": 0, "wall_s": 1.22, "maxrss_mb": 1010, "stdout_lines": 109636}
{"cmd": "callgraph -algo=cha …", "exit": 0, "wall_s": 1.42, "maxrss_mb": 1095, "stdout_lines": 355490}
{"cmd": "scip-go index …", "exit": 0, "wall_s": 3.17, "maxrss_mb": 275, "stdout_lines": 11}          (cold cache)
{"cmd": "scip-go index …", "exit": 0, "wall_s": 0.07, "maxrss_mb": 48, "stdout_lines": 11}           (warm)
{"cmd": "go build -o /dev/null -gcflags=…=-S -N -l ./...", "exit": 0, "wall_s": 3.3, "maxrss_mb": 287}   (cold)
{"cmd": "go build -o /dev/null -gcflags=…=-S -N -l ./...", "exit": 0, "wall_s": 0.24, "maxrss_mb": 265}  (warm)
listing bytes 10252335 CALL 3541 module CALLs from module lines 234 register CALLs from module lines 86 cache 179 MB
```

The call graphs and the listing are whole-program, the standard library included. The
listing's cold run includes compiling every dependency.

## Replaced modules

A `replace` directive with an absolute directory keeps the machine path as the symbol's package.
scip-go rewrites a replacement path that starts with `.` to the replaced module's path from its
`go.mod`, and leaves any other path as written (`internal/loader/loader.go`). In `m2`:

```text
scip-go exit 0
scip-go gomod $W/rev/dep v1.0.0 `example.com/dep`/D().
```

A snapshot made from this index would carry that path in its keys, while `spec/snapshot.md`
says a snapshot holds no absolute path.

## Reading the index

The SCIP Go bindings v0.10.0 read an index with `IndexVisitor.ParseStreaming`, one document at a
time. A scratch counter on that function read the index of `m1`, the index of `m2`, and the two
files concatenated, which is how `spec/adapter-contract.md` allows several `Index` messages:

```text
m1 documents 5, m2 documents 1, concatenated documents 6
```

A second scratch reader decoded the same files with `proto.Unmarshal` of the whole file into
`scip.Index`, and with `ParseStreaming` with and without callbacks for every section. It also
read a truncated index and three crafted files whose metadata, first document or first external
symbol declares a length of 2^40. Each run was under `ulimit -v 4000000`; `total allocated` is
`runtime.MemStats.TotalAlloc`:

```text
unmarshal /concat.scip: documents 6, error <nil>, total allocated 0 MiB
stream /concat.scip: documents 6, error <nil>, total allocated 0 MiB
stream-all /concat.scip: documents 6, error <nil>, total allocated 0 MiB
unmarshal uncated.scip: documents 2, error proto: cannot parse invalid wire-format data, total allocated 0 MiB
stream uncated.scip: documents 2, error failed to read data for documents: unexpected EOF, total allocated 0 MiB
stream-all uncated.scip: documents 2, error failed to read data for documents: unexpected EOF, total allocated 0 MiB
unmarshal uge-doc.scip: documents 0, error proto: cannot parse invalid wire-format data, total allocated 0 MiB
unmarshal uge-ext.scip: documents 0, error proto: cannot parse invalid wire-format data, total allocated 0 MiB
unmarshal ge-meta.scip: documents 0, error proto: cannot parse invalid wire-format data, total allocated 0 MiB
```

`ParseStreaming` on the crafted files did not return. Each run ended in

```text
runtime: out of memory: cannot allocate 1099511627776-byte block (3833856 in use)
fatal error: out of memory
```

including the metadata file, whose section it reads without a callback. It allocates the
declared length before checking it against the input. `proto.Unmarshal` of the whole file
refused every crafted and truncated file, and read the concatenated one.

## The first `go` command: work directory, telemetry and settings

A `go` command run with a nonexistent temporary directory shows whether it builds a work
directory. Below, `GOTMPDIR=/dev/null`; an unqualified query against a query for named
variables:

```text
unqualified: exit 1 go: creating work dir: mkdir /dev/null/go-build3814206055: not a directory
named: exit 0 {	"GOFLAGS":"-mod=mod",	"GOPROXY":"https://proxy.example.invalid"}
```

Every `go` command also opens telemetry counter files under `os.UserConfigDir()/go/telemetry` at
startup (`cmd/go/main.go`). The probe user's env file set `GOPROXY` and `GOFLAGS`. The first query
below ran with `HOME`, `XDG_CONFIG_HOME` and `GOENV` redirected and the user's directory watched;
the control kept the user's `HOME`:

```text
exit 0 {	"GOENV":"$W/rev2/user/.config/go/env",	"GOFLAGS":"-mod=mod",	"GOMODCACHE":"$W/b2/out/go/pkg/mod",	"GOPATH":"$W/b2/out/go",	"GOPROXY":"https://proxy.example.invalid",	"GOTELEMETRYDIR":"$W/b2/out/config/go/telemetry"}
events under the user's directory and the root: 0
files written under the redirected config: ./go/telemetry/local/go@devel-devel-linux-amd<date>.v1.count ./go/telemetry/local/upload.token ./go/telemetry/local/weekends 
### the same without redirecting XDG_CONFIG_HOME (control)
events under the user's directory: 5 $W/rev2/user/.config/go/telemetry $W/rev2/user/.config/go/telemetry/local/go@devel-devel-linux-amd64-2026-11-24.v1.count 
```

With `HOME` and `XDG_CONFIG_HOME` under an output directory and `GOENV` naming the user's env
file, the user's settings are read, telemetry lands under the output directory, and nothing is
written under the user's home or the module. A `GOPATH` and `GOMODCACHE` set nowhere then
default under the redirected home. `GOENV=off` reads no file. In the probe below, the env file
sets six of the eight settings, then all eight, and every query has `HOME` under `<out>/work`:

```text
env file without GOPATH or GOMODCACHE: {"GOINSECURE":"file-in.example.invalid","GOMODCACHE":"$W/rev4/out/work/hm/go/pkg/mod","GONOPROXY":"file-np.example.invalid","GONOSUMDB":"file-ns.example.invalid","GOPATH":"$W/rev4/out/work/hm/go","GOPRIVATE":"file.example.invalid","GOPROXY":"https://file.example.invalid","GOSUMDB":"off","GOTELEMETRYDIR":"$W/rev4/out/work/config/go/telemetry"}
env file with GOPATH and GOMODCACHE:   {"GOINSECURE":"file-in.example.invalid","GOMODCACHE":"$W/rev4/file-modcache","GONOPROXY":"file-np.example.invalid","GONOSUMDB":"file-ns.example.invalid","GOPATH":"$W/rev4/file-gopath","GOPRIVATE":"file.example.invalid","GOPROXY":"https://file.example.invalid","GOSUMDB":"off","GOTELEMETRYDIR":"$W/rev4/out/work/config/go/telemetry"}
all eight exported against the file:    {"GOINSECURE":"x-in.example.invalid","GOMODCACHE":"$W/rev4/x-modcache","GONOPROXY":"x-np.example.invalid","GONOSUMDB":"x-ns.example.invalid","GOPATH":"$W/rev4/x-gopath","GOPRIVATE":"x.example.invalid","GOPROXY":"https://x.example.invalid","GOSUMDB":"sum.x.example.invalid","GOTELEMETRYDIR":"$W/rev4/out/work/config/go/telemetry"}
GOENV=off:                              {"GOINSECURE":"","GOMODCACHE":"$W/rev4/out/work/hm/go/pkg/mod","GONOPROXY":"","GONOSUMDB":"","GOPATH":"$W/rev4/out/work/hm/go","GOPRIVATE":"","GOPROXY":"https://proxy.golang.org,direct","GOSUMDB":"sum.golang.org","GOTELEMETRYDIR":"$W/rev4/out/work/config/go/telemetry"}
### where telemetry lands, watched under the user's home and the module
events under the user's home and the module: 0; files under <out>/work/config: ./go/telemetry/local/weekends ./go/telemetry/local/go@devel-devel-linux-amd<date>.v1.count ./go/telemetry/local/upload.token 
```

Each exported value wins over the env file's (`cmd/go/internal/cfg/cfg.go`). A relative
`GOENV` is resolved against the working directory:

```text
https://relative.example.invalid sub/env  (run in cwd)
https://proxy.golang.org,direct sub/env  (run in <out>/work)
```

On Darwin, `os.UserConfigDir` is `$HOME/Library/Application Support` and does not read
`XDG_CONFIG_HOME` (`os/file.go`), and the telemetry directory derives from it
(`cmd/vendor/golang.org/x/telemetry/internal/telemetry/dir.go`). This was read from the
sources and not run.

## VCS stamping

`go list -json` with all fields stamps VCS information, and in a git checkout runs git to do it.
A copy of `p2` was made a git checkout, and a recording `git` was put first on `PATH`. Each run
followed a `touch` of a tracked file:

```text
go list -e -test -deps -json ./...                 git calls: 7 [git status --porcelain] .git/index rewritten: yes
  the same with -buildvcs=false                     git calls: 0 [] .git/index rewritten: no
go list -e -test -deps -json=ImportPath,GoFiles ./... git calls: 0 [] .git/index rewritten: no
scip-go index                                     git calls: 4 [git rev-parse --show-toplevel] .git/index rewritten: no
go/packages probe                                 git calls: 0 [] .git/index rewritten: no
```

Field selection (`-json=<fields>`, `cmd/go/internal/list/list.go`, `needAll`) and
`-buildvcs=false` both stop the git calls. scip-go's own git queries are read-only. Whether a
stamping run rewrites a stale `.git/index` varies; repeated with SHA-256 digests of the index:

```text
go list -e -test -deps -json run 1: index 8f0a43c3ffa6 -> 8f0a43c3ffa6
go list -e -test -deps -json run 2: index 8f0a43c3ffa6 -> 8f0a43c3ffa6
go list -json run 1: index 8f0a43c3ffa6 -> 3e996ef294e2
go list -json run 2: index 3e996ef294e2 -> a5a146f3e26d
plain git status: a5a146f3e26d -> bd3e31aade30
```

The `git status --porcelain` call, and that call's rewrite of a stale index, do not vary.

## Vendor mode, the module graph and checksums

A module requiring `golang.org/x/mod` v0.43.0 was vendored, and then read with an empty module
cache under `-mod=vendor`. The module `m2` with its absolute `replace` was vendored too:

```text
vendored: golang.org
modules.txt
go list -m -json all (vendor, empty cache): exit 1 go: can't compute 'all' using the vendor directory
go list -m -json (vendor, empty cache): exit 0 
go mod edit -json (vendor, empty cache): Require ['golang.org/x/mod'] Replace None
absolute replace, vendored: Replace $W/rev/dep
# example.com/dep v0.0.0 => $W/rev/dep
# example.com/dep => $W/rev/dep
inconsistent vendor/modules.txt, go list -m -json: exit 1 go: inconsistent vendoring in $W/b2/v:
  go list -e -test -deps -json: exit 1 go: inconsistent vendoring in $W/b2/v:
```

`go mod edit -json` reads `go.mod` alone and reports a `replace` in vendor mode. A missing line
in `go.sum` under `-mod=readonly`:

```text
go.sum lines after tidy: 2
go list -m -json: exit 0 
go list -m -json all: exit 0 
"Err": "missing go.sum entry for module providing package golang.org/x/mod/semver (imported by example.com/sumprobe); to add:\n\tgo get example.com/sumprobe"
  inventory exit 0 
go mod edit -json: exit 0
```

## Error kinds

The module `kinds` has one file per failure: `dep.go` imports an absent module, `header.go`
starts `packag kinds`, `importsyntax.go` has an unterminated import string, `wrongpkg.go` says
`package other`, `ty/ty.go` has a type error after `//line gen.y:40`, and `un/locked.go` has no
read permission. `go list -e -test -deps -json ./...`, then a scratch go/packages probe that
prints each package's `Errors` with their kind (1 list, 2 parse, 3 type) and its `TypeErrors`
with the remapped and the physical position:

```text
   example.org/absent/pkg Error: {'Pos': 'dep.go:3:8', 'Err': 'cannot find module providing package example.org/absent/pkg: import lookup disabled by -mo', 'ImportStack': ['example.com/kinds']} DepsErrors: None
   example.org/absent/pkg GoFiles None InvalidGoFiles None
   example.com/kinds Error: {'Pos': 'header.go:1:1', 'Err': "expected 'package', found packag", 'ImportStack': ['example.com/kinds']} DepsErrors: [('dep.go:3:8', 'cannot find module providing package example.org/absent/pkg: import lo')]
   example.com/kinds GoFiles ['dep.go', 'header.go', 'importsyntax.go', 'wrongpkg.go'] InvalidGoFiles ['header.go', 'importsyntax.go', 'wrongpkg.go']
   example.com/kinds/ty GoFiles ['ty.go'] InvalidGoFiles None
   example.com/kinds/un Error: {'Pos': '', 'Err': 'open $W/b2/k/un/locked.go: permission denied', 'ImportStack': []} DepsErrors: None
   example.com/kinds/un GoFiles ['un.go'] InvalidGoFiles ['locked.go']
example.com/kinds Errors kind=1 pos="header.go:1:1" msg="expected 'package', found packag"
example.com/kinds Errors kind=2 pos="$W/b2/k/header.go:1:1" msg="expected 'package', found packag"
example.com/kinds Errors kind=2 pos="$W/b2/k/importsyntax.go:4:2" msg="string literal not terminated"
example.com/kinds Errors kind=3 pos="-" msg="package ; expected package kinds"
example.com/kinds Errors kind=3 pos="$W/b2/k/wrongpkg.go:1:1" msg="package other; expected package kinds"
example.com/kinds Errors kind=3 pos="$W/b2/k/dep.go:3:10" msg="could not import example.org/absent/pkg (invalid package name: \"\")"
example.com/kinds Errors kind=3 pos="$W/b2/k/importsyntax.go:4:2" msg="invalid import path (invalid syntax)"
example.com/kinds TypeErrors adjusted=:0 physical=:0 msg="package ; expected package kinds"
example.com/kinds TypeErrors adjusted=wrongpkg.go:1 physical=wrongpkg.go:1 msg="package other; expected package kinds"
example.com/kinds TypeErrors adjusted=dep.go:3 physical=dep.go:3 msg="could not import example.org/absent/pkg (invalid package name: \"\")"
example.com/kinds TypeErrors adjusted=importsyntax.go:4 physical=importsyntax.go:4 msg="invalid import path (invalid syntax)"
example.com/kinds files=4 syntax=4
example.com/kinds/ty Errors kind=1 pos="" msg="# example.com/kinds/ty\nty/gen.y:40: cannot use \"x\" (untyped string constant) as int value in return statement"
example.com/kinds/ty Errors kind=3 pos="$W/b2/k/ty/gen.y:40" msg="cannot use \"x\" (untyped string constant) as int value in return statement"
example.com/kinds/ty TypeErrors adjusted=gen.y:40 physical=ty.go:4 msg="cannot use \"x\" (untyped string constant) as int value in return statement"
example.com/kinds/ty files=1 syntax=1
example.com/kinds/un Errors kind=1 pos="" msg="open $W/b2/k/un/locked.go: permission denied"
example.com/kinds/un files=1 syntax=1
```

scip-go drops a file whose package clause does not parse. The unreadable file was made
readable again and then removed before this run:

```text
scip-go exit 0
dep.go importsyntax.go ty/ty.go un/un.go wrongpkg.go 
```

A broken package inside the module is reported at that package, and its importers carry copies
in `DepsErrors`. `go list` copies every imported package's errors (`collectDepsErrors` in
`list.go`). In `m3`, the root package imports `a`, which imports `b`, whose package clause is
broken:

```text
   example.com/m3/b Main Error: (['example.com/m3/b'], "expected 'package', found packag") DepsErrors: [] InvalidGoFiles: ['b.go']
   example.com/m3/a Main Error: None DepsErrors: [(['example.com/m3/b'], "expected 'package', found packag")] InvalidGoFiles: None
   example.com/m3 Main Error: None DepsErrors: [(['example.com/m3/b'], "expected 'package', found packag")] InvalidGoFiles: None
```

## Directories the package list does not reach

In `m3`, `conly` holds only a file importing `"C"`, and `skipped` is named by `ignore ./skipped` in
`go.mod`. `go list -e -json ./...` under `CGO_ENABLED=0`, then `go/build`:

```text
	"ImportPath": "example.com/m3",
	"ImportPath": "example.com/m3/a",
	"ImportPath": "example.com/m3/b",
  go list ./conly: {'GoFiles': None, 'IgnoredGoFiles': ['c.go'], 'CgoFiles': None} build constraints exclude all Go files in $W/rev3/m/conly
ImportDir conly: err=no buildable Go source files in $W/rev3/m/conly GoFiles=[] IgnoredGoFiles=[c.go] CgoFiles=[] InvalidGoFiles=[]
  MatchFile c.go: true <nil>
ImportDir skipped: err=<nil> GoFiles=[s.go] IgnoredGoFiles=[] CgoFiles=[] InvalidGoFiles=[]
  MatchFile s.go: true <nil>
  go mod edit -json Ignore: [{'Path': './skipped'}]
```

`MatchFile` accepts the cgo-only file, while `ImportDir` with `CgoEnabled=false` lists it among
`IgnoredGoFiles`. `go/build` does not know about the `ignore` directive. In `m4`, `app` imports
`ign/lib` from a directory named by `ignore ./ign`, `emb` has `//go:embed missing.txt` with no such
file, and `app/dangling.go` is a link to a file that does not exist:

```text
   example.com/m4/ign/lib DepOnly GoFiles ['lib.go'] InvalidGoFiles None Error None
   example.com/m4/app root GoFiles ['app.go'] InvalidGoFiles ['dangling.go'] Error ('', 'open $W/rev4/m/app/dangling.go: no such file or directo')
   example.com/m4/emb root GoFiles ['emb.go'] InvalidGoFiles None Error None
scip-go exit 0 documents: app/app.go emb/emb.go 
example.com/m4/app Errors kind=1 pos="" msg="open $W/rev4/m/app/dangling.go: no such file or directory"
example.com/m4/app files=1 syntax=1
example.com/m4/emb Errors kind=1 pos="emb/emb.go:5:12" msg="pattern missing.txt: no matching files found"
example.com/m4/emb files=1 syntax=1
### reading the dangling link as an unprivileged and a privileged process would
read fails: FileNotFoundError 2
```

- The imported package under the ignored directory is a main-module package with `DepOnly`, and
  scip-go writes no document for it.
- The missing embed target is no `Error` of the field-selected listing. go/packages reports it as a
  list error at the directive.
- Reading the dangling link fails with `ENOENT` whatever the reader's privileges.

## Names and instantiation

`_` is never defined, whatever it names. The module `blank` declares it as a function, a type, a
const, a field and a method, beside `T` and `F`:

```text
  occ roles=1 range=[0 8 13] enclosing=[] `example.com/blank`/
  occ roles=1 range=[8 5 6] enclosing=[8 0 22] `example.com/blank`/T#
  occ roles=1 range=[12 5 6] enclosing=[12 0 11] `example.com/blank`/F().
```

For a generic function called across packages, `go/types` records the instance under the
selector's `Sel`. A scratch type-check of `lib.Max[int](1, 2)` and `lib.Max(1, 2)`:

```text
call at main.go:6:18 fun=*ast.IndexExpr index.X=*ast.SelectorExpr Instances[Sel]=true
call at main.go:7:13 fun=*ast.SelectorExpr Instances[Sel]=true
```

## Compiler diagnostics

go/packages reports every compiler diagnostic twice. Once as a structured error with a
`TypeErrors` entry, whose `PositionFor(pos, false)` is physical. Once inside one list error per
package, with an empty `Pos`, holding the compiler's text output. In `m5`:
- `two` has three type errors in one file;
- `mix` has a missing embed target and a type error;
- `lined` has a type error after `//line gen.y:40:7` (a directive with a column) and one after
  `//line other.y:3` (without).

```text
example.com/m5/lined Errors kind=1 pos="" msg="# example.com/m5/lined\nlined/gen.y:40:29: cannot use \"l\" (untyped string constant) as int value in return statement\nlined/other.y:3: cannot use \"k\" (untyped string constant) as int value in return statement"
example.com/m5/lined Errors kind=3 pos="$W/rev5/m/lined/gen.y:40:29" msg="cannot use \"l\" (untyped string constant) as int value in return statement"
example.com/m5/lined Errors kind=3 pos="$W/rev5/m/lined/other.y:3" msg="cannot use \"k\" (untyped string constant) as int value in return statement"
example.com/m5/lined TypeErrors adjusted=gen.y:40 physical=lined.go:4 msg="cannot use \"l\" (untyped string constant) as int value in return statement"
example.com/m5/lined TypeErrors adjusted=other.y:3 physical=lined.go:7 msg="cannot use \"k\" (untyped string constant) as int value in return statement"
example.com/m5/lined files=1 syntax=1
example.com/m5/mix Errors kind=1 pos="mix/mix.go:5:12" msg="pattern missing.txt: no matching files found"
example.com/m5/mix Errors kind=3 pos="$W/rev5/m/mix/mix.go:8:23" msg="cannot use \"m\" (untyped string constant) as int value in return statement"
example.com/m5/mix TypeErrors adjusted=mix.go:8 physical=mix.go:8 msg="cannot use \"m\" (untyped string constant) as int value in return statement"
example.com/m5/mix files=1 syntax=1
example.com/m5/two Errors kind=1 pos="" msg="# example.com/m5/two\ntwo/two.go:3:23: cannot use \"a\" (untyped string constant) as int value in return statement\ntwo/two.go:5:26: cannot use 1 (untyped int constant) as string value in return statement\ntwo/two.go:7:13: cannot use \"c\" (untyped string constant) as int value in variable declaration"
example.com/m5/two Errors kind=3 pos="$W/rev5/m/two/two.go:7:13" msg="cannot use \"c\" (untyped string constant) as int value in variable declaration"
example.com/m5/two Errors kind=3 pos="$W/rev5/m/two/two.go:3:23" msg="cannot use \"a\" (untyped string constant) as int value in return statement"
example.com/m5/two Errors kind=3 pos="$W/rev5/m/two/two.go:5:26" msg="cannot use 1 (untyped int constant) as string value in return statement"
example.com/m5/two TypeErrors adjusted=two.go:7 physical=two.go:7 msg="cannot use \"c\" (untyped string constant) as int value in variable declaration"
example.com/m5/two TypeErrors adjusted=two.go:3 physical=two.go:3 msg="cannot use \"a\" (untyped string constant) as int value in return statement"
example.com/m5/two TypeErrors adjusted=two.go:5 physical=two.go:5 msg="cannot use 1 (untyped int constant) as string value in return statement"
example.com/m5/two files=1 syntax=1
```

The compiler's block is a `# <import path>` line, then one line per diagnostic,
`<path>:<line>:<col>: <text>` or, where a `//line` directive gives no column,
`<path>:<line>: <text>`. Its paths are relative to the module root and remapped. A missing embed
target is a separate list error with a `Pos`. In `m6`:
- `redecl` declares `var x` twice after `//line gen.y:10:1`;
- `limit` has 14 type errors after `//line generated.y:40:1`;
- `fine` has none.

The structured errors of `limit` are shortened here to their first four and last:

```text
example.com/m6/fine files=1 syntax=1
example.com/m6/limit Errors kind=1 pos="" msg="# example.com/m6/limit\nlimit/generated.y:40:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:41:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:42:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:43:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:44:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:45:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:46:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:47:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:48:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:49:24: cannot use \"s\" (untyped string constant) as int value in return statement\nlimit/generated.y:49:24: too many errors"
example.com/m6/limit Errors kind=3 pos="$W/rev6/m/limit/generated.y:40:24" msg="cannot use \"s\" (untyped string constant) as int value in return statement"
example.com/m6/limit Errors kind=3 pos="$W/rev6/m/limit/generated.y:41:24" msg="cannot use \"s\" (untyped string constant) as int value in return statement"
example.com/m6/limit Errors kind=3 pos="$W/rev6/m/limit/generated.y:42:24" msg="cannot use \"s\" (untyped string constant) as int value in return statement"
example.com/m6/limit Errors kind=3 pos="$W/rev6/m/limit/generated.y:43:24" msg="cannot use \"s\" (untyped string constant) as int value in return statement"
example.com/m6/limit Errors kind=3 pos="$W/rev6/m/limit/generated.y:53:25" msg="cannot use \"s\" (untyped string constant) as int value in return statement"
example.com/m6/limit TypeErrors adjusted=generated.y:40 physical=l.go:4 msg="cannot use \"s\" (untyped string constant) as int value in return statement"
example.com/m6/limit TypeErrors adjusted=generated.y:53 physical=l.go:17 msg="cannot use \"s\" (untyped string constant) as int value in return statement"
example.com/m6/limit files=1 syntax=1
example.com/m6/redecl Errors kind=1 pos="" msg="# example.com/m6/redecl\nredecl/gen.y:12:5: x redeclared in this block\n\tredecl/gen.y:10:5[$W/rev6/m/redecl/r.go:4:5]: other declaration of x"
example.com/m6/redecl Errors kind=3 pos="$W/rev6/m/redecl/gen.y:12:5" msg="x redeclared in this block"
example.com/m6/redecl Errors kind=3 pos="$W/rev6/m/redecl/gen.y:10:5" msg="\tother declaration of x"
example.com/m6/redecl TypeErrors adjusted=gen.y:12 physical=r.go:6 msg="x redeclared in this block"
example.com/m6/redecl TypeErrors adjusted=gen.y:10 physical=r.go:4 msg="\tother declaration of x"
example.com/m6/redecl files=1 syntax=1
```

- **Redeclaration.** It is printed with a continuation line indented with a tab. The line carries
  its own position, remapped and then physical in brackets, and `TypeErrors` holds it as an
  entry of its own.
- **The error limit.** The compiler stops after 10 diagnostics with the positioned line
  `limit/generated.y:49:24: too many errors`, while `TypeErrors` holds all 14.
- **The loader's environment.** Its command runner sets `PWD` on every go command it runs, and
  its release-tag probe `go list -e -f {{context.ReleaseTags}} -- unsafe` sets `GO111MODULE=off`
  (`internal/gocommand` of x/tools v0.53.0).

## Licences

The Go modules a Go adapter would use, as their `LICENSE` files in the module cache state:

- `golang.org/x/tools` v0.53.0: the Go Authors' BSD-style licence;
- `github.com/scip-code/scip/bindings/go/scip` v0.10.0: Apache-2.0;
- `google.golang.org/protobuf` v1.36.12: the Go Authors' BSD-style licence;
- `github.com/santhosh-tekuri/jsonschema/v6` v6.0.2: Apache-2.0.

`golang.org/x/tools`' `go.mod` also requires `golang.org/x/mod`, `x/net`, `x/sync`,
`x/telemetry`, `github.com/google/go-cmp` and `github.com/yuin/goldmark`. The bindings' module
requires `github.com/fatih/color`, `github.com/hexops/gotextdiff` and
`github.com/sourcegraph/beaut`. The licences of these were not read for this record.

## Not measured

- Other Go versions, other scip-go versions, macOS and Windows. Go's behaviour on Darwin is
  read from its sources only.
- Writes outside the watched directories, for example to the module cache during a download,
  since every run was offline.
- How often multi-name declarations, conversions, builtin calls and calls between packages
  occur in real modules.
- The engine's handling of a `call` node on a conversion or a builtin. Unverified: it would record
  an `open-call` gap, since scip-go writes no occurrence there.
- `go tool objdump`, and what `go/types` reports for a package with type errors.
- Mapping compiler symbols to definitions beyond the probe: methods of generic types with
  several type parameters, method values, and closures (`func1` symbols).

## Sources

- scip-go: https://github.com/scip-code/scip-go (release v0.2.10, read from the module cache)
- SCIP and its Go bindings: https://github.com/scip-code/scip (bindings v0.10.0)
- The compiler's flags: https://pkg.go.dev/cmd/compile (read as `go doc cmd/compile`)
- `go/types`: https://pkg.go.dev/go/types
- `go/ast`: https://pkg.go.dev/go/ast
- `go/packages`: https://pkg.go.dev/golang.org/x/tools/go/packages
- SSA and call graphs: https://pkg.go.dev/golang.org/x/tools/go/ssa and
  https://pkg.go.dev/golang.org/x/tools/go/callgraph

These pages were not opened for this record. Each fact taken from them comes from the local
source or `go doc` output named beside it.
