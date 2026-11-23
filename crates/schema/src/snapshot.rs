//! Rows of a snapshot, as specified in `spec/snapshot.md`.
//!
//! Field order in each struct is the order in which the engine writes the fields.

use serde::{Deserialize, Serialize};

use crate::position::{Range, Site};

/// Value of `format` in `manifest.json`; marks a directory as an engine-written snapshot.
pub const FORMAT: &str = "gosanja-snapshot";

/// Snapshot schema version this crate reads and writes.
pub const SCHEMA_VERSION: crate::compat::Version = crate::compat::Version::new(0, 1);

/// `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Always [`FORMAT`].
    pub format: String,
    /// Snapshot schema version, `major.minor`.
    pub schema: String,
    /// The engine that wrote the snapshot.
    pub tool: Tool,
    /// Display information about the project.
    pub project: Project,
    /// One entry per indexed scope, in path order.
    pub scopes: Vec<Scope>,
    /// Number of rows per file.
    pub counts: Counts,
}

/// A named tool with its version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    /// Tool version, as the tool states it.
    pub version: String,
}

/// Display information about the indexed project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// A display name; it carries no identity.
    pub name: String,
}

/// One indexed scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    /// Scope root relative to the project root, or `.`.
    pub path: String,
    /// The adapter that produced the inputs; absent when an index was given directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<AdapterInfo>,
    /// Tools named in the metadata of the SCIP input, sorted.
    pub indexers: Vec<Tool>,
    /// State of each layer.
    pub layers: Layers,
    /// Build contexts the adapter reported, sorted by id.
    pub builds: Vec<BuildContext>,
}

/// The adapter a scope was produced with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub name: String,
    pub version: String,
    /// Contract version the adapter speaks.
    pub contract: String,
}

/// State of the three layers of a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layers {
    /// Definitions, references and relationships.
    pub base: LayerState,
    /// Syntactic facts.
    pub syntax: LayerState,
    /// Call evidence.
    pub calls: LayerState,
}

/// Whether a layer contributed to the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerState {
    /// The layer was read.
    Present,
    /// The layer was not offered.
    Absent,
    /// The layer was offered and could not be used.
    Failed,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// What a piece of call evidence is true for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildContext {
    /// Unique within its scope; call evidence refers to it.
    pub id: String,
    /// Compiler name and version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<String>,
    /// Target platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Build profile, for example `debug`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Enabled features, tags or flags, sorted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,
}

/// Row counts recorded in the manifest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Rows of `files.jsonl`.
    pub files: u64,
    /// Rows of `entities.jsonl`.
    pub entities: u64,
    /// Rows of `externals.jsonl`.
    pub externals: u64,
    /// Rows of `relations.jsonl`.
    pub relations: u64,
    /// Rows of `syntax.jsonl`.
    pub syntax: u64,
    /// Rows of `gaps.jsonl`.
    pub gaps: u64,
    /// References to local symbols, which are not recorded.
    pub dropped_local_references: u64,
}

/// A row of `files.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRow {
    /// Path relative to the project root.
    pub path: String,
    /// Language as the index names it, lower case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Length of the file in bytes.
    pub bytes: u64,
    /// Number of lines.
    pub lines: u64,
    /// SHA-256 of the file's bytes, 64 hex digits.
    pub sha256: String,
}

/// What a definition is, as far as the core distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// Holds other entities: a package, module or namespace.
    Container,
    /// A type.
    Type,
    /// Something that can be called.
    Callable,
    /// A named value outside a type.
    Value,
    /// A named value inside a type: a field or a variant.
    Member,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// A row of `entities.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRow {
    /// Identity within the snapshot.
    pub key: String,
    /// The full SCIP symbol; absent on a container that no input names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// `<language>:<kind>` from the index; open set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Role; on a group row present only when all members agree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// The last descriptor's name, unescaped.
    pub name: String,
    /// The containing entity or group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Set on a container with no definition in the inputs.
    #[serde(default, skip_serializing_if = "is_false")]
    pub r#virtual: bool,
    /// Set on a collision group.
    #[serde(default, skip_serializing_if = "is_false")]
    pub group: bool,
    /// The group this definition belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_of: Option<String>,
    /// Where the entity is defined; absent on virtual and group rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub def: Option<Def>,
    /// Provenance.
    pub src: String,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde calls skip_serializing_if with a reference"
)]
const fn is_false(value: &bool) -> bool {
    !*value
}

/// Where an entity is defined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Def {
    /// A `files.jsonl` path.
    pub file: String,
    /// The name token.
    pub name: Range,
    /// The whole definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Range>,
    /// Where `body` came from; present whenever `body` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_src: Option<String>,
}

/// A row of `externals.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalRow {
    /// Identity within the snapshot.
    pub key: String,
    /// The full SCIP symbol.
    pub symbol: String,
    /// The package the symbol belongs to.
    pub package: Package,
    /// Provenance.
    pub src: String,
}

/// The package part of a SCIP symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub manager: String,
    pub name: String,
    pub version: String,
}

