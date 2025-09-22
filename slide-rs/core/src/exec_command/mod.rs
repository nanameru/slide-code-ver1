// Simplified exec_command module for slide-code-test
// Provides basic function signatures needed by openai_tools.rs

use std::collections::BTreeMap;
use crate::openai_tools::{JsonSchema, ResponsesApiTool};

pub fn create_exec_command_tool_for_responses_api() -> ResponsesApiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: Some("The command to execute".to_string()),
        },
    );
    properties.insert(
        "workdir".to_string(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_string()),
        },
    );

    ResponsesApiTool {
        name: "exec_command".to_string(),
        description: "Execute a command in the terminal".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_string()]),
            additional_properties: Some(false),
        },
    }
}

pub fn create_write_stdin_tool_for_responses_api() -> ResponsesApiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "session_id".to_string(),
        JsonSchema::String {
            description: Some("The session ID to write to".to_string()),
        },
    );
    properties.insert(
        "input".to_string(),
        JsonSchema::String {
            description: Some("The input to write to stdin".to_string()),
        },
    );

    ResponsesApiTool {
        name: "write_stdin".to_string(),
        description: "Write input to stdin of a running command".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["session_id".to_string(), "input".to_string()]),
            additional_properties: Some(false),
        },
    }
}