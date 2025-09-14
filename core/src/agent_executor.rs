use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::file_operations::{FileOperationManager, FileOperationRequest, FileOperationResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub task_type: TaskType,
    pub parameters: HashMap<String, String>,
    pub timeout_seconds: u64,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    FileOperation,
    ShellCommand,
    CodeGeneration,
    CodeAnalysis,
    TestExecution,
    ProjectSetup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub task_id: String,
    pub success: bool,
    pub message: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub execution_time_ms: u128,
    pub files_modified: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub file_operations: bool,
    pub shell_commands: bool,
    pub code_generation: bool,
    pub network_access: bool,
    pub sandbox_mode: bool,
}

pub struct AgentExecutor {
    pub working_directory: PathBuf,
    pub file_manager: FileOperationManager,
    pub capabilities: AgentCapabilities,
    pub max_execution_time: Duration,
    pub sandbox_mode: bool,
}

impl Default for AgentExecutor {
    fn default() -> Self {
        let working_dir = std::env::current_dir().unwrap_or_default();
        let file_manager = FileOperationManager::new(working_dir.clone());

        Self {
            working_directory: working_dir,
            file_manager,
            capabilities: AgentCapabilities {
                file_operations: true,
                shell_commands: true,
                code_generation: true,
                network_access: false,
                sandbox_mode: true,
            },
            max_execution_time: Duration::from_secs(300), // 5 minutes
            sandbox_mode: true,
        }
    }
}

impl AgentExecutor {
    pub fn new(working_directory: PathBuf) -> Self {
        let file_manager = FileOperationManager::new(working_directory.clone());

        Self {
            working_directory,
            file_manager,
            capabilities: AgentCapabilities {
                file_operations: true,
                shell_commands: true,
                code_generation: true,
                network_access: false,
                sandbox_mode: true,
            },
            max_execution_time: Duration::from_secs(300),
            sandbox_mode: true,
        }
    }

