//! Core library: minimal surface for TUI integration.
//! Keep exports minimal to ensure the crate builds end-to-end.

pub mod approval_manager;
pub mod client;
pub mod codex2;
pub mod config_types;
pub mod exec_env;
pub mod exec_sandboxed;
pub mod is_safe_command;
pub mod openai_tools;
pub mod parse_command;
pub mod safety;
pub mod seatbelt;
pub mod shell;
pub mod tool_apply_patch;
pub mod tool_executor;
pub use codex2 as codex;
