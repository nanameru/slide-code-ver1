//! Connection manager for Model Context Protocol (MCP) servers.
//!
//! Spawns one client per configured server and aggregates tools under fully
//! qualified names: "<server>__<tool>". Names are validated/sanitized to fit
//! OpenAI tool naming constraints and kept stable by sorting/deduplication.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use mcp_types::Tool;
use sha1::{Digest, Sha1};
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::config_types::McpServerConfig;

const MCP_TOOL_NAME_DELIMITER: &str = "__";
const MAX_TOOL_NAME_LENGTH: usize = 64;
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

pub type ClientStartErrors = HashMap<String, anyhow::Error>;

fn is_valid_mcp_server_name(server_name: &str) -> bool {
    !server_name.is_empty()
        && server_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

struct ToolInfo {
    server_name: String,
    tool_name: String,
    tool: Tool,
}

struct ManagedClient {
    client: Arc<slide_mcp_client::McpClient>,
    startup_timeout: Duration,
}

fn qualify_tools(tools: Vec<ToolInfo>) -> HashMap<String, ToolInfo> {
    let mut used_names = HashSet::new();
    let mut qualified_tools = HashMap::new();
    for tool in tools {
        let mut qualified_name = format!(
            "{}{}{}",
            tool.server_name, MCP_TOOL_NAME_DELIMITER, tool.tool_name
        );
        if qualified_name.len() > MAX_TOOL_NAME_LENGTH {
            let mut hasher = Sha1::new();
            hasher.update(qualified_name.as_bytes());
            let sha1 = hasher.finalize();
            let sha1_str = format!("{sha1:x}");
            let prefix_len = MAX_TOOL_NAME_LENGTH - sha1_str.len();
            qualified_name = format!("{}{}", &qualified_name[..prefix_len], sha1_str);
        }
        if used_names.contains(&qualified_name) {
            warn!("skipping duplicated tool {}", qualified_name);
            continue;
        }
        used_names.insert(qualified_name.clone());
        qualified_tools.insert(qualified_name, tool);
    }
    qualified_tools
}

async fn list_all_tools(clients: &HashMap<String, ManagedClient>) -> Result<Vec<ToolInfo>> {
    let mut join_set = JoinSet::new();
    for (server_name, managed) in clients.iter() {
        let server = server_name.clone();
        let client = managed.client.clone();
        let timeout = managed.startup_timeout;
        join_set.spawn(async move {
            let res = client.list_tools(None, Some(timeout)).await;
            (server, res)
        });
    }

    let mut aggregated: Vec<ToolInfo> = Vec::with_capacity(join_set.len());
    while let Some(res) = join_set.join_next().await {
        let (server, list_res) = match res {
            Ok(pair) => pair,
            Err(e) => {
                warn!("Task panic when listing tools for MCP server: {e:#}");
                continue;
            }
        };
        let list = match list_res {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to list tools for MCP server '{server}': {e:#}");
                continue;
            }
        };
        for tool in list.tools {
            aggregated.push(ToolInfo {
                server_name: server.clone(),
                tool_name: tool.name.clone(),
                tool,
            });
        }
    }

    info!(
        "aggregated {} tools from {} servers",
        aggregated.len(),
        clients.len()
    );

    Ok(aggregated)
}

#[derive(Default)]
pub struct McpConnectionManager {
    clients: HashMap<String, ManagedClient>,
    tools: HashMap<String, ToolInfo>,
}

impl McpConnectionManager {
    pub async fn new(
        mcp_servers: HashMap<String, McpServerConfig>,
    ) -> Result<(Self, ClientStartErrors)> {
        if mcp_servers.is_empty() {
            return Ok((Self::default(), ClientStartErrors::default()));
        }

        let mut join_set = JoinSet::new();
        let mut errors = ClientStartErrors::new();

        for (server_name, cfg) in mcp_servers {
            if !is_valid_mcp_server_name(&server_name) {
                errors.insert(
                    server_name,
                    anyhow!("invalid server name: must match ^[a-zA-Z0-9_-]+$"),
                );
                continue;
            }
            let startup_timeout = cfg
                .startup_timeout_ms
                .map(Duration::from_millis)
                .unwrap_or(DEFAULT_STARTUP_TIMEOUT);
            join_set.spawn(async move {
                let McpServerConfig { command, args, env, .. } = cfg;
                let client_res = slide_mcp_client::McpClient::new_stdio_client(
                    OsString::from(command),
                    args.into_iter().map(OsString::from).collect(),
                    env,
                )
                .await;
                match client_res {
                    Ok(client) => {
                        // Initialize
                        let params = mcp_types::InitializeRequestParams {
                            capabilities: mcp_types::ClientCapabilities {
                                experimental: None,
                                roots: None,
                                sampling: None,
                                elicitation: Some(serde_json::json!({})),
                            },
                            client_info: mcp_types::Implementation {
                                name: "slide-mcp-client".to_string(),
                                version: env!("CARGO_PKG_VERSION").to_string(),
                                title: Some("Slide".to_string()),
                                user_agent: None,
                            },
                            protocol_version: mcp_types::MCP_SCHEMA_VERSION.to_string(),
                        };
                        let initialize_notification_params = None;
                        match client
                            .initialize(
                                params,
                                initialize_notification_params,
                                Some(startup_timeout),
                            )
                            .await
                        {
                            Ok(_) => (server_name, Ok((client, startup_timeout))),
                            Err(e) => (server_name, Err(e)),
                        }
                    }
                    Err(e) => (server_name, Err(anyhow!(e))),
                }
            });
        }

        let mut clients: HashMap<String, ManagedClient> = HashMap::with_capacity(join_set.len());
        while let Some(res) = join_set.join_next().await {
            let (server, client_res) = match res {
                Ok(v) => v,
                Err(e) => {
                    warn!("Task panic when starting MCP server: {e:#}");
                    continue;
                }
            };
            match client_res {
                Ok((client, startup_timeout)) => {
                    clients.insert(
                        server,
                        ManagedClient {
                            client: Arc::new(client),
                            startup_timeout,
                        },
                    );
                }
                Err(e) => {
                    errors.insert(server, e);
                }
            }
        }

        let tools_vec = list_all_tools(&clients)
            .await
            .context("failed to list tools from MCP servers")
            .unwrap_or_default();
        let tools = qualify_tools(tools_vec);
        Ok((Self { clients, tools }, errors))
    }

    pub fn list_all_tools(&self) -> HashMap<String, Tool> {
        self.tools
            .iter()
            .map(|(name, t)| (name.clone(), t.tool.clone()))
            .collect()
    }

    pub fn parse_tool_name(&self, name: &str) -> Option<(String, String)> {
        self.tools
            .get(name)
            .map(|t| (t.server_name.clone(), t.tool_name.clone()))
    }

    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<mcp_types::CallToolResult> {
        let client = self
            .clients
            .get(server)
            .ok_or_else(|| anyhow!(format!("unknown MCP server '{server}'")))?
            .client
            .clone();
        client
            .call_tool(tool.to_string(), arguments, timeout)
            .await
            .with_context(|| format!("tool call failed for `{server}/{tool}`"))
    }
}
