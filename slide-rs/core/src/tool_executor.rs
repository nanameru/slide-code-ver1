use crate::approval_manager::AskForApproval;
use crate::config_types::ShellEnvironmentPolicy;
use crate::exec_env::create_env;
use crate::seatbelt::SandboxPolicy;
use crate::tool_apply_patch::{tool_apply_patch, ApplyPatchInput};
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use protocol::models::{ResponseInputItem, FunctionCallOutputPayload};

/// ツール実行を管理する統合実行エンジン
pub struct ToolExecutor {
    cwd: PathBuf,
    shell_environment_policy: ShellEnvironmentPolicy,
    sandbox_policy: SandboxPolicy,
    #[allow(dead_code)]
    turn_diff_tracker: Option<std::sync::Arc<tokio::sync::Mutex<crate::turn_diff_tracker::TurnDiffTracker>>>,
}

impl ToolExecutor {
    /// パス解決は path_utils に委譲して一本化
    fn normalize_path_str(&self, raw: &str) -> PathBuf {
        crate::path_utils::resolve_path_with_cwd(&self.cwd, raw)
    }
    pub fn new(
        _approval_policy: AskForApproval,
        sandbox_policy: SandboxPolicy,
        cwd: PathBuf,
        shell_environment_policy: ShellEnvironmentPolicy,
    ) -> Self {
        Self {
            cwd,
            shell_environment_policy,
            sandbox_policy,
            turn_diff_tracker: None,
        }
    }

    pub fn with_turn_diff_tracker(mut self, tracker: std::sync::Arc<tokio::sync::Mutex<crate::turn_diff_tracker::TurnDiffTracker>>) -> Self {
        self.turn_diff_tracker = Some(tracker);
        self
    }

    /// AIレスポンスからツール呼び出しを検出・実行
    pub async fn process_response(&mut self, response: &str) -> Result<String> {
        let mut result = response.to_string();

        // JSON形式のツール呼び出しを検出
        let tool_calls = self.extract_tool_calls(response)?;
        if !tool_calls.is_empty() {
            for tool_call in tool_calls {
                let execution_result = self.execute_tool_call(tool_call).await?;
                result.push_str(&format!(
                    "\n\n[Tool Execution Result]\n{}",
                    execution_result
                ));
            }
        }

        Ok(result)
    }

