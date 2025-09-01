//! Core library: sessions, tools, safety, exec, and providers.
//! This is a lightweight skeleton aligned with the requested design.

pub mod codex;
pub mod client;
pub mod client_common;
pub mod chat_completions; // existing minimal impl kept
pub mod openai_tools;
pub mod plan_tool;
pub mod apply_patch;
pub mod tool_apply_patch;
pub mod exec;
pub mod seatbelt;
pub mod landlock;
pub mod spawn;
pub mod safety;
pub mod is_safe_command;
pub mod exec_command;
pub mod mcp_connection_manager;
pub mod mcp_tool_call;
pub mod conversation_manager;
pub mod conversation_history;
pub mod message_history;
pub mod environment_context;
pub mod user_notification;
pub mod config;
pub mod config_types;
pub mod config_profile;
pub mod model_provider_info;
pub mod openai_model_info;
pub mod model_family;
pub mod git_info;
pub mod parse_command;
pub mod terminal;
pub mod shell;
pub mod bash;
pub mod util;
pub mod error;
pub mod flags;
pub mod project_doc;
pub mod rollout;
pub mod user_agent;
pub mod codex_conversation;
pub mod custom_prompts;
pub mod exec_env;
pub mod turn_diff_tracker;
pub mod protocol;

pub use chat_completions::*;