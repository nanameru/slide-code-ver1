//! Core library: Complete AI agent implementation with file operations.
//! Provides comprehensive functionality for AI-driven code manipulation.

// Core AI agent functionality
pub mod client;
pub mod codex2;
pub mod openai_tools;
pub mod error;
pub mod config;
pub mod protocol;
pub mod conversation_manager;
pub mod message_history;
pub mod openai_model_info;

// Advanced implementation modules
pub mod safety_impl;
pub mod seatbelt;
pub mod approval_mode;
pub mod bash_parser;

// File operation and AI agent modules
pub mod file_operations;
pub mod agent_executor;
pub mod tool_apply_patch;
pub mod exec_impl;
pub mod spawn_impl;

pub use codex2 as codex;