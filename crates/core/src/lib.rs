//! The language-neutral core of gosanja.
//!
//! It reads a SCIP index and an adapter's side-car files, builds the facts of a snapshot and
//! writes them. It knows the two interfaces specified under `spec/` and no language.

pub mod adapter;
pub mod anchor;
pub mod graph;
pub mod identity;
pub mod publish;
pub mod schemas;
pub mod scip_stream;
pub mod sidecar;
pub mod validate;
pub mod write;
