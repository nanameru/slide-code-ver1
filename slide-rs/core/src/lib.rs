//! Core library: minimal surface for TUI integration.
//! Keep exports minimal to ensure the crate builds end-to-end.

pub mod approval_manager;
pub mod client;
pub mod codex2;
pub mod config_types;
pub mod exec_env;
pub mod exec_sandboxed;
pub mod openai_tools;
pub mod safety;
pub mod seatbelt;
pub mod tool_executor;
pub mod parse_command;
pub use codex2 as codex;