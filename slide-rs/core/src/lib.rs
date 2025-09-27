//! Core library: minimal surface for TUI integration.
//! Keep exports minimal to ensure the crate builds end-to-end.

pub mod approval_manager;
pub mod client;
pub mod codex2;
pub mod config_types;
pub mod conversation_history;
pub mod event_mapping;
pub mod exec_env;
pub mod mcp_connection_manager;
pub mod mcp_tool_call;
pub mod exec_sandboxed;
pub mod is_safe_command;
pub mod openai_tools;
pub mod parse_command;
pub mod safety;
pub mod seatbelt;
pub mod shell;
pub mod tool_apply_patch;
pub mod tool_executor;
pub mod protocol; // re-export protocol crate types under crate::protocol
pub mod model_family;
pub mod plan_tool;
pub mod exec_command;
pub mod container_exec;
pub mod unified_exec;
pub mod error;
pub use codex2 as codex;
