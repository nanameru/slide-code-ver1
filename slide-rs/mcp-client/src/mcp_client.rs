use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use mcp_types::{
    CallToolRequest, CallToolRequestParams, InitializeRequest, InitializeRequestParams,
    InitializedNotification, ListToolsRequest, ListToolsRequestParams, ListToolsResult,
    RequestId,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time;
use tracing::{debug, error, info, warn};

const CHANNEL_CAPACITY: usize = 128;

type PendingSender = oneshot::Sender<serde_json::Value>;

pub struct McpClient {
    #[allow(dead_code)]
    child: tokio::process::Child,
    outgoing_tx: mpsc::Sender<serde_json::Value>,
    pending: Arc<Mutex<HashMap<i64, PendingSender>>>,
    id_counter: AtomicI64,
}

/// Minimal traits to adapt to mcp-types generated request/notification types.
pub trait HasMethod { fn method() -> String; type Params; }
pub trait HasResult { type Result; }

impl HasMethod for InitializeRequest { fn method() -> String { "initialize".to_string() } type Params = InitializeRequestParams; }
impl HasResult for InitializeRequest { type Result = mcp_types::InitializeResult; }

impl HasMethod for CallToolRequest { fn method() -> String { "tools/call".to_string() } type Params = CallToolRequestParams; }
impl HasResult for CallToolRequest { type Result = mcp_types::CallToolResult; }

impl HasMethod for ListToolsRequest { fn method() -> String { "tools/list".to_string() } type Params = ListToolsRequestParams; }
impl HasResult for ListToolsRequest { type Result = mcp_types::ListToolsResult; }

impl HasMethod for InitializedNotification { fn method() -> String { "notifications/initialized".to_string() } type Params = serde_json::Value; }

impl McpClient {
    pub async fn new_stdio_client(
        program: OsString,
        args: Vec<OsString>,
        env: Option<HashMap<String, String>>,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(create_env_for_mcp_server(env))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("failed to capture child stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("failed to capture child stdout"))?;

        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<serde_json::Value>(CHANNEL_CAPACITY);
        let pending: Arc<Mutex<HashMap<i64, PendingSender>>> = Arc::new(Mutex::new(HashMap::new()));

        let writer_handle = {
            let mut stdin = stdin;
            tokio::spawn(async move {
                while let Some(msg) = outgoing_rx.recv().await {
                    match serde_json::to_string(&msg) {
                        Ok(json) => {
                            debug!("MCP message to server: {json}");
                            if stdin.write_all(json.as_bytes()).await.is_err() {
                                error!("failed to write message to child stdin");
                                break;
                            }
                            if stdin.write_all(b"\n").await.is_err() {
                                error!("failed to write newline to child stdin");
                                break;
                            }
                        }
                        Err(e) => error!("failed to serialize JSONRPCMessage: {e}"),
                    }
                }
            })
        };

        let reader_handle = {
            let pending = pending.clone();
            let mut lines = BufReader::new(stdout).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    debug!("MCP message from server: {line}");
                    match serde_json::from_str::<serde_json::Value>(&line) {
                        Ok(v) => Self::dispatch_value(v, &pending).await,
                        Err(e) => error!("failed to parse JSON: {e}; line = {}", line),
                    }
                }
            })
        };

        let _ = (writer_handle, reader_handle);

        Ok(Self {
            child,
            outgoing_tx,
            pending,
            id_counter: AtomicI64::new(1),
        })
    }

    pub async fn send_request<R>(&self, params: <R as HasMethod>::Params, timeout: Option<Duration>) -> Result<R::Result>
    where
        R: HasMethod + HasResult,
        <R as HasMethod>::Params: Serialize,
        R::Result: DeserializeOwned,
    {
        let id = self.id_counter.fetch_add(1, Ordering::SeqCst);
        let request_id = RequestId::Integer(id);
        let params_json = serde_json::to_value(&params)?;
        let id_value = match request_id {
            RequestId::Integer(i) => serde_json::Value::from(i),
            RequestId::String(s) => serde_json::Value::from(s),
        };
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id_value,
            "method": R::method(),
            "params": params_json,
        });
        let (tx, rx) = oneshot::channel();
        {
            let mut guard = self.pending.lock().await;
            guard.insert(id, tx);
        }
        if self.outgoing_tx.send(message).await.is_err() {
            return Err(anyhow!("failed to send message to writer task - channel closed"));
        }
        let msg = match timeout {
            Some(duration) => match time::timeout(duration, rx).await {
                Ok(Ok(msg)) => msg,
                Ok(Err(_)) => {
                    let mut guard = self.pending.lock().await;
                    guard.remove(&id);
                    return Err(anyhow!("response channel closed before a reply was received"));
                }
                Err(_) => {
                    let mut guard = self.pending.lock().await;
                    guard.remove(&id);
                    return Err(anyhow!("request timed out"));
                }
            },
            None => rx
                .await
                .map_err(|_| anyhow!("response channel closed before a reply was received"))?,
        };
        // msg is the full JSON object for the response. Extract result or error.
        let obj = msg.as_object().ok_or_else(|| anyhow!("non-object JSON-RPC message"))?;
        if let Some(err) = obj.get("error") {
            let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or_default();
            let message = err.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
            return Err(anyhow!(format!("server returned JSON-RPC error: code = {code}, message = {message}")));
        }
        let result = obj
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("missing result in JSON-RPC response"))?;
        let typed: R::Result = serde_json::from_value(result)?;
        Ok(typed)
    }

    pub async fn send_notification<N>(&self, params: <N as HasMethod>::Params) -> Result<()>
    where
        N: HasMethod,
        <N as HasMethod>::Params: Serialize,
    {
        let params_json = serde_json::to_value(&params)?;
        let method = N::method();
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params_json,
        });
        self.outgoing_tx
            .send(notification)
            .await
            .with_context(|| format!("failed to send notification to writer task"))
    }

    pub async fn initialize(
        &self,
        initialize_params: InitializeRequestParams,
        initialize_notification_params: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<mcp_types::InitializeResult> {
        let response = self
            .send_request::<InitializeRequest>(initialize_params, timeout)
            .await?;
        self
            .send_notification::<InitializedNotification>(initialize_notification_params.unwrap_or(serde_json::Value::Null))
            .await?;
        Ok(response)
    }

    pub async fn list_tools(
        &self,
        _params: Option<ListToolsRequestParams>,
        timeout: Option<Duration>,
    ) -> Result<ListToolsResult> {
        // The generated ListToolsRequest does not carry params; just send default params via type param.
        self.send_request::<ListToolsRequest>(mcp_types::ListToolsRequestParams::default(), timeout)
            .await
    }

    pub async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<mcp_types::CallToolResult> {
        // mcp-types uses Map for arguments; convert Value::Object or default to empty map.
        let mut args_map = serde_json::Map::new();
        if let Some(val) = arguments {
            if let Some(obj) = val.as_object() {
                args_map = obj.clone();
            }
        }
        let params = CallToolRequestParams { name, arguments: args_map };
        debug!("MCP tool call: {params:?}");
        self.send_request::<CallToolRequest>(params, timeout).await
    }

    async fn dispatch_value(value: serde_json::Value, pending: &Arc<Mutex<HashMap<i64, PendingSender>>>) {
        // Only route replies that include a numeric id.
        let id_opt = value.get("id").and_then(|v| v.as_i64());
        if let Some(id) = id_opt {
            let tx_opt = {
                let mut guard = pending.lock().await;
                guard.remove(&id)
            };
            if let Some(tx) = tx_opt {
                let _ = tx.send(value);
            } else {
                warn!(id, "no pending request found for response");
            }
        } else if value.get("method").is_some() {
            // Notification; we do not route to pending.
            info!("<- notification: {}", value);
        } else {
            info!("<- unhandled message: {}", value);
        }
    }
}

#[cfg(unix)]
const DEFAULT_ENV_VARS: &[&str] = &[
    "HOME",
    "LOGNAME",
    "PATH",
    "SHELL",
    "USER",
    "__CF_USER_TEXT_ENCODING",
    "LANG",
    "LC_ALL",
    "TERM",
    "TMPDIR",
    "TZ",
];

#[cfg(windows)]
const DEFAULT_ENV_VARS: &[&str] = &[
    "PATH",
    "PATHEXT",
    "USERNAME",
    "USERDOMAIN",
    "USERPROFILE",
    "TEMP",
    "TMP",
];

fn create_env_for_mcp_server(extra_env: Option<HashMap<String, String>>) -> HashMap<String, String> {
    DEFAULT_ENV_VARS
        .iter()
        .filter_map(|var| match std::env::var(var) {
            Ok(value) => Some((var.to_string(), value)),
            Err(_) => None,
        })
        .chain(extra_env.unwrap_or_default())
        .collect::<HashMap<_, _>>()
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.try_wait();
    }
}
