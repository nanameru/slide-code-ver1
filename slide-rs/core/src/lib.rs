//! Core library: minimal surface for TUI integration.
//! Keep exports minimal to ensure the crate builds end-to-end.

pub mod client;
pub mod codex2;
pub use codex2 as codex;