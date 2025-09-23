//! MCPツール設定とパラメータ定義

use serde::{Deserialize, Serialize};
use mcp_types::{Tool, ToolInputSchema};
use schemars::JsonSchema;
use std::collections::HashMap;
use std::path::PathBuf;

use slide_core::{
    Config as SlideConfig, ConfigOverrides,
    approval_manager::AskForApproval,
    config_types::SandboxMode,
};

/// MCPツール呼び出しのクライアント設定パラメータ
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub struct McpToolCallParam {
    /// セッション開始時の初期ユーザープロンプト
    pub prompt: String,

    /// モデル名のオプション上書き（例: "gpt-4", "claude-3"）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// config.tomlの設定プロファイル
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// セッション作業ディレクトリ（相対パスの場合はサーバープロセスのcwdから解決）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// シェルコマンドの承認ポリシー
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<McpApprovalPolicy>,

    /// サンドボックスモード
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<McpSandboxMode>,

    /// SLIDE_HOME/config.tomlを上書きする個別設定
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<HashMap<String, serde_json::Value>>,

    /// デフォルトの代わりに使用する指示セット
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_instructions: Option<String>,

    /// 会話にプランツールを含めるかどうか
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_plan_tool: Option<bool>,
}

/// カスタム承認ポリシー列挙型（JsonSchemaサポート付き）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum McpApprovalPolicy {
    Untrusted,
    OnFailure,
    OnRequest,
    Never,
}

impl From<McpApprovalPolicy> for AskForApproval {
    fn from(value: McpApprovalPolicy) -> Self {
        match value {
            McpApprovalPolicy::Untrusted => AskForApproval::UnlessTrusted,
            McpApprovalPolicy::OnFailure => AskForApproval::OnFailure,
            McpApprovalPolicy::OnRequest => AskForApproval::OnRequest,
            McpApprovalPolicy::Never => AskForApproval::Never,
        }
    }
}

/// カスタムサンドボックスモード列挙型（JsonSchemaサポート付き）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum McpSandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl From<McpSandboxMode> for SandboxMode {
    fn from(value: McpSandboxMode) -> Self {
        match value {
            McpSandboxMode::ReadOnly => SandboxMode::ReadOnly,
            McpSandboxMode::WorkspaceWrite => SandboxMode::WorkspaceWrite,
            McpSandboxMode::DangerFullAccess => SandboxMode::DangerFullAccess,
        }
    }
}

/// MCPツール呼び出し用のツール定義を構築
pub(crate) fn create_tool_for_mcp_tool_call() -> Tool {
    let schema = schemars::gen::SchemaSettings::draft2019_09()
        .with(|s| {
            s.inline_subschemas = true;
            s.option_add_null_type = false;
        })
        .into_generator()
        .into_root_schema_for::<McpToolCallParam>();

    let schema_value = serde_json::to_value(&schema)
        .expect("MCP tool schema should serialize to JSON");

    let tool_input_schema = serde_json::from_value::<ToolInputSchema>(schema_value)
        .unwrap_or_else(|e| {
            panic!("failed to create Tool from schema: {e}");
        });

    Tool {
        name: "slide".to_string(),
        title: Some("Slide".to_string()),
        input_schema: tool_input_schema,
        output_schema: None,
        description: Some(
            "Run a Slide session. Accepts configuration parameters matching the Slide Config struct.".to_string(),
        ),
        annotations: None,
    }
}

impl McpToolCallParam {
    /// 初期ユーザープロンプトと有効なConfig オブジェクトを返す
    pub fn into_config(
        self,
        slide_sandbox_exe: Option<PathBuf>,
    ) -> std::io::Result<(String, SlideConfig)> {
        let Self {
            prompt,
            model,
            profile,
            cwd,
            approval_policy,
            sandbox,
            config: cli_overrides,
            base_instructions,
            include_plan_tool,
        } = self;

        // slide-coreで認識される`ConfigOverrides`を構築
        let overrides = ConfigOverrides {
            model,
            review_model: None,
            config_profile: profile,
            cwd: cwd.map(PathBuf::from),
            approval_policy: approval_policy.map(Into::into),
            sandbox_mode: sandbox.map(Into::into),
            model_provider: None,
            slide_sandbox_exe,
            base_instructions,
            include_plan_tool,
            include_apply_patch_tool: None,
            include_view_image_tool: None,
            show_raw_agent_reasoning: None,
            tools_web_search_request: None,
        };

        let cli_overrides = cli_overrides
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, convert_json_to_toml(v)))
            .collect();

        let cfg = SlideConfig::load_with_cli_overrides(cli_overrides, overrides)?;

        Ok((prompt, cfg))
    }
}

/// JSONからTOML値への変換ヘルパー
fn convert_json_to_toml(value: serde_json::Value) -> toml::Value {
    match value {
        serde_json::Value::Null => toml::Value::String("".to_string()),
        serde_json::Value::Bool(b) => toml::Value::Boolean(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                toml::Value::String(n.to_string())
            }
        },
        serde_json::Value::String(s) => toml::Value::String(s),
        serde_json::Value::Array(arr) => {
            toml::Value::Array(arr.into_iter().map(convert_json_to_toml).collect())
        },
        serde_json::Value::Object(obj) => {
            let mut table = toml::map::Map::new();
            for (k, v) in obj {
                table.insert(k, convert_json_to_toml(v));
            }
            toml::Value::Table(table)
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallReplyParam {
    /// このSlideセッションの会話ID
    pub conversation_id: String,

    /// Slide会話を続けるための次のユーザープロンプト
    pub prompt: String,
}

/// slide-replyツール呼び出し用のツール定義を構築
pub(crate) fn create_tool_for_reply_param() -> Tool {
    let schema = schemars::gen::SchemaSettings::draft2019_09()
        .with(|s| {
            s.inline_subschemas = true;
            s.option_add_null_type = false;
        })
        .into_generator()
        .into_root_schema_for::<McpToolCallReplyParam>();

    let schema_value = serde_json::to_value(&schema)
        .expect("Slide reply tool schema should serialize to JSON");

    let tool_input_schema = serde_json::from_value::<ToolInputSchema>(schema_value)
        .unwrap_or_else(|e| {
            panic!("failed to create Tool from schema: {e}");
        });

    Tool {
        name: "slide-reply".to_string(),
        title: Some("Slide Reply".to_string()),
        input_schema: tool_input_schema,
        output_schema: None,
        description: Some(
            "Continue a Slide conversation by providing the conversation id and prompt."
                .to_string(),
        ),
        annotations: None,
    }
}