    pub fn with_capabilities(mut self, capabilities: AgentCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_sandbox_mode(mut self, sandbox: bool) -> Self {
        self.sandbox_mode = sandbox;
        self.file_manager = self.file_manager.allow_outside_workspace(!sandbox);
        self
    }

    pub fn with_max_execution_time(mut self, duration: Duration) -> Self {
        self.max_execution_time = duration;
        self
    }

    pub async fn execute_task(&self, task: AgentTask) -> Result<AgentResult> {
        let start_time = std::time::Instant::now();
        let task_timeout = Duration::from_secs(task.timeout_seconds.max(1));
        let effective_timeout = self.max_execution_time.min(task_timeout);

        let result = timeout(effective_timeout, self.execute_task_internal(task.clone())).await;

        let execution_time = start_time.elapsed();

        match result {
            Ok(Ok(mut agent_result)) => {
                agent_result.execution_time_ms = execution_time.as_millis();
                Ok(agent_result)
            },
            Ok(Err(e)) => Ok(AgentResult {
                task_id: task.id,
                success: false,
                message: format!("Task execution failed: {}", e),
                output: None,
                error: Some(e.to_string()),
                execution_time_ms: execution_time.as_millis(),
                files_modified: vec![],
            }),
            Err(_) => Ok(AgentResult {
                task_id: task.id,
                success: false,
                message: "Task execution timed out".to_string(),
                output: None,
                error: Some("Execution timed out".to_string()),
                execution_time_ms: execution_time.as_millis(),
                files_modified: vec![],
            }),
        }
    }

    async fn execute_task_internal(&self, task: AgentTask) -> Result<AgentResult> {
        match task.task_type {
            TaskType::FileOperation => self.execute_file_operation(task).await,
            TaskType::ShellCommand => self.execute_shell_command(task).await,
            TaskType::CodeGeneration => self.execute_code_generation(task).await,
            TaskType::CodeAnalysis => self.execute_code_analysis(task).await,
            TaskType::TestExecution => self.execute_test_execution(task).await,
            TaskType::ProjectSetup => self.execute_project_setup(task).await,
        }
    }

    async fn execute_file_operation(&self, task: AgentTask) -> Result<AgentResult> {
        if !self.capabilities.file_operations {
            return Ok(AgentResult {
                task_id: task.id,
                success: false,
                message: "File operations are disabled".to_string(),
                output: None,
                error: Some("Capability disabled".to_string()),
                execution_time_ms: 0,
                files_modified: vec![],
            });
        }

        let operation = task.parameters.get("operation")
            .ok_or_else(|| anyhow!("Missing 'operation' parameter"))?;

        let file_path = task.parameters.get("path")
            .ok_or_else(|| anyhow!("Missing 'path' parameter"))?;

        let content = task.parameters.get("content");
        let backup = task.parameters.get("backup")
            .map(|v| v == "true")
            .unwrap_or(false);

        let file_operation = match operation.as_str() {
            "read" => crate::file_operations::FileOperation::Read,
            "write" => crate::file_operations::FileOperation::Write,
            "create" => crate::file_operations::FileOperation::Create,
            "delete" => crate::file_operations::FileOperation::Delete,
            "list" => crate::file_operations::FileOperation::List,
            "backup" => crate::file_operations::FileOperation::Backup,
            "restore" => crate::file_operations::FileOperation::Restore,
            _ => return Err(anyhow!("Unsupported file operation: {}", operation)),
        };

        let request = FileOperationRequest {
            operation: file_operation,
            path: file_path.clone(),
            content: content.cloned(),
            backup,
        };

        let file_result = self.file_manager.execute_operation(request).await?;

        let files_modified = if file_result.success && matches!(operation.as_str(), "write" | "create" | "delete") {
            vec![file_path.clone()]
        } else {
            vec![]
        };

        Ok(AgentResult {
            task_id: task.id,
            success: file_result.success,
            message: file_result.message,
            output: file_result.content,
            error: if file_result.success { None } else { Some("File operation failed".to_string()) },
            execution_time_ms: 0, // Will be set by caller
            files_modified,
        })
    }

    async fn execute_shell_command(&self, task: AgentTask) -> Result<AgentResult> {
        if !self.capabilities.shell_commands {
            return Ok(AgentResult {
                task_id: task.id,
                success: false,
                message: "Shell commands are disabled".to_string(),
                output: None,
                error: Some("Capability disabled".to_string()),
                execution_time_ms: 0,
                files_modified: vec![],
            });
        }

        let command = task.parameters.get("command")
            .ok_or_else(|| anyhow!("Missing 'command' parameter"))?;

        let working_dir = if let Some(dir) = task.working_directory {
            PathBuf::from(dir)
        } else {
            self.working_directory.clone()
        };

        // Security check for sandbox mode
        if self.sandbox_mode {
            if self.is_dangerous_command(command) {
                return Ok(AgentResult {
                    task_id: task.id,
                    success: false,
                    message: "Dangerous command blocked in sandbox mode".to_string(),
                    output: None,
                    error: Some("Command blocked for security".to_string()),
                    execution_time_ms: 0,
                    files_modified: vec![],
                });
            }
        }

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };

        cmd.current_dir(working_dir);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let success = output.status.success();
        let combined_output = if stderr.is_empty() {
            stdout
        } else if stdout.is_empty() {
            stderr.clone()
        } else {
            format!("STDOUT:\n{}\n\nSTDERR:\n{}", stdout, stderr)
        };

        Ok(AgentResult {
            task_id: task.id,
            success,
            message: if success {
                "Command executed successfully".to_string()
            } else {
                format!("Command failed with exit code: {:?}", output.status.code())
            },
            output: Some(combined_output),
            error: if success { None } else { Some(stderr) },
            execution_time_ms: 0, // Will be set by caller
            files_modified: vec![], // Could be enhanced to detect file changes
        })
    }