    /// 複数のツールを並列実行
    pub async fn execute_multiple_tools(
        &mut self,
        tool_calls: Vec<ToolCall>,
    ) -> Result<Vec<String>> {
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

    /// OpenAI Function Calling形式のツール実行（ResponseInputItem返却版）
    pub async fn execute_function_call_structured(
        &mut self,
        name: &str,
        arguments: &str,
        call_id: String,
    ) -> Result<ResponseInputItem> {
        let start = Instant::now();
        
        match self.execute_function_call(name, arguments).await {
            Ok(result) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                Ok(ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: FunctionCallOutputPayload {
                        content: result,
                        success: Some(true),
                    },
                })
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                Ok(ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: FunctionCallOutputPayload {
                        content: format!("Error: {}", e),
                        success: Some(false),
                    },
                })
            }
        }
    }

    /// ツール呼び出しを構造化された形式で実行（リトライ機能付き）
    pub async fn execute_tool_call_structured(
        &mut self,
        call: ToolCall,
        call_id: String,
    ) -> Result<ResponseInputItem> {
        let start = Instant::now();
        let max_retries = 3;
        let mut retries = 0;
        
        loop {
            match self.execute_tool_call(call.clone()).await {
                Ok(result) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return Ok(ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: FunctionCallOutputPayload {
                            content: result,
                            success: Some(true),
                        },
                    });
                }
                Err(e) => {
                    if retries < max_retries {
                        retries += 1;
                        let delay = Duration::from_millis(100 * (1 << retries)); // 指数バックオフ
                        tracing::warn!(
                            "Tool execution failed (attempt {}/{}): {}. Retrying in {:?}...",
                            retries, max_retries + 1, e, delay
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    } else {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        return Ok(ResponseInputItem::FunctionCallOutput {
                            call_id,
                            output: FunctionCallOutputPayload {
                                content: format!("Error after {} retries: {}", max_retries, e),
                                success: Some(false),
                            },
                        });
                    }
                }
            }
        }
    }

    /// レスポンスからツール呼び出しを抽出
    pub fn extract_tool_calls(&self, response: &str) -> Result<Vec<ToolCall>> {
        let mut tool_calls = Vec::new();

        // 1) JSON形式のツール呼び出しパターンを検索（厳密トリガー）
        for line in response.lines() {
            let line = line.trim();
            if line.starts_with('{') && line.contains("\"tool\"") {
                if let Ok(call) = self.parse_tool_call(line) {
                    tool_calls.push(call);
                }
            }
            if line.contains("<tool_call>") {
                if let Some(extracted) = self.extract_xml_tool_call(line) {
                    if let Ok(call) = self.parse_tool_call(&extracted) {
                        tool_calls.push(call);
                    }
                }
            }
        }

        // 2) exec_command 提案の簡易パース（モデルがJSONを出さない場合のフォールバック）
        //    例:
        //    exec_command:
        //    cat '/abs/path/to/file'
        //    または同一行に続くケースも許容
        if tool_calls.is_empty() {
            let mut lines = response.lines().peekable();
            let mut in_exec_block = false;
            while let Some(raw) = lines.next() {
                let line = raw.trim();
                if !in_exec_block {
                    // トリガー行検出
                    if line.eq_ignore_ascii_case("exec_command:") || line.eq_ignore_ascii_case("exec_command") {
                        in_exec_block = true;
                        // 次行を見る
                        continue;
                    }
                    // 同一行で "exec_command:" の後にコマンドが続くパターン
                    if let Some(pos) = line.to_ascii_lowercase().find("exec_command:") {
                        let after = line[pos + "exec_command:".len()..].trim();
                        if !after.is_empty() {
                            let cmd = after;
                            let argv = crate::parse_command::parse_command_string(cmd);
                            tool_calls.push(ToolCall::Shell {
                                command: argv,
                                working_dir: None,
                                with_escalated_permissions: false,
                                justification: None,
                                timeout_ms: None,
                            });
                            break;
                        } else {
                            in_exec_block = true;
                            continue;
                        }
                    }
                } else {
                    // exec_command ブロック内の最初の非空行をコマンドとして扱う
                    if !line.is_empty() {
                        // 箇条書き記号や見出しの可能性がある行はスキップ
                        let is_meta = line.ends_with(':') || line.starts_with('-');
                        if !is_meta {
                            let cmd = line;
                            let argv = crate::parse_command::parse_command_string(cmd);
                            tool_calls.push(ToolCall::Shell {
                                command: argv,
                                working_dir: None,
                                with_escalated_permissions: false,
                                justification: None,
                                timeout_ms: None,
                            });
                            break;
                        }
                    }
                }
            }
        }

        Ok(tool_calls)
    }

    /// JSON形式のツール呼び出しをパース
    fn parse_tool_call(&self, json_str: &str) -> Result<ToolCall> {
        let value: Value = serde_json::from_str(json_str)?;

        let tool_name = value["tool"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?;

        match tool_name {
            "shell" => {
                let command = if let Some(cmd_array) = value["command"].as_array() {
                    cmd_array
                        .iter()
                        .map(|v| v.as_str().unwrap_or_default().to_string())
                        .collect()
                } else if let Some(cmd_str) = value["command"].as_str() {
                    // シンプルな文字列の場合は分割
                    crate::parse_command::parse_command_string(cmd_str)
                } else {
                    return Err(anyhow::anyhow!("Invalid command format"));
                };

                let working_dir = value["working_dir"].as_str().map(PathBuf::from);
                let with_escalated_permissions = value["with_escalated_permissions"]
                    .as_bool()
                    .unwrap_or(false);
                let justification = value["justification"].as_str().map(String::from);
                let timeout_ms = value["timeout_ms"].as_u64();

                Ok(ToolCall::Shell {
                    command,
                    working_dir,
                    with_escalated_permissions,
                    justification,
                    timeout_ms,
                })
            }
            "apply_patch" => {
                let input = value["input"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing patch input"))?;
                Ok(ToolCall::ApplyPatch {
                    input: input.to_string(),
                })
            }
            _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name)),
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
                    cmd_array
                        .iter()
                        .map(|v| v.as_str().unwrap_or_default().to_string())
                        .collect()
                } else {
                    return Err(anyhow::anyhow!("Invalid command format"));
                };

                let working_dir = value["workdir"].as_str().map(PathBuf::from);
                let timeout_ms = value["timeout_ms"].as_u64();
                let with_escalated_permissions = value["with_escalated_permissions"]
                    .as_bool()
                    .unwrap_or(false);
                let justification = value["justification"].as_str().map(String::from);

                Ok(ToolCall::Shell {
                    command,
                    working_dir,
                    with_escalated_permissions,
                    justification,
                    timeout_ms,
                })
            }
            "view_image" => {
                // NO-OP: 現状はコンテキストへ添付するだけの想定。将来必要なら画像解析へ拡張
                // 引数: { "path": "/abs/path/to/image.png" }
                // ここでは実際の画像処理は行わず、成功扱いのメッセージを返せるように
                // 上位でのハンドリングに委ねるため、簡易的にshellと同様の経路に載せない。
                // 本関数はToolCallを返す設計のため、view_imageは直接execute_function_call_structuredで
                // 取り扱う方が自然だが、互換のためにUnknown扱いを避ける。
                // ここではダミーとしてapply_patch相当のトンネルに入れるのは不自然なので、
                // 一旦エラーにせず実行側で弾かないよう、軽量な疑似コールへフォールバックする。
                // 簡易実装: pathの妥当性だけ検査して成功メッセージを返すための仮ツールとして扱う。
                let _path = value["path"].as_str().unwrap_or("");
                // 直接実行せず、上位のexecute_function_call経路でハンドルするためUnknownを返さない。
                // ここでは読み取り系の疑似ツールに変換してログに出すだけにする（最小無害）。
                Err(anyhow::anyhow!("view_image is a no-op function in this build"))
            }
            "apply_patch" => {
                let input = value["input"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing patch input"))?;
                Ok(ToolCall::ApplyPatch {
                    input: input.to_string(),
                })
            }
            _ => Err(anyhow::anyhow!("Unknown function: {}", name)),
        }
    }

    /// 個別のツール呼び出しを実行
    pub async fn execute_tool_call(&mut self, call: ToolCall) -> Result<String> {
        match call {
            ToolCall::Shell {
                command,
                working_dir,
                with_escalated_permissions,
                justification,
                timeout_ms,
            } => {
                self.execute_shell_command(
                    command,
                    working_dir,
                    with_escalated_permissions,
                    justification,
                    timeout_ms,
                )
                .await
            }
            ToolCall::ApplyPatch { input } => {
                let result = tool_apply_patch(ApplyPatchInput { patch: input.clone() }, true);
                if result.applied {
                    if let Some(tr) = &self.turn_diff_tracker {
                        let trc = tr.clone();
                        tokio::spawn(async move {
                            let mut g = trc.lock().await;
                            g.record_apply_patch_content(&input);
                        });
                    }
                    Ok(format!("Change Approved\n☑ {}", result.message))
                } else {
                    Ok(format!("Proposed Change failed\n{}", result.message))
                }
            }
        }
    }
}

