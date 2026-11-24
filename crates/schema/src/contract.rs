//! What an adapter prints and writes, as specified in `spec/adapter-contract.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::position::{PositionEncoding, Range};
use crate::snapshot::{BuildContext, Mechanism, OutlineNode, Resolution, Tool};

/// Adapter contract version this crate reads and writes.
pub const CONTRACT_VERSION: crate::compat::Version = crate::compat::Version::new(0, 3);

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

impl LayerName {
    /// Name used by the adapter protocol.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Syntax => "syntax",
            Self::Calls => "calls",
            Self::Unknown => "unknown",
        }
    }
}

/// Requested layers. The base layer is always included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerRequest {
    syntax: bool,
    calls: bool,
}

impl LayerRequest {
    /// Ask for every known layer.
    pub const ALL: Self = Self {
        syntax: true,
        calls: true,
    };

    /// Check whether the request includes this layer.
    #[must_use]
    pub const fn contains(self, layer: LayerName) -> bool {
        match layer {
            LayerName::Base => true,
            LayerName::Syntax => self.syntax,
            LayerName::Calls => self.calls,
            LayerName::Unknown => false,
        }
    }

    /// Requested layers in protocol argument order.
    pub fn iter(self) -> impl Iterator<Item = LayerName> {
        [LayerName::Base, LayerName::Syntax, LayerName::Calls]
            .into_iter()
            .filter(move |&layer| self.contains(layer))
    }
}

impl std::str::FromStr for LayerRequest {
    type Err = String;

    fn from_str(list: &str) -> Result<Self, Self::Err> {
        let mut base = false;
        let mut request = Self {
            syntax: false,
            calls: false,
        };
        for name in list.split(',') {
            let selected = match name {
                "base" => &mut base,
                "syntax" => &mut request.syntax,
                "calls" => &mut request.calls,
                _ => return Err(format!("unknown layer {name:?}")),
            };
            if *selected {
                return Err(format!("duplicate layer {name:?}"));
            }
            *selected = true;
        }
        if !base {
            return Err(format!("layer list {list:?} must include \"base\""));
        }
        Ok(request)
    }
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
    /// File names whose presence in a project root makes the adapter a candidate for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markers: Option<Vec<String>>,
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
    /// Files the layer was meant to cover and could not process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<Vec<SkippedFile>>,
}

/// A file a layer could not process.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SkippedFile {
    /// Path relative to the scope root.
    pub file: String,
    /// Why, as a diagnostic code.
    pub code: String,
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

#[cfg(test)]
mod tests {
    use super::{LayerName, LayerRequest};

    #[test]
    fn layer_arguments_use_the_manifest_spellings() {
        let layers = [
            LayerName::Base,
            LayerName::Syntax,
            LayerName::Calls,
            LayerName::Unknown,
        ];
        assert_eq!(
            layers.map(LayerName::as_str),
            ["base", "syntax", "calls", "unknown"]
        );
        for layer in layers {
            assert_eq!(serde_json::to_value(layer).unwrap(), layer.as_str());
        }
    }

    #[test]
    fn requests_include_base_and_list_each_selected_layer_in_order() {
        for (list, expected) in [
            ("base", &[LayerName::Base][..]),
            ("syntax,base", &[LayerName::Base, LayerName::Syntax][..]),
            ("calls,base", &[LayerName::Base, LayerName::Calls][..]),
            (
                "calls,syntax,base",
                &[LayerName::Base, LayerName::Syntax, LayerName::Calls][..],
            ),
        ] {
            let request = list.parse::<LayerRequest>().unwrap();
            assert_eq!(request.iter().collect::<Vec<_>>(), expected);
            assert!(!request.contains(LayerName::Unknown));
        }
        assert_eq!(
            LayerRequest::ALL,
            "base,syntax,calls".parse::<LayerRequest>().unwrap()
        );
        for list in [
            "syntax",
            "base,base",
            "base,graph",
            "base,",
            "base, syntax",
            "",
        ] {
            assert!(list.parse::<LayerRequest>().is_err(), "{list:?}");
        }
    }
}