    async fn execute_code_generation(&self, task: AgentTask) -> Result<AgentResult> {
        if !self.capabilities.code_generation {
            return Ok(AgentResult {
                task_id: task.id,
                success: false,
                message: "Code generation is disabled".to_string(),
                output: None,
                error: Some("Capability disabled".to_string()),
                execution_time_ms: 0,
                files_modified: vec![],
            });
        }

        // This is a placeholder for AI-powered code generation
        // In a real implementation, this would interface with an AI model
        let default_language = "rust".to_string();
        let language = task.parameters.get("language").unwrap_or(&default_language);
        let description = task.parameters.get("description")
            .ok_or_else(|| anyhow!("Missing 'description' parameter"))?;

        let generated_code = self.generate_code_placeholder(language, description);

        Ok(AgentResult {
            task_id: task.id,
            success: true,
            message: format!("Generated {} code for: {}", language, description),
            output: Some(generated_code),
            error: None,
            execution_time_ms: 0,
            files_modified: vec![],
        })
    }

    async fn execute_code_analysis(&self, task: AgentTask) -> Result<AgentResult> {
        let file_path = task.parameters.get("file")
            .ok_or_else(|| anyhow!("Missing 'file' parameter"))?;

        // Read the file content
        let request = FileOperationRequest {
            operation: crate::file_operations::FileOperation::Read,
            path: file_path.clone(),
            content: None,
            backup: false,
        };

        let file_result = self.file_manager.execute_operation(request).await?;

        if !file_result.success {
            return Ok(AgentResult {
                task_id: task.id,
                success: false,
                message: format!("Failed to read file: {}", file_result.message),
                output: None,
                error: Some("File read failed".to_string()),
                execution_time_ms: 0,
                files_modified: vec![],
            });
        }

        let content = file_result.content.unwrap_or_default();
        let analysis = self.analyze_code_placeholder(&content, file_path);

        Ok(AgentResult {
            task_id: task.id,
            success: true,
            message: format!("Analyzed file: {}", file_path),
            output: Some(analysis),
            error: None,
            execution_time_ms: 0,
            files_modified: vec![],
        })
    }

    async fn execute_test_execution(&self, task: AgentTask) -> Result<AgentResult> {
        let default_test_command = "cargo test".to_string();
        let test_command = task.parameters.get("test_command")
            .unwrap_or(&default_test_command);

        // Execute the test command
        let test_task = AgentTask {
            id: format!("{}-test", task.id),
            task_type: TaskType::ShellCommand,
            parameters: {
                let mut params = HashMap::new();
                params.insert("command".to_string(), test_command.clone());
                params
            },
            timeout_seconds: task.timeout_seconds,
            working_directory: task.working_directory,
        };

        self.execute_shell_command(test_task).await
    }

    async fn execute_project_setup(&self, task: AgentTask) -> Result<AgentResult> {
        let project_type = task.parameters.get("type")
            .ok_or_else(|| anyhow!("Missing 'type' parameter"))?;

        let project_name = task.parameters.get("name")
            .ok_or_else(|| anyhow!("Missing 'name' parameter"))?;

        let setup_command = match project_type.as_str() {
            "rust" => format!("cargo init {}", project_name),
            "node" => format!("npm init -y && mkdir {}", project_name),
            "python" => format!("mkdir {} && cd {} && python -m venv venv", project_name, project_name),
            _ => return Err(anyhow!("Unsupported project type: {}", project_type)),
        };

        // Execute the setup command
        let setup_task = AgentTask {
            id: format!("{}-setup", task.id),
            task_type: TaskType::ShellCommand,
            parameters: {
                let mut params = HashMap::new();
                params.insert("command".to_string(), setup_command);
                params
            },
            timeout_seconds: task.timeout_seconds,
            working_directory: task.working_directory,
        };

        self.execute_shell_command(setup_task).await
    }

