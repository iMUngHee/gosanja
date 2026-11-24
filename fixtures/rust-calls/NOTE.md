# rust-calls: the fixture's note

The fixture of the Rust adapter's calls layer. "Steps 1 to 5" below are the five checks the
layer makes from an instruction to a row, as `docs/adapters/rust.md` numbers them.
This note has two parts. Part one is fixed from the source and the index alone. For every
call in the source it records whether the index holds a reference on the call's line, and if
so that reference's range and SCIP symbol. For the symbols two definitions share, it gives
the collision group the engine's snapshot makes. From the source alone, it names the
definition each call may reach. From these it gives the caller and callee a row may have,
and the evidence that is forbidden whatever the compiler emits. Part two is recorded
from the raw IR of the pinned toolchain: for each of these calls, whether an
instruction exists, at what site and how many. The expected rows are the multiset the two
parts give together.

Conventions:
- Lines are 1-based.
- A site's column is given in UTF-8 bytes, as the index counts (`b`), and in characters
  (`u`, the manifest's `utf-32`).
- A *range in the index* is written as the index stores it: zero-based, end exclusive,
  `[start line, start column, end line, end column]`, columns in bytes.
- `[part 2]` marks a reading that depends on what the compiler emits or records.

## The workspace

- `calls-lib`: the library. Nearly every construct is in `src/lib.rs`. The other files are:
  - `drops.rs` (destructors);
  - `plain.rs`;
  - `mounted.rs`: mounted by `#[path]` as `under_test` under `cfg(test)` and as `in_build`
    otherwise;
  - `shared.rs`: also mounted by the integration test `tests/mount.rs`;
  - `bom.rs`: begins with a byte order mark.

  The features are `default = ["fast"]`, `fast` and `off`.
- `calls-app`: three binaries.
  - `calls-app` (`src/main.rs`) calls the library. It needs the feature `engine`, which
    `default` enables only through `front`.
  - `calls-twin` (`src/bin/twin.rs`) defines a `helper` with the same name as the one in
    `main.rs`.
  - `calls-extra` (`src/bin/extra.rs`) needs the feature `extra`, which `default` does not
    enable, so it is never built.
- `calls-derive`: a procedural macro library with a build script. It holds:
  - the derives `Describe` and `Label`;
  - the attribute macros `proxy`, `retype`, `retrait`, `keep` and `copy_to`.
- `fake-core`: a path dependency in the root but not a workspace member. Its library is named
  `core`.
- `.cargo/config.toml` includes `.cargo/flags.toml`, which sets `--cfg calls_flag`.

The build compiles the library, the two binaries that have their features, and the procedural
macro library. It never compiles the integration test, the build script or `calls-extra`.
It runs with `cfg(test)` off. rust-analyzer indexes with `cfg(test)` on. The index shows
this in two ways:
- it has no definition of `only_outside_test`, of the nested `run` in `nested_run::run`, or
  of the `cfg(not(test))` alternative of `alt`;
- `PickPlate::label()` and `PickBoard::label()` refer to `Panel::label` and `Sheet::label`.

## Build and index

```
$ cd fixtures/rust-calls && rustc +1.99.1 --version && cargo +1.99.1 --version
rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo 1.99.1 (5f94df478 2026-08-27)
$ CARGO_TARGET_DIR=<scratch> cargo +1.99.1 build --locked --offline --workspace
   Compiling calls-derive v0.1.0 (…/fixtures/rust-calls/calls-derive)
   Compiling fake-core v0.1.0 (…/fixtures/rust-calls/fake-core)
   Compiling calls-lib v0.1.0 (…/fixtures/rust-calls/calls-lib)
   Compiling calls-app v0.1.0 (…/fixtures/rust-calls/calls-app)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
$ CARGO_TARGET_DIR=<scratch> cargo +1.99.1 test --locked --offline --workspace --no-run
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.12s
```

Both commands exit 0 with no warnings. `calls-app` and `calls-twin` exit 0.

`index.scip` was made by the steps of `index()` in `fixtures/regenerate.sh`, run for
`rust-calls`:
1. copy the fixture to `/tmp/gosanja-fixtures/rust-calls`;
2. run `rust-analyzer scip .` there with `CARGO_TARGET_DIR` in a temporary directory;
3. copy `index.scip` back.

The script does not list `rust-calls` yet. The indexer was `rust-analyzer
0.3.3049-standalone`. The index is 119 582 bytes, with sha256
`4511b063ccaafe062bc7c7d971588b030582ef694a9bfbe57e153cb3c0a428ab`. A second run gave the
same bytes.

On stderr, rust-analyzer reports duplicate symbols for:
- the crate roots of the three binaries;
- `main()` in `extra.rs` and `main.rs`;
- `helper()` in `main.rs`;
- the function-local `drops/Token#`.

It also prints `Bug: definition at calls-lib/src/shared.rs:6:7-6:18 should have been in an
SCIP document but was not`. This concerns the integration test's copy of `shared.rs`. The
index holds `shared.rs` once, as calls-lib's module `shared`.

## How part one was derived

Every claim below was computed from three inputs:
- **The index.** `index.scip` was read with the official SCIP bindings (scip 0.10.0) and
  dumped as one line per occurrence and symbol.
- **The sources.** Every `.rs` file was parsed with syn 2.0.119 and proc-macro2 1.0.107,
  the parser of the adapter's syntax module. From it come every `fn`, `const` and `static`
  with its extent, its `fn` line, its name token, its signature lines and its attributes and
  those of the items around it. It also gives the taken lines, the macro invocations and
  every call expression.
- **The engine's snapshot.** `gosanja index --scip index.scip` at commit 3626631 gives the
  collision groups.

The sites are:
- every reference occurrence of a function or method symbol;
- every call node of the adapter's syntax layer that has no such reference;
- every call expression the parser sees that has neither.

The only call expressions the parser sees that are not sites are five `&&` in
calls-derive, which are not calls.

For each site, steps 2 to 5 were then evaluated for the instruction the call would give
if the compiler emits it:
- steps 3 and 4 against the index;
- step 5 against the parsed source.

The following were stated by hand, from the source:
- which targets are built, and which code `cfg` removes from the build;
- in which target a callee is defined;
- where generated code is placed;
- which sites lie in a closure.

Calls written by macros have no reference of their own. They are listed apart and evaluated
the same way.

## Corrections after the first comparison

Part one was compared once with the rows a reader of the layer's rule writes on this fixture.
Three sites differed, and each was checked again against the source and the index alone:
`drops.rs` 77:21 and `lib.rs` 45:5 are rows that part one had missed, and `lib.rs` 604:83 is
withheld by step 4, which part one had not applied. The table rows say `corrected` and why.

## Shared symbols

Definitions in two places share a symbol in two cases. Both are in calls-app, whose binaries
rust-analyzer gives one symbol namespace:

| Symbol | Collision group key in the snapshot | Members |
| --- | --- | --- |
| `rust-analyzer cargo calls-app 0.1.0 helper().` | `cargo:calls-app:helper().` | `~1` `calls-app/src/bin/twin.rs` [0,3,0,9]; `~2` `calls-app/src/main.rs` [6,3,6,9] |
| `rust-analyzer cargo calls-app 0.1.0 main().` | `cargo:calls-app:main().` | `~1` `calls-app/src/bin/extra.rs` [0,3,0,7]; `~2` `calls-app/src/bin/twin.rs` [4,3,4,7]; `~3` `calls-app/src/main.rs` [10,3,10,7] |

How each `helper` call reads:

| Call | Reference in the index | May reach, from the source alone |
| --- | --- | --- |
| `main.rs` 17:13 | [16,12,16,18], to the shared symbol | `helper` of `main.rs`, line 7 (member `~2`) |
| `twin.rs` 6:13 | [5,12,5,18], to the shared symbol | `helper` of `twin.rs`, line 1 (member `~1`) |

Each binary can call only its own `helper`. Which member the compiler names is observed in
part two. The adapter's end-to-end test then asserts the two results in the snapshot:
- the base `ref` points to the group;
- the calls layer's relation goes to the member.

`main` is never a callee. It is a caller of rows, compared by position.

`drops/Token#` is defined twice in `drops.rs` (lines 62 and 70). The two are different local
structs, which rust-analyzer gives one symbol. They are constructors, not calls.

## Totals

336 sites:

| Expected | Sites |
| --- | --- |
| none: step 2 | 98 |
| row | 79 |
| none: step 5 | 34 |
| same call | 34 |
| constructor | 25 |
| none: step 4 | 21 |
| not a call | 13 |
| none: closure | 9 |
| none: no instruction | 5 |
| row? | 5 |
| none: indirect | 5 |
| none: step 3 | 5 |
| none: coroutine | 3 |
| **all sites** | 336 |

- *Same call* counts two kinds of site:
  - the white-space references beside an operator: the index gives one operator three
    one-character references (space, operator, space), and the operator's own reference is
    the site;
  - the two `(leaf)()` call nodes that duplicate a reference.
- *Closure*, *coroutine* and *indirect* mean that the callee is no definition (a closure's or
  a coroutine's body) or that the call is through a pointer or vtable.
- Beyond the 336 sites, 11 instructions are written by macros and have no reference of their
  own. None of them gives a row (see "Calls written by macros" below).

The rows add up to 80 relations and 86 evidence entries. Each `row?` is counted at its
expected value: `always -> leaf` may gain a second entry if `always` is inlined into
`calls_always`, and `calls_always -> always` then has no instruction.

## The constructs

Each construct, where it is, and what part one expects. Line numbers are those of
`calls-lib/src/lib.rs` unless a file is named.

| Construct | Where | Part one |
| --- | --- | --- |
| a direct call | `direct` 16 | row `direct -> leaf` |
| a call across the two targets | `main.rs` 12 `calls_lib::direct()` | none, step 2; `callee-unresolved` in `main.rs` if the symbol is `v0` [part 2] |
| a library's generic function instantiated and called in the binary | `main.rs` 14 `calls_lib::generic(7u64)` | row `main -> generic`: calls-app emits `generic::<u64>`, which calls-lib does not instantiate |
| a library's `#[inline]` function called in the binary | `main.rs` 15 `calls_lib::hinted()` (`hinted` 558) | `row?`: a row if calls-app's IR defines `hinted` [part 2] |
| a generic function called at two types | `two_types` 24 | 2 entries `two_types -> generic` |
| a method of a generic type called at two types | `get_two` 298 | 2 entries `get_two -> Wrapper::get`; `Wrapper::get -> leaf` 2 entries (two instances) |
| a call inside a closure, and one inside an `async fn` | `make_adder` 37, `after_await` 52 | rows from `make_adder` and `after_await` |
| two calls to one function on one line | `two_on_one_line` 312 | 2 entries |
| a call through a trait object | `dyn_call` 88 | none: indirect |
| a call of a returned closure | `use_adder` 45 | none, step 3 |
| an operator implemented in the project | `use_operator` 326 | row `use_operator -> Meters::add` at the `+` (326:15) |
| an `#[inline(always)]` function | `always` 67, `calls_always` 72 | `row?` [part 2: inlining at opt-level 0] |
| a function made by a local macro and called | `make_fn!` 102, `use_made` 113 | none, step 3 |
| an attribute macro adding a proxy `impl` whose method has the written one's name and position | `#[proxy] impl Engine` 201, `use_proxy` 218-220 | none to `Engine::run`, `EngineProxy::run` or `beside`; generated `proxy_helper` call none (step 4) |
| the same kind of macro on `impl Slot<u8>` making `impl Slot<u16>`, both called on one line | `#[retype(u16)]` 239, `use_slots` 247 | none for either: 2 instructions resolve to `Slot<u8>::run` against 1 reference |
| on `impl a::Task for Local` making `impl b::Task for Local` | `#[retrait(b::Task)]` 387, `use_chosen` 400 | none: the callee is under the macro |
| a function passed by name on a line that calls another | `pass_by_name` 414 | row `pass_by_name -> apply`; `leaf` not counted |
| a macro invocation whose argument is a call and that saves it as another macro, with a nested function whose closure evaluates it | `save!` 567, `saver` 578 | rows `saver -> leaf` (579, the call `save!` makes in place), `saver -> inner`, `saver -> direct`; none for the closure in `inner`, whose line 581 `inner` also touches |
| two binaries that each define a `helper` and call it | `main.rs` 7/17, `twin.rs` 1/6 | a row from each `main` to its own `helper` |
| `run` with a nested `run` under `cfg(not(test))` whose closure shares a line with a call of the outer one | `nested_run::run` 591, line 594 | one row from the outer `run`, at 594:55; none at the closure's site 594:24 (innermost written item: the nested `run`) |
| `run` holding a nested function with a constant `run` whose initialiser is a closure on such a line | `const_run::run` 601, line 604 | none from line 604: the index holds `nested`, so the line's two references to `leaf` lie in different definitions (step 4), and the site 604:48 is also outside the caller's own items |
| a file with a byte order mark holding a caller and a callee, beside a readable file with a call | `bom.rs`; `plain.rs` | none in `bom.rs` (`parse-failed`); row `plain -> leaf` |
| `run` holding a call and a `macro_rules!` whose template is a closure on one line, with a nested module's `run` using it | `templated::run` 613, line 614 | none from 614 (taken); row at 615; row `templated::run -> inner::run` |
| a local type under a derive whose body shares a line with a call | `local_derive` 625 | none at 627 (taken); row at 628 |
| a written method on the line of a derive on another type that makes a method of that name | 634 `Plate` / `Panel::label`, `use_labels` 662 | none: line 634 is taken |
| one on a line inside such a type's body, the derive on the line before | 638-640 `Board` / `Sheet::label`, `use_labels` 663 | none: line 640 is taken |
| the same method on a line of its own | `Card::label` 645, `use_labels` 664 | rows `use_labels -> Card::label`, `Card::label -> leaf` |
| a member exporting `_Read` and another declaring it as foreign | `read_value` 563 (`export_name = "_Read"`), `main.rs` 4/16 | none from `main.rs`, no code; row `read_value -> leaf` in the library |
| a file mounted by the library and by its integration test | `shared.rs`, `tests/mount.rs` | rows `use_shared -> shared_call`, `shared_call -> shared_leaf`; the test is not built |
| a function and an `impl` under a macro that returns them unchanged, with plain functions beside them | `keep` 423-435, `use_kept` 441 | none to or from `kept` and `kept_method`; row `use_kept -> plain_beside`, `plain_beside -> leaf` |
| that macro imported as `test` and under a module named `clippy` | `spelled` 449, `ByClippy` 468, `use_spelled` 476 | none |
| `cfg_attr(feature, inline)`, and `cfg_attr` of an off feature with a macro inside | `maybe_inline` 480, `maybe_kept` 485 | rows for `maybe_inline`; none for `maybe_kept` |
| two methods of one name for two traits whose closure bodies share a line, the first calling `(leaf)()` | `Shared` 503-506 | none from either `go`; rows to both from `use_go` |
| two such methods with each closure on its own line | `Apart` 510-520 | one row from each `go` |
| a blanket `impl` on a primitive, called | `Tagged` 254, `use_blanket` 261 | rows |
| two methods of one name for two traits, the first's body on the line where the second begins, the first calling `(leaf)()` | `Joined` 524-527 | one entry from the second `go` at 526:69; none from the first |
| a trait alias chosen by `cfg(test)` between the written and the generated trait | `Chosen` 394-401 | none |
| a written method whose one-line body calls a helper, with a generated copy | `#[copy_to(Copied)]` 544, `use_copies` 554 | none: step 4 at 554 (2 instructions resolve to `Original::work`); the copy's `helper()` none (step 5) |
| two alternatives of one function on one line under `cfg(test)` and its opposite, called from a third | `alt` 403, `use_alt` 406 | none: line 403 holds two signatures |
| a default method called on a type of the project | `Greeter::greet` 164, `use_greet` 174 | rows |
| a generic function with a call, instantiated twice | `twice` 28 | 2 entries `twice -> leaf` |
| a file mounted as one module under `cfg(test)` and as another otherwise | `mounted.rs`, `use_mounted` 295 | rows `use_mounted -> mounted`, `mounted -> leaf` |
| a function taken as a value on a line that calls another | `value_and_call` 354 | row `-> direct`; `leaf` not a call |
| a definition whose name follows non-ASCII text | `after_accent` 357, after `/* é 😀 */` | rows |
| a `#[no_mangle]` function called from its own member and from one that declares it | `shared_entry` 138; `use_shared_entry` 330; `main.rs` 13 | row from `use_shared_entry`; none from `main.rs`, no code |
| a function only under `cfg(test)` and one only without | 333-351 | none: the first has no instruction, the second no definition in the index |
| a binary with unmet `required-features` | `calls-extra` | not built |
| non-ASCII text before a call | `accented` 151 | row `accented -> leaf` |
| a configuration file including a second that sets a `--cfg` | `.cargo/config.toml`, `only_with_flag` 303 | rows `use_flag -> only_with_flag`, `only_with_flag -> leaf` |

## What the index gives here

Each item was read from the index dump and can be found in the site table.

1. **Operators.** One `+` gives three one-character references: the space (326:14), `+`
   (326:15) and the space (326:16). Only `+` is an operator token, so step 4 counts one. Under
   a reading of "an operator and no name" that counted every reference that is not a name,
   the spaces would count as well. The `^` on `u32` gives the same three references, outside
   the project.
2. **Macro arguments and templates.**
   - A call in the arguments of a macro has a reference when the expansion uses the argument
     as an expression: `pin!(after_await())` at 58:27, and `save!(leaf())` at 579:11.
   - When the argument is only pasted into a new macro's definition, it has none. A first
     version of `save!` did that, so step 4 would have stopped the construct before step 5.
     `save!` therefore also evaluates its argument in place.
   - Calls written in a `macro_rules!` template (105, 188, 614) have no reference.
3. **Generated items.** These have no definition occurrence:
   - the methods of the proxies and of `Slot<u16>`;
   - `Copied::work`;
   - `Named::describe`;
   - `Plate::label` and `Board::label`.

   Their references point to symbols without a definition, or to the written method. Step 3
   resolves a generated method to the written method at its position, and step 4 or 5 then
   withholds it.
4. **Foreign declarations.** `shared_entry` and `_Read` in `main.rs`'s `extern` block are
   definitions of kind 17 in `main.rs`, and the calls refer to those. The parser reads the
   `safe fn` items of that block as verbatim tokens, so they are absent from its item list.
   They sit on their own lines, so no rule of step 5 depends on them.
5. **Enclosing ranges.** Every definition of a function or method in the index carries one.
   A constant's covers its attributes and initialiser: `const_run/run.` has [602,12,603,54].
6. **The byte order mark.** rust-analyzer reads `bom.rs` and gives `bom_leaf` at [2,7,2,15].
   The adapter's syntax layer marks the file `byte-order-mark`, and step 5 withholds both its
   ends (`parse-failed`).
7. **A callee whose name is on the line after `fn`.** `pub fn` / `split()` (142-143) is
   never a callee. The compiler's subprogram line is the `fn` line [part 2], and step 3 needs
   the name token on it. As a caller, `split` gives a row.

## Sites

`Found by` names the source of the site:
- `index`: a reference;
- `call node`: the adapter's syntax layer has a call there too;
- `parser only`: a call expression with neither.

`Call? / refs on line (counted)` gives:
- whether this reference is written as a call;
- how many references to the symbol are on the line;
- how many of them step 4 counts.

The symbol ids refer to the table "Symbols" at the end.

| Site (line:col b/u) | Found by | Reference: range in the index | Symbol | Call? / refs on line (counted) | May reach, from the source | Expected | Entries | Why |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **calls-app/src/bin/extra.rs** | | | | | | | | |
| 2:24b/24u | index, call node | `[1, 23, 1, 27]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | no instruction: `calls-extra` needs the feature `extra`, which `default` does not enable; not built |
| **calls-app/src/bin/twin.rs** | | | | | | | | |
| 6:13b/13u | index, call node | `[5, 12, 5, 18]` | S2 `helper().` | yes / 1 (1) | its own binary's `helper` (calls-app/src/bin/twin.rs:1), the member of the group in the calling file | row | 1 |  |
| **calls-app/src/main.rs** | | | | | | | | |
| 12:28b/28u | index, call node | `[11, 27, 11, 33]` | S3 `direct().` | yes / 1 (1) | calls-lib/src/lib.rs:16 `direct().` | none | - | step 2: calls-app's IR only declares `direct`; `callee-unresolved` in main.rs if the symbol is `v0` [part 2] |
| 13:21b/21u | index, call node | `[12, 20, 12, 32]` | S4 `shared_entry().` | yes / 1 (1) | calls-lib's `#[unsafe(no_mangle)] shared_entry` (lib.rs 138) | none | - | step 2, no code: calls-app declares `shared_entry` in its `extern` block; the symbol is not `v0` |
| 14:24b/24u | index, call node | `[13, 23, 13, 30]` | S5 `generic().` | yes / 1 (1) | calls-lib/src/lib.rs:20 `generic().` | row | 1 |  |
| 15:24b/24u | index, call node | `[14, 23, 14, 29]` | S6 `hinted().` | yes / 1 (1) | calls-lib/src/lib.rs:558 `hinted().` | row? [part 2] | 1 |  |
| 16:13b/13u | index, call node | `[15, 12, 15, 17]` | S7 `_Read().` | yes / 1 (1) | calls-lib's `read_value`, exported as `_Read` (lib.rs 563) | none | - | step 2, no code: calls-app declares `_Read` in its `extern` block; `_Read` begins with `_R` and is no `v0` symbol |
| 17:13b/13u | index, call node | `[16, 12, 16, 18]` | S2 `helper().` | yes / 1 (1) | its own binary's `helper` (calls-app/src/main.rs:7), the member of the group in the calling file | row | 1 |  |
| **calls-derive/src/lib.rs** | | | | | | | | |
| 6:16b/16u | index, call node | `[5, 15, 5, 26]` | S8 `struct_name().` | yes / 1 (1) | calls-derive/src/lib.rs:11 `struct_name().` | none | - | step 4: no function or method definition holds the counted references on 6 |
| 8:10b/10u | index, call node | `[7, 9, 7, 14]` | S9 `str/impl#[str]parse().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 8:18b/18u | index, call node | `[7, 17, 7, 23]` | S10 `` result/impl#[`Result<T, E>`]unwrap(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 12:28b/28u | index, call node | `[11, 27, 11, 36]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 13:36b/36u | index, call node | `[12, 35, 12, 39]` | S12 `token_stream/impl#[IntoIter][Iterator]next().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 15:22b/22u | index, call node | `[14, 21, 14, 30]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 15:33b/33u | index | `[14, 32, 14, 33]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 15:34b/34u | index | `[14, 33, 14, 35]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 15:36b/36u | index | `[14, 35, 14, 36]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 16:58b/58u | index, call node | `[15, 57, 15, 61]` | S12 `token_stream/impl#[IntoIter][Iterator]next().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 18:25b/25u | index, call node | `[17, 24, 17, 33]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 28:22b/22u | index, call node | `[27, 21, 27, 31]` | S15 `impl_parts().` | yes / 1 (1) | calls-derive/src/lib.rs:36 `impl_parts().` | none | - | step 4: no function or method definition holds the counted references on 28 |
| 28:38b/38u | index, call node | `[27, 37, 27, 42]` | S16 `impl#[TokenStream][Clone]clone().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 32:21b/21u | index, call node | `[31, 20, 31, 26]` | S17 `respan().` | yes / 1 (1) | calls-derive/src/lib.rs:64 `respan().` | none | - | step 4: no function or method definition holds the counted references on 32 |
| 32:33b/33u | index, call node | `[31, 32, 31, 37]` | S9 `str/impl#[str]parse().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 32:41b/41u | index, call node | `[31, 40, 31, 46]` | S10 `` result/impl#[`Result<T, E>`]unwrap(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 33:10b/10u | index, call node | `[32, 9, 32, 18]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 33:22b/22u | index, call node | `[32, 21, 32, 26]` | S18 `iter/traits/iterator/Iterator#chain().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 33:39b/39u | index, call node | `[32, 38, 32, 45]` | S19 `iter/traits/iterator/Iterator#collect().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 37:27b/27u | index, call node | `[36, 26, 36, 35]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 39:36b/36u | index, call node | `[38, 35, 38, 39]` | S12 `token_stream/impl#[IntoIter][Iterator]next().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 41:46b/46u | index, call node | `[40, 45, 40, 54]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 41:57b/57u | index | `[40, 56, 40, 57]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 41:58b/58u | index | `[40, 57, 40, 59]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 41:60b/60u | index | `[40, 59, 40, 60]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 42:62b/62u | index, call node | `[41, 61, 41, 65]` | S12 `token_stream/impl#[IntoIter][Iterator]next().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 43:26b/26u | call node only | `[42, 25, 42, 29]` | S20 `option/Option#Some#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 43:36b/36u | index, call node | `[42, 35, 42, 44]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 46:50b/50u | index, call node | `[45, 49, 45, 55]` | S21 `` option/impl#[`Option<T>`]expect(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 46:80b/80u | index, call node | `[45, 79, 45, 86]` | S22 `fn_span().` | yes / 1 (1) | calls-derive/src/lib.rs:53 `fn_span().` | row | 1 |  |
| 46:93b/93u | index, call node | `[45, 92, 45, 98]` | S23 `impl#[Group]stream().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 56:22b/22u | index, call node | `[55, 21, 55, 30]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 56:33b/33u | index | `[55, 32, 55, 33]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 56:34b/34u | index | `[55, 33, 55, 35]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 56:36b/36u | index | `[55, 35, 55, 36]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 58:26b/26u | index, call node | `[57, 25, 57, 29]` | S24 `impl#[Ident]span().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 65:24b/24u | index, call node | `[64, 23, 64, 26]` | S25 `` vec/impl#[`Vec<T>`]new(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 68:36b/36u | index, call node | `[67, 35, 67, 38]` | S26 `impl#[Group]new().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 68:46b/46u | index, call node | `[67, 45, 67, 54]` | S27 `impl#[Group]delimiter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 68:59b/59u | index, call node | `[67, 58, 67, 64]` | S17 `respan().` | yes / 1 (1) | calls-derive/src/lib.rs:64 `respan().` | row | 1 |  |
| 68:72b/72u | index, call node | `[67, 71, 67, 77]` | S23 `impl#[Group]stream().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 69:19b/19u | index, call node | `[68, 18, 68, 26]` | S28 `impl#[Group]set_span().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 70:32b/32u | call node only | `[69, 31, 69, 36]` | S29 `TokenTree#Group#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 72:19b/19u | index, call node | `[71, 18, 71, 26]` | S30 `impl#[TokenTree]set_span().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 74:13b/13u | index, call node | `[73, 12, 73, 16]` | S31 `` vec/impl#[`Vec<T, A>`]push(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 76:9b/9u | index, call node | `[75, 8, 75, 17]` | S32 `` vec/impl#[`Vec<T, A>`][IntoIterator]into_iter(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 76:21b/21u | index, call node | `[75, 20, 75, 27]` | S19 `iter/traits/iterator/Iterator#collect().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 83:22b/22u | index, call node | `[82, 21, 82, 31]` | S15 `impl_parts().` | yes / 1 (1) | calls-derive/src/lib.rs:36 `impl_parts().` | none | - | step 4: no function or method definition holds the counted references on 83 |
| 83:38b/38u | index, call node | `[82, 37, 82, 42]` | S16 `impl#[TokenStream][Clone]clone().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 85:21b/21u | index, call node | `[84, 20, 84, 26]` | S17 `respan().` | yes / 1 (1) | calls-derive/src/lib.rs:64 `respan().` | none | - | step 4: no function or method definition holds the counted references on 85 |
| 85:33b/33u | index, call node | `[84, 32, 84, 37]` | S9 `str/impl#[str]parse().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 85:41b/41u | index, call node | `[84, 40, 84, 46]` | S10 `` result/impl#[`Result<T, E>`]unwrap(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 86:10b/10u | index, call node | `[85, 9, 85, 18]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 86:22b/22u | index, call node | `[85, 21, 85, 26]` | S18 `iter/traits/iterator/Iterator#chain().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 86:39b/39u | index, call node | `[85, 38, 85, 45]` | S19 `iter/traits/iterator/Iterator#collect().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 100:28b/28u | index, call node | `[99, 27, 99, 43]` | S33 `trait_impl_parts().` | yes / 1 (1) | calls-derive/src/lib.rs:106 `trait_impl_parts().` | none | - | step 4: no function or method definition holds the counted references on 100 |
| 100:50b/50u | index, call node | `[99, 49, 99, 54]` | S16 `impl#[TokenStream][Clone]clone().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 102:21b/21u | index, call node | `[101, 20, 101, 26]` | S17 `respan().` | yes / 1 (1) | calls-derive/src/lib.rs:64 `respan().` | none | - | step 4: no function or method definition holds the counted references on 102 |
| 102:33b/33u | index, call node | `[101, 32, 101, 37]` | S9 `str/impl#[str]parse().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 102:41b/41u | index, call node | `[101, 40, 101, 46]` | S10 `` result/impl#[`Result<T, E>`]unwrap(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 103:10b/10u | index, call node | `[102, 9, 102, 18]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 103:22b/22u | index, call node | `[102, 21, 102, 26]` | S18 `iter/traits/iterator/Iterator#chain().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 103:39b/39u | index, call node | `[102, 38, 102, 45]` | S19 `iter/traits/iterator/Iterator#collect().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 107:27b/27u | index, call node | `[106, 26, 106, 35]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 109:36b/36u | index, call node | `[108, 35, 108, 39]` | S12 `token_stream/impl#[IntoIter][Iterator]next().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 111:46b/46u | index, call node | `[110, 45, 110, 54]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 111:57b/57u | index | `[110, 56, 110, 57]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 111:58b/58u | index | `[110, 57, 110, 59]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 111:60b/60u | index | `[110, 59, 110, 60]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 112:62b/62u | index, call node | `[111, 61, 111, 65]` | S12 `token_stream/impl#[IntoIter][Iterator]next().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 113:26b/26u | call node only | `[112, 25, 112, 29]` | S20 `option/Option#Some#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 113:36b/36u | index, call node | `[112, 35, 112, 44]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 117:38b/38u | index, call node | `[116, 37, 116, 43]` | S23 `impl#[Group]stream().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 117:47b/47u | index, call node | `[116, 46, 116, 55]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 118:47b/47u | index, call node | `[117, 46, 117, 50]` | S12 `token_stream/impl#[IntoIter][Iterator]next().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 120:34b/34u | index, call node | `[119, 33, 119, 42]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 120:45b/45u | index | `[119, 44, 119, 45]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 120:46b/46u | index | `[119, 45, 119, 47]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 120:48b/48u | index | `[119, 47, 119, 48]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 121:69b/69u | index, call node | `[120, 68, 120, 72]` | S12 `token_stream/impl#[IntoIter][Iterator]next().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 123:36b/36u | index, call node | `[122, 35, 122, 41]` | S21 `` option/impl#[`Option<T>`]expect(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 123:77b/77u | index, call node | `[122, 76, 122, 85]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 123:96b/96u | index, call node | `[122, 95, 122, 99]` | S24 `impl#[Ident]span().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 137:25b/25u | index, call node | `[136, 24, 136, 27]` | S25 `` vec/impl#[`Vec<T>`]new(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 139:23b/23u | index, call node | `[138, 22, 138, 27]` | S16 `impl#[TokenStream][Clone]clone().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 141:46b/46u | index, call node | `[140, 45, 140, 54]` | S13 `string/impl#[T][ToString]to_string().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 141:57b/57u | index | `[140, 56, 140, 57]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 141:58b/58u | index | `[140, 57, 140, 59]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 141:60b/60u | index | `[140, 59, 140, 60]` | S14 `` string/impl#[String][`PartialEq<&str>`]eq(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 143:22b/22u | index, call node | `[142, 21, 142, 27]` | S34 `` vec/impl#[`Vec<T, A>`][`Extend<T>`]extend(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 143:34b/34u | index, call node | `[142, 33, 142, 38]` | S16 `impl#[TokenStream][Clone]clone().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 149:14b/14u | index, call node | `[148, 13, 148, 17]` | S31 `` vec/impl#[`Vec<T, A>`]push(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 151:10b/10u | index, call node | `[150, 9, 150, 18]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 151:22b/22u | index, call node | `[150, 21, 150, 26]` | S18 `iter/traits/iterator/Iterator#chain().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 151:34b/34u | index, call node | `[150, 33, 150, 40]` | S19 `iter/traits/iterator/Iterator#collect().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 158:16b/16u | index, call node | `[157, 15, 157, 26]` | S8 `struct_name().` | yes / 1 (1) | calls-derive/src/lib.rs:11 `struct_name().` | none | - | step 4: no function or method definition holds the counted references on 158 |
| 158:34b/34u | index, call node | `[157, 33, 157, 38]` | S16 `impl#[TokenStream][Clone]clone().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 159:28b/28u | index, call node | `[158, 27, 158, 36]` | S11 `token_stream/impl#[TokenStream][IntoIterator]into_iter().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 159:40b/40u | index, call node | `[158, 39, 158, 43]` | S35 `iter/traits/iterator/Iterator#last().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 160:46b/46u | index, call node | `[159, 45, 159, 55]` | S36 `impl#[Group]span_close().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 161:30b/30u | index, call node | `[160, 29, 160, 33]` | S37 `impl#[TokenTree]span().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 162:23b/23u | index, call node | `[161, 22, 161, 31]` | S38 `impl#[Span]call_site().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 165:5b/5u | index, call node | `[164, 4, 164, 10]` | S17 `respan().` | yes / 1 (1) | calls-derive/src/lib.rs:64 `respan().` | none | - | step 4: no function or method definition holds the counted references on 165 |
| 165:17b/17u | index, call node | `[164, 16, 164, 21]` | S9 `str/impl#[str]parse().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 165:25b/25u | index, call node | `[164, 24, 164, 30]` | S10 `` result/impl#[`Result<T, E>`]unwrap(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| **calls-lib/src/bom.rs** | | | | | | | | |
| 8:5b/5u | index | `[7, 4, 7, 12]` | S39 `bom/bom_leaf().` | yes / 1 (1) | calls-lib/src/bom.rs:3 `bom/bom_leaf().` | none | - | step 5: parse-failed: calls-lib/src/bom.rs begins with a byte order mark |
| **calls-lib/src/drops.rs** | | | | | | | | |
| 6:35b/35u | index, call node | `[5, 34, 5, 38]` | S40 `drops/helpers/drop().` | yes / 1 (1) | calls-lib/src/drops.rs:36 `drops/helpers/drop().` | row | 1 |  |
| 19:17b/17u | call node only | `[18, 16, 18, 21]` | S41 `drops/Guard#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 20:5b/5u | index, call node | `[19, 4, 19, 8]` | S42 `mem/drop().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 24:18b/18u | call node only | `[23, 17, 23, 22]` | S41 `drops/Guard#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 28:35b/35u | call node only | `[27, 34, 27, 39]` | S41 `drops/Guard#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 32:31b/31u | call node only | `[31, 30, 31, 35]` | S41 `drops/Guard#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 32:49b/49u | call node only | `[31, 48, 31, 53]` | S41 `drops/Guard#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 40:5b/5u | index, call node | `[39, 4, 39, 8]` | S42 `mem/drop().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 44:5b/5u | index, call node | `[43, 4, 43, 8]` | S42 `mem/drop().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 44:10b/10u | call node only | `[43, 9, 43, 13]` | S20 `option/Option#Some#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 44:15b/15u | call node only | `[43, 14, 43, 19]` | S41 `drops/Guard#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 52:21b/21u | call node only | `[51, 20, 51, 25]` | S41 `drops/Guard#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 53:9b/9u | index, call node | `[52, 8, 52, 12]` | S42 `mem/drop().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 58:18b/18u | call node only | `[57, 17, 57, 22]` | S43 `drops/Outer#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 71:23b/23u | call node only | `[70, 22, 70, 27]` | S44 `drops/Token#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 77:21b/21u | index, call node | `[76, 20, 76, 24]` | S45 `mem/drop().` | yes / 1 (1) | fake-core/src/lib.rs:4 `mem/drop().` | row | 1 | corrected: `fake-core` is a path dependency under the root and calls-lib's IR defines the instance `drop::<Guard>`, so the callee is a definition of the project |
| 77:26b/26u | call node only | `[76, 25, 76, 30]` | S41 `drops/Guard#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| **calls-lib/src/lib.rs** | | | | | | | | |
| 10:45b/45u | index | `[9, 44, 9, 55]` | S46 `drops/drop_at_end().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 10:58b/58u | index | `[9, 57, 9, 68]` | S47 `drops/drop_holder().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 10:71b/71u | index | `[9, 70, 9, 79]` | S48 `drops/drop_none().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 10:82b/82u | index | `[9, 81, 9, 89]` | S49 `drops/drop_now().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 10:92b/92u | index | `[9, 91, 9, 101]` | S50 `drops/drop_outer().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 10:104b/104u | index | `[9, 103, 9, 117]` | S51 `drops/drop_lookalike().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 10:120b/120u | index | `[9, 119, 9, 128]` | S52 `drops/drop_pair().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 10:131b/131u | index | `[9, 130, 9, 143]` | S53 `drops/drop_shadowed().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 10:146b/146u | index | `[9, 145, 9, 154]` | S54 `drops/drop_some().` | no / 1 (0) | - | not a call | - | a path in `pub use` |
| 17:5b/5u | index, call node | `[16, 4, 16, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 25:6b/6u | index, call node | `[24, 5, 24, 12]` | S5 `generic().` | yes / 2 (2) | calls-lib/src/lib.rs:20 `generic().` | row | 1 |  |
| 25:21b/21u | index, call node | `[24, 20, 24, 27]` | S5 `generic().` | yes / 2 (2) | calls-lib/src/lib.rs:20 `generic().` | row | 1 |  |
| 29:5b/5u | index, call node | `[28, 4, 28, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 2 |  |
| 34:6b/6u | index, call node | `[33, 5, 33, 10]` | S55 `twice().` | yes / 2 (2) | calls-lib/src/lib.rs:28 `twice().` | row | 1 |  |
| 34:18b/18u | index, call node | `[33, 17, 33, 22]` | S55 `twice().` | yes / 2 (2) | calls-lib/src/lib.rs:28 `twice().` | row | 1 |  |
| 39:9b/9u | index, call node | `[38, 8, 38, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 40:10b/10u | index | `[39, 9, 39, 10]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 40:11b/11u | index | `[39, 10, 39, 11]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 40:12b/12u | index | `[39, 11, 39, 12]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 45:5b/5u | index, call node | `[44, 4, 44, 14]` | S57 `make_adder().` | yes / 1 (1) | calls-lib/src/lib.rs:37 `make_adder().` | row | 1 | corrected: the reference to `make_adder` is followed by `(`; only the call of the returned closure, the next site, has no row |
| 45:5b/5u | call node only | none | - | - | a closure's body | none | - | closure: step 3: the callee is the returned closure's body, which has no definition |
| 53:13b/13u | index, call node | `[52, 12, 52, 17]` | S58 `ready().` | yes / 1 (1) | calls-lib/src/lib.rs:48 `ready().` | row | 1 |  |
| 53:20b/20u | index | `[52, 19, 52, 20]` | S59 `future/future/Future#poll().` | no / 2 (0) | a coroutine body | none | - | coroutine: step 3: `.await` polls the body of `ready`, a coroutine with no definition |
| 53:21b/21u | index | `[52, 20, 52, 25]` | S59 `future/future/Future#poll().` | no / 2 (0) | a coroutine body | none | - | coroutine: step 3: as at 53:20 |
| 54:5b/5u | index, call node | `[53, 4, 53, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 58:27b/27u | index | `[57, 26, 57, 37]` | S60 `after_await().` | yes / 1 (1) | calls-lib/src/lib.rs:52 `after_await().` | row | 1 |  |
| 59:24b/24u | index, call node | `[58, 23, 58, 27]` | S61 `task/wake/impl#[Waker]noop().` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 60:18b/18u | index, call node | `[59, 17, 59, 23]` | S62 `` pin/impl#[`Pin<Ptr>`]as_mut(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 60:27b/27u | index, call node | `[59, 26, 59, 30]` | S59 `future/future/Future#poll().` | yes / 1 (1) | a coroutine body | none | - | coroutine: step 3: the call is to the coroutine body of `after_await`, which has no definition |
| 60:46b/46u | index, call node | `[59, 45, 59, 55]` | S63 `` task/wake/impl#[`Context<'a>`]from_waker(). `` | yes / 1 (1) | outside the project | none | - | step 2, no code: outside the project |
| 68:5b/5u | index, call node | `[67, 4, 67, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row? [part 2] | 1 | one entry from `always`; a second, at this site with `calls_always` as the scope, if the call to `always` is inlined [part 2] |
| 72:5b/5u | index, call node | `[71, 4, 71, 10]` | S64 `always().` | yes / 1 (1) | calls-lib/src/lib.rs:67 `always().` | row? [part 2] | 1 | a row if the call stays a call; none if `#[inline(always)]` inlines `always` at opt-level 0 [part 2] |
| 83:9b/9u | index, call node | `[82, 8, 82, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 88:11b/11u | index, call node | `[87, 10, 87, 14]` | S65 `Shape#area().` | yes / 1 (1) | any implementation of the trait | none | - | indirect: no direct callee: a call through the vtable (`indirect-call`) |
| 92:5b/5u | index, call node | `[91, 4, 91, 12]` | S66 `dyn_call().` | yes / 1 (1) | calls-lib/src/lib.rs:87 `dyn_call().` | row | 1 |  |
| 99:12b/12u | index, call node | `[98, 11, 98, 19]` | S67 `impl#[Named]describe().` | yes / 1 (1) | the derive's `Named::describe`, at the derive's call site, line 95 [part 2] | none | - | step 3: 0 definitions named `describe` have their name token on calls-lib/src/lib.rs:95 |
| 110:46b/46u | index, call node | `[109, 45, 109, 49]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: signature: line 110, which the caller's signature touches, holds the macro invocation `make_fn!` |
| 113:12b/12u | index, call node | `[112, 11, 112, 15]` | S68 `real/made().` | yes / 1 (1) | calls-lib/src/lib.rs:110 `real/made().` | none | - | step 3: 2 definitions named `made` have their name token on calls-lib/src/lib.rs:110 |
| 113:35b/35u | index, call node | `[112, 34, 112, 38]` | S69 `made_by_macro/made().` | yes / 1 (1) | the `made` of `make_fn!(made)`, whose `fn` is the template's (104) or the invocation's (110) [part 2] | none | - | step 3: 0 definitions named `made` have their name token on calls-lib/src/lib.rs:104 |
| 117:6b/6u | index, call node | `[116, 5, 116, 9]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 121:5b/5u | index, call node | `[120, 4, 120, 8]` | S70 `fill().` | yes / 1 (1) | calls-lib/src/lib.rs:116 `fill().` | row | 1 |  |
| 128:9b/9u | index, call node | `[127, 8, 127, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 2 |  |
| 134:5b/5u | call node only | `[133, 4, 133, 11]` | S71 `Wrapper#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 134:18b/18u | index, call node | `[133, 17, 133, 20]` | S72 `` impl#[`Wrapper<T>`]get(). `` | yes / 1 (1) | calls-lib/src/lib.rs:127 `` impl#[`Wrapper<T>`]get(). `` | row | 1 |  |
| 139:5b/5u | index, call node | `[138, 4, 138, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 144:5b/5u | index, call node | `[143, 4, 143, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 148:5b/5u | index, call node | `[147, 4, 147, 9]` | S73 `split().` | yes / 1 (1) | calls-lib/src/lib.rs:143 `split().` | none | - | step 3: 0 definitions named `split` have their name token on calls-lib/src/lib.rs:142 |
| 151:54b/49u | index, call node | `[150, 53, 150, 57]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 155:9b/9u | index, call node | `[154, 8, 154, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 160:6b/6u | call node only | `[159, 5, 159, 12]` | S71 `Wrapper#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 160:20b/20u | index, call node | `[159, 19, 159, 27]` | S74 `` impl#[`Wrapper<i16>`]concrete(). `` | yes / 1 (1) | calls-lib/src/lib.rs:154 `` impl#[`Wrapper<i16>`]concrete(). `` | row | 1 |  |
| 160:32b/32u | call node only | `[159, 31, 159, 38]` | S71 `Wrapper#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 160:46b/46u | index, call node | `[159, 45, 159, 48]` | S72 `` impl#[`Wrapper<T>`]get(). `` | yes / 1 (1) | calls-lib/src/lib.rs:127 `` impl#[`Wrapper<T>`]get(). `` | row | 1 |  |
| 165:9b/9u | index, call node | `[164, 8, 164, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 174:11b/11u | index, call node | `[173, 10, 173, 15]` | S75 `Greeter#greet().` | yes / 1 (1) | calls-lib/src/lib.rs:164 `Greeter#greet().` | row | 1 |  |
| 177:47b/47u | index, call node | `[176, 46, 176, 50]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: signature: line 177, which the caller's signature touches, holds the signature of another `fn same` |
| 179:12b/12u | index, call node | `[178, 11, 178, 15]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: signature: line 177, which the caller's signature touches, holds the signature of another `fn same` |
| 182:13b/13u | index, call node | `[181, 12, 181, 16]` | S76 `first/same().` | yes / 1 (1) | calls-lib/src/lib.rs:177 `first/same().` | none | - | step 4: 1 counted reference(s) on 182 against 2 instructions of one emitted function calling it |
| 182:29b/29u | index, call node | `[181, 28, 181, 32]` | S77 `second/same().` | yes / 1 (1) | calls-lib/src/lib.rs:178 `second/same().` | none | - | step 4: 1 counted reference(s) on 182 against 2 instructions of one emitted function calling it |
| 196:5b/5u | call node only | `[195, 4, 195, 7]` | S78 `local 19` | yes / 1 (1) | a closure's body | none | - | closure: step 3: the callee is the closure's body, which has no definition |
| 204:9b/9u | index, call node | `[203, 8, 203, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `run` is under `calls_derive::proxy` |
| 208:9b/9u | index, call node | `[207, 8, 207, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `beside` is under `calls_derive::proxy` |
| 218:16b/16u | index, call node | `[217, 15, 217, 18]` | S79 `impl#[Engine]run().` | yes / 1 (1) | calls-lib/src/lib.rs:203 `impl#[Engine]run().` | none | - | step 5: written: the callee `run` is under `calls_derive::proxy` |
| 219:21b/21u | index, call node | `[218, 20, 218, 23]` | S80 `impl#[EngineProxy]run().` | yes / 1 (1) | the generated `EngineProxy::run`, all its tokens at the written `fn` of `Engine::run` (203) | none | - | step 4: no reference to `impl#[Engine]run().` written as a call on calls-lib/src/lib.rs:219 |
| 220:16b/16u | index, call node | `[219, 15, 219, 21]` | S81 `impl#[Engine]beside().` | yes / 1 (1) | calls-lib/src/lib.rs:207 `impl#[Engine]beside().` | none | - | step 5: written: the callee `beside` is under `calls_derive::proxy` |
| 229:9b/9u | index, call node | `[228, 8, 228, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `run` is under `calls_derive::proxy` |
| 234:14b/14u | index, call node | `[233, 13, 233, 16]` | S82 `impl#[GenProxy]run().` | yes / 1 (1) | the generated `GenProxy::run`, at the written `fn` of `Gen::run` (228) | none | - | step 4: no reference to `impl#[Gen]run().` written as a call on calls-lib/src/lib.rs:234 |
| 242:9b/9u | index, call node | `[241, 8, 241, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `run` is under `calls_derive::retype` |
| 247:6b/6u | call node only | `[246, 5, 246, 9]` | S83 `Slot#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 247:16b/16u | index, call node | `[246, 15, 246, 18]` | S84 `` impl#[`Slot<u8>`]run(). `` | yes / 1 (1) | calls-lib/src/lib.rs:241 `` impl#[`Slot<u8>`]run(). `` | none | - | step 4: 1 counted reference(s) on 247 against 2 instructions of one emitted function calling it |
| 247:23b/23u | call node only | `[246, 22, 246, 26]` | S83 `Slot#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 247:34b/34u | index, call node | `[246, 33, 246, 36]` | S85 `` impl#[`Slot<u16>`]run(). `` | yes / 1 (1) | the generated `Slot<u16>::run`, at the written `fn` of `Slot<u8>::run` (241) | none | - | step 4: 1 counted reference(s) on 247 against 2 instructions of one emitted function calling it |
| 256:9b/9u | index, call node | `[255, 8, 255, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 261:9b/9u | index, call node | `[260, 8, 260, 11]` | S86 `impl#[T][Tagged]tag().` | yes / 1 (1) | calls-lib/src/lib.rs:255 `impl#[T][Tagged]tag().` | row | 1 |  |
| 270:9b/9u | index, call node | `[269, 8, 269, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 275:13b/13u | index, call node | `[274, 12, 274, 21]` | S87 `impl#[Aliased]via_alias().` | yes / 1 (1) | calls-lib/src/lib.rs:269 `impl#[Aliased]via_alias().` | row | 1 |  |
| 295:22b/22u | index, call node | `[294, 21, 294, 28]` | S88 `under_test/mounted().` | yes / 1 (1) | calls-lib/src/mounted.rs:3 `under_test/mounted().` | row | 1 |  |
| 295:40b/40u | index, call node | `[294, 39, 294, 44]` | S89 `plain/plain().` | yes / 1 (1) | calls-lib/src/plain.rs:3 `plain/plain().` | row | 1 |  |
| 299:6b/6u | call node only | `[298, 5, 298, 12]` | S71 `Wrapper#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 299:19b/19u | index, call node | `[298, 18, 298, 21]` | S72 `` impl#[`Wrapper<T>`]get(). `` | yes / 2 (2) | calls-lib/src/lib.rs:127 `` impl#[`Wrapper<T>`]get(). `` | row | 1 |  |
| 299:26b/26u | call node only | `[298, 25, 298, 32]` | S71 `Wrapper#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 299:40b/40u | index, call node | `[298, 39, 298, 42]` | S72 `` impl#[`Wrapper<T>`]get(). `` | yes / 2 (2) | calls-lib/src/lib.rs:127 `` impl#[`Wrapper<T>`]get(). `` | row | 1 |  |
| 304:5b/5u | index, call node | `[303, 4, 303, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 308:5b/5u | index, call node | `[307, 4, 307, 18]` | S90 `only_with_flag().` | yes / 1 (1) | calls-lib/src/lib.rs:303 `only_with_flag().` | row | 1 |  |
| 312:6b/6u | index, call node | `[311, 5, 311, 9]` | S1 `leaf().` | yes / 2 (2) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 312:14b/14u | index, call node | `[311, 13, 311, 17]` | S1 `leaf().` | yes / 2 (2) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 321:9b/9u | call node only | `[320, 8, 320, 14]` | S91 `Meters#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 321:22b/22u | index | `[320, 21, 320, 22]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 321:23b/23u | index | `[320, 22, 320, 23]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 321:24b/24u | index | `[320, 23, 320, 24]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 326:5b/5u | call node only | `[325, 4, 325, 10]` | S91 `Meters#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 326:14b/14u | index | `[325, 13, 325, 14]` | S92 `` impl#[Meters][`Add<Self>`]add(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 326:15b/15u | index | `[325, 14, 325, 15]` | S92 `` impl#[Meters][`Add<Self>`]add(). `` | yes / 3 (1) | calls-lib/src/lib.rs:320 `` impl#[Meters][`Add<Self>`]add(). `` | row | 1 |  |
| 326:16b/16u | index | `[325, 15, 325, 16]` | S92 `` impl#[Meters][`Add<Self>`]add(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 326:17b/17u | call node only | `[325, 16, 325, 22]` | S91 `Meters#` | yes / 2 (2) | - | constructor | - | a constructor: no function is called [part 2] |
| 330:5b/5u | index, call node | `[329, 4, 329, 16]` | S93 `shared_entry().` | yes / 1 (1) | calls-lib/src/lib.rs:138 `shared_entry().` | row | 1 |  |
| 335:5b/5u | index, call node | `[334, 4, 334, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | no instruction: `cfg(test)`: `only_in_test` is not compiled |
| 340:5b/5u | index, call node | `[339, 4, 339, 16]` | S94 `only_in_test().` | yes / 1 (1) | calls-lib/src/lib.rs:334 `only_in_test().` | none | - | no instruction: `cfg(test)`: `call_only_in_test` is not compiled |
| 345:5b/5u | index | `[344, 4, 344, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 4: no function or method definition holds the counted references on 345 |
| 350:5b/5u | parser only | none | - | - | lib.rs 344 `only_outside_test`, compiled in the build (`cfg(not(test))`) and absent from the index | none | - | step 3: 0 definitions named `only_outside_test` have their name token on calls-lib/src/lib.rs:344 |
| 354:6b/6u | index | `[353, 5, 353, 9]` | S1 `leaf().` | no / 1 (0) | - | not a call | - | `leaf` taken as a value |
| 354:12b/12u | index, call node | `[353, 11, 353, 17]` | S3 `direct().` | yes / 1 (1) | calls-lib/src/lib.rs:16 `direct().` | row | 1 |  |
| 357:46b/42u | index, call node | `[356, 45, 356, 49]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 360:5b/5u | index, call node | `[359, 4, 359, 16]` | S95 `after_accent().` | yes / 1 (1) | calls-lib/src/lib.rs:357 `after_accent().` | row | 1 |  |
| 364:5b/5u | call node only | `[363, 4, 363, 11]` | S71 `Wrapper#` | yes / 1 (1) | - | constructor | - | a constructor: no function is called [part 2] |
| 365:10b/10u | index, call node | `[364, 9, 364, 12]` | S72 `` impl#[`Wrapper<T>`]get(). `` | yes / 1 (1) | calls-lib/src/lib.rs:127 `` impl#[`Wrapper<T>`]get(). `` | row? [part 2] | 1 | a row if the compiler gives the call line 365, the reference's; step 4 if 364, where the expression starts [part 2] |
| 370:9b/9u | index, call node | `[369, 8, 369, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row? [part 2] | 1 | a row if the compiler gives the call line 370; step 4 if 369 (`crate::`) [part 2] |
| 390:9b/9u | index, call node | `[389, 8, 389, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `work` is under `calls_derive::retrait` |
| 400:13b/13u | index, call node | `[399, 12, 399, 16]` | S96 `impl#[Local][Task]work().` | yes / 1 (1) | calls-lib/src/lib.rs:389 `impl#[Local][Task]work().` | none | - | step 5: written: the callee `work` is under `calls_derive::retrait` |
| 403:36b/36u | index, call node | `[402, 35, 402, 39]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | no instruction: `cfg(test)`: this alternative of `alt` is not compiled |
| 403:85b/85u | index | `[402, 84, 402, 90]` | S3 `direct().` | yes / 1 (1) | calls-lib/src/lib.rs:16 `direct().` | none | - | step 4: no function or method definition holds the counted references on 403 |
| 406:5b/5u | index, call node | `[405, 4, 405, 7]` | S97 `alt().` | yes / 1 (1) | calls-lib/src/lib.rs:403 `alt().` | none | - | step 5: signature: line 403, which the callee's signature touches, holds the signature of another `fn alt` |
| 410:5b/5u | call node only | `[409, 4, 409, 5]` | S98 `local 35` | yes / 1 (1) | any function of that type passed in | none | - | indirect: no direct callee: a call through a function pointer (`indirect-call`) |
| 414:5b/5u | index, call node | `[413, 4, 413, 9]` | S99 `apply().` | yes / 1 (1) | calls-lib/src/lib.rs:409 `apply().` | row | 1 |  |
| 414:11b/11u | index | `[413, 10, 413, 14]` | S1 `leaf().` | no / 1 (0) | - | not a call | - | `leaf` passed by name; not counted |
| 420:13b/13u | index, call node | `[419, 12, 419, 23]` | S100 `shared/shared_call().` | yes / 1 (1) | calls-lib/src/shared.rs:7 `shared/shared_call().` | row | 1 |  |
| 425:5b/5u | index, call node | `[424, 4, 424, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `kept` is under `calls_derive::keep` |
| 433:9b/9u | index, call node | `[432, 8, 432, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `kept_method` is under `calls_derive::keep` |
| 438:5b/5u | index, call node | `[437, 4, 437, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 443:9b/9u | index, call node | `[442, 8, 442, 12]` | S101 `kept().` | yes / 1 (1) | calls-lib/src/lib.rs:424 `kept().` | none | - | step 5: written: the callee `kept` is under `calls_derive::keep` |
| 444:14b/14u | index, call node | `[443, 13, 443, 24]` | S102 `impl#[Kept]kept_method().` | yes / 1 (1) | calls-lib/src/lib.rs:432 `impl#[Kept]kept_method().` | none | - | step 5: written: the callee `kept_method` is under `calls_derive::keep` |
| 445:9b/9u | index, call node | `[444, 8, 444, 20]` | S103 `plain_beside().` | yes / 1 (1) | calls-lib/src/lib.rs:437 `plain_beside().` | row | 1 |  |
| 457:20b/20u | index, call node | `[456, 19, 456, 23]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `by_test` is under `test` |
| 471:9b/9u | index, call node | `[470, 8, 470, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `by_clippy` is under `clippy::skip` |
| 476:22b/22u | index, call node | `[475, 21, 475, 28]` | S104 `spelled/impl#[ByTest]by_test().` | yes / 1 (1) | calls-lib/src/lib.rs:456 `spelled/impl#[ByTest]by_test().` | none | - | step 5: written: the callee `by_test` is under `test` |
| 476:42b/42u | index, call node | `[475, 41, 475, 50]` | S105 `impl#[ByClippy]by_clippy().` | yes / 1 (1) | calls-lib/src/lib.rs:470 `impl#[ByClippy]by_clippy().` | none | - | step 5: written: the callee `by_clippy` is under `clippy::skip` |
| 481:5b/5u | index, call node | `[480, 4, 480, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 486:5b/5u | index, call node | `[485, 4, 485, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: written: the caller `maybe_kept` is under `calls_derive::keep` |
| 490:6b/6u | index, call node | `[489, 5, 489, 17]` | S106 `maybe_inline().` | yes / 1 (1) | calls-lib/src/lib.rs:480 `maybe_inline().` | row | 1 |  |
| 490:22b/22u | index, call node | `[489, 21, 489, 31]` | S107 `maybe_kept().` | yes / 1 (1) | calls-lib/src/lib.rs:485 `maybe_kept().` | none | - | step 5: written: the callee `maybe_kept` is under `calls_derive::keep` |
| 505:9b/9u | call node only | none | - | - | a closure's body | none | - | closure: step 3: the callee is the closure's body |
| 505:13b/13u | call node only | none | - | - | - | same call | - | the same call as the reference at 505:14: `(leaf)()` has no callee token |
| 505:14b/14u | index | `[504, 13, 504, 17]` | S1 `leaf().` | no / 2 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: the innermost written item at the site is `fn go` (504), not the caller |
| 505:76b/76u | call node only | none | - | - | a closure's body | none | - | closure: step 3: the callee is the closure's body |
| 505:80b/80u | index, call node | `[504, 79, 504, 83]` | S1 `leaf().` | yes / 2 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: the closure's line 505 is also touched by `fn go` (504), which does not enclose the caller |
| 512:9b/9u | call node only | none | - | - | a closure's body | none | - | closure: step 3: the callee is the closure's body |
| 512:13b/13u | index, call node | `[511, 12, 511, 16]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 518:9b/9u | call node only | none | - | - | a closure's body | none | - | closure: step 3: the callee is the closure's body |
| 518:13b/13u | index, call node | `[517, 12, 517, 16]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 526:9b/9u | call node only | none | - | - | - | same call | - | the same call as the reference at 526:10: `(leaf)()` has no callee token |
| 526:10b/10u | index | `[525, 9, 525, 13]` | S1 `leaf().` | no / 2 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: the innermost written item at the site is `fn go` (525), not the caller |
| 526:69b/69u | index, call node | `[525, 68, 525, 72]` | S1 `leaf().` | yes / 2 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 531:16b/16u | index, call node | `[530, 15, 530, 17]` | S108 `impl#[Shared][First]go().` | yes / 1 (1) | calls-lib/src/lib.rs:504 `impl#[Shared][First]go().` | row | 1 |  |
| 532:17b/17u | index, call node | `[531, 16, 531, 18]` | S109 `impl#[Shared][Second]go().` | yes / 1 (1) | calls-lib/src/lib.rs:505 `impl#[Shared][Second]go().` | row | 1 |  |
| 533:16b/16u | index, call node | `[532, 15, 532, 17]` | S110 `impl#[Apart][First]go().` | yes / 1 (1) | calls-lib/src/lib.rs:511 `impl#[Apart][First]go().` | row | 1 |  |
| 534:17b/17u | index, call node | `[533, 16, 533, 18]` | S111 `impl#[Apart][Second]go().` | yes / 1 (1) | calls-lib/src/lib.rs:517 `impl#[Apart][Second]go().` | row | 1 |  |
| 535:16b/16u | index, call node | `[534, 15, 534, 17]` | S112 `impl#[Joined][First]go().` | yes / 1 (1) | calls-lib/src/lib.rs:525 `impl#[Joined][First]go().` | row | 1 |  |
| 536:17b/17u | index, call node | `[535, 16, 535, 18]` | S113 `impl#[Joined][Second]go().` | yes / 1 (1) | calls-lib/src/lib.rs:526 `impl#[Joined][Second]go().` | row | 1 |  |
| 546:33b/33u | index, call node | `[545, 32, 545, 38]` | S114 `helper().` | yes / 1 (1) | calls-lib/src/lib.rs:549 `helper().` | none | - | step 5: written: the caller `work` is under `calls_derive::copy_to` |
| 550:5b/5u | index, call node | `[549, 4, 549, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 554:15b/15u | index, call node | `[553, 14, 553, 18]` | S115 `impl#[Original]work().` | yes / 1 (1) | calls-lib/src/lib.rs:546 `impl#[Original]work().` | none | - | step 4: 1 counted reference(s) on 554 against 2 instructions of one emitted function calling it |
| 554:30b/30u | index, call node | `[553, 29, 553, 33]` | S116 `impl#[Copied]work().` | yes / 1 (1) | the generated `Copied::work`, a copy with every token at its written position (546) | none | - | step 4: 1 counted reference(s) on 554 against 2 instructions of one emitted function calling it |
| 559:5b/5u | index, call node | `[558, 4, 558, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 564:5b/5u | index, call node | `[563, 4, 563, 8]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 579:11b/11u | index | `[578, 10, 578, 14]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 584:9b/9u | call node only | `[583, 8, 583, 16]` | S117 `local 48` | yes / 1 (1) | a closure's body | none | - | closure: step 3: the callee is the closure's body |
| 586:17b/17u | index, call node | `[585, 16, 585, 21]` | S118 `inner().` | yes / 1 (1) | calls-lib/src/lib.rs:580 `inner().` | row | 1 |  |
| 587:10b/10u | index | `[586, 9, 586, 10]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 587:11b/11u | index | `[586, 10, 586, 11]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 587:12b/12u | index | `[586, 11, 586, 12]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 587:13b/13u | index, call node | `[586, 12, 586, 18]` | S3 `direct().` | yes / 1 (1) | calls-lib/src/lib.rs:16 `direct().` | row | 1 |  |
| 594:13b/13u | parser only | none | - | - | a closure's body | none | - | closure: step 3: the callee is the body of the closure in the nested `run`, which has no definition |
| 594:24b/24u | index | `[593, 23, 593, 27]` | S1 `leaf().` | yes / 2 (2) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: the innermost written item at the site is `fn run` (592), not the caller |
| 594:55b/55u | index, call node | `[593, 54, 593, 58]` | S1 `leaf().` | yes / 2 (2) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 595:34b/34u | index | `[594, 33, 594, 36]` | S119 `nested_run/run().` | no / 1 (0) | - | not a call | - | the outer `run` taken as a value |
| 596:14b/14u | index | `[595, 13, 595, 14]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 596:15b/15u | index | `[595, 14, 595, 15]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 596:16b/16u | index | `[595, 15, 595, 16]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 596:17b/17u | call node only | `[595, 16, 595, 21]` | S120 `local 51` | yes / 1 (1) | any function of that type passed in | none | - | indirect: no direct callee: a call through a function pointer |
| 604:48b/48u | index | `[603, 47, 603, 51]` | S1 `leaf().` | yes / 2 (2) | calls-lib/src/lib.rs:12 `leaf().` | none | - | corrected reason (the outcome was already none): step 4: the line's two references to `leaf` lie in `nested` and in the outer `run` |
| 604:56b/56u | call node only | `[603, 55, 603, 58]` | S121 `const_run/run.` | yes / 1 (1) | any function of that type passed in | none | - | indirect: no direct callee: the constant `run` is a function pointer |
| 604:83b/83u | index, call node | `[603, 82, 603, 86]` | S1 `leaf().` | yes / 2 (2) | calls-lib/src/lib.rs:12 `leaf().` | none | - | corrected: step 4: the line's two references to `leaf` lie in `nested` and in the outer `run`, so neither instruction has a row |
| 605:34b/34u | index | `[604, 33, 604, 39]` | S122 `const_run/nested().` | no / 1 (0) | - | not a call | - | `nested` taken as a value |
| 606:14b/14u | index | `[605, 13, 605, 14]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 606:15b/15u | index | `[605, 14, 605, 15]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 3 (1) | outside the project | none | - | step 2, no code: outside the project |
| 606:16b/16u | index | `[605, 15, 605, 16]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 3 (1) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 606:17b/17u | call node only | `[605, 16, 605, 21]` | S123 `local 53` | yes / 1 (1) | any function of that type passed in | none | - | indirect: no direct callee: a call through a function pointer |
| 614:28b/28u | index, call node | `[613, 27, 613, 31]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: the site's line 614 is taken (macro_rules! definition) |
| 615:29b/29u | index, call node | `[614, 28, 614, 32]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 618:17b/17u | call node only | none | - | - | a closure's body | none | - | closure: step 3: the callee is the closure's body |
| 621:14b/14u | index | `[620, 13, 620, 14]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 6 (2) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 621:15b/15u | index | `[620, 14, 620, 15]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 6 (2) | outside the project | none | - | step 2, no code: outside the project |
| 621:16b/16u | index | `[620, 15, 620, 16]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 6 (2) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 621:23b/23u | index | `[620, 22, 620, 23]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 6 (2) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 621:24b/24u | index | `[620, 23, 620, 24]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 6 (2) | outside the project | none | - | step 2, no code: outside the project |
| 621:25b/25u | index | `[620, 24, 620, 25]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 6 (2) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 621:33b/33u | index, call node | `[620, 32, 620, 35]` | S124 `templated/inner/run().` | yes / 1 (1) | calls-lib/src/lib.rs:617 `templated/inner/run().` | row | 1 |  |
| 627:45b/45u | index, call node | `[626, 44, 626, 48]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: the site's line 627 is taken (struct under derive) |
| 628:18b/18u | index, call node | `[627, 17, 627, 21]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 629:10b/10u | index | `[628, 9, 628, 10]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 6 (2) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 629:11b/11u | index | `[628, 10, 628, 11]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 6 (2) | outside the project | none | - | step 2, no code: outside the project |
| 629:12b/12u | index | `[628, 11, 628, 12]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 6 (2) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 629:19b/19u | index | `[628, 18, 628, 19]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 6 (2) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 629:20b/20u | index | `[628, 19, 628, 20]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | yes / 6 (2) | outside the project | none | - | step 2, no code: outside the project |
| 629:21b/21u | index | `[628, 20, 628, 21]` | S56 `` ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | no / 6 (2) | - | same call | - | the white space beside an operator: the index gives one operator three one-character references (space, operator, space); the operator's own is the site |
| 634:87b/87u | index, call node | `[633, 86, 633, 90]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: signature: line 634, which the caller's signature touches, is taken (struct under derive) |
| 640:58b/58u | index, call node | `[639, 57, 639, 61]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | none | - | step 5: signature: line 640, which the caller's signature touches, is taken (struct under derive) |
| 646:9b/9u | index, call node | `[645, 8, 645, 12]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| 662:20b/20u | index, call node | `[661, 19, 661, 24]` | S125 `impl#[Panel]label().` | yes / 1 (1) | in the build `PickPlate` is `Plate`: the derive's `Plate::label`, at the span of `Plate`'s `;` (634) | none | - | step 5: signature: line 634, which the callee's signature touches, is taken (struct under derive) |
| 663:20b/20u | index, call node | `[662, 19, 662, 24]` | S126 `impl#[Sheet]label().` | yes / 1 (1) | in the build `PickBoard` is `Board`: the derive's `Board::label`, at the span of `Board`'s closing brace (640) | none | - | step 5: signature: line 640, which the callee's signature touches, is taken (struct under derive) |
| 664:15b/15u | index, call node | `[663, 14, 663, 19]` | S127 `impl#[Card]label().` | yes / 1 (1) | calls-lib/src/lib.rs:645 `impl#[Card]label().` | row | 1 |  |
| **calls-lib/src/mounted.rs** | | | | | | | | |
| 4:12b/12u | index, call node | `[3, 11, 3, 15]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| **calls-lib/src/plain.rs** | | | | | | | | |
| 4:12b/12u | index, call node | `[3, 11, 3, 15]` | S1 `leaf().` | yes / 1 (1) | calls-lib/src/lib.rs:12 `leaf().` | row | 1 |  |
| **calls-lib/src/shared.rs** | | | | | | | | |
| 8:5b/5u | index, call node | `[7, 4, 7, 15]` | S128 `shared/shared_leaf().` | yes / 1 (1) | calls-lib/src/shared.rs:3 `shared/shared_leaf().` | row | 1 |  |
| **calls-lib/tests/mount.rs** | | | | | | | | |
| 8:24b/24u | index | `[7, 23, 7, 34]` | S100 `shared/shared_call().` | yes / 1 (1) | calls-lib/src/shared.rs:7 `shared/shared_call().` | none | - | no instruction: an integration test is not a selected target; not built |

## Calls written by macros

These instructions have no reference of their own. Their site is a position a macro gave
them: the written `fn` the attribute macro respans to, the token the derive respans to, or
the template token. Each was evaluated like a site.

| Instruction | Site (line:col b) | Callee | Expected | Why |
| --- | --- | --- | --- | --- |
| `EngineProxy::run` calls `crate::proxy_helper()` | 203:9 | lib.rs 212 `proxy_helper` | none | step 4: no reference to `proxy_helper().` written as a call on calls-lib/src/lib.rs:203 |
| `GenProxy::run` calls `crate::proxy_helper()` | 228:9 | lib.rs 212 `proxy_helper` | none | step 4: no reference to `proxy_helper().` written as a call on calls-lib/src/lib.rs:228 |
| `Slot<u16>::run` calls `crate::proxy_helper()` | 241:9 | lib.rs 212 `proxy_helper` | none | step 4: no reference to `proxy_helper().` written as a call on calls-lib/src/lib.rs:241 |
| `<Local as b::Task>::work` calls `crate::proxy_helper()` | 389:5 | lib.rs 212 `proxy_helper` | none | step 4: no reference to `proxy_helper().` written as a call on calls-lib/src/lib.rs:389 |
| `Plate::label` (derive) calls `crate::proxy_helper()` | 634:48 | lib.rs 212 `proxy_helper` | none | step 4: no reference to `proxy_helper().` written as a call on calls-lib/src/lib.rs:634 |
| `Board::label` (derive) calls `crate::proxy_helper()` | 640:19 | lib.rs 212 `proxy_helper` | none | step 4: no reference to `proxy_helper().` written as a call on calls-lib/src/lib.rs:640 |
| `Copied::work` calls `helper()` at the written token | 546:33 | lib.rs 549 `helper` | none | step 5: written: the caller `work` is under `calls_derive::copy_to` |
| `made_by_macro::made` calls `$crate::leaf()` from the template (105) [part 2] | 105:21 | lib.rs 12 `leaf` | none | step 4: no reference to `leaf().` written as a call on calls-lib/src/lib.rs:105 |
| the closure of `adder!` calls `$crate::leaf()` from the template (188) | 188:21 | lib.rs 12 `leaf` | none | step 4: no reference to `leaf().` written as a call on calls-lib/src/lib.rs:188 |
| the closure of `closure_of!`, in `templated::inner::run`, calls `crate::leaf()` from the template (614) | 614:80 | lib.rs 12 `leaf` | none | step 5: the site's line 614 is taken (macro_rules! definition) |
| the closure in `saver::inner` calls the call that `save!` saved as `saved!`; its site is `leaf()` in `save!(leaf())` (579) | 579:11 | lib.rs 12 `leaf` | none | step 5: the closure's line 581 is also touched by `fn inner` (580), which does not enclose the caller |

If the compiler places the `$crate::leaf()` of `make_fn!(made)` at the invocation (110)
instead of the template (105), step 5 stops it instead: the site lies in `made_by_macro`,
outside `real::made` [part 2].

## Relations expected if the compiler emits the calls

Each evidence entry is one instruction. A generic caller is emitted once per instantiation,
and step 4 counts per emitted function:
- `twice -> leaf` has 2 entries at one site, from `twice` at `u8` and at `char`;
- `Wrapper<T>::get -> leaf` has 2 entries, from `get` at `u8` and at `i16`.

Ends are given as the file, line and column (1-based, bytes) of the name token.

| Caller (file:line:col) | Callee (file:line:col) | Evidence entries | Sites (`?` = [part 2], `xN` = N entries) |
| --- | --- | --- | --- |
| `main().` calls-app/src/bin/twin.rs:5:4 | `helper().` calls-app/src/bin/twin.rs:1:4 | 1 | 6:13 |
| `main().` calls-app/src/main.rs:11:4 | `helper().` calls-app/src/main.rs:7:4 | 1 | 17:13 |
| `main().` calls-app/src/main.rs:11:4 | `generic().` calls-lib/src/lib.rs:20:8 | 1 | 14:24 |
| `main().` calls-app/src/main.rs:11:4 | `hinted().` calls-lib/src/lib.rs:558:8 | 1 | 15:24? |
| `impl_parts().` calls-derive/src/lib.rs:36:4 | `fn_span().` calls-derive/src/lib.rs:53:4 | 1 | 46:80 |
| `respan().` calls-derive/src/lib.rs:64:4 | `respan().` calls-derive/src/lib.rs:64:4 | 1 | 68:59 |
| `drops/impl#[Guard][Drop]drop().` calls-lib/src/drops.rs:6:8 | `drops/helpers/drop().` calls-lib/src/drops.rs:36:12 | 1 | 6:35 |
| `drops/drop_lookalike().` calls-lib/src/drops.rs:76:8 | `mem/drop().` fake-core/src/lib.rs:4:12 | 1 | 77:21 |
| `direct().` calls-lib/src/lib.rs:16:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 17:5 |
| `two_types().` calls-lib/src/lib.rs:24:8 | `generic().` calls-lib/src/lib.rs:20:8 | 2 | 25:6, 25:21 |
| `twice().` calls-lib/src/lib.rs:28:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 2 | 29:5 x2 |
| `use_twice().` calls-lib/src/lib.rs:33:8 | `twice().` calls-lib/src/lib.rs:28:8 | 2 | 34:6, 34:18 |
| `make_adder().` calls-lib/src/lib.rs:37:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 39:9 |
| `use_adder().` calls-lib/src/lib.rs:44:8 | `make_adder().` calls-lib/src/lib.rs:37:8 | 1 | 45:5 |
| `after_await().` calls-lib/src/lib.rs:52:14 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 54:5 |
| `after_await().` calls-lib/src/lib.rs:52:14 | `ready().` calls-lib/src/lib.rs:48:14 | 1 | 53:13 |
| `run_after_await().` calls-lib/src/lib.rs:57:8 | `after_await().` calls-lib/src/lib.rs:52:14 | 1 | 58:27 |
| `always().` calls-lib/src/lib.rs:67:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 68:5? |
| `calls_always().` calls-lib/src/lib.rs:71:8 | `always().` calls-lib/src/lib.rs:67:8 | 1 | 72:5? |
| `impl#[Square][Shape]area().` calls-lib/src/lib.rs:82:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 83:9 |
| `use_dyn().` calls-lib/src/lib.rs:91:8 | `dyn_call().` calls-lib/src/lib.rs:87:8 | 1 | 92:5 |
| `fill().` calls-lib/src/lib.rs:116:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 117:6 |
| `use_fill().` calls-lib/src/lib.rs:120:8 | `fill().` calls-lib/src/lib.rs:116:8 | 1 | 121:5 |
| `` impl#[`Wrapper<T>`]get(). `` calls-lib/src/lib.rs:127:12 | `leaf().` calls-lib/src/lib.rs:12:8 | 2 | 128:9 x2 |
| `use_wrapper().` calls-lib/src/lib.rs:133:8 | `` impl#[`Wrapper<T>`]get(). `` calls-lib/src/lib.rs:127:12 | 1 | 134:18 |
| `shared_entry().` calls-lib/src/lib.rs:138:19 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 139:5 |
| `split().` calls-lib/src/lib.rs:143:1 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 144:5 |
| `accented().` calls-lib/src/lib.rs:151:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 151:54 |
| `` impl#[`Wrapper<i16>`]concrete(). `` calls-lib/src/lib.rs:154:12 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 155:9 |
| `use_impls().` calls-lib/src/lib.rs:159:8 | `` impl#[`Wrapper<T>`]get(). `` calls-lib/src/lib.rs:127:12 | 1 | 160:46 |
| `use_impls().` calls-lib/src/lib.rs:159:8 | `` impl#[`Wrapper<i16>`]concrete(). `` calls-lib/src/lib.rs:154:12 | 1 | 160:20 |
| `Greeter#greet().` calls-lib/src/lib.rs:164:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 165:9 |
| `use_greet().` calls-lib/src/lib.rs:173:8 | `Greeter#greet().` calls-lib/src/lib.rs:164:8 | 1 | 174:11 |
| `impl#[T][Tagged]tag().` calls-lib/src/lib.rs:255:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 256:9 |
| `use_blanket().` calls-lib/src/lib.rs:260:8 | `impl#[T][Tagged]tag().` calls-lib/src/lib.rs:255:8 | 1 | 261:9 |
| `impl#[Aliased]via_alias().` calls-lib/src/lib.rs:269:12 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 270:9 |
| `use_alias().` calls-lib/src/lib.rs:274:8 | `impl#[Aliased]via_alias().` calls-lib/src/lib.rs:269:12 | 1 | 275:13 |
| `use_mounted().` calls-lib/src/lib.rs:294:8 | `under_test/mounted().` calls-lib/src/mounted.rs:3:8 | 1 | 295:22 |
| `use_mounted().` calls-lib/src/lib.rs:294:8 | `plain/plain().` calls-lib/src/plain.rs:3:8 | 1 | 295:40 |
| `get_two().` calls-lib/src/lib.rs:298:8 | `` impl#[`Wrapper<T>`]get(). `` calls-lib/src/lib.rs:127:12 | 2 | 299:19, 299:40 |
| `only_with_flag().` calls-lib/src/lib.rs:303:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 304:5 |
| `use_flag().` calls-lib/src/lib.rs:307:8 | `only_with_flag().` calls-lib/src/lib.rs:303:8 | 1 | 308:5 |
| `two_on_one_line().` calls-lib/src/lib.rs:311:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 2 | 312:6, 312:14 |
| `use_operator().` calls-lib/src/lib.rs:325:8 | `` impl#[Meters][`Add<Self>`]add(). `` calls-lib/src/lib.rs:320:8 | 1 | 326:15 |
| `use_shared_entry().` calls-lib/src/lib.rs:329:8 | `shared_entry().` calls-lib/src/lib.rs:138:19 | 1 | 330:5 |
| `value_and_call().` calls-lib/src/lib.rs:353:8 | `direct().` calls-lib/src/lib.rs:16:8 | 1 | 354:12 |
| `after_accent().` calls-lib/src/lib.rs:357:22 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 357:46 |
| `use_after_accent().` calls-lib/src/lib.rs:359:8 | `after_accent().` calls-lib/src/lib.rs:357:22 | 1 | 360:5 |
| `chained().` calls-lib/src/lib.rs:363:8 | `` impl#[`Wrapper<T>`]get(). `` calls-lib/src/lib.rs:127:12 | 1 | 365:10? |
| `split_path().` calls-lib/src/lib.rs:368:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 370:9? |
| `pass_by_name().` calls-lib/src/lib.rs:413:8 | `apply().` calls-lib/src/lib.rs:409:8 | 1 | 414:5 |
| `use_shared().` calls-lib/src/lib.rs:419:8 | `shared/shared_call().` calls-lib/src/shared.rs:7:8 | 1 | 420:13 |
| `plain_beside().` calls-lib/src/lib.rs:437:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 438:5 |
| `use_kept().` calls-lib/src/lib.rs:441:8 | `plain_beside().` calls-lib/src/lib.rs:437:8 | 1 | 445:9 |
| `maybe_inline().` calls-lib/src/lib.rs:480:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 481:5 |
| `use_cfg_attr().` calls-lib/src/lib.rs:489:8 | `maybe_inline().` calls-lib/src/lib.rs:480:8 | 1 | 490:6 |
| `impl#[Apart][First]go().` calls-lib/src/lib.rs:511:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 512:13 |
| `impl#[Apart][Second]go().` calls-lib/src/lib.rs:517:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 518:13 |
| `impl#[Joined][Second]go().` calls-lib/src/lib.rs:526:50 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 526:69 |
| `use_go().` calls-lib/src/lib.rs:529:8 | `impl#[Shared][First]go().` calls-lib/src/lib.rs:504:8 | 1 | 531:16 |
| `use_go().` calls-lib/src/lib.rs:529:8 | `impl#[Shared][Second]go().` calls-lib/src/lib.rs:505:57 | 1 | 532:17 |
| `use_go().` calls-lib/src/lib.rs:529:8 | `impl#[Apart][First]go().` calls-lib/src/lib.rs:511:8 | 1 | 533:16 |
| `use_go().` calls-lib/src/lib.rs:529:8 | `impl#[Apart][Second]go().` calls-lib/src/lib.rs:517:8 | 1 | 534:17 |
| `use_go().` calls-lib/src/lib.rs:529:8 | `impl#[Joined][First]go().` calls-lib/src/lib.rs:525:8 | 1 | 535:16 |
| `use_go().` calls-lib/src/lib.rs:529:8 | `impl#[Joined][Second]go().` calls-lib/src/lib.rs:526:50 | 1 | 536:17 |
| `helper().` calls-lib/src/lib.rs:549:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 550:5 |
| `hinted().` calls-lib/src/lib.rs:558:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 559:5 |
| `read_value().` calls-lib/src/lib.rs:563:19 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 564:5 |
| `saver().` calls-lib/src/lib.rs:578:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 579:11 |
| `saver().` calls-lib/src/lib.rs:578:8 | `direct().` calls-lib/src/lib.rs:16:8 | 1 | 587:13 |
| `saver().` calls-lib/src/lib.rs:578:8 | `inner().` calls-lib/src/lib.rs:580:8 | 1 | 586:17 |
| `nested_run/run().` calls-lib/src/lib.rs:591:12 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 594:55 |
| `templated/run().` calls-lib/src/lib.rs:613:12 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 615:29 |
| `templated/run().` calls-lib/src/lib.rs:613:12 | `templated/inner/run().` calls-lib/src/lib.rs:617:20 | 1 | 621:33 |
| `local_derive().` calls-lib/src/lib.rs:625:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 628:18 |
| `impl#[Card]label().` calls-lib/src/lib.rs:645:12 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 646:9 |
| `use_labels().` calls-lib/src/lib.rs:660:8 | `impl#[Card]label().` calls-lib/src/lib.rs:645:12 | 1 | 664:15 |
| `under_test/mounted().` calls-lib/src/mounted.rs:3:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 4:12 |
| `plain/plain().` calls-lib/src/plain.rs:3:8 | `leaf().` calls-lib/src/lib.rs:12:8 | 1 | 4:12 |
| `shared/shared_call().` calls-lib/src/shared.rs:7:8 | `shared/shared_leaf().` calls-lib/src/shared.rs:3:8 | 1 | 8:5 |

## Symbols

Every symbol a site refers to, in full, with its kind as the index gives it. The
definitions are counted by place.

| Id | SCIP symbol | Kind | Definitions (file:line:col of the name token) | Collision group in the snapshot |
| --- | --- | --- | --- | --- |
| S1 | `rust-analyzer cargo calls-lib 0.1.0 leaf().` | 17 | calls-lib/src/lib.rs:12:8 |  |
| S2 | `rust-analyzer cargo calls-app 0.1.0 helper().` | 17 | calls-app/src/bin/twin.rs:1:4, calls-app/src/main.rs:7:4 | `cargo:calls-app:helper().`: `cargo:calls-app:helper().~1` (calls-app/src/bin/twin.rs:1:4), `cargo:calls-app:helper().~2` (calls-app/src/main.rs:7:4) |
| S3 | `rust-analyzer cargo calls-lib 0.1.0 direct().` | 17 | calls-lib/src/lib.rs:16:8 |  |
| S4 | `rust-analyzer cargo calls-app 0.1.0 shared_entry().` | 17 | calls-app/src/main.rs:2:13 |  |
| S5 | `rust-analyzer cargo calls-lib 0.1.0 generic().` | 17 | calls-lib/src/lib.rs:20:8 |  |
| S6 | `rust-analyzer cargo calls-lib 0.1.0 hinted().` | 17 | calls-lib/src/lib.rs:558:8 |  |
| S7 | `rust-analyzer cargo calls-app 0.1.0 _Read().` | 17 | calls-app/src/main.rs:4:13 |  |
| S8 | `rust-analyzer cargo calls-derive 0.1.0 struct_name().` | 17 | calls-derive/src/lib.rs:11:4 |  |
| S9 | `rust-analyzer cargo core https://github.com/rust-lang/rust/library/core str/impl#[str]parse().` | - | none |  |
| S10 | `` rust-analyzer cargo core https://github.com/rust-lang/rust/library/core result/impl#[`Result<T, E>`]unwrap(). `` | - | none |  |
| S11 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro token_stream/impl#[TokenStream][IntoIterator]into_iter().` | - | none |  |
| S12 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro token_stream/impl#[IntoIter][Iterator]next().` | - | none |  |
| S13 | `rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc string/impl#[T][ToString]to_string().` | - | none |  |
| S14 | `` rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc string/impl#[String][`PartialEq<&str>`]eq(). `` | - | none |  |
| S15 | `rust-analyzer cargo calls-derive 0.1.0 impl_parts().` | 17 | calls-derive/src/lib.rs:36:4 |  |
| S16 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[TokenStream][Clone]clone().` | - | none |  |
| S17 | `rust-analyzer cargo calls-derive 0.1.0 respan().` | 17 | calls-derive/src/lib.rs:64:4 |  |
| S18 | `rust-analyzer cargo core https://github.com/rust-lang/rust/library/core iter/traits/iterator/Iterator#chain().` | - | none |  |
| S19 | `rust-analyzer cargo core https://github.com/rust-lang/rust/library/core iter/traits/iterator/Iterator#collect().` | - | none |  |
| S20 | `rust-analyzer cargo core https://github.com/rust-lang/rust/library/core option/Option#Some#` | - | none |  |
| S21 | `` rust-analyzer cargo core https://github.com/rust-lang/rust/library/core option/impl#[`Option<T>`]expect(). `` | - | none |  |
| S22 | `rust-analyzer cargo calls-derive 0.1.0 fn_span().` | 17 | calls-derive/src/lib.rs:53:4 |  |
| S23 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[Group]stream().` | - | none |  |
| S24 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[Ident]span().` | - | none |  |
| S25 | `` rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc vec/impl#[`Vec<T>`]new(). `` | - | none |  |
| S26 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[Group]new().` | - | none |  |
| S27 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[Group]delimiter().` | - | none |  |
| S28 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[Group]set_span().` | - | none |  |
| S29 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro TokenTree#Group#` | - | none |  |
| S30 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[TokenTree]set_span().` | - | none |  |
| S31 | `` rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc vec/impl#[`Vec<T, A>`]push(). `` | - | none |  |
| S32 | `` rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc vec/impl#[`Vec<T, A>`][IntoIterator]into_iter(). `` | - | none |  |
| S33 | `rust-analyzer cargo calls-derive 0.1.0 trait_impl_parts().` | 17 | calls-derive/src/lib.rs:106:4 |  |
| S34 | `` rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc vec/impl#[`Vec<T, A>`][`Extend<T>`]extend(). `` | - | none |  |
| S35 | `rust-analyzer cargo core https://github.com/rust-lang/rust/library/core iter/traits/iterator/Iterator#last().` | - | none |  |
| S36 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[Group]span_close().` | - | none |  |
| S37 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[TokenTree]span().` | - | none |  |
| S38 | `rust-analyzer cargo proc_macro https://github.com/rust-lang/rust/library/proc_macro impl#[Span]call_site().` | - | none |  |
| S39 | `rust-analyzer cargo calls-lib 0.1.0 bom/bom_leaf().` | 17 | calls-lib/src/bom.rs:3:8 |  |
| S40 | `rust-analyzer cargo calls-lib 0.1.0 drops/helpers/drop().` | 17 | calls-lib/src/drops.rs:36:12 |  |
| S41 | `rust-analyzer cargo calls-lib 0.1.0 drops/Guard#` | 49 | calls-lib/src/drops.rs:3:12 |  |
| S42 | `rust-analyzer cargo core https://github.com/rust-lang/rust/library/core mem/drop().` | - | none |  |
| S43 | `rust-analyzer cargo calls-lib 0.1.0 drops/Outer#` | 49 | calls-lib/src/drops.rs:47:12 |  |
| S44 | `rust-analyzer cargo calls-lib 0.1.0 drops/Token#` | 49 | calls-lib/src/drops.rs:62:12, calls-lib/src/drops.rs:70:16 | `cargo:calls-lib:drops/Token#`: `cargo:calls-lib:drops/Token#~1` (calls-lib/src/drops.rs:62:12), `cargo:calls-lib:drops/Token#~2` (calls-lib/src/drops.rs:70:16) |
| S45 | `rust-analyzer cargo fake-core 0.1.0 mem/drop().` | 17 | fake-core/src/lib.rs:4:12 |  |
| S46 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_at_end().` | 17 | calls-lib/src/drops.rs:23:8 |  |
| S47 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_holder().` | 17 | calls-lib/src/drops.rs:27:8 |  |
| S48 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_none().` | 17 | calls-lib/src/drops.rs:39:8 |  |
| S49 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_now().` | 17 | calls-lib/src/drops.rs:18:8 |  |
| S50 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_outer().` | 17 | calls-lib/src/drops.rs:57:8 |  |
| S51 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_lookalike().` | 17 | calls-lib/src/drops.rs:76:8 |  |
| S52 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_pair().` | 17 | calls-lib/src/drops.rs:31:8 |  |
| S53 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_shadowed().` | 17 | calls-lib/src/drops.rs:61:8 |  |
| S54 | `rust-analyzer cargo calls-lib 0.1.0 drops/drop_some().` | 17 | calls-lib/src/drops.rs:43:8 |  |
| S55 | `rust-analyzer cargo calls-lib 0.1.0 twice().` | 17 | calls-lib/src/lib.rs:28:8 |  |
| S56 | `` rust-analyzer cargo core https://github.com/rust-lang/rust/library/core ops/bit/impl#[u32][`BitXor<Self>`]bitxor(). `` | - | none |  |
| S57 | `rust-analyzer cargo calls-lib 0.1.0 make_adder().` | 17 | calls-lib/src/lib.rs:37:8 |  |
| S58 | `rust-analyzer cargo calls-lib 0.1.0 ready().` | 17 | calls-lib/src/lib.rs:48:14 |  |
| S59 | `rust-analyzer cargo core https://github.com/rust-lang/rust/library/core future/future/Future#poll().` | - | none |  |
| S60 | `rust-analyzer cargo calls-lib 0.1.0 after_await().` | 17 | calls-lib/src/lib.rs:52:14 |  |
| S61 | `rust-analyzer cargo core https://github.com/rust-lang/rust/library/core task/wake/impl#[Waker]noop().` | - | none |  |
| S62 | `` rust-analyzer cargo core https://github.com/rust-lang/rust/library/core pin/impl#[`Pin<Ptr>`]as_mut(). `` | - | none |  |
| S63 | `` rust-analyzer cargo core https://github.com/rust-lang/rust/library/core task/wake/impl#[`Context<'a>`]from_waker(). `` | - | none |  |
| S64 | `rust-analyzer cargo calls-lib 0.1.0 always().` | 17 | calls-lib/src/lib.rs:67:8 |  |
| S65 | `rust-analyzer cargo calls-lib 0.1.0 Shape#area().` | 70 | calls-lib/src/lib.rs:76:8 |  |
| S66 | `rust-analyzer cargo calls-lib 0.1.0 dyn_call().` | 17 | calls-lib/src/lib.rs:87:8 |  |
| S67 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Named]describe().` | 80 | none |  |
| S68 | `rust-analyzer cargo calls-lib 0.1.0 real/made().` | 17 | calls-lib/src/lib.rs:110:23 |  |
| S69 | `rust-analyzer cargo calls-lib 0.1.0 made_by_macro/made().` | 17 | calls-lib/src/lib.rs:110:90 |  |
| S70 | `rust-analyzer cargo calls-lib 0.1.0 fill().` | 17 | calls-lib/src/lib.rs:116:8 |  |
| S71 | `rust-analyzer cargo calls-lib 0.1.0 Wrapper#` | 49 | calls-lib/src/lib.rs:124:12 |  |
| S72 | `` rust-analyzer cargo calls-lib 0.1.0 impl#[`Wrapper<T>`]get(). `` | 26 | calls-lib/src/lib.rs:127:12 |  |
| S73 | `rust-analyzer cargo calls-lib 0.1.0 split().` | 17 | calls-lib/src/lib.rs:143:1 |  |
| S74 | `` rust-analyzer cargo calls-lib 0.1.0 impl#[`Wrapper<i16>`]concrete(). `` | 26 | calls-lib/src/lib.rs:154:12 |  |
| S75 | `rust-analyzer cargo calls-lib 0.1.0 Greeter#greet().` | 26 | calls-lib/src/lib.rs:164:8 |  |
| S76 | `rust-analyzer cargo calls-lib 0.1.0 first/same().` | 17 | calls-lib/src/lib.rs:177:24 |  |
| S77 | `rust-analyzer cargo calls-lib 0.1.0 second/same().` | 17 | calls-lib/src/lib.rs:178:1 |  |
| S78 | `local 19` | 61 | none |  |
| S79 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Engine]run().` | 26 | calls-lib/src/lib.rs:203:12 |  |
| S80 | `rust-analyzer cargo calls-lib 0.1.0 impl#[EngineProxy]run().` | 26 | none |  |
| S81 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Engine]beside().` | 26 | calls-lib/src/lib.rs:207:12 |  |
| S82 | `rust-analyzer cargo calls-lib 0.1.0 impl#[GenProxy]run().` | 26 | none |  |
| S83 | `rust-analyzer cargo calls-lib 0.1.0 Slot#` | 49 | calls-lib/src/lib.rs:237:12 |  |
| S84 | `` rust-analyzer cargo calls-lib 0.1.0 impl#[`Slot<u8>`]run(). `` | 26 | calls-lib/src/lib.rs:241:12 |  |
| S85 | `` rust-analyzer cargo calls-lib 0.1.0 impl#[`Slot<u16>`]run(). `` | 26 | none |  |
| S86 | `rust-analyzer cargo calls-lib 0.1.0 impl#[T][Tagged]tag().` | 26 | calls-lib/src/lib.rs:255:8 |  |
| S87 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Aliased]via_alias().` | 26 | calls-lib/src/lib.rs:269:12 |  |
| S88 | `rust-analyzer cargo calls-lib 0.1.0 under_test/mounted().` | 17 | calls-lib/src/mounted.rs:3:8 |  |
| S89 | `rust-analyzer cargo calls-lib 0.1.0 plain/plain().` | 17 | calls-lib/src/plain.rs:3:8 |  |
| S90 | `rust-analyzer cargo calls-lib 0.1.0 only_with_flag().` | 17 | calls-lib/src/lib.rs:303:8 |  |
| S91 | `rust-analyzer cargo calls-lib 0.1.0 Meters#` | 49 | calls-lib/src/lib.rs:315:12 |  |
| S92 | `` rust-analyzer cargo calls-lib 0.1.0 impl#[Meters][`Add<Self>`]add(). `` | 26 | calls-lib/src/lib.rs:320:8 |  |
| S93 | `rust-analyzer cargo calls-lib 0.1.0 shared_entry().` | 17 | calls-lib/src/lib.rs:138:19 |  |
| S94 | `rust-analyzer cargo calls-lib 0.1.0 only_in_test().` | 17 | calls-lib/src/lib.rs:334:8 |  |
| S95 | `rust-analyzer cargo calls-lib 0.1.0 after_accent().` | 17 | calls-lib/src/lib.rs:357:22 |  |
| S96 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Local][Task]work().` | 26 | calls-lib/src/lib.rs:389:8 |  |
| S97 | `rust-analyzer cargo calls-lib 0.1.0 alt().` | 17 | calls-lib/src/lib.rs:403:21 |  |
| S98 | `local 35` | 61 | none |  |
| S99 | `rust-analyzer cargo calls-lib 0.1.0 apply().` | 17 | calls-lib/src/lib.rs:409:8 |  |
| S100 | `rust-analyzer cargo calls-lib 0.1.0 shared/shared_call().` | 17 | calls-lib/src/shared.rs:7:8 |  |
| S101 | `rust-analyzer cargo calls-lib 0.1.0 kept().` | 17 | calls-lib/src/lib.rs:424:8 |  |
| S102 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Kept]kept_method().` | 26 | calls-lib/src/lib.rs:432:12 |  |
| S103 | `rust-analyzer cargo calls-lib 0.1.0 plain_beside().` | 17 | calls-lib/src/lib.rs:437:8 |  |
| S104 | `rust-analyzer cargo calls-lib 0.1.0 spelled/impl#[ByTest]by_test().` | 26 | calls-lib/src/lib.rs:456:16 |  |
| S105 | `rust-analyzer cargo calls-lib 0.1.0 impl#[ByClippy]by_clippy().` | 26 | calls-lib/src/lib.rs:470:12 |  |
| S106 | `rust-analyzer cargo calls-lib 0.1.0 maybe_inline().` | 17 | calls-lib/src/lib.rs:480:8 |  |
| S107 | `rust-analyzer cargo calls-lib 0.1.0 maybe_kept().` | 17 | calls-lib/src/lib.rs:485:8 |  |
| S108 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Shared][First]go().` | 26 | calls-lib/src/lib.rs:504:8 |  |
| S109 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Shared][Second]go().` | 26 | calls-lib/src/lib.rs:505:57 |  |
| S110 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Apart][First]go().` | 26 | calls-lib/src/lib.rs:511:8 |  |
| S111 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Apart][Second]go().` | 26 | calls-lib/src/lib.rs:517:8 |  |
| S112 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Joined][First]go().` | 26 | calls-lib/src/lib.rs:525:8 |  |
| S113 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Joined][Second]go().` | 26 | calls-lib/src/lib.rs:526:50 |  |
| S114 | `rust-analyzer cargo calls-lib 0.1.0 helper().` | 17 | calls-lib/src/lib.rs:549:8 |  |
| S115 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Original]work().` | 26 | calls-lib/src/lib.rs:546:12 |  |
| S116 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Copied]work().` | 26 | none |  |
| S117 | `local 48` | 61 | none |  |
| S118 | `rust-analyzer cargo calls-lib 0.1.0 inner().` | 17 | calls-lib/src/lib.rs:580:8 |  |
| S119 | `rust-analyzer cargo calls-lib 0.1.0 nested_run/run().` | 17 | calls-lib/src/lib.rs:591:12 |  |
| S120 | `local 51` | 61 | none |  |
| S121 | `rust-analyzer cargo calls-lib 0.1.0 const_run/run.` | 8 | calls-lib/src/lib.rs:604:19 |  |
| S122 | `rust-analyzer cargo calls-lib 0.1.0 const_run/nested().` | 17 | calls-lib/src/lib.rs:602:12 |  |
| S123 | `local 53` | 61 | none |  |
| S124 | `rust-analyzer cargo calls-lib 0.1.0 templated/inner/run().` | 17 | calls-lib/src/lib.rs:617:20 |  |
| S125 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Panel]label().` | 80 | calls-lib/src/lib.rs:634:70 |  |
| S126 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Sheet]label().` | 80 | calls-lib/src/lib.rs:640:41 |  |
| S127 | `rust-analyzer cargo calls-lib 0.1.0 impl#[Card]label().` | 80 | calls-lib/src/lib.rs:645:12 |  |
| S128 | `rust-analyzer cargo calls-lib 0.1.0 shared/shared_leaf().` | 17 | calls-lib/src/shared.rs:3:8 |  |

## Part two

Recorded from the raw IR of the pinned toolchain. For each call above it gives:
- whether an instruction exists, at what site and how many;
- the member of the `helper` group the compiler names;
- the `[part 2]` readings;
- the control relation.

### Pinned compiler observations

Rust 1.99.1, x86_64-unknown-linux-gnu, dev profile, 2026-11-25. Each selected
target produced a separate IR module. The binaries call their own definitions
of `helper`: calls-app/src/main.rs:7 and calls-app/src/bin/twin.rs:1. For the
regeneration control use `direct → leaf`, calls-lib/src/lib.rs:16 → 12, with one
instruction at 17:5. The two generic instantiations of `twice` each emit the
call at 29:5. The two instantiations of `Wrapper::get` each emit the call at
128:9.

Of the 86 possible entries in part one, the emitted instructions corroborate
83. The compiler inlines `always` into `calls_always`, leaving the instruction
at the inlined body's line 68 and none calling `always` at 72. For
`split_path`, it places the call at 369:5, one line before the indexed
reference. The binary emits `hinted` and calls it, but only declares the
`leaf` called by that copy at 559:5. The compiler places the call to
`make_adder` and the invocation of its returned closure at 45:5. Only the
former has a written callee definition. Both instructions at line 604 are
withheld because the two references to `leaf` have different enclosing
functions. The lookalike `drop` instance is defined in calls-lib.ll with its
source in fake-core/src/lib.rs.

### Expected evidence from the source and IR

Positions in this table are one-based character positions. Each entry groups
identical caller, callee, site and build. The engine uses zero-based positions.

| Emitted target | Caller | Callee | Site | Copies |
| --- | --- | --- | --- | ---: |
| calls-app | calls-app/src/main.rs:11:4 | calls-app/src/main.rs:7:4 | calls-app/src/main.rs:17:13 | 1 |
| calls-app | calls-app/src/main.rs:11:4 | calls-lib/src/lib.rs:20:8 | calls-app/src/main.rs:14:13 | 1 |
| calls-app | calls-app/src/main.rs:11:4 | calls-lib/src/lib.rs:558:8 | calls-app/src/main.rs:15:13 | 1 |
| calls-derive | calls-derive/src/lib.rs:36:4 | calls-derive/src/lib.rs:53:4 | calls-derive/src/lib.rs:46:80 | 1 |
| calls-derive | calls-derive/src/lib.rs:64:4 | calls-derive/src/lib.rs:64:4 | calls-derive/src/lib.rs:68:59 | 1 |
| calls-lib | calls-lib/src/drops.rs:6:8 | calls-lib/src/drops.rs:36:12 | calls-lib/src/drops.rs:6:26 | 1 |
| calls-lib | calls-lib/src/drops.rs:76:8 | fake-core/src/lib.rs:4:12 | calls-lib/src/drops.rs:77:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:116:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:117:6 | 1 |
| calls-lib | calls-lib/src/lib.rs:120:8 | calls-lib/src/lib.rs:116:8 | calls-lib/src/lib.rs:121:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:127:12 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:128:9 | 2 |
| calls-lib | calls-lib/src/lib.rs:133:8 | calls-lib/src/lib.rs:127:12 | calls-lib/src/lib.rs:134:18 | 1 |
| calls-lib | calls-lib/src/lib.rs:138:19 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:139:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:143:1 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:144:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:151:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:151:49 | 1 |
| calls-lib | calls-lib/src/lib.rs:154:12 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:155:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:159:8 | calls-lib/src/lib.rs:127:12 | calls-lib/src/lib.rs:160:46 | 1 |
| calls-lib | calls-lib/src/lib.rs:159:8 | calls-lib/src/lib.rs:154:12 | calls-lib/src/lib.rs:160:20 | 1 |
| calls-lib | calls-lib/src/lib.rs:164:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:165:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:16:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:17:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:173:8 | calls-lib/src/lib.rs:164:8 | calls-lib/src/lib.rs:174:11 | 1 |
| calls-lib | calls-lib/src/lib.rs:24:8 | calls-lib/src/lib.rs:20:8 | calls-lib/src/lib.rs:25:21 | 1 |
| calls-lib | calls-lib/src/lib.rs:24:8 | calls-lib/src/lib.rs:20:8 | calls-lib/src/lib.rs:25:6 | 1 |
| calls-lib | calls-lib/src/lib.rs:255:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:256:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:260:8 | calls-lib/src/lib.rs:255:8 | calls-lib/src/lib.rs:261:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:269:12 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:270:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:274:8 | calls-lib/src/lib.rs:269:12 | calls-lib/src/lib.rs:275:13 | 1 |
| calls-lib | calls-lib/src/lib.rs:28:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:29:5 | 2 |
| calls-lib | calls-lib/src/lib.rs:294:8 | calls-lib/src/mounted.rs:3:8 | calls-lib/src/lib.rs:295:6 | 1 |
| calls-lib | calls-lib/src/lib.rs:294:8 | calls-lib/src/plain.rs:3:8 | calls-lib/src/lib.rs:295:33 | 1 |
| calls-lib | calls-lib/src/lib.rs:298:8 | calls-lib/src/lib.rs:127:12 | calls-lib/src/lib.rs:299:19 | 1 |
| calls-lib | calls-lib/src/lib.rs:298:8 | calls-lib/src/lib.rs:127:12 | calls-lib/src/lib.rs:299:40 | 1 |
| calls-lib | calls-lib/src/lib.rs:303:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:304:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:307:8 | calls-lib/src/lib.rs:303:8 | calls-lib/src/lib.rs:308:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:311:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:312:14 | 1 |
| calls-lib | calls-lib/src/lib.rs:311:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:312:6 | 1 |
| calls-lib | calls-lib/src/lib.rs:325:8 | calls-lib/src/lib.rs:320:8 | calls-lib/src/lib.rs:326:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:329:8 | calls-lib/src/lib.rs:138:19 | calls-lib/src/lib.rs:330:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:33:8 | calls-lib/src/lib.rs:28:8 | calls-lib/src/lib.rs:34:18 | 1 |
| calls-lib | calls-lib/src/lib.rs:33:8 | calls-lib/src/lib.rs:28:8 | calls-lib/src/lib.rs:34:6 | 1 |
| calls-lib | calls-lib/src/lib.rs:353:8 | calls-lib/src/lib.rs:16:8 | calls-lib/src/lib.rs:354:12 | 1 |
| calls-lib | calls-lib/src/lib.rs:357:18 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:357:42 | 1 |
| calls-lib | calls-lib/src/lib.rs:359:8 | calls-lib/src/lib.rs:357:18 | calls-lib/src/lib.rs:360:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:363:8 | calls-lib/src/lib.rs:127:12 | calls-lib/src/lib.rs:365:10 | 1 |
| calls-lib | calls-lib/src/lib.rs:37:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:39:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:413:8 | calls-lib/src/lib.rs:409:8 | calls-lib/src/lib.rs:414:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:419:8 | calls-lib/src/shared.rs:7:8 | calls-lib/src/lib.rs:420:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:437:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:438:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:441:8 | calls-lib/src/lib.rs:437:8 | calls-lib/src/lib.rs:445:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:44:8 | calls-lib/src/lib.rs:37:8 | calls-lib/src/lib.rs:45:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:480:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:481:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:489:8 | calls-lib/src/lib.rs:480:8 | calls-lib/src/lib.rs:490:6 | 1 |
| calls-lib | calls-lib/src/lib.rs:511:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:512:13 | 1 |
| calls-lib | calls-lib/src/lib.rs:517:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:518:13 | 1 |
| calls-lib | calls-lib/src/lib.rs:526:50 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:526:69 | 1 |
| calls-lib | calls-lib/src/lib.rs:529:8 | calls-lib/src/lib.rs:504:8 | calls-lib/src/lib.rs:531:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:529:8 | calls-lib/src/lib.rs:505:57 | calls-lib/src/lib.rs:532:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:529:8 | calls-lib/src/lib.rs:511:8 | calls-lib/src/lib.rs:533:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:529:8 | calls-lib/src/lib.rs:517:8 | calls-lib/src/lib.rs:534:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:529:8 | calls-lib/src/lib.rs:525:8 | calls-lib/src/lib.rs:535:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:529:8 | calls-lib/src/lib.rs:526:50 | calls-lib/src/lib.rs:536:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:52:14 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:54:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:52:14 | calls-lib/src/lib.rs:48:14 | calls-lib/src/lib.rs:53:13 | 1 |
| calls-lib | calls-lib/src/lib.rs:549:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:550:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:563:19 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:564:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:578:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:579:11 | 1 |
| calls-lib | calls-lib/src/lib.rs:578:8 | calls-lib/src/lib.rs:16:8 | calls-lib/src/lib.rs:587:13 | 1 |
| calls-lib | calls-lib/src/lib.rs:578:8 | calls-lib/src/lib.rs:580:8 | calls-lib/src/lib.rs:586:17 | 1 |
| calls-lib | calls-lib/src/lib.rs:57:8 | calls-lib/src/lib.rs:52:14 | calls-lib/src/lib.rs:58:27 | 1 |
| calls-lib | calls-lib/src/lib.rs:591:12 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:594:48 | 1 |
| calls-lib | calls-lib/src/lib.rs:613:12 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:615:22 | 1 |
| calls-lib | calls-lib/src/lib.rs:613:12 | calls-lib/src/lib.rs:617:20 | calls-lib/src/lib.rs:621:26 | 1 |
| calls-lib | calls-lib/src/lib.rs:625:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:628:18 | 1 |
| calls-lib | calls-lib/src/lib.rs:645:12 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:646:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:660:8 | calls-lib/src/lib.rs:645:12 | calls-lib/src/lib.rs:664:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:67:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:68:5 | 1 |
| calls-lib | calls-lib/src/lib.rs:82:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/lib.rs:83:9 | 1 |
| calls-lib | calls-lib/src/lib.rs:91:8 | calls-lib/src/lib.rs:87:8 | calls-lib/src/lib.rs:92:5 | 1 |
| calls-lib | calls-lib/src/mounted.rs:3:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/mounted.rs:4:5 | 1 |
| calls-lib | calls-lib/src/plain.rs:3:8 | calls-lib/src/lib.rs:12:8 | calls-lib/src/plain.rs:4:5 | 1 |
| calls-lib | calls-lib/src/shared.rs:7:8 | calls-lib/src/shared.rs:3:8 | calls-lib/src/shared.rs:8:5 | 1 |
| calls-twin | calls-app/src/bin/twin.rs:5:4 | calls-app/src/bin/twin.rs:1:4 | calls-app/src/bin/twin.rs:6:13 | 1 |

### Raw instructions by project source location

The inventory below includes direct and indirect call and invoke instructions
with project debug locations, before source corroboration. Columns here are
one-based UTF-8 bytes. A missing location has zero observed instructions at
that exact position. A source token and its instruction can occupy different
columns or different lines. LLVM intrinsics are excluded. A register operand
denotes an indirect call. The final component of the demangled target is shown
for lookup; endpoint identity above uses the actual definition. Compiler-
generated calls can share a location with a written call.

#### calls-app · calls-app/src/main.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 12 | 17 `direct` |
| 13 | 21 `shared_entry` |
| 14 | 13 `generic` |
| 15 | 13 `hinted` |
| 16 | 13 `@_Read` |
| 17 | 13 `helper` |

#### calls-app · calls-lib/src/lib.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 559 | 5 `leaf` |

#### calls-derive · calls-derive/src/lib.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 5 | 1 `panic_in_cleanup` |
| 6 | 16 `struct_name` |
| 7 | 16 `format`, 16 `new`, 16 `new_display` |
| 8 | 5 `deref`, 10 `parse` |
| 9 | 1 `drop_glue` ×4 |
| 11 | 1 `panic_in_cleanup` |
| 12 | 28 `into_iter` |
| 13 | 36 `next` |
| 15 | 16 `eq`, 22 `to_string`, 44 `drop_glue` ×3 |
| 16 | 58 `next` |
| 18 | 25 `to_string` |
| 19 | 9 `drop_glue` ×3 |
| 20 | 5 `drop_glue` ×3 |
| 21 | 5 `from_str`, 5 `panic_fmt` |
| 22 | 1 `drop_glue` ×2 |
| 27 | 1 `panic_in_cleanup` |
| 28 | 22 `impl_parts`, 38 `clone` |
| 29 | 16 `format`, 16 `new`, 16 `new_display` |
| 32 | 21 `respan`, 28 `deref`, 33 `parse` |
| 33 | 10 `into_iter`, 22 `chain`, 39 `collect` |
| 34 | 1 `drop_glue` ×8 |
| 36 | 1 `panic_in_cleanup` |
| 37 | 27 `into_iter` |
| 39 | 36 `next` |
| 41 | 40 `eq`, 46 `to_string`, 66 `drop_glue` ×3 |
| 42 | 62 `next` |
| 43 | 21 `drop_glue`, 36 `to_string` |
| 44 | 17 `drop_glue` ×3 |
| 46 | 50 `expect`, 80 `fn_span`, 93 `stream`, 102 `drop_glue` ×3 |
| 50 | 5 `from_str`, 5 `panic_fmt` |
| 51 | 1 `drop_glue` ×3 |
| 53 | 1 `panic_in_cleanup` |
| 54 | 18 `into_iter`, 18 `next` |
| 56 | 16 `eq`, 22 `to_string`, 40 `drop_glue` ×3 |
| 58 | 26 `span` |
| 60 | 5 `drop_glue` ×6 |
| 61 | 5 `from_str`, 5 `panic_fmt` |
| 64 | 1 `panic_in_cleanup` |
| 65 | 19 `new` |
| 66 | 22 `into_iter`, 22 `next` |
| 68 | 29 `new`, 46 `delimiter`, 59 `respan`, 72 `stream` |
| 69 | 19 `set_span` |
| 70 | 13 `drop_glue` |
| 71 | 9 `drop_glue` |
| 72 | 19 `set_span` |
| 74 | 13 `push` |
| 75 | 5 `drop_glue` ×3 |
| 76 | 9 `into_iter`, 21 `collect` |
| 77 | 1 `drop_glue` ×2 |
| 82 | 1 `panic_in_cleanup` |
| 83 | 22 `impl_parts`, 38 `clone` |
| 84 | 16 `format`, 16 `new`, 16 `new_display` ×2 |
| 85 | 21 `respan`, 28 `deref`, 33 `parse` |
| 86 | 10 `into_iter`, 22 `chain`, 39 `collect` |
| 87 | 1 `drop_glue` ×8 |
| 93 | 1 `drop_glue` |
| 99 | 1 `panic_in_cleanup` |
| 100 | 28 `trait_impl_parts`, 50 `clone` |
| 101 | 16 `format`, 16 `new`, 16 `new_display` ×3 |
| 102 | 21 `respan`, 28 `deref`, 33 `parse` |
| 103 | 10 `into_iter`, 22 `chain`, 39 `collect` |
| 104 | 1 `drop_glue` ×10 |
| 106 | 1 `panic_in_cleanup` |
| 107 | 27 `into_iter` |
| 109 | 36 `next` |
| 111 | 40 `eq`, 46 `to_string`, 65 `drop_glue` ×3 |
| 112 | 62 `next` |
| 113 | 21 `drop_glue`, 36 `to_string` |
| 114 | 17 `drop_glue` ×3 |
| 117 | 38 `stream`, 47 `into_iter` |
| 118 | 47 `next` |
| 120 | 28 `eq`, 34 `to_string`, 52 `drop_glue` ×3 |
| 121 | 69 `next` |
| 123 | 36 `expect`, 77 `to_string`, 96 `span`, 102 `drop_glue` ×2 |
| 124 | 21 `drop_glue` ×3 |
| 125 | 17 `drop_glue` ×3 |
| 126 | 13 `drop_glue` ×6 |
| 130 | 5 `from_str`, 5 `panic_fmt` |
| 131 | 1 `drop_glue` ×3 |
| 136 | 1 `panic_in_cleanup` |
| 137 | 20 `new` |
| 139 | 18 `into_iter`, 18 `next`, 23 `clone` |
| 141 | 40 `eq`, 46 `to_string`, 66 `drop_glue` ×3 |
| 143 | 22 `extend`, 34 `clone` |
| 149 | 14 `push` |
| 150 | 5 `drop_glue` ×4 |
| 151 | 10 `into_iter`, 22 `chain`, 34 `collect` |
| 152 | 1 `drop_glue` ×4 |
| 157 | 1 `panic_in_cleanup` |
| 158 | 16 `struct_name`, 34 `clone` |
| 159 | 28 `into_iter`, 40 `last` |
| 160 | 46 `span_close`, 57 `drop_glue` ×2 |
| 161 | 30 `span`, 35 `drop_glue` ×2 |
| 162 | 17 `call_site` |
| 164 | 16 `format`, 16 `new`, 16 `new_display` |
| 165 | 5 `respan`, 12 `deref`, 17 `parse` |
| 166 | 1 `drop_glue` ×5 |

#### calls-lib · calls-lib/src/bom.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 8 | 5 `bom_leaf` |

#### calls-lib · calls-lib/src/drops.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 6 | 26 `drop` |
| 20 | 5 `drop` |
| 25 | 1 `drop_glue` |
| 29 | 1 `drop_glue` |
| 33 | 1 `drop_glue` |
| 40 | 5 `drop` |
| 44 | 5 `drop` |
| 53 | 9 `drop` |
| 59 | 1 `drop_glue` |
| 61 | 1 `panic_in_cleanup` |
| 73 | 5 `drop_glue` |
| 74 | 1 `drop_glue` ×2 |
| 77 | 5 `drop` |

#### calls-lib · calls-lib/src/lib.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 17 | 5 `leaf` |
| 25 | 6 `generic`, 21 `generic` |
| 29 | 5 `leaf` ×2 |
| 34 | 6 `twice`, 18 `twice` |
| 39 | 9 `leaf` |
| 45 | 5 `make_adder`, 5 `{closure#0}` |
| 48 | 29 `panic_const_async_fn_resumed` |
| 52 | 35 `panic_const_async_fn_resumed`, 35 `panic_const_async_fn_resumed_panic`, 35 `panic_in_cleanup` |
| 53 | 13 `ready`, 21 `into_future`, 21 `{closure#0}`, 25 `drop_glue` ×2 |
| 54 | 5 `leaf` |
| 57 | 1 `panic_in_cleanup` |
| 58 | 22 `pin_new_unchecked_in_helper`, 27 `after_await` |
| 59 | 17 `noop` |
| 60 | 27 `{closure#0}`, 37 `from_waker` |
| 64 | 1 `drop_glue` ×2 |
| 68 | 5 `leaf` |
| 83 | 9 `leaf` |
| 88 | 11 `%2` |
| 92 | 5 `dyn_call` |
| 99 | 5 `describe` |
| 105 | 13 `leaf` |
| 110 | 39 `leaf` |
| 113 | 6 `made`, 20 `made` |
| 117 | 6 `leaf` |
| 121 | 5 `fill` |
| 128 | 9 `leaf` ×2 |
| 134 | 18 `get` |
| 138 | 1 `panic_cannot_unwind` |
| 139 | 5 `leaf` |
| 144 | 5 `leaf` |
| 148 | 5 `split` |
| 151 | 54 `leaf` |
| 155 | 9 `leaf` |
| 160 | 20 `concrete`, 46 `get` |
| 165 | 9 `leaf` |
| 174 | 11 `greet` |
| 177 | 40 `leaf` |
| 179 | 5 `leaf` |
| 182 | 6 `same`, 21 `same` |
| 188 | 13 `leaf` |
| 196 | 5 `{closure#0}` |
| 203 | 9 `proxy_helper` |
| 204 | 9 `leaf` |
| 208 | 9 `leaf` |
| 218 | 16 `run` |
| 219 | 21 `run` |
| 220 | 16 `beside` |
| 228 | 9 `proxy_helper` |
| 234 | 14 `run` |
| 241 | 9 `proxy_helper` |
| 242 | 9 `leaf` |
| 247 | 16 `run`, 34 `run` |
| 256 | 9 `leaf` |
| 261 | 9 `tag` |
| 270 | 9 `leaf` |
| 275 | 13 `via_alias` |
| 295 | 6 `mounted`, 33 `plain` |
| 299 | 19 `get`, 40 `get` |
| 304 | 5 `leaf` |
| 308 | 5 `only_with_flag` |
| 312 | 6 `leaf`, 14 `leaf` |
| 326 | 5 `add` |
| 330 | 5 `shared_entry` |
| 345 | 5 `leaf` |
| 350 | 5 `only_outside_test` |
| 354 | 12 `direct` |
| 357 | 46 `leaf` |
| 360 | 5 `after_accent` |
| 365 | 10 `get` |
| 369 | 5 `leaf` |
| 389 | 5 `proxy_helper` |
| 390 | 9 `leaf` |
| 400 | 5 `work` |
| 403 | 85 `direct` |
| 406 | 5 `alt` |
| 410 | 5 `%f` |
| 414 | 5 `apply` |
| 420 | 5 `shared_call` |
| 425 | 5 `leaf` |
| 433 | 9 `leaf` |
| 438 | 5 `leaf` |
| 443 | 9 `kept` |
| 444 | 14 `kept_method` |
| 445 | 9 `plain_beside` |
| 457 | 13 `leaf` |
| 471 | 9 `leaf` |
| 476 | 22 `by_test`, 42 `by_clippy` |
| 481 | 5 `leaf` |
| 486 | 5 `leaf` |
| 490 | 6 `maybe_inline`, 22 `maybe_kept` |
| 505 | 9 `{closure#0}`, 13 `leaf`, 76 `{closure#0}`, 80 `leaf` |
| 512 | 9 `{closure#0}`, 13 `leaf` |
| 518 | 9 `{closure#0}`, 13 `leaf` |
| 526 | 9 `leaf`, 69 `leaf` |
| 531 | 9 `go` |
| 532 | 9 `go` |
| 533 | 9 `go` |
| 534 | 9 `go` |
| 535 | 9 `go` |
| 536 | 9 `go` |
| 546 | 33 `helper` ×2 |
| 550 | 5 `leaf` |
| 554 | 15 `work`, 30 `work` |
| 563 | 1 `panic_cannot_unwind` |
| 564 | 5 `leaf` |
| 579 | 11 `leaf` ×2 |
| 584 | 9 `{closure#0}` |
| 586 | 17 `inner` |
| 587 | 13 `direct` |
| 594 | 13 `{closure#0}`, 17 `leaf`, 48 `leaf` |
| 596 | 17 `run` |
| 604 | 41 `leaf`, 56 `call_once`, 76 `leaf` |
| 606 | 17 `nested` |
| 614 | 21 `leaf`, 73 `leaf` |
| 615 | 22 `leaf` |
| 618 | 17 `{closure#0}` |
| 621 | 26 `run` |
| 627 | 45 `leaf` |
| 628 | 18 `leaf` |
| 634 | 48 `proxy_helper`, 87 `leaf` |
| 640 | 19 `proxy_helper`, 58 `leaf` |
| 646 | 9 `leaf` |
| 662 | 9 `label` |
| 663 | 9 `label` |
| 664 | 9 `label` |

#### calls-lib · calls-lib/src/mounted.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 4 | 5 `leaf` |

#### calls-lib · calls-lib/src/plain.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 4 | 5 `leaf` |

#### calls-lib · calls-lib/src/shared.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 8 | 5 `shared_leaf` |

#### calls-lib · fake-core/src/lib.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 6 | 5 `drop_glue` |

#### calls-twin · calls-app/src/bin/twin.rs

| Source line | Byte column, target and multiplicity |
| ---: | --- |
| 6 | 13 `helper` |

