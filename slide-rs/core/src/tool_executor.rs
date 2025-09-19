use crate::exec_sandboxed::{SandboxedExecutor, ExecParams};
use crate::seatbelt::SandboxPolicy;
use crate::approval_manager::AskForApproval;
use crate::config_types::ShellEnvironmentPolicy;
use serde_json::Value;
use std::path::PathBuf;
use anyhow::Result;

/// ツール実行を管理する統合実行エンジン
pub struct ToolExecutor {
    executor: SandboxedExecutor,
    cwd: PathBuf,
    shell_environment_policy: ShellEnvironmentPolicy,
}

impl ToolExecutor {
    pub fn new(
        approval_policy: AskForApproval,
        sandbox_policy: SandboxPolicy,
        cwd: PathBuf,
        shell_environment_policy: ShellEnvironmentPolicy,
    ) -> Self {
        let executor = SandboxedExecutor::new(approval_policy, sandbox_policy);
        Self {
            executor,
            cwd,
            shell_environment_policy,
        }
    }

    /// AIレスポンスからツール呼び出しを検出・実行
    pub async fn process_response(&mut self, response: &str) -> Result<String> {
        let mut result = response.to_string();

        // JSON形式のツール呼び出しを検出
        if let Some(tool_calls) = self.extract_tool_calls(response)? {
            for tool_call in tool_calls {
                let execution_result = self.execute_tool_call(tool_call).await?;
                result.push_str(&format!("\n\n[Tool Execution Result]\n{}", execution_result));
            }
        }

        Ok(result)
    }

    /// レスポンスからツール呼び出しを抽出
    fn extract_tool_calls(&self, response: &str) -> Result<Option<Vec<ToolCall>>> {
        let mut tool_calls = Vec::new();

        // JSON形式のツール呼び出しパターンを検索
        for line in response.lines() {
            let line = line.trim();

            // {"tool": "shell", "command": ["ls", "-la"]} 形式を検出
            if line.starts_with('{') && line.contains("\"tool\"") {
                if let Ok(call) = self.parse_tool_call(line) {
                    tool_calls.push(call);
                }
            }

            // <tool_call>...</tool_call> XML形式も対応
            if line.contains("<tool_call>") {
                if let Some(extracted) = self.extract_xml_tool_call(line) {
                    if let Ok(call) = self.parse_tool_call(&extracted) {
                        tool_calls.push(call);
                    }
                }
            }
        }

        if tool_calls.is_empty() {
            Ok(None)
        } else {
            Ok(Some(tool_calls))
        }
    }

    /// JSON形式のツール呼び出しをパース
    fn parse_tool_call(&self, json_str: &str) -> Result<ToolCall> {
        let value: Value = serde_json::from_str(json_str)?;

        let tool_name = value["tool"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?;

        match tool_name {
            "shell" => {
                let command = if let Some(cmd_array) = value["command"].as_array() {
                    cmd_array.iter()
                        .map(|v| v.as_str().unwrap_or_default().to_string())
                        .collect()
                } else if let Some(cmd_str) = value["command"].as_str() {
                    // シンプルな文字列の場合は分割
                    crate::parse_command::parse_command(cmd_str)
                } else {
                    return Err(anyhow::anyhow!("Invalid command format"));
                };

                let working_dir = value["working_dir"].as_str()
                    .map(PathBuf::from);
                let with_escalated_permissions = value["with_escalated_permissions"]
                    .as_bool().unwrap_or(false);
                let justification = value["justification"].as_str()
                    .map(String::from);
                let timeout_ms = value["timeout_ms"].as_u64();

                Ok(ToolCall::Shell {
                    command,
                    working_dir,
                    with_escalated_permissions,
                    justification,
                    timeout_ms,
                })
            }
            "read_file" => {
                let path = value["path"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing file path"))?;
                Ok(ToolCall::ReadFile {
                    path: PathBuf::from(path)
                })
            }
            "write_file" => {
                let path = value["path"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing file path"))?;
                let content = value["content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing file content"))?;
                Ok(ToolCall::WriteFile {
                    path: PathBuf::from(path),
                    content: content.to_string(),
                })
            }
            _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name))
        }
    }

