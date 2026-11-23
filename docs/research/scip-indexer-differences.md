# SCIP indexer differences

What rust-analyzer, scip-go and scip-typescript actually write for three small fixtures, so
that the snapshot and adapter contract is specified from real output rather than from one
producer. Everything below is a local measurement made on 2026-11-23 on Linux x86-64 unless
a line says otherwise. The fixtures are the checked-in `fixtures/rust-basic`,
`fixtures/go-basic` and `fixtures/ts-basic`; `fixtures/regenerate.sh` rebuilds their indexes
and `fixtures/README.md` records the tool versions.

## Summary

| Item | rust-analyzer 0.3.3049 | scip-go 0.2.10 | scip-typescript 0.6.2 |
| --- | --- | --- | --- |
| `Document.position_encoding` | UTF-8 code units | UTF-8 code units | UTF-16 code units |
| Name range form | legacy `range` only | typed `single_line_range` only | legacy `range` only |
| Enclosing range on definitions | legacy, on every non-local definition | typed multi-line, on functions; none on the package | legacy, on every definition |
| Enclosing range on references | set, but to the *target definition's* body when it is in the same file | none | none |
| Role bits seen | `Definition` only | `Definition`, `ReadAccess` on the reference | `Definition` only |
| Top-level fields | 1, 2, 2 (one `Index` message) | 1, 2 (two `Index` messages) | 1, 2 (two `Index` messages) |
| Named definition | `views/render().` at `src/views.rs` [2,7,13], body [2,0,8,1] | see Go section | see TypeScript section |
| Specific reference | `views/empty().` at `src/views.rs` [4,8,13], inside `render` | see Go section | see TypeScript section |
| Absolute paths in the index | `Metadata.project_root` only | `Metadata.project_root` | `Metadata.project_root` |
| Same bytes on repeated runs | yes, 3 of 3 | no, 2 distinct outputs in 6 runs | yes, 3 of 3 |

Ranges are zero-based `[line, start, end]` or `[startLine, startCol, endLine, endCol]`, end
exclusive, in the document's declared unit.

## Rust fixture

`fixtures/rust-basic` is the crate `demo` 0.1.0 (edition 2024) with `src/lib.rs` and
`src/views.rs`. It has free functions, a struct with a field, an inherent method, a trait
implementation from `std`, a macro call, locals and lifetimes, and one line with a non-BMP
character before a reference.

Tools: rustc and cargo 1.100.0 (`0ef1bc386 2026-11-09`). rust-analyzer is the standalone
release build `rust-analyzer 0.3.3049-standalone` for x86_64-unknown-linux-gnu, because the
rustup component is not available for this toolchain.

Index command, run from the repository root:

```sh
RUST_ANALYZER=/path/to/rust-analyzer fixtures/regenerate.sh rust-basic
```

The script copies the fixture to `/tmp/gosanja-fixtures/rust-basic` and runs
`rust-analyzer scip .` there. Its output ended with `Generating SCIP finished 1.07s` and
`rust-basic: rust-analyzer 0.3.3049-standalone`.

### Decoder

The decoder is a scratch Cargo project outside the repository with the dependencies the
plan pins (`Cargo.lock` resolved `scip 0.10.0` and `protobuf 3.7.2`):

```toml
[dependencies]
scip = "=0.10.0"
protobuf = "=3.7.2"
```

`src/main.rs` reads the top-level `Index` one field at a time, so every metadata field,
document and external symbol is counted, and prints which range field each occurrence
carries:

