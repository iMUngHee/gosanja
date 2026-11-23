//! What an adapter prints and writes, as specified in `spec/adapter-contract.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::position::{PositionEncoding, Range};
use crate::snapshot::{BuildContext, Mechanism, OutlineNode, Resolution, Tool};

/// Adapter contract version this crate reads and writes.
pub const CONTRACT_VERSION: crate::compat::Version = crate::compat::Version::new(0, 1);

/// File name of the result an adapter writes into its output directory.
pub const RESULT_FILE: &str = "result.json";

/// A layer an adapter can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerName {
    /// Definitions, references and relationships, as a SCIP index.
    Base,
    /// Syntactic facts.
    Syntax,
    /// Call evidence.
    Calls,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// What `describe` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterManifest {
    /// Contract version the adapter speaks, `major.minor`.
    pub contract: String,
    /// Adapter name: lower case letters, digits and `-`.
    pub name: String,
    /// The adapter's own version.
    pub version: String,
    /// Languages the adapter handles, lower case.
    pub languages: Vec<String>,
    /// Unit of every column in the adapter's side-car files.
    pub position_encoding: PositionEncoding,
    /// Layers the adapter can produce; always includes `base`.
    pub layers: Vec<LayerName>,
    /// External tools needed at run time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Vec<Requirement>>,
}

/// An external tool an adapter needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    pub tool: String,
    /// Version requirement, in the tool's own terms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// `result.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterResult {
    /// Contract version, as in the manifest.
    pub contract: String,
    /// One entry per layer the adapter reports on, keyed by layer name.
    pub layers: BTreeMap<String, LayerResult>,
    /// Build contexts actually used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builds: Option<Vec<BuildContext>>,
    /// Tools that were run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

/// Outcome of one layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerResult {
    /// Whether the layer was produced.
    pub status: LayerStatus,
    /// What went wrong, or what was skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<Diagnostic>>,
}

/// Whether a layer was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerStatus {
    /// The layer's file was written.
    Ok,
    /// The layer could not be produced.
    Failed,
    /// A value from a newer minor.
    #[serde(other)]
    Unknown,
}

/// A problem an adapter reports as data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Names the kind of problem; matches `^[a-z][a-z0-9-]{0,63}$`.
    pub code: String,
    /// Free text for a human; never stored in a snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Points at a definition by the position of its name token.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Anchor {
    /// Path relative to the scope root.
    pub file: String,
    /// Zero-based line of the name token.
    pub line: u32,
    /// Start column of the name token, in the adapter's position encoding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
    /// The definition's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A row of an adapter's `syntax.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxFact {
    /// The definition described.
    pub anchor: Anchor,
    /// The whole definition, first token to last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Range>,
    /// Visibility as written in the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Keywords on the definition, in source order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifiers: Option<Vec<String>>,
    /// Attributes directly on the definition, in source order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<String>>,
    /// Control-flow outline of the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<OutlineNode>>,
}

/// A row of an adapter's `calls.jsonl`: one observation of a call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallEvidence {
    /// The calling definition.
    pub caller: Anchor,
    /// The called definition.
    pub callee: Anchor,
    /// How control reaches the callee.
    pub mechanism: Mechanism,
    /// How far the target was established; never `declared`.
    pub resolution: Resolution,
    /// Where the call was observed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<CallSite>,
    /// Id of a build context in `result.json`.
    pub build: String,
}

/// Where a call was observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    /// Path relative to the scope root.
    pub file: String,
    /// Zero-based line.
    pub line: u32,
    /// Column, in the adapter's position encoding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
}
