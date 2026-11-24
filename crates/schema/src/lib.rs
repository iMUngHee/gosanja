//! Types of the two gosanja interfaces: the snapshot and the adapter contract.
//!
//! The documents under `spec/` are normative; these types follow them. An adapter written in
//! Rust can use this crate to write its side-car files, and a consumer can use it to read a
//! snapshot.

pub mod compat;
pub mod contract;
pub mod diff;
pub mod position;
pub mod snapshot;
