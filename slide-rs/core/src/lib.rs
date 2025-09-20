//! Core library: minimal surface for TUI integration.
//! Keep exports minimal to ensure the crate builds end-to-end.

pub mod approval_manager;
pub mod client;
pub mod codex2;
pub mod config_types;
pub mod error;
pub mod exec_basic;
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

// Re-export exec_basic as exec for compatibility
pub use exec_basic as exec;
pub use codex2 as codex;
