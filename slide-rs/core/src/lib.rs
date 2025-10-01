//! Core library: minimal surface for TUI integration.
//! Keep exports minimal to ensure the crate builds end-to-end.

pub mod approval_manager;
pub mod client;
pub mod client_common;
pub mod codex2;
pub mod config_types;
pub mod conversation_history;
pub mod event_mapping;
pub mod compact;
pub mod environment_context;
pub mod util;
pub mod exec; // MEE-50: 統一実行エントリポイント
pub mod exec_env;
pub mod mcp_connection_manager;
pub mod mcp_tool_call;
pub mod exec_sandboxed;
pub mod is_safe_command;
pub mod openai_tools;
pub mod parse_command;
pub mod path_utils;
pub mod safety;
pub mod seatbelt;
pub mod spawn; // MEE-50: sandbox用の共通プロセス起動
pub mod landlock; // MEE-50: Linux Landlock+seccomp sandbox
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
pub mod turn_diff_tracker;
pub use codex2 as codex;
