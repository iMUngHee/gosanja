# gosanja specifications

Two interfaces of gosanja are specified here and are normative:

| Document | Interface | Version |
| --- | --- | --- |
| [snapshot.md](snapshot.md) | the snapshot the engine writes | schema 0.1 |
| [adapter-contract.md](adapter-contract.md) | what an adapter program is given and must produce | contract 0.1 |

The JSON Schemas under [schema/](schema/) define the shape of every file. Where a document and
a schema disagree, that is a defect in both: report it, and until it is fixed the schema decides
the shape and the document decides the meaning.

## Conventions shared by both interfaces

- Text is UTF-8 with LF line endings. A `.jsonl` file holds one JSON object per line, no blank
  lines, and ends with a newline. An empty table is an empty file.
- A path is relative, uses `/` as its separator, and has no empty, `.` or `..` segment.
- A range is `[startLine, startCol, endLine, endCol]`. Lines and columns are zero-based and the
  end is exclusive. In a snapshot, columns count UTF-8 bytes. In adapter output, columns count
  the unit the adapter declares, and the engine converts them.
- A version is `major.minor`, two non-negative integers.
- Field order in a JSON object carries no meaning. The engine writes fields in the order the
  documents list them so that its output is byte-identical between runs.

## Three independent versions

| Version | Where it is recorded | Governs |
| --- | --- | --- |
| gosanja release | the workspace package version, semver | the command line and the Rust API |
| snapshot schema | `schema` in `manifest.json` | every file of a snapshot |
| adapter contract | `contract` in an adapter's manifest and `result.json` | `describe`, `index` and every file an adapter writes |

A release may change without either interface changing, and the reverse.

## Change classes

| Change | Class |
| --- | --- |
| Add an optional field | minor |
| Add a value to a closed enumeration | minor |
| Remove a field, change its type or meaning, or make an optional field required | major |
| Remove or rename a value of a closed enumeration | major |
| Change a sort order, an escaping rule or the key format | major |

While the major version is 0, a minor change may also break compatibility. The changelog says so
for each such change.

## What a reader must do

These rules bind the engine when it reads adapter output, and any program that reads a snapshot.

1. Ignore fields it does not know.
2. While the writer's major is 0: accept only its own minor. Refuse anything else before doing
   any work.
3. From major 1: refuse a different major. Accept a writer minor at or below its own in full.
   For a newer writer minor, skip every row that holds an unknown value in a closed enumeration,
   and count the skipped rows.
4. A row that depends on a skipped row is skipped and counted as well: a relation whose endpoint
   was skipped, an entity whose parent or group was skipped. A reader never keeps a dangling key.
5. Rule 3 comes before any other treatment of a malformed row. A row that fails its schema only
   because of an unknown enumeration value under a newer minor is skipped, not treated as
   malformed. A row that is malformed in any other way is handled as the interface says, which
   for adapter output means discarding the layer.

The engine implements rules 2 to 5 in one function, `gosanja_schema::compat`.

## Changing a specification

One commit changes all of the following together: the document, the JSON Schema, the affected
goldens under `fixtures/`, the version number if the change class requires it, and an entry in
`CHANGELOG.md` naming the interface, the old and new version, and what a reader or an adapter
author has to do.
