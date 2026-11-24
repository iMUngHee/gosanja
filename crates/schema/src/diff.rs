//! The diff document types. Fields follow their wire order.
use crate::snapshot::{
    self, AdapterInfo, BuildContext, Layers, Mechanism, Project, RefRole, Resolution, Role, Tool,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAT: &str = "gosanja-diff";
pub const SCHEMA_VERSION: crate::compat::Version = crate::compat::Version::new(0, 2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub schema: String,
    pub tool: Tool,
    pub weights: Weights,
    pub old: Side,
    pub new: Side,
    pub counts: Counts,
    pub packages_added: Vec<String>,
    pub packages_removed: Vec<String>,
    pub not_compared: Vec<Excluded>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub struct Weights {
    pub name: u32,
    pub file: u32,
    pub parents: u32,
    pub size: u32,
    pub outgoing: u32,
    pub incoming: u32,
    pub threshold: u32,
}

/// Feature weights sum to 100. Each feature is at most 1,000,000, so the
/// weighted sum before division is at most 100,000,000 and fits in u32.
pub const WEIGHTS: Weights = Weights {
    name: 30,
    file: 18,
    parents: 18,
    size: 12,
    outgoing: 13,
    incoming: 9,
    threshold: 440_000,
};

const _: () = assert!(
    WEIGHTS.name
        + WEIGHTS.file
        + WEIGHTS.parents
        + WEIGHTS.size
        + WEIGHTS.outgoing
        + WEIGHTS.incoming
        == 100
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Side {
    pub schema: String,
    pub project: Project,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<AdapterInfo>,
    pub layers: Layers,
    pub builds: Vec<BuildContext>,
    pub counts: snapshot::Counts,
    pub gaps: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Counts {
    pub entities: EntityCounts,
    pub externals: ExternalCounts,
    pub relations: RelationCounts,
    pub candidates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityCounts {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub rekeyed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExternalCounts {
    pub added: usize,
    pub removed: usize,
    pub rekeyed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RelationCounts {
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Sides<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Properties {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Sides<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Sides<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Sides<Role>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<Sides<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Sides<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Sides<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualifiers: Option<Sides<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Sides<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<Sides<BTreeMap<String, u64>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Description {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#virtual: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "lowercase")]
pub enum EntityRow {
    Added {
        new: String,
        #[serde(flatten)]
        description: Description,
    },
    Removed {
        old: String,
        #[serde(flatten)]
        description: Description,
    },
    Changed {
        old: String,
        new: String,
        properties: Box<Properties>,
    },
    Rekeyed {
        old: String,
        new: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "lowercase")]
pub enum ExternalRow {
    Added { new: String, package: String },
    Removed { old: String, package: String },
    Rekeyed { old: String, new: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Excluded {
    Calls,
    Syntax,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Endpoint {
    Both,
    From,
    To,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(tag = "rel", rename_all = "lowercase")]
pub enum RelationKind {
    Ref {
        roles: Vec<RefRole>,
    },
    Call {
        mechanism: Mechanism,
        resolution: Resolution,
        src: String,
    },
    Implements,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationRow {
    pub change: Change,
    pub from: Sides<String>,
    pub to: Sides<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<Endpoint>,
    #[serde(flatten)]
    pub kind: RelationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRow {
    pub old: String,
    pub new: String,
    pub rank: u32,
    pub score: u32,
    pub name: u32,
    pub file: u32,
    pub parents: u32,
    pub size: u32,
    pub outgoing: u32,
    pub incoming: u32,
}
