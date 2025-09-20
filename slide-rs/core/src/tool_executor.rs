use crate::exec::{SandboxedExecutor, ExecParams, ExecToolCallOutput};
use crate::seatbelt::SandboxPolicy;
use crate::approval_manager::AskForApproval;
use crate::config_types::ShellEnvironmentPolicy;
use crate::exec_env::create_env;
use crate::tool_apply_patch::{parse_patch, apply_file_operation, ApplyPatchInput, tool_apply_patch};
use serde_json::Value;
use std::collections::HashMap;
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

    /// 複数のツールを並列実行
    pub async fn execute_multiple_tools(&mut self, tool_calls: Vec<ToolCall>) -> Result<Vec<String>> {
        let mut results = Vec::new();

        for tool_call in tool_calls {
            let result = self.execute_tool_call(tool_call).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// OpenAI Function Calling形式のツール実行
    pub async fn execute_function_call(&mut self, name: &str, arguments: &str) -> Result<String> {
        let call = self.parse_function_call(name, arguments)?;
        self.execute_tool_call(call).await
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
                    crate::parse_command::parse_command_string(cmd_str)
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
            "apply_patch" => {
                let input = value["input"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing patch input"))?;
                Ok(ToolCall::ApplyPatch {
                    input: input.to_string(),
                })
            }
            "list_files" => {
                let path = value["path"].as_str().map(PathBuf::from);
                Ok(ToolCall::ListFiles { path })
            }
            "search_files" => {
                let query = value["query"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing search query"))?;
                let path = value["path"].as_str().map(PathBuf::from);
                Ok(ToolCall::SearchFiles {
                    query: query.to_string(),
                    path,
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

    /// OpenAI Function Call形式をToolCallに変換
    fn parse_function_call(&self, name: &str, arguments: &str) -> Result<ToolCall> {
        let value: Value = serde_json::from_str(arguments)?;

        match name {
            "shell" => {
                let command = if let Some(cmd_array) = value["command"].as_array() {
                    cmd_array.iter()
                        .map(|v| v.as_str().unwrap_or_default().to_string())
                        .collect()
                } else {
                    return Err(anyhow::anyhow!("Invalid command format"));
                };

                let working_dir = value["workdir"].as_str().map(PathBuf::from);
                let timeout_ms = value["timeout_ms"].as_u64();
                let with_escalated_permissions = value["with_escalated_permissions"]
                    .as_bool().unwrap_or(false);
                let justification = value["justification"].as_str().map(String::from);

                Ok(ToolCall::Shell {
                    command,
                    working_dir,
                    with_escalated_permissions,
                    justification,
                    timeout_ms,
                })
            }
            "apply_patch" => {
                let input = value["input"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing patch input"))?;
                Ok(ToolCall::ApplyPatch {
                    input: input.to_string(),
                })
            }
            _ => Err(anyhow::anyhow!("Unknown function: {}", name))
        }
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
                let env = create_env(&self.shell_environment_policy);
                let params = ExecParams {
                    command,
                    cwd: working_dir.unwrap_or_else(|| self.cwd.clone()),
                    timeout_ms,
                    env,
                    with_escalated_permissions: Some(with_escalated_permissions),
                    justification,
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
            ToolCall::ApplyPatch { input } => {
                let result = tool_apply_patch(ApplyPatchInput { patch: input }, true);
                Ok(result.message)
            }
            ToolCall::ListFiles { path } => {
                let target_path = path.unwrap_or_else(|| self.cwd.clone());

                match tokio::fs::read_dir(&target_path).await {
                    Ok(mut entries) => {
                        let mut files = Vec::new();
                        while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                            if let Ok(name) = entry.file_name().into_string() {
                                files.push(name);
                            }
                        }
                        files.sort();
                        Ok(format!("Files in {}:\n{}", target_path.display(), files.join("\n")))
                    }
                    Err(e) => Ok(format!("Failed to list files in {}: {}", target_path.display(), e)),
                }
            }
            ToolCall::SearchFiles { query, path } => {
                let search_path = path.unwrap_or_else(|| self.cwd.clone());
                // シンプルなファイル名検索（実際のプロジェクトではより高度な検索を実装）
                match self.search_files_recursive(&search_path, &query).await {
                    Ok(results) => {
                        if results.is_empty() {
                            Ok(format!("No files found matching '{}'", query))
                        } else {
                            Ok(format!("Found {} matches:\n{}", results.len(), results.join("\n")))
                        }
                    }
                    Err(e) => Ok(format!("Search failed: {}", e)),
                }
            }
        }
    }
}

impl ToolExecutor {
    /// ファイルを再帰的に検索
    async fn search_files_recursive(&self, dir: &PathBuf, query: &str) -> Result<Vec<String>> {
        let mut results = Vec::new();
        let mut stack = vec![dir.clone()];

        while let Some(current_dir) = stack.pop() {
            if let Ok(mut entries) = tokio::fs::read_dir(&current_dir).await {
                while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                    let path = entry.path();
                    let file_name = entry.file_name();

                    if let Some(name_str) = file_name.to_str() {
                        if name_str.contains(query) {
                            results.push(path.to_string_lossy().to_string());
                        }
                    }

                    if path.is_dir() {
                        stack.push(path);
                    }
                }
            }
        }

        Ok(results)
    }

    /// 設定の更新
    pub fn update_working_directory(&mut self, new_cwd: PathBuf) {
        self.cwd = new_cwd;
    }

    pub fn update_shell_environment_policy(&mut self, policy: ShellEnvironmentPolicy) {
        self.shell_environment_policy = policy;
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
    ApplyPatch {
        input: String,
    },
    ListFiles {
        path: Option<PathBuf>,
    },
    SearchFiles {
        query: String,
        path: Option<PathBuf>,
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