    /// XML形式のツール呼び出しからJSONを抽出
    fn extract_xml_tool_call(&self, line: &str) -> Option<String> {
        if let Some(start) = line.find("<tool_call>") {
            if let Some(end) = line.find("</tool_call>") {
                let content = &line[start + 11..end];
                return Some(content.to_string());
            }
        }
        None
    }

    /// 個別のツール呼び出しを実行
    async fn execute_tool_call(&mut self, call: ToolCall) -> Result<String> {
        match call {
            ToolCall::Shell {
                command,
                working_dir,
                with_escalated_permissions,
                justification,
                timeout_ms
            } => {
                let params = ExecParams {
                    command,
                    working_dir: working_dir.or_else(|| Some(self.cwd.clone())),
                    timeout_ms,
                    with_escalated_permissions,
                    justification,
                    environment_policy: self.shell_environment_policy.clone(),
                };

                match self.executor.execute(params).await {
                    Ok(result) => {
                        Ok(format!(
                            "Command executed successfully (exit code: {})\nSTDOUT:\n{}\nSTDERR:\n{}",
                            result.exit_code,
                            result.stdout,
                            result.stderr
                        ))
                    }
                    Err(e) => {
                        Ok(format!("Command execution failed: {}", e))
                    }
                }
            }
            ToolCall::ReadFile { path } => {
                let full_path = if path.is_absolute() {
                    path
                } else {
                    self.cwd.join(path)
                };

                match tokio::fs::read_to_string(&full_path).await {
                    Ok(content) => Ok(format!("File content:\n{}", content)),
                    Err(e) => Ok(format!("Failed to read file {}: {}", full_path.display(), e)),
                }
            }
            ToolCall::WriteFile { path, content } => {
                let full_path = if path.is_absolute() {
                    path
                } else {
                    self.cwd.join(path)
                };

                // ディレクトリが存在しない場合は作成
                if let Some(parent) = full_path.parent() {
                    if let Err(e) = tokio::fs::create_dir_all(parent).await {
                        return Ok(format!("Failed to create directory {}: {}", parent.display(), e));
                    }
                }

                match tokio::fs::write(&full_path, content).await {
                    Ok(_) => Ok(format!("Successfully wrote to {}", full_path.display())),
                    Err(e) => Ok(format!("Failed to write file {}: {}", full_path.display(), e)),
                }
            }
        }
    }
}

/// 検出されたツール呼び出しの種類
#[derive(Debug, Clone)]
pub enum ToolCall {
    Shell {
        command: Vec<String>,
        working_dir: Option<PathBuf>,
        with_escalated_permissions: bool,
        justification: Option<String>,
        timeout_ms: Option<u64>,
    },
    ReadFile {
        path: PathBuf,
    },
    WriteFile {
        path: PathBuf,
        content: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval_manager::AskForApproval;
    use crate::seatbelt::SandboxPolicy;

    #[test]
    fn test_extract_tool_calls() {
        let executor = ToolExecutor::new(
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            PathBuf::from("."),
            ShellEnvironmentPolicy::default(),
        );

        let response = r#"
I'll help you read that file.

{"tool": "shell", "command": ["cat", "test.txt"]}

This command will display the contents of test.txt.
        "#;

        let calls = executor.extract_tool_calls(response).unwrap();
        assert!(calls.is_some());
        let calls = calls.unwrap();
        assert_eq!(calls.len(), 1);

        match &calls[0] {
            ToolCall::Shell { command, .. } => {
                assert_eq!(command, &vec!["cat".to_string(), "test.txt".to_string()]);
            }
            _ => panic!("Expected Shell tool call"),
        }
    }

    #[test]
    fn test_parse_read_file_tool() {
        let executor = ToolExecutor::new(
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            PathBuf::from("."),
            ShellEnvironmentPolicy::default(),
        );

        let json = r#"{"tool": "read_file", "path": "example.txt"}"#;
        let call = executor.parse_tool_call(json).unwrap();

        match call {
            ToolCall::ReadFile { path } => {
                assert_eq!(path, PathBuf::from("example.txt"));
            }
            _ => panic!("Expected ReadFile tool call"),
        }
    }
}