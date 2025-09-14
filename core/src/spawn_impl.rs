use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{Duration, Instant};

use crate::agent_executor::{AgentExecutor, AgentTask, AgentResult, TaskType, AgentCapabilities};
use crate::exec_impl::{ExecutionManager, ExecRequest};
use crate::file_operations::FileOperationManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub agent_id: String,
    pub task_type: TaskType,
    pub parameters: HashMap<String, String>,
    pub working_directory: Option<String>,
    pub timeout_seconds: u64,
    pub priority: SpawnPriority,
    pub dependencies: Vec<String>, // IDs of tasks that must complete first
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpawnPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnedAgent {
    pub agent_id: String,
    pub status: AgentStatus,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub result: Option<AgentResult>,
    pub progress: f32, // 0.0 to 1.0
    pub current_operation: Option<String>,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnManagerConfig {
    pub max_concurrent_agents: usize,
    pub default_timeout: Duration,
    pub log_retention_minutes: u64,
    pub enable_agent_monitoring: bool,
    pub sandbox_mode: bool,
}

impl Default for SpawnManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_agents: 10,
            default_timeout: Duration::from_secs(300), // 5 minutes
            log_retention_minutes: 60,
            enable_agent_monitoring: true,
            sandbox_mode: true,
        }
    }
}

pub struct SpawnManager {
    config: SpawnManagerConfig,
    agents: Arc<RwLock<HashMap<String, SpawnedAgent>>>,
    executors: Arc<RwLock<HashMap<String, Arc<AgentExecutor>>>>,
    running_count: Arc<Mutex<usize>>,
    working_directory: PathBuf,
    file_manager: Arc<FileOperationManager>,
    exec_manager: Arc<ExecutionManager>,
}

impl SpawnManager {
    pub fn new(working_directory: PathBuf) -> Self {
        let file_manager = Arc::new(FileOperationManager::new(working_directory.clone()));
        let exec_manager = Arc::new(ExecutionManager::new(working_directory.clone()));

        Self {
            config: SpawnManagerConfig::default(),
            agents: Arc::new(RwLock::new(HashMap::new())),
            executors: Arc::new(RwLock::new(HashMap::new())),
            running_count: Arc::new(Mutex::new(0)),
            working_directory,
            file_manager,
            exec_manager,
        }
    }