```rust
//! Dumps what a SCIP index carries, decoded with the pinned `scip` bindings.
//! The top-level `Index` is read field by field so concatenated messages are counted.
use protobuf::CodedInputStream;
use scip::types::{occurrence::Typed_enclosing_range as E, occurrence::Typed_range as R, *};

fn range(o: &Occurrence) -> String {
    match &o.typed_range {
        Some(R::SingleLineRange(r)) => format!("typed-single [{},{},{}]", r.line, r.start_character, r.end_character),
        Some(R::MultiLineRange(r)) => format!("typed-multi [{},{},{},{}]", r.start_line, r.start_character, r.end_line, r.end_character),
        Some(_) => "typed-unknown-variant".into(),
        None => format!("legacy{:?}", o.range),
    }
}
fn enclosing(o: &Occurrence) -> String {
    match &o.typed_enclosing_range {
        Some(E::SingleLineEnclosingRange(r)) => format!(" enclosing=typed-single [{},{},{}]", r.line, r.start_character, r.end_character),
        Some(E::MultiLineEnclosingRange(r)) => format!(" enclosing=typed-multi [{},{},{},{}]", r.start_line, r.start_character, r.end_line, r.end_character),
        Some(_) => " enclosing=typed-unknown-variant".into(),
        None if o.enclosing_range.is_empty() => String::new(),
        None => format!(" enclosing=legacy{:?}", o.enclosing_range),
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: scip-probe <index.scip>");
    let bytes = std::fs::read(&path).unwrap();
    let mut is = CodedInputStream::from_bytes(&bytes);
    let (mut metas, mut docs, mut externals) = (0, 0, 0);
    while let Some(tag) = is.read_raw_tag_or_eof().unwrap() {
        match tag {
            10 => {
                metas += 1;
                let m: Metadata = is.read_message().unwrap();
                let t = m.tool_info.get_or_default();
                println!("metadata#{metas}: version={:?} tool={} {} args={:?} project_root={:?} text_document_encoding={:?}",
                    m.version, t.name, t.version, t.arguments, m.project_root, m.text_document_encoding);
            }
            18 => {
                docs += 1;
                let d: Document = is.read_message().unwrap();
                println!("document {} language={} position_encoding={:?} occurrences={} symbols={}",
                    d.relative_path, d.language, d.position_encoding, d.occurrences.len(), d.symbols.len());
                for s in &d.symbols {
                    println!("  symbol {} kind={:?} display_name={:?} enclosing_symbol={:?} relationships={}",
                        s.symbol, s.kind, s.display_name, s.enclosing_symbol, s.relationships.len());
                }
                for o in &d.occurrences {
                    println!("  occ roles={} {} {}{}", o.symbol_roles, range(o), o.symbol, enclosing(o));
                }
            }
            26 => {
                externals += 1;
                let _: SymbolInformation = is.read_message().unwrap();
            }
            t => panic!("unexpected top-level tag {t}"),
        }
    }
    println!("summary: metadata_messages={metas} documents={docs} external_symbols={externals}");
}
```

Run from the decoder directory with `cargo run -q --bin scip-probe -- <repository>/fixtures/rust-basic/index.scip`. Observed output:

