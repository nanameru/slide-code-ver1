//! Core library: minimal surface for TUI integration.
//! Keep exports minimal to ensure the crate builds end-to-end.

pub mod client;
pub mod codex2;
pub mod openai_tools;
pub mod error;
pub mod config;
pub mod protocol;
pub mod conversation_manager;
pub mod message_history;
pub mod openai_model_info;

// Implementation modules for tool functionality
pub mod safety_impl;
pub mod seatbelt;
pub mod approval_mode;
pub mod bash_parser;

pub use codex2 as codex;