    pub fn with_config(mut self, config: SpawnManagerConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn spawn_agent(&self, request: SpawnRequest) -> Result<String> {
        // Validate the spawn request
        self.validate_spawn_request(&request).await?;

        // Check dependencies
        if !self.check_dependencies(&request.dependencies).await? {
            return Err(anyhow!("Dependencies not met for agent {}", request.agent_id));
        }

        // Check concurrent agent limit
        let running_count = *self.running_count.lock().await;
        if running_count >= self.config.max_concurrent_agents {
            return Err(anyhow!(
                "Maximum concurrent agents ({}) reached",
                self.config.max_concurrent_agents
            ));
        }

        // Create the spawned agent record
        let spawned_agent = SpawnedAgent {
            agent_id: request.agent_id.clone(),
            status: AgentStatus::Queued,
            created_at: Self::current_timestamp(),
            started_at: None,
            completed_at: None,
            result: None,
            progress: 0.0,
            current_operation: Some("Initializing".to_string()),
            logs: vec![format!("Agent {} queued", request.agent_id)],
        };

        // Store the agent
        {
            let mut agents = self.agents.write().await;
            agents.insert(request.agent_id.clone(), spawned_agent);
        }

        // Create the executor for this agent
        let executor = self.create_executor_for_request(&request).await?;
        {
            let mut executors = self.executors.write().await;
            executors.insert(request.agent_id.clone(), Arc::new(executor));
        }

        // Start the agent execution in a separate task
        let agent_id = request.agent_id.clone();
        let task = self.create_agent_task(&request)?;

        self.execute_agent_async(agent_id.clone(), task).await;

        Ok(agent_id)
    }

    pub async fn get_agent_status(&self, agent_id: &str) -> Option<SpawnedAgent> {
        let agents = self.agents.read().await;
        agents.get(agent_id).cloned()
    }

    pub async fn list_agents(&self) -> Vec<SpawnedAgent> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    pub async fn cancel_agent(&self, agent_id: &str) -> Result<()> {
        let mut agents = self.agents.write().await;

        if let Some(agent) = agents.get_mut(agent_id) {
            match agent.status {
                AgentStatus::Queued | AgentStatus::Running => {
                    agent.status = AgentStatus::Cancelled;
                    agent.completed_at = Some(Self::current_timestamp());
                    agent.logs.push(format!("Agent {} cancelled", agent_id));

                    // If it was running, decrement the counter
                    if matches!(agent.status, AgentStatus::Running) {
                        let mut count = self.running_count.lock().await;
                        *count = count.saturating_sub(1);
                    }

                    Ok(())
                },
                _ => Err(anyhow!("Agent {} cannot be cancelled in status {:?}", agent_id, agent.status)),
            }
        } else {
            Err(anyhow!("Agent {} not found", agent_id))
        }
    }

    pub async fn cleanup_completed_agents(&self) -> usize {
        let mut agents = self.agents.write().await;
        let mut executors = self.executors.write().await;

        let cutoff_time = Self::current_timestamp() - (self.config.log_retention_minutes * 60);
        let mut removed_count = 0;

        agents.retain(|id, agent| {
            let should_remove = matches!(
                agent.status,
                AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled | AgentStatus::Timeout
            ) && agent.completed_at.unwrap_or(0) < cutoff_time;

            if should_remove {
                executors.remove(id);
                removed_count += 1;
            }

            !should_remove
        });

        removed_count
    }

    async fn execute_agent_async(&self, agent_id: String, task: AgentTask) {
        // Clone necessary data for the async task
        let agents = self.agents.clone();
        let executors = self.executors.clone();
        let running_count = self.running_count.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            // Mark as running
            {
                let mut agents_lock = agents.write().await;
                if let Some(agent) = agents_lock.get_mut(&agent_id) {
                    agent.status = AgentStatus::Running;
                    agent.started_at = Some(Self::current_timestamp());
                    agent.progress = 0.1;
                    agent.current_operation = Some("Starting execution".to_string());
                    agent.logs.push(format!("Agent {} started execution", agent_id));
                }
            }

            // Increment running count
            {
                let mut count = running_count.lock().await;
                *count += 1;
            }

            // Get the executor
            let executor = {
                let executors_lock = executors.read().await;
                executors_lock.get(&agent_id).cloned()
            };

            let result = if let Some(executor) = executor {
                // Execute the task with timeout
                let timeout_duration = Duration::from_secs(task.timeout_seconds.max(1));

                match tokio::time::timeout(timeout_duration, executor.execute_task(task)).await {
                    Ok(Ok(result)) => Some((result, AgentStatus::Completed)),
                    Ok(Err(_)) => Some((
                        crate::agent_executor::AgentResult {
                            task_id: agent_id.clone(),
                            success: false,
                            message: "Task execution failed".to_string(),
                            output: None,
                            error: Some("Execution error".to_string()),
                            execution_time_ms: 0,
                            files_modified: vec![],
                        },
                        AgentStatus::Failed,
                    )),
                    Err(_) => Some((
                        crate::agent_executor::AgentResult {
                            task_id: agent_id.clone(),
                            success: false,
                            message: "Task execution timed out".to_string(),
                            output: None,
                            error: Some("Timeout".to_string()),
                            execution_time_ms: timeout_duration.as_millis(),
                            files_modified: vec![],
                        },
                        AgentStatus::Timeout,
                    )),
                }
            } else {
                Some((
                    crate::agent_executor::AgentResult {
                        task_id: agent_id.clone(),
                        success: false,
                        message: "Executor not found".to_string(),
                        output: None,
                        error: Some("No executor".to_string()),
                        execution_time_ms: 0,
                        files_modified: vec![],
                    },
                    AgentStatus::Failed,
                ))
            };

            // Update agent status
            {
                let mut agents_lock = agents.write().await;
                if let Some(agent) = agents_lock.get_mut(&agent_id) {
                    if let Some((task_result, status)) = result {
                        agent.result = Some(task_result);
                        agent.status = status;
                        agent.completed_at = Some(Self::current_timestamp());
                        agent.progress = 1.0;
                        agent.current_operation = Some("Completed".to_string());
                        agent.logs.push(format!("Agent {} completed", agent_id));
                    }
                }
            }

            // Decrement running count
            {
                let mut count = running_count.lock().await;
                *count = count.saturating_sub(1);
            }
        });
    }

    async fn validate_spawn_request(&self, request: &SpawnRequest) -> Result<()> {
        if request.agent_id.is_empty() {
            return Err(anyhow!("Agent ID cannot be empty"));
        }

        // Check if agent ID is already in use
        let agents = self.agents.read().await;
        if agents.contains_key(&request.agent_id) {
            return Err(anyhow!("Agent ID '{}' is already in use", request.agent_id));
        }

        // Validate working directory
        if let Some(ref wd) = request.working_directory {
            let wd_path = PathBuf::from(wd);
            if self.config.sandbox_mode && wd_path.is_absolute() {
                let canonical_wd = wd_path.canonicalize().unwrap_or(wd_path);
                let canonical_workspace = self.working_directory.canonicalize()
                    .unwrap_or_else(|_| self.working_directory.clone());

                if !canonical_wd.starts_with(&canonical_workspace) {
                    return Err(anyhow!(
                        "Working directory '{}' is outside the workspace",
                        wd
                    ));
                }
            }
        }

        Ok(())
    }

    async fn check_dependencies(&self, dependencies: &[String]) -> Result<bool> {
        if dependencies.is_empty() {
            return Ok(true);
        }

        let agents = self.agents.read().await;

        for dep_id in dependencies {
            if let Some(dep_agent) = agents.get(dep_id) {
                match dep_agent.status {
                    AgentStatus::Completed => continue,
                    AgentStatus::Failed | AgentStatus::Cancelled | AgentStatus::Timeout => {
                        return Ok(false);
                    },
                    AgentStatus::Queued | AgentStatus::Running => {
                        return Ok(false);
                    },
                }
            } else {
                return Err(anyhow!("Dependency agent '{}' not found", dep_id));
            }
        }

        Ok(true)
    }

    async fn create_executor_for_request(&self, request: &SpawnRequest) -> Result<AgentExecutor> {
        let working_dir = request.working_directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.working_directory.clone());

        let capabilities = AgentCapabilities {
            file_operations: true,
            shell_commands: true,
            code_generation: true,
            network_access: !self.config.sandbox_mode,
            sandbox_mode: self.config.sandbox_mode,
        };

        let executor = AgentExecutor::new(working_dir)
            .with_capabilities(capabilities)
            .with_sandbox_mode(self.config.sandbox_mode)
            .with_max_execution_time(Duration::from_secs(request.timeout_seconds));

        Ok(executor)
    }

    fn create_agent_task(&self, request: &SpawnRequest) -> Result<AgentTask> {
        Ok(AgentTask {
            id: request.agent_id.clone(),
            task_type: request.task_type.clone(),
            parameters: request.parameters.clone(),
            timeout_seconds: request.timeout_seconds,
            working_directory: request.working_directory.clone(),
        })
    }

    pub async fn get_system_metrics(&self) -> HashMap<String, serde_json::Value> {
        let mut metrics = HashMap::new();

        let agents = self.agents.read().await;
        let running_count = *self.running_count.lock().await;

        // Agent statistics
        let total_agents = agents.len();
        let queued_count = agents.values().filter(|a| matches!(a.status, AgentStatus::Queued)).count();
        let completed_count = agents.values().filter(|a| matches!(a.status, AgentStatus::Completed)).count();
        let failed_count = agents.values().filter(|a| matches!(a.status, AgentStatus::Failed)).count();

        metrics.insert("total_agents".to_string(), total_agents.into());
        metrics.insert("running_agents".to_string(), running_count.into());
        metrics.insert("queued_agents".to_string(), queued_count.into());
        metrics.insert("completed_agents".to_string(), completed_count.into());
        metrics.insert("failed_agents".to_string(), failed_count.into());

        // Configuration
        metrics.insert("max_concurrent_agents".to_string(), self.config.max_concurrent_agents.into());
        metrics.insert("sandbox_mode".to_string(), self.config.sandbox_mode.into());

        // System info
        let system_info = self.exec_manager.get_system_info().await;
        for (key, value) in system_info {
            metrics.insert(format!("system_{}", key), value.into());
        }

        metrics
    }

    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_spawn_manager_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SpawnManager::new(temp_dir.path().to_path_buf());

        let request = SpawnRequest {
            agent_id: "test-agent-1".to_string(),
            task_type: TaskType::CodeGeneration,
            parameters: {
                let mut params = HashMap::new();
                params.insert("language".to_string(), "rust".to_string());
                params.insert("description".to_string(), "hello world".to_string());
                params
            },
            working_directory: None,
            timeout_seconds: 10,
            priority: SpawnPriority::Normal,
            dependencies: vec![],
        };

        let agent_id = manager.spawn_agent(request).await.unwrap();
        assert_eq!(agent_id, "test-agent-1");

        // Wait a bit for async execution
        tokio::time::sleep(Duration::from_millis(100)).await;

        let agent_status = manager.get_agent_status(&agent_id).await.unwrap();
        assert!(matches!(agent_status.status, AgentStatus::Running | AgentStatus::Completed));
    }
}