```text
metadata#1: version=UnspecifiedProtocolVersion tool=rust-analyzer 0.3.3049-standalone args=[] project_root="file:///tmp/gosanja-fixtures/rust-basic" text_document_encoding=UTF8
document src/lib.rs language=rust position_encoding=UTF8CodeUnitOffsetFromLineStart occurrences=4 symbols=1
  symbol rust-analyzer cargo demo 0.1.0 crate/ kind=Module display_name="" enclosing_symbol="" relationships=0
  occ roles=1 legacy[0, 0, 5, 0] rust-analyzer cargo demo 0.1.0 crate/ enclosing=legacy[0, 0, 5, 0]
  occ roles=0 legacy[2, 8, 13] rust-analyzer cargo demo 0.1.0 views/
  occ roles=0 legacy[4, 8, 13] rust-analyzer cargo demo 0.1.0 views/
  occ roles=0 legacy[4, 15, 21] rust-analyzer cargo demo 0.1.0 views/render().
document src/views.rs language=rust position_encoding=UTF8CodeUnitOffsetFromLineStart occurrences=53 symbols=15
  symbol rust-analyzer cargo demo 0.1.0 views/ kind=Module display_name="views" enclosing_symbol="" relationships=0
  symbol rust-analyzer cargo demo 0.1.0 views/render(). kind=Function display_name="render" enclosing_symbol="" relationships=0
  symbol local 0 kind=Parameter display_name="items" enclosing_symbol="rust-analyzer cargo demo 0.1.0 views/render()." relationships=0
  symbol rust-analyzer cargo demo 0.1.0 views/empty(). kind=Function display_name="empty" enclosing_symbol="" relationships=0
  symbol rust-analyzer cargo demo 0.1.0 views/Joined# kind=Struct display_name="Joined" enclosing_symbol="" relationships=0
  symbol rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'a>`]new(). kind=StaticMethod display_name="new" enclosing_symbol="" relationships=0
  symbol rust-analyzer cargo demo 0.1.0 views/badge(). kind=Function display_name="badge" enclosing_symbol="" relationships=0
  symbol local 1 kind=TypeParameter display_name="'a" enclosing_symbol="rust-analyzer cargo demo 0.1.0 views/Joined#" relationships=0
  symbol rust-analyzer cargo demo 0.1.0 views/Joined#items. kind=Field display_name="items" enclosing_symbol="" relationships=0
  symbol local 2 kind=TypeParameter display_name="'a" enclosing_symbol="rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'a>`]" relationships=0
  symbol local 3 kind=Parameter display_name="items" enclosing_symbol="rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'a>`]new()." relationships=0
  symbol rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'a>`] kind=TypeAlias display_name="" enclosing_symbol="" relationships=0
  symbol rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'_>`][Display]fmt(). kind=Method display_name="fmt" enclosing_symbol="" relationships=0
  symbol local 4 kind=SelfParameter display_name="self" enclosing_symbol="rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'_>`][Display]fmt()." relationships=0
  symbol local 5 kind=Parameter display_name="f" enclosing_symbol="rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'_>`][Display]fmt()." relationships=0
  occ roles=1 legacy[0, 0, 33, 0] rust-analyzer cargo demo 0.1.0 views/ enclosing=legacy[0, 0, 33, 0]
  occ roles=1 legacy[2, 7, 13] rust-analyzer cargo demo 0.1.0 views/render(). enclosing=legacy[2, 0, 8, 1]
  occ roles=1 legacy[2, 14, 19] local 0 enclosing=legacy[2, 14, 19]
  occ roles=0 legacy[2, 33, 39] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc string/String#
  occ roles=0 legacy[3, 7, 12] local 0 enclosing=legacy[2, 14, 19]
  occ roles=0 legacy[3, 13, 21] rust-analyzer cargo core https://github.com/rust-lang/rust/library/core slice/impl#[`[T]`]is_empty().
  occ roles=0 legacy[4, 8, 13] rust-analyzer cargo demo 0.1.0 views/empty(). enclosing=legacy[10, 0, 12, 1]
  occ roles=0 legacy[6, 8, 14] rust-analyzer cargo demo 0.1.0 views/Joined# enclosing=legacy[18, 0, 20, 1]
  occ roles=0 legacy[6, 16, 19] rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'a>`]new(). enclosing=legacy[23, 4, 25, 5]
  occ roles=0 legacy[6, 20, 25] local 0 enclosing=legacy[2, 14, 19]
  occ roles=0 legacy[6, 27, 36] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc string/impl#[T][ToString]to_string().
  occ roles=1 legacy[10, 7, 12] rust-analyzer cargo demo 0.1.0 views/empty(). enclosing=legacy[10, 0, 12, 1]
  occ roles=0 legacy[10, 18, 24] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc string/String#
  occ roles=0 legacy[11, 4, 10] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc string/String#
  occ roles=0 legacy[11, 12, 16] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc string/impl#[String][`From<&str>`]from().
  occ roles=1 legacy[14, 7, 12] rust-analyzer cargo demo 0.1.0 views/badge(). enclosing=legacy[14, 0, 16, 1]
  occ roles=0 legacy[14, 18, 24] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc string/String#
  occ roles=0 legacy[15, 14, 19] rust-analyzer cargo demo 0.1.0 views/empty(). enclosing=legacy[10, 0, 12, 1]
  occ roles=0 legacy[15, 23, 29] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc slice/impl#[`[T]`]concat().
  occ roles=1 legacy[18, 11, 17] rust-analyzer cargo demo 0.1.0 views/Joined# enclosing=legacy[18, 0, 20, 1]
  occ roles=1 legacy[18, 18, 20] local 1 enclosing=legacy[18, 18, 20]
  occ roles=1 legacy[19, 4, 9] rust-analyzer cargo demo 0.1.0 views/Joined#items. enclosing=legacy[19, 4, 24]
  occ roles=0 legacy[19, 12, 14] local 1 enclosing=legacy[18, 18, 20]
  occ roles=0 legacy[19, 17, 19] local 1 enclosing=legacy[18, 18, 20]
  occ roles=1 legacy[22, 5, 7] local 2 enclosing=legacy[22, 5, 7]
  occ roles=0 legacy[22, 9, 15] rust-analyzer cargo demo 0.1.0 views/Joined# enclosing=legacy[18, 0, 20, 1]
  occ roles=0 legacy[22, 16, 18] local 2 enclosing=legacy[22, 5, 7]
  occ roles=1 legacy[23, 11, 14] rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'a>`]new(). enclosing=legacy[23, 4, 25, 5]
  occ roles=1 legacy[23, 15, 20] local 3 enclosing=legacy[23, 15, 20]
  occ roles=0 legacy[23, 23, 25] local 2 enclosing=legacy[22, 5, 7]
  occ roles=0 legacy[23, 28, 30] local 2 enclosing=legacy[22, 5, 7]
  occ roles=0 legacy[23, 40, 44] rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'a>`] enclosing=legacy[22, 0, 26, 1]
  occ roles=0 legacy[24, 8, 14] rust-analyzer cargo demo 0.1.0 views/Joined# enclosing=legacy[18, 0, 20, 1]
  occ roles=0 legacy[24, 17, 22] local 3 enclosing=legacy[23, 15, 20]
  occ roles=0 legacy[24, 17, 22] rust-analyzer cargo demo 0.1.0 views/Joined#items. enclosing=legacy[19, 4, 24]
  occ roles=0 legacy[28, 5, 8] rust-analyzer cargo std https://github.com/rust-lang/rust/library/std crate/
  occ roles=0 legacy[28, 10, 13] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc fmt/
  occ roles=0 legacy[28, 15, 22] rust-analyzer cargo core https://github.com/rust-lang/rust/library/core fmt/Display#
  occ roles=0 legacy[28, 27, 33] rust-analyzer cargo demo 0.1.0 views/Joined# enclosing=legacy[18, 0, 20, 1]
  occ roles=1 legacy[29, 7, 10] rust-analyzer cargo demo 0.1.0 views/impl#[`Joined<'_>`][Display]fmt(). enclosing=legacy[29, 4, 31, 5]
  occ roles=1 legacy[29, 12, 16] local 4 enclosing=legacy[29, 11, 16]
  occ roles=1 legacy[29, 18, 19] local 5 enclosing=legacy[29, 18, 19]
  occ roles=0 legacy[29, 26, 29] rust-analyzer cargo std https://github.com/rust-lang/rust/library/std crate/
  occ roles=0 legacy[29, 31, 34] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc fmt/
  occ roles=0 legacy[29, 36, 45] rust-analyzer cargo core https://github.com/rust-lang/rust/library/core fmt/Formatter#
  occ roles=0 legacy[29, 54, 57] rust-analyzer cargo std https://github.com/rust-lang/rust/library/std crate/
  occ roles=0 legacy[29, 59, 62] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc fmt/
  occ roles=0 legacy[29, 64, 70] rust-analyzer cargo core https://github.com/rust-lang/rust/library/core fmt/Result#
  occ roles=0 legacy[30, 8, 13] rust-analyzer cargo core https://github.com/rust-lang/rust/library/core macros/write!
  occ roles=0 legacy[30, 15, 16] local 5 enclosing=legacy[29, 18, 19]
  occ roles=0 legacy[30, 24, 28] local 4 enclosing=legacy[29, 11, 16]
  occ roles=0 legacy[30, 29, 34] rust-analyzer cargo demo 0.1.0 views/Joined#items. enclosing=legacy[19, 4, 24]
  occ roles=0 legacy[30, 35, 39] rust-analyzer cargo alloc https://github.com/rust-lang/rust/library/alloc slice/impl#[`[T]`]join().