/// Kind of a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rel {
    /// The body of `from` refers to `to`.
    Ref,
    /// `from` calls `to`, with evidence.
    Call,
    /// `from` implements or overrides `to`.
    Implements,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// A SCIP role bit carried on a reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefRole {
    /// The reference is an import.
    Import,
    /// The reference writes the target.
    Write,
    /// The reference reads the target.
    Read,
    /// The reference is in generated code.
    Generated,
    /// The reference is in test code.
    Test,
    /// The reference is a forward definition.
    ForwardDefinition,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// How control reaches a callee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mechanism {
    /// Nothing is claimed.
    Unknown,
    /// Static dispatch.
    Static,
    /// Dispatch through an interface or trait object.
    Dynamic,
    /// A call through a function value.
    FunctionValue,
    /// A call of a closure.
    Closure,
    /// A destructor run when a value goes out of use.
    Drop,
    /// Only the scheduling of concurrent or deferred work is shown.
    Spawn,
    /// Only the resumption of suspended work is shown.
    Await,
    /// The call is in compiler-generated code.
    Generated,
    /// A value from a newer minor.
    #[serde(other, rename = "unknown-value")]
    Unrecognised,
}

/// How far the target of a call was established, weakest claim first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    /// The callee name is bound to this declaration; nothing is claimed about what runs.
    Declared,
    /// A target with this name exists; nothing else matched.
    NameOnly,
    /// The target is one of the implementations of the declared target.
    Hierarchy,
    /// As `Hierarchy`, and the implementing type is converted to the interface somewhere.
    CoercionFiltered,
    /// The target after generic instantiation.
    MonoInstantiated,
    /// Exactly this definition is called.
    Direct,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// A row of `relations.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationRow {
    /// The entity the relation starts from.
    pub from: String,
    /// An entity, a group or an external.
    pub to: String,
    /// Kind of relation.
    pub rel: Rel,
    /// Role bits; `ref` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<RefRole>>,
    /// How control reaches the callee; `call` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanism: Option<Mechanism>,
    /// How far the target was established; `call` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
    /// Where the relation was seen; `ref` and `implements`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sites: Option<Vec<Site>>,
    /// One entry per observation; `call` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Vec<Evidence>>,
    /// Provenance.
    pub src: String,
}

/// One observation of a call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Evidence {
    /// File the call was observed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Zero-based line; present whenever `file` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Start column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
    /// `[endLine, endCol]`, when the producer gave a range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<[u32; 2]>,
    /// Build context the observation is true for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

/// A row of `syntax.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxRow {
    /// The entity described.
    pub key: String,
    /// Visibility as written in the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Keywords on the definition, in source order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifiers: Option<Vec<String>>,
    /// Attributes directly on the definition, in source order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<String>>,
    /// Control-flow outline of the body; absent means not analysed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<OutlineNode>>,
    /// Provenance.
    pub src: String,
}

/// Kind of an outline node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutlineKind {
    /// A construct that chooses between alternatives.
    Branch,
    /// One alternative of a branch.
    Arm,
    /// Any loop.
    Loop,
    /// Leaving the current flow.
    Exit,
    /// A suspension point.
    Await,
    /// Starting concurrent or deferred work.
    Spawn,
    /// A nested function body.
    Closure,
    /// A call expression.
    Call,
    /// A region the adapter could not read.
    Opaque,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// A node of a control-flow outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineNode {
    /// Node kind.
    pub k: OutlineKind,
    /// The construct in the language's own words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Extent of the construct.
    pub range: Range,
    /// The callee name token; `call` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callee: Option<Range>,
    /// Nested nodes in source order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Self>>,
}

/// Which layer a gap concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
    /// Definitions, references and relationships.
    Base,
    /// Syntactic facts.
    Syntax,
    /// Call evidence.
    Calls,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// What could not be established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GapKind {
    /// Several distinct definitions share one symbol.
    SymbolCollision,
    /// A document's position encoding is unspecified.
    UndecodableDocument,
    /// A document's path is absolute or resolves outside the root.
    DocumentOutsideRoot,
    /// A document's file is missing or unreadable.
    SourceUnavailable,
    /// Positions fall outside their line or inside a multi-byte character.
    PositionOutOfRange,
    /// References in a file lie in no body.
    UnattributedReferences,
    /// Two bodies overlap without nesting.
    OverlappingBodies,
    /// The adapter does not offer an optional layer.
    MissingLayer,
    /// An optional layer failed or was malformed.
    FailedLayer,
    /// An anchor matched no definition or several.
    Unanchored,
    /// A reported call's callee token matches no reference.
    OpenCall,
    /// The syntax layer marked a region as unreadable.
    OpaqueRegion,
    /// Call evidence names a build context the adapter did not report.
    UnknownBuildContext,
    /// Rows were skipped under the compatibility rules.
    UnknownValue,
    /// A value from a newer minor.
    #[serde(other)]
    Unrecognised,
}

/// Why an optional layer could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailedDetail {
    /// The adapter reported the layer as failed.
    ReportedFailed,
    /// The adapter offered the layer and reported no status.
    NoStatus,
    /// The adapter reported success and wrote no file.
    FileMissing,
    /// A row of the layer failed its schema.
    RowInvalid,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// A row of `gaps.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GapRow {
    /// What is missing.
    pub gap: GapKind,
    /// File the gap concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Range the gap concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// How many items the gap stands for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    /// Rows the gap concerns, sorted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
    /// Layer the gap concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<Layer>,
    /// The anchor as the adapter gave it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<crate::contract::Anchor>,
    /// An adapter's diagnostic code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A fixed phrase, never free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<FailedDetail>,
    /// Provenance.
    pub src: String,
}

impl GapRow {
    /// A gap of the given kind with only its provenance set.
    #[must_use]
    pub fn new(gap: GapKind, src: impl Into<String>) -> Self {
        Self {
            gap,
            file: None,
            range: None,
            count: None,
            keys: None,
            layer: None,
            anchor: None,
            code: None,
            detail: None,
            src: src.into(),
        }
    }
}
