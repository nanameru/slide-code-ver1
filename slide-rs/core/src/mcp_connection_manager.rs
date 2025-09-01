use std::time::Duration;

#[derive(Default, Clone)]
pub struct McpConnectionManager;

impl McpConnectionManager {
    pub fn new() -> Self { Self }

    pub fn list_all_tools(&self) -> Vec<String> { Vec::new() }

    pub fn parse_tool_name(&self, name: &str) -> Option<(String, String)> {
        // Parse FQ name like "server__tool"
        name.split_once("__").map(|(s, t)| (s.to_string(), t.to_string()))
    }

    pub async fn call_tool(
        &self,
        _server: &str,
        _tool: &str,
        _arguments: Option<serde_json::Value>,
        _timeout: Option<Duration>,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({"ok": true}))
    }
}