summary: metadata_messages=1 documents=2 external_symbols=0
```

### Observations

- **Position encoding.** Both documents declare `UTF8CodeUnitOffsetFromLineStart` and the
  metadata says `text_document_encoding=UTF8`. The columns agree: on line 15,
  `    ["🦀", &empty()].concat()`, the reference to `empty` starts at 14, which is the UTF-8
  byte offset. In UTF-16 it would be 12 and in code points 11.
- **Range form.** Every occurrence uses the deprecated `range` array; no occurrence has
  `typed_range` or `typed_enclosing_range`. Three-element arrays are single-line ranges,
  four-element arrays multi-line ones.
- **Name ranges of modules.** The definitions of `crate/` and `views/` have a name range
  that covers the whole file, [0,0,5,0] and [0,0,33,0], so a definition's name range is not
  always a name token.
- **Enclosing ranges on definitions.** Every non-local definition has one: `render`
  [2,0,8,1], `empty` [10,0,12,1], `Joined#` [18,0,20,1], the field `Joined#items.`
  [19,4,24], `new` [23,4,25,5], `fmt` [29,4,31,5]. Local definitions repeat their own
  range, except `&self`, whose enclosing range [29,11,16] starts one column earlier.
- **Enclosing ranges on references.** rust-analyzer also puts an enclosing range on
  references, but it is the body of the referenced definition, not the parent expression
  that `scip.proto` asks for ("This range must enclose the occurrence range"). The reference
  to `empty` at [4,8,13] carries [10,0,12,1], the body of `empty`. A reference whose target
  is in another file has none: `views::render` in `src/lib.rs` at [4,15,21]. The
  rust-analyzer source sets `enclosing_range` from `token.definition_body` when that body is
  in the same file, for every occurrence
  (https://github.com/rust-lang/rust-analyzer/blob/master/crates/rust-analyzer/src/cli/scip.rs,
  read on 2026-11-23; `Unverified:` that the 0.3.3049 build has the same code, although
  its output matches it). The `scip.proto` text is at
  https://github.com/scip-code/scip/blob/v0.10.0/scip.proto.
- **Roles.** Only the `Definition` bit (1) appears. References have 0, including the
  re-export `pub use views::render;`, which has no `Import` bit.
- **Symbol shapes.** Global symbols have the form
  `rust-analyzer cargo demo 0.1.0 views/render().`: scheme `rust-analyzer`, manager
  `cargo`, package `demo`, version `0.1.0`. Symbols from the standard library use a URL as
  the version, for example `rust-analyzer cargo alloc
  https://github.com/rust-lang/rust/library/alloc string/String#`, so a version component
  can contain `:` and `/`. The crate root is `crate/`. An inherent method is
  ``views/impl#[`Joined<'a>`]new().`` and a trait method
  ``views/impl#[`Joined<'_>`][Display]fmt().``. The inherent `impl` block is itself a
  symbol, ``views/impl#[`Joined<'a>`]``, with kind `TypeAlias`, an empty display name and
  no definition occurrence; the `Self` at [23,40,44] refers to it. The `Display` impl block
  has no symbol. Locals are `local 0` to `local 5` and are the only symbols with
  `enclosing_symbol` set. No symbol has relationships, so `fmt` carries no
  `is_implementation` link to `core fmt/Display#fmt`.
- **Two occurrences on one range.** The field shorthand `Joined { items }` gives a
  reference to the parameter `local 3` and one to the field `Joined#items.`, both at
  [24,17,22].
- **Macros.** `write!` is a reference to `core macros/write!`, and the arguments inside it
  are mapped back to source positions ([30,15,16], [30,24,28], [30,29,34]).
- **Message count.** The top-level fields are 1, 2, 2: one metadata, two documents and no
  external symbols. The bytes alone cannot show how many `Index` messages were
  concatenated; the rust-analyzer source writes the index with a single
  `scip::write_message_to_file` call (same source as above), so it is one message.
- **Known fact.** The named definition is `rust-analyzer cargo demo 0.1.0 views/render().`
  at `src/views.rs` [2,7,13] with enclosing range [2,0,8,1]. The specific reference is the
  occurrence of `rust-analyzer cargo demo 0.1.0 views/empty().` at `src/views.rs` [4,8,13],
  role 0, which lies inside the body of `render`; `empty` is defined at [10,7,12]. Ownership
  of that reference follows from `render`'s definition enclosing range, not from the
  reference's own enclosing range, which names `empty`'s body.

## Go and TypeScript fixtures

Local measurement, 2026-11-23, Linux x86-64. From the repository root run
`fixtures/regenerate.sh go-basic ts-basic`. The script stages the two source
projects at fixed paths, runs the following commands there, and copies each
raw `index.scip` back to its fixture directory.

```sh
(cd /tmp/gosanja-fixtures/go-basic && scip-go index --module-version v1.0.0)
(cd /tmp/gosanja-fixtures/ts-basic && scip-typescript index --no-progress-bar)
```

Go 1.27.3 built `github.com/scip-code/scip-go/cmd/scip-go@v0.2.10`. Node
26.13.1 ran npm's `@sourcegraph/scip-typescript@0.6.2` with TypeScript 6.0.3.

```console
$ strings fixtures/go-basic/index.scip fixtures/ts-basic/index.scip | grep -o 'file:///[^[:space:]]*'
file:///tmp/gosanja-fixtures/go-basic
file:///tmp/gosanja-fixtures/ts-basic
```

To compare consecutive runs, save both raw files after each invocation:

```sh
samples=$(mktemp -d)
for run in 1 2; do
    fixtures/regenerate.sh go-basic ts-basic
    for fixture in go-basic ts-basic; do
        cp "fixtures/$fixture/index.scip" "$samples/$fixture.$run.scip"
    done
done
(cd "$samples" && cmp go-basic.1.scip go-basic.2.scip)
(cd "$samples" && cmp ts-basic.1.scip ts-basic.2.scip)
```

In the measured pair, Go's `cmp` reported `differ: byte 462, line 6`;
TypeScript's exited 0. With the decoder dependencies below, run this command
from the scratch package directory to compare the decoded Go messages:

```sh
node --input-type=module - "$samples" <<'JS'
import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'
import {fromBinary} from '@bufbuild/protobuf'
import {IndexSchema} from '@scip-code/scip'
const indexes = [1, 2].map(run => {
  const bytes = readFileSync(process.argv[2] + '/go-basic.' + run + '.scip')
  const index = fromBinary(IndexSchema, bytes)
  for (const doc of index.documents) {
    console.log(run, doc.symbols.map(s => s.symbol.split('/').at(-1) || '(package)').join(', '))
    doc.symbols.sort((a, b) => a.symbol < b.symbol ? -1 : a.symbol > b.symbol ? 1 : 0)
  }
  return index
})
assert.deepEqual(indexes[0], indexes[1])
console.log('Decoded indexes match after sorting Document.symbols.')
JS
```

```text
1 greet()., main()., (package)
2 (package), greet()., main().
Decoded indexes match after sorting Document.symbols.
```

The Go indexer builds this list by iterating its package symbol map in
`SymbolsForFile`. Consumers should accept either ordering. The checked-in
Go file is raw indexer output; sorting above is only for the comparison.

https://github.com/scip-code/scip-go/blob/v0.2.10/internal/lookup/lookup.go

### Decode both samples

In a scratch directory install the decoder with
`npm install @bufbuild/protobuf@2.17.0 @scip-code/scip@0.10.0`. Save the
following as `inspect.mjs` there, then pass the two fixture indexes to
`node inspect.mjs /path/to/go-basic/index.scip /path/to/ts-basic/index.scip`.
The output lists top-level field numbers, then each occurrence's symbol,
role bits and populated range fields. Protobuf JSON omits default zero
coordinates inside typed ranges.

