//! Test support for every crate: `contract` checks values against `contract/`, `transcript` and `runner` replay
//! `contract/serial/` transcripts.

pub mod contract;
pub mod runner;
pub mod transcript;

use std::path::PathBuf;

/// The repository root, which holds `contract/`, `frontend/` and `rust/`.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().expect("repository root")
}