    fn is_dangerous_command(&self, command: &str) -> bool {
        let dangerous_commands = [
            "rm -rf", "del /f", "format", "fdisk", "mkfs",
            "shutdown", "reboot", "halt", "poweroff",
            "curl", "wget", "nc", "netcat", "ssh", "scp",
            "sudo", "su", "chmod 777", "chown",
        ];

        dangerous_commands.iter().any(|&dangerous| command.contains(dangerous))
    }

    fn generate_code_placeholder(&self, language: &str, description: &str) -> String {
        match language {
            "rust" => format!(
                "// Generated Rust code for: {}\n\
                fn main() {{\n    \
                    // TODO: Implement {}\n    \
                    println!(\"Hello, World!\");\n\
                }}\n",
                description, description
            ),
            "python" => format!(
                "# Generated Python code for: {}\n\
                def main():\n    \
                    # TODO: Implement {}\n    \
                    print(\"Hello, World!\")\n\n\
                if __name__ == \"__main__\":\n    \
                    main()\n",
                description, description
            ),
            "javascript" => format!(
                "// Generated JavaScript code for: {}\n\
                function main() {{\n    \
                    // TODO: Implement {}\n    \
                    console.log(\"Hello, World!\");\n\
                }}\n\n\
                main();\n",
                description, description
            ),
            _ => format!(
                "// Generated code for: {}\n\
                // Language: {}\n\
                // TODO: Implement the functionality\n",
                description, language
            ),
        }
    }

    fn analyze_code_placeholder(&self, content: &str, file_path: &str) -> String {
        let lines = content.lines().count();
        let chars = content.chars().count();
        let functions = content.matches("fn ").count() + content.matches("function ").count() + content.matches("def ").count();

        format!(
            "Code Analysis for: {}\n\
            ===================\n\
            Lines of code: {}\n\
            Characters: {}\n\
            Functions detected: {}\n\
            \n\
            File appears to be: {}\n\
            \n\
            Suggestions:\n\
            - Consider adding documentation\n\
            - Add unit tests if not present\n\
            - Review error handling\n",
            file_path,
            lines,
            chars,
            functions,
            self.detect_language(file_path)
        )
    }

    fn detect_language(&self, file_path: &str) -> &str {
        if file_path.ends_with(".rs") { "Rust" }
        else if file_path.ends_with(".py") { "Python" }
        else if file_path.ends_with(".js") || file_path.ends_with(".ts") { "JavaScript/TypeScript" }
        else if file_path.ends_with(".go") { "Go" }
        else if file_path.ends_with(".cpp") || file_path.ends_with(".cc") { "C++" }
        else if file_path.ends_with(".c") { "C" }
        else if file_path.ends_with(".java") { "Java" }
        else { "Unknown" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_agent_executor_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let executor = AgentExecutor::new(temp_dir.path().to_path_buf());

        let task = AgentTask {
            id: "test-1".to_string(),
            task_type: TaskType::FileOperation,
            parameters: {
                let mut params = HashMap::new();
                params.insert("operation".to_string(), "create".to_string());
                params.insert("path".to_string(), "test.txt".to_string());
                params.insert("content".to_string(), "Hello from agent!".to_string());
                params
            },
            timeout_seconds: 10,
            working_directory: None,
        };

        let result = executor.execute_task(task).await.unwrap();
        assert!(result.success);
        assert_eq!(result.files_modified.len(), 1);
    }

    #[tokio::test]
    async fn test_code_generation() {
        let temp_dir = TempDir::new().unwrap();
        let executor = AgentExecutor::new(temp_dir.path().to_path_buf());

        let task = AgentTask {
            id: "gen-1".to_string(),
            task_type: TaskType::CodeGeneration,
            parameters: {
                let mut params = HashMap::new();
                params.insert("language".to_string(), "rust".to_string());
                params.insert("description".to_string(), "hello world program".to_string());
                params
            },
            timeout_seconds: 10,
            working_directory: None,
        };

        let result = executor.execute_task(task).await.unwrap();
        assert!(result.success);
        assert!(result.output.unwrap().contains("fn main"));
    }
}