```js
import {readFileSync} from 'node:fs'
import {fromBinary, toJson} from '@bufbuild/protobuf'
import {BinaryReader} from '@bufbuild/protobuf/wire'
import {IndexSchema} from '@scip-code/scip'
for (const path of process.argv.slice(2)) {
  const bytes = readFileSync(path)
  const reader = new BinaryReader(bytes)
  const fields = []
  while (reader.pos < reader.len) {
    const [field, type] = reader.tag()
    fields.push(field)
    reader.skip(type, field)
  }
  const index = toJson(IndexSchema, fromBinary(IndexSchema, bytes))
  for (const doc of index.documents) {
    console.log(index.metadata.toolInfo.name, index.metadata.toolInfo.version,
                fields.join(','), doc.relativePath, doc.positionEncoding)
    for (const o of doc.occurrences) {
      const {symbol, symbolRoles = 0, ...ranges} = o
      console.log(symbol, symbolRoles, JSON.stringify(ranges))
    }
  }
}
```

Observed output:

```text
scip-go 0.2.10 1,2 main.go UTF8CodeUnitOffsetFromLineStart
scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/ 1 {"singleLineRange":{"startCharacter":8,"endCharacter":12}}
scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/greet(). 1 {"singleLineRange":{"line":2,"startCharacter":5,"endCharacter":10},"multiLineEnclosingRange":{"startLine":2,"endLine":4,"endCharacter":1}}
scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/main(). 1 {"singleLineRange":{"line":6,"startCharacter":5,"endCharacter":9},"multiLineEnclosingRange":{"startLine":6,"endLine":8,"endCharacter":1}}
scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/greet(). 8 {"singleLineRange":{"line":7,"startCharacter":5,"endCharacter":10}}
scip-typescript 0.6.2 1,2 src/index.ts UTF16CodeUnitOffsetFromLineStart
scip-typescript npm ts-basic 1.0.0 src/`index.ts`/ 1 {"range":[0,0,0],"enclosingRange":[0,0,7,0]}
scip-typescript npm ts-basic 1.0.0 src/`index.ts`/greet(). 1 {"range":[0,16,21],"enclosingRange":[0,0,2,1]}
scip-typescript npm ts-basic 1.0.0 src/`index.ts`/run(). 1 {"range":[4,16,19],"enclosingRange":[4,0,6,1]}
scip-typescript npm ts-basic 1.0.0 src/`index.ts`/greet(). 0 {"range":[5,9,14]}
```

### Go

`main.go` declares UTF-8 positions. All four names have typed single-line
ranges. Both function definitions have a typed multi-line enclosing range,
while the package definition has only its name range. The reference to
`greet` at zero-based [7,5,10] falls inside `main` at [6,0,8,1] and matches
the definition at [2,5,10] by its full symbol.

### TypeScript

`src/index.ts` declares UTF-16 positions. All four occurrences use the
legacy integer-array name range. All three definitions, including the file
symbol at [0,0,0], supply an enclosing range. The reference to `greet` at
[5,9,14] falls inside `run` at [4,0,6,1] and matches the definition at
[0,16,21]. The inputs use ASCII, so these fixtures verify the encoding
declarations and do not exercise byte conversion for non-ASCII text.

### Message count

Each fixture contains top-level fields 1 (metadata) and 2 (one document).
Each producer writes **two concatenated Index messages** in this case. The
field numbers establish the count in conjunction with these versions'
writer implementations: Go marshals one Index per metadata, document or
external-symbol callback; TypeScript writes an Index for metadata and one
per document. Protobuf concatenation has no intrinsic message delimiter.

Producer sources:
- https://github.com/scip-code/scip-go/blob/v0.2.10/cmd/scip-go/main.go
- https://www.npmjs.com/package/@sourcegraph/scip-typescript/v/0.6.2 (published
  source files `src/main.ts` and `src/ProjectIndexer.ts`).

## Typed ranges with the pinned Rust bindings

The plan reads typed ranges through `scip` 0.10.0 and `protobuf` 3.7.2. Two checks, both
run with the decoder above.

**Real producer output.** Decoding the checked-in Go index with
`cargo run -q --bin scip-probe -- <repository>/fixtures/go-basic/index.scip` gave:

