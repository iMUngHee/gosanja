# Go flow fixture

This module has two packages, `flow` and `meter`, and uses `golang.org/x/mod/semver`
for a dependency outside the module. The native build context with cgo disabled admits
`flow.go`, `tagged.go`, `zz_generated.go`, `meter/meter.go`, `flow_test.go` and
`external_test.go`. The internal and external test files each have their own document.
The cgo file, the root's `ignore` file and the directory containing only an `ignore` file
are outside that selection. The package in `tagged.go` has the `go1.21` build attribute.

`Flow` places the branch, arm, loop, exit, spawn and closure forms in one body. It has if,
else-if and else arms, both kinds of switch, select, counted and range loops, and return,
break, continue, goto and fallthrough. The conversions and builtins are separate from their
call arguments. `make2()(1)` has a named inner call and an outer call with no written callee.
`fs[0]()` also has no written callee. The literal bound to `bump` contains a call to `seed`.
`seed` carries a `//go:noinline` directive.

The table uses the index's zero-based physical lines and UTF-8 byte columns.

| File and range | Indexed fact |
| --- | --- |
| `flow.go [12,4,5]` and `[12,7,8]` | `A` and `B` are separate definitions with the same enclosing range `[12,0,27]`. Their initializers call `seed` and `next`. |
| `flow.go [63,32,35]` | `Max[int](1, 2)` references the generic function in `flow`. |
| `flow.go [63,54,57]` and `[63,76,79]` | The explicit and inferred cross-package calls both reference `meter.Max`. |
| `flow.go [65,19,23]` | The call token `seed` follows the two-character identifier `개수`, which occupies six bytes. |
| `external_test.go [10,16,19]` | The external test references the generic `flow.Max` declaration. |
| `zz_generated.go [5,5,14]` | The definition of `Generated` stays on the physical line despite the `//line` directive. |

`F` and `G` have separate closure initializers in a shared declaration. The blank names
have no definition anchor. The generic `meter.Box` type has a method called by the external
test. `Promoted` calls `Circle.Score` directly and through an embedded value, then
`Gauge.Load` through an embedded pointer. The index has type-level implementation
relations from `Circle` and `EmbeddedValue` to `Shape`, and from `Gauge` and
`EmbeddedPointer` to `Loader`. It also has method-level relations from `Circle.Score`
and `Gauge.Load` to the corresponding interface members. `Gauge.Load` is a pointer-receiver
method.