impl ToolExecutor {
    async fn execute_shell_command(
        &self,
        command: Vec<String>,
        working_dir: Option<PathBuf>,
        with_escalated_permissions: bool,
        justification: Option<String>,
        timeout_ms: Option<u64>,
    ) -> Result<String> {
        if command.is_empty() {
            return Ok("Shell tool call did not include a command.".to_string());
        }

        if with_escalated_permissions {
            return Ok(
                "Command requested escalated permissions, which are not supported in this build."
                    .to_string(),
            );
        }

        // MEE-50: すべての実行をprocess_exec_tool_call経由に統一
        // 参考: codex-1/core/src/exec.rs
        let cwd = working_dir.unwrap_or_else(|| self.cwd.clone());
        let env_map = create_env(&self.shell_environment_policy);

        // justificationを後で使うためclone
        let justification_for_message = justification.clone();

        let params = crate::exec::ExecParams {
            command: command.clone(),
            cwd,
            timeout_ms,
            env: env_map,
            with_escalated_permissions: Some(with_escalated_permissions),
            justification,
        };

        // すべての実行が同一経路でSandboxPolicyを通る
        let result = crate::exec::process_exec_tool_call(
            params,
            crate::exec::SandboxType::None,
            &self.sandbox_policy,
            &None,
            None,
        )
        .await?;

        let mut message = format!(
            "Change Approved\n☑ Command `{}` exited with code {}",
            command.join(" "),
            result.exit_code
        );

        if !result.stdout.trim().is_empty() {
            message.push_str("\n\nSTDOUT:\n");
            message.push_str(result.stdout.trim_end());
        }

        if !result.stderr.trim().is_empty() {
            message.push_str("\n\nSTDERR:\n");
            message.push_str(result.stderr.trim_end());
        }

        if let Some(justification) = justification_for_message {
            if !justification.is_empty() {
                message.push_str(&format!("\n\nJustification: {}", justification));
            }
        }

        Ok(message)
    }

    /// 設定の更新
    pub fn update_working_directory(&mut self, new_cwd: PathBuf) {
        self.cwd = new_cwd;
    }

    /// Check if a path is writable according to the sandbox policy
    fn is_path_writable(&self, path: &std::path::Path) -> bool {
        let writable_roots = self.sandbox_policy.get_writable_roots_with_cwd(&self.cwd);
        writable_roots.iter().any(|root| root.is_path_writable(path))
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
    ApplyPatch {
        input: String,
    },
}

impl ToolCall {
    /// Short human-readable summary for logging or UI display.
    pub fn summary(&self) -> String {
        match self {
            ToolCall::Shell {
                command,
                working_dir,
                ..
            } => {
                let joined = command.join(" ");
                if let Some(dir) = working_dir {
                    format!("shell {} (cwd: {})", joined, dir.display())
                } else {
                    format!("shell {}", joined)
                }
            }
            ToolCall::ApplyPatch { input } => {
                format!("apply_patch ({} bytes)", input.len())
            }
        }
    }
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
        assert_eq!(calls.len(), 1);

        match &calls[0] {
            ToolCall::Shell { command, .. } => {
                assert_eq!(command, &vec!["cat".to_string(), "test.txt".to_string()]);
            }
            _ => panic!("Expected Shell tool call"),
        }
    }

}