```text
metadata#1: version=UnspecifiedProtocolVersion tool=scip-go 0.2.10 args=["index", "--module-version", "v1.0.0"] project_root="file:///tmp/gosanja-fixtures/go-basic" text_document_encoding=UTF8
document main.go language=go position_encoding=UTF8CodeUnitOffsetFromLineStart occurrences=4 symbols=3
  symbol scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/ kind=Package display_name="main" enclosing_symbol="" relationships=0
  symbol scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/greet(). kind=Function display_name="greet" enclosing_symbol="" relationships=0
  symbol scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/main(). kind=Function display_name="main" enclosing_symbol="" relationships=0
  occ roles=1 typed-single [0,8,12] scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/
  occ roles=1 typed-single [2,5,10] scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/greet(). enclosing=typed-multi [2,0,4,1]
  occ roles=1 typed-single [6,5,9] scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/main(). enclosing=typed-multi [6,0,8,1]
  occ roles=8 typed-single [7,5,10] scip-go gomod example.com/go-basic v1.0.0 `example.com/go-basic`/greet().
summary: metadata_messages=1 documents=1 external_symbols=0
```

The Rust bindings decode scip-go's typed single-line name ranges and typed multi-line
enclosing ranges, with the same values the Node decoder in the Go section printed.

**All four typed variants and precedence.** No fixture produces a typed multi-line name
range or a typed single-line enclosing range, so `src/bin/wire_check.rs` encodes four
occurrences by hand from the field numbers in `scip.proto` v0.10.0
(`range = 1`, `enclosing_range = 7`, `single_line_range = 8`, `multi_line_range = 9`,
`single_line_enclosing_range = 10`, `multi_line_enclosing_range = 11`), without using the
`scip` crate to encode, and decodes them with it:

```rust
//! Encodes occurrences by hand from the field numbers in upstream scip.proto v0.10.0,
//! without the `scip` crate, then decodes them with the pinned bindings.
use protobuf::Message;
use scip::types::{occurrence::Typed_enclosing_range as E, occurrence::Typed_range as R, Index};

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 { out.push((v as u8) | 0x80); v >>= 7; }
    out.push(v as u8);
}
fn field_len(out: &mut Vec<u8>, field: u64, body: &[u8]) {
    varint(out, field << 3 | 2); varint(out, body.len() as u64); out.extend_from_slice(body);
}
fn ints(fields: &[(u64, i32)]) -> Vec<u8> {
    let mut b = Vec::new();
    for &(f, v) in fields { varint(&mut b, f << 3); varint(&mut b, v as u64); }
    b
}
fn packed(vs: &[i32]) -> Vec<u8> {
    let mut b = Vec::new();
    for &v in vs { varint(&mut b, v as u64); }
    b
}
fn occ(symbol: &str, parts: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut b = Vec::new();
    field_len(&mut b, 2, symbol.as_bytes()); // Occurrence.symbol = 2
    for (f, body) in parts { field_len(&mut b, *f, body); }
    b
}

fn main() {
    // SingleLineRange: line=1, start_character=2, end_character=3.
    // MultiLineRange: start_line=1, start_character=2, end_line=3, end_character=4.
    let occs = [
        occ("a", &[(8, ints(&[(1, 1), (2, 2), (3, 3)])), (11, ints(&[(3, 4), (4, 1)]))]),
        occ("b", &[(9, ints(&[(1, 1), (2, 2), (3, 3), (4, 4)])), (10, ints(&[(1, 1), (3, 9)]))]),
        occ("c", &[(1, packed(&[5, 6, 7])), (8, ints(&[(1, 1), (2, 2), (3, 3)]))]),
        occ("d", &[(1, packed(&[5, 6, 7])), (7, packed(&[5, 0, 9, 1]))]),
    ];
    let mut doc = Vec::new();
    field_len(&mut doc, 1, b"x.rs"); // Document.relative_path = 1
    for o in &occs { field_len(&mut doc, 2, o); } // Document.occurrences = 2
    let mut index = Vec::new();
    field_len(&mut index, 2, &doc); // Index.documents = 2

    let decoded = Index::parse_from_bytes(&index).unwrap();
    for o in &decoded.documents[0].occurrences {
        let typed = match &o.typed_range {
            Some(R::SingleLineRange(r)) => format!("single [{},{},{}]", r.line, r.start_character, r.end_character),
            Some(R::MultiLineRange(r)) => format!("multi [{},{},{},{}]", r.start_line, r.start_character, r.end_line, r.end_character),
            Some(_) => "unknown variant".into(),
            None => "none".into(),
        };
        let enclosing = match &o.typed_enclosing_range {
            Some(E::SingleLineEnclosingRange(r)) => format!("single [{},{},{}]", r.line, r.start_character, r.end_character),
            Some(E::MultiLineEnclosingRange(r)) => format!("multi [{},{},{},{}]", r.start_line, r.start_character, r.end_line, r.end_character),
            Some(_) => "unknown variant".into(),
            None => "none".into(),
        };
        println!("{}: typed_range={typed} range={:?} typed_enclosing_range={enclosing} enclosing_range={:?}",
            o.symbol, o.range, o.enclosing_range);
    }
}
```

`cargo run -q --bin wire_check` printed:

```text
a: typed_range=single [1,2,3] range=[] typed_enclosing_range=multi [0,0,4,1] enclosing_range=[]
b: typed_range=multi [1,2,3,4] range=[] typed_enclosing_range=single [1,0,9] enclosing_range=[]
c: typed_range=single [1,2,3] range=[5, 6, 7] typed_enclosing_range=none enclosing_range=[]
d: typed_range=none range=[5, 6, 7] typed_enclosing_range=none enclosing_range=[5, 0, 9, 1]
```

Every variant decodes into the matching `Typed_range` or `Typed_enclosing_range` arm,
fields omitted on the wire decode as 0, and an occurrence carrying both `range` and
`typed_range` keeps both, so choosing typed first, as `scip.proto` and plan D2 require, is
the reader's job. The generated `Typed_range` and `Typed_enclosing_range` enums are
`#[non_exhaustive]`, so a `match` on them does not compile without a wildcard arm; the
first build of the decoder failed with `error[E0004]: non-exhaustive patterns: &Some(_) not
covered`.

## Absolute paths and repeatability

Each indexer writes the directory it ran in into `Metadata.project_root`, which is why
`fixtures/regenerate.sh` always indexes a copy at `/tmp/gosanja-fixtures/<fixture>`. Listing
every URL and absolute path among the strings of each checked-in index:

```sh
for f in rust-basic go-basic ts-basic; do
  echo "== $f"
  strings -n 4 fixtures/$f/index.scip |
    grep -o -E '[a-z]+://[^ `]*|(^|[^:/])/(tmp|home|usr|root|opt)/[^ ]*' | sort | uniq -c
done
```

```text
== rust-basic
      1 file:///tmp/gosanja-fixtures/rust-basic
     11 https://github.com/rust-lang/rust/library/alloc
      5 https://github.com/rust-lang/rust/library/core
      3 https://github.com/rust-lang/rust/library/std
== go-basic
      1 file:///tmp/gosanja-fixtures/go-basic
== ts-basic
      1 file:///tmp/gosanja-fixtures/ts-basic
```

The only local path is `project_root`. The `https://` strings are the version components of
standard-library symbols, not paths on the machine. rust-analyzer builds no artefacts for
this fixture, and the script points `CARGO_TARGET_DIR` at a temporary directory anyway.

Repeatability was measured by running the script on a copy of `fixtures/` and hashing the
result after each run (6 runs for Go, 3 for the others):

```sh
for i in 1 2 3 4 5 6; do ./regenerate.sh go-basic >/dev/null 2>&1
  sha256sum go-basic/index.scip | cut -c1-16; done | sort | uniq -c
```

Output, grouped by fixture under a heading line added here:

```text
== go-basic, 6 runs
      5 95b530a87f1083ef
      1 a1676114f292f94f
== ts-basic, 3 runs
      3 08ea3ab94feb3861
== rust-basic, 3 runs
      3 1c9b8ce25ec714b0
```

rust-analyzer and scip-typescript gave the same bytes every time. scip-go did not; the
decoded difference was the order of `Document.symbols` (see the Go section). A regenerated
Go index can therefore differ from the checked-in one without any change in its facts.

## What this means for the contract

Proposed design input for plan step 2, not a decision:

- Body ranges for ownership come only from occurrences with the `Definition` bit. A
  reference's `enclosing_range` is ignored, because rust-analyzer fills it with the target's
  body; if a later indexer fills it as `scip.proto` describes, using it is a separate
  decision.
- Both range forms occur in practice (Go typed, Rust and TypeScript legacy), so D2's
  reading of both is required, not defensive.
- Package versions can contain `:` and `/` (`https://github.com/rust-lang/rust/library/alloc`),
  so the D3 escape of key components has real input from the first fixture.
- A name range can cover a whole file (rust-analyzer modules), so code must not assume a
  name range is one token on one line.
- Two different symbols can occur on one range (Rust field shorthand); the core keeps both.
- Nothing may depend on the order of `Document.symbols` (scip-go) and nothing may read
  `Metadata.project_root`; the core locates documents relative to `--root`.
- A reader that matches on the typed-range enums needs a wildcard arm for future variants,
  which D9's handling of unknown values already implies.

## Known gaps

- The Rust fixture has no doc comments or attributes, so whether rust-analyzer's enclosing
  ranges include them, as `scip.proto` asks, is not measured.
- No macro-generated items, no second crate and no dependency, so cross-crate symbols other
  than the standard library and the recall loss on macro-generated items are not exercised.
- Only one rust-analyzer build was run; the source link above is for current `master`, not
  for that exact build.
- Only the Rust fixture has non-ASCII text, so UTF-16 conversion is not exercised by real
  scip-typescript output here; plan criterion 6 covers it with synthetic documents.
