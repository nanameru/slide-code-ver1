use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::safety_impl::{SandboxPolicy, assess_command_safety, SafetyCheck};
use slide_common::ApprovalMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub description: String,
    pub commands: Vec<String>,
    pub dependencies: Vec<String>,
    pub estimated_duration: u32,
    pub status: StepStatus,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    Pending,
    Ready,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: String,
    pub title: String,
    pub description: String,
    pub steps: HashMap<String, PlanStep>,
    pub execution_order: Vec<String>,
    pub status: PlanStatus,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub workspace_root: PathBuf,
    pub approval_mode: ApprovalMode,
    pub sandbox_policy: SandboxPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlanStatus {
    Draft,
    Ready,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct PlanExecutionConfig {
    pub workspace_root: PathBuf,
    pub approval_mode: ApprovalMode,
    pub sandbox_policy: SandboxPolicy,
    pub parallel_execution: bool,
    pub stop_on_failure: bool,
    pub max_concurrent_steps: usize,
}

impl Default for PlanExecutionConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("."),
            approval_mode: ApprovalMode::Suggest,
            sandbox_policy: SandboxPolicy::WorkspaceWrite,
            parallel_execution: true,
            stop_on_failure: true,
            max_concurrent_steps: 3,
        }
    }
}

/// Advanced plan management system for complex task orchestration
pub struct PlanManager {
    plans: HashMap<String, ExecutionPlan>,
    active_plan: Option<String>,
    config: PlanExecutionConfig,
}

impl PlanManager {
    pub fn new(config: PlanExecutionConfig) -> Self {
        Self {
            plans: HashMap::new(),
            active_plan: None,
            config,
        }
    }

    /// Create a new execution plan
    pub fn create_plan(
        &mut self,
        title: String,
        description: String,
        steps: Vec<(String, String, Vec<String>)>, // (title, description, commands)
    ) -> Result<String> {
        let plan_id = Uuid::new_v4().to_string();
        let now = current_timestamp();

        let mut plan_steps = HashMap::new();
        let mut execution_order = Vec::new();

        for (i, (step_title, step_desc, commands)) in steps.into_iter().enumerate() {
            let step_id = format!("step_{}", i + 1);
            
            let step = PlanStep {
                id: step_id.clone(),
                title: step_title,
                description: step_desc,
                commands,
                dependencies: if i > 0 { vec![format!("step_{}", i)] } else { Vec::new() },
                estimated_duration: 30, // Default 30 seconds
                status: StepStatus::Pending,
                created_at: now,
                started_at: None,
                completed_at: None,
                error_message: None,
            };

            plan_steps.insert(step_id.clone(), step);
            execution_order.push(step_id);
        }

        let plan = ExecutionPlan {
            id: plan_id.clone(),
            title,
            description,
            steps: plan_steps,
            execution_order,
            status: PlanStatus::Draft,
            created_at: now,
            started_at: None,
            completed_at: None,
            workspace_root: self.config.workspace_root.clone(),
            approval_mode: self.config.approval_mode.clone(),
            sandbox_policy: self.config.sandbox_policy,
        };

        self.plans.insert(plan_id.clone(), plan);
        tracing::info!("Created execution plan: {}", plan_id);

        Ok(plan_id)
    }

    /// Add a step to an existing plan
    pub fn add_step(
        &mut self,
        plan_id: &str,
        title: String,
        description: String,
        commands: Vec<String>,
        dependencies: Vec<String>,
    ) -> Result<String> {
        let plan = self.plans.get_mut(plan_id)
            .context("Plan not found")?;

        if plan.status != PlanStatus::Draft {
            anyhow::bail!("Cannot modify plan in {} status", plan.status as u8);
        }

        let step_id = Uuid::new_v4().to_string();
        let step = PlanStep {
            id: step_id.clone(),
            title,
            description,
            commands,
            dependencies,
            estimated_duration: 30,
            status: StepStatus::Pending,
            created_at: current_timestamp(),
            started_at: None,
            completed_at: None,
            error_message: None,
        };

        plan.steps.insert(step_id.clone(), step);
        plan.execution_order.push(step_id.clone());

        tracing::info!("Added step {} to plan {}", step_id, plan_id);
        Ok(step_id)
    }

    /// Validate plan before execution
    pub fn validate_plan(&self, plan_id: &str) -> Result<Vec<String>> {
        let plan = self.plans.get(plan_id)
            .context("Plan not found")?;

        let mut warnings = Vec::new();

        // Check for circular dependencies
        if let Err(e) = self.check_circular_dependencies(plan) {
            warnings.push(format!("Circular dependency detected: {}", e));
        }

        // Validate all commands in steps
        for step in plan.steps.values() {
            for command in &step.commands {
                match assess_command_safety(
                    &[command.clone()],
                    plan.approval_mode.clone(),
                    &plan.sandbox_policy,
                    &std::collections::HashSet::new(),
                    false,
                ) {
                    SafetyCheck::Reject { reason } => {
                        warnings.push(format!("Unsafe command in step '{}': {} ({})", 
                            step.title, command, reason));
                    }
                    SafetyCheck::AskUser => {
                        warnings.push(format!("Command requires approval in step '{}': {}", 
                            step.title, command));
                    }
                    SafetyCheck::AutoApprove => {}
                }
            }
        }

        // Check if all dependencies exist
        for step in plan.steps.values() {
            for dep in &step.dependencies {
                if !plan.steps.contains_key(dep) {
                    warnings.push(format!("Step '{}' depends on non-existent step '{}'", 
                        step.title, dep));
                }
            }
        }

        Ok(warnings)
    }

    /// Prepare plan for execution
    pub fn prepare_plan(&mut self, plan_id: &str) -> Result<()> {
        let plan = self.plans.get_mut(plan_id)
            .context("Plan not found")?;

        if plan.status != PlanStatus::Draft {
            anyhow::bail!("Plan is not in draft status");
        }

        // Validate plan
        let warnings = self.validate_plan(plan_id)?;
        if !warnings.is_empty() {
            tracing::warn!("Plan validation warnings: {:?}", warnings);
        }

        // Update step statuses based on dependencies
        self.update_step_readiness(plan_id)?;
        
        plan.status = PlanStatus::Ready;
        tracing::info!("Plan {} is ready for execution", plan_id);

        Ok(())
    }

    /// Update step readiness based on dependencies
    fn update_step_readiness(&mut self, plan_id: &str) -> Result<()> {
        let plan = self.plans.get_mut(plan_id)
            .context("Plan not found")?;

        let mut updated = true;
        while updated {
            updated = false;
            
            for step_id in plan.execution_order.clone() {
                let step = plan.steps.get(&step_id).unwrap();
                
                if step.status == StepStatus::Pending {
                    let dependencies_completed = step.dependencies.iter()
                        .all(|dep_id| {
                            plan.steps.get(dep_id)
                                .map(|dep| dep.status == StepStatus::Completed)
                                .unwrap_or(false)
                        });
                    
                    if dependencies_completed {
                        plan.steps.get_mut(&step_id).unwrap().status = StepStatus::Ready;
                        updated = true;
                    }
                }
            }
        }

        Ok(())
    }

    /// Start plan execution
    pub async fn start_plan(&mut self, plan_id: &str) -> Result<()> {
        let plan = self.plans.get_mut(plan_id)
            .context("Plan not found")?;

        if plan.status != PlanStatus::Ready {
            anyhow::bail!("Plan is not ready for execution");
        }

        plan.status = PlanStatus::InProgress;
        plan.started_at = Some(current_timestamp());
        self.active_plan = Some(plan_id.to_string());

        tracing::info!("Started execution of plan: {}", plan_id);

        // In a real implementation, this would start async execution
        // For now, we'll just mark it as started
        Ok(())
    }

    /// Execute next ready step
    pub async fn execute_next_step(&mut self, plan_id: &str) -> Result<Option<String>> {
        let plan = self.plans.get(plan_id)
            .context("Plan not found")?;

        if plan.status != PlanStatus::InProgress {
            return Ok(None);
        }

        // Find next ready step
        let next_step_id = plan.execution_order.iter()
            .find(|step_id| {
                plan.steps.get(*step_id)
                    .map(|step| step.status == StepStatus::Ready)
                    .unwrap_or(false)
            })
            .cloned();

        if let Some(step_id) = next_step_id {
            self.execute_step(plan_id, &step_id).await?;
            Ok(Some(step_id))
        } else {
            // Check if plan is complete
            let all_completed = plan.steps.values()
                .all(|step| matches!(step.status, StepStatus::Completed | StepStatus::Skipped));
            
            if all_completed {
                self.complete_plan(plan_id)?;
            }
            
            Ok(None)
        }
    }

    /// Execute a specific step
    async fn execute_step(&mut self, plan_id: &str, step_id: &str) -> Result<()> {
        let plan = self.plans.get_mut(plan_id)
            .context("Plan not found")?;
        
        let step = plan.steps.get_mut(step_id)
            .context("Step not found")?;

        step.status = StepStatus::InProgress;
        step.started_at = Some(current_timestamp());

        tracing::info!("Executing step: {} - {}", step_id, step.title);

        // In a real implementation, this would execute the commands
        // For now, we'll simulate execution
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Mark as completed
        let step = plan.steps.get_mut(step_id).unwrap();
        step.status = StepStatus::Completed;
        step.completed_at = Some(current_timestamp());

        // Update dependent steps
        self.update_step_readiness(plan_id)?;

        Ok(())
    }

    /// Complete plan execution
    fn complete_plan(&mut self, plan_id: &str) -> Result<()> {
        let plan = self.plans.get_mut(plan_id)
            .context("Plan not found")?;

        plan.status = PlanStatus::Completed;
        plan.completed_at = Some(current_timestamp());

        if self.active_plan.as_ref() == Some(plan_id) {
            self.active_plan = None;
        }

        tracing::info!("Completed execution of plan: {}", plan_id);
        Ok(())
    }

    /// Cancel plan execution
    pub fn cancel_plan(&mut self, plan_id: &str) -> Result<()> {
        let plan = self.plans.get_mut(plan_id)
            .context("Plan not found")?;

        plan.status = PlanStatus::Cancelled;
        plan.completed_at = Some(current_timestamp());

        if self.active_plan.as_ref() == Some(plan_id) {
            self.active_plan = None;
        }

        tracing::info!("Cancelled execution of plan: {}", plan_id);
        Ok(())
    }

    /// Get plan status
    pub fn get_plan(&self, plan_id: &str) -> Option<&ExecutionPlan> {
        self.plans.get(plan_id)
    }

    /// List all plans
    pub fn list_plans(&self) -> Vec<&ExecutionPlan> {
        self.plans.values().collect()
    }

    /// Get active plan
    pub fn get_active_plan(&self) -> Option<&ExecutionPlan> {
        self.active_plan.as_ref()
            .and_then(|id| self.plans.get(id))
    }

    /// Check for circular dependencies
    fn check_circular_dependencies(&self, plan: &ExecutionPlan) -> Result<()> {
        use std::collections::HashSet;

        fn visit(
            step_id: &str,
            plan: &ExecutionPlan,
            visiting: &mut HashSet<String>,
            visited: &mut HashSet<String>,
        ) -> Result<()> {
            if visiting.contains(step_id) {
                anyhow::bail!("Circular dependency involving step: {}", step_id);
            }
            if visited.contains(step_id) {
                return Ok(());
            }

            visiting.insert(step_id.to_string());

            if let Some(step) = plan.steps.get(step_id) {
                for dep in &step.dependencies {
                    visit(dep, plan, visiting, visited)?;
                }
            }

            visiting.remove(step_id);
            visited.insert(step_id.to_string());
            Ok(())
        }

        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();

        for step_id in &plan.execution_order {
            if !visited.contains(step_id) {
                visit(step_id, plan, &mut visiting, &mut visited)?;
            }
        }

        Ok(())
    }

    /// Save plan to file
    pub fn save_plan(&self, plan_id: &str, path: &Path) -> Result<()> {
        let plan = self.plans.get(plan_id)
            .context("Plan not found")?;

        let json = serde_json::to_string_pretty(plan)
            .context("Failed to serialize plan")?;

        std::fs::write(path, json)
            .context("Failed to write plan file")?;

        tracing::info!("Saved plan {} to {}", plan_id, path.display());
        Ok(())
    }

    /// Load plan from file
    pub fn load_plan(&mut self, path: &Path) -> Result<String> {
        let content = std::fs::read_to_string(path)
            .context("Failed to read plan file")?;

        let plan: ExecutionPlan = serde_json::from_str(&content)
            .context("Failed to deserialize plan")?;

        let plan_id = plan.id.clone();
        self.plans.insert(plan_id.clone(), plan);

        tracing::info!("Loaded plan {} from {}", plan_id, path.display());
        Ok(plan_id)
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Create a simple sequential plan
pub fn create_sequential_plan(
    title: String,
    description: String,
    commands: Vec<String>,
) -> Vec<(String, String, Vec<String>)> {
    commands.into_iter()
        .enumerate()
        .map(|(i, cmd)| {
            (
                format!("Step {}", i + 1),
                format!("Execute: {}", cmd),
                vec![cmd],
            )
        })
        .collect()
}

/// Create a parallel plan (steps that can run concurrently)
pub fn create_parallel_plan(
    title: String,
    description: String,
    step_groups: Vec<Vec<String>>,
) -> Vec<(String, String, Vec<String>)> {
    step_groups.into_iter()
        .enumerate()
        .map(|(i, commands)| {
            (
                format!("Group {}", i + 1),
                format!("Execute {} commands in parallel", commands.len()),
                commands,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_plan() {
        let config = PlanExecutionConfig::default();
        let mut manager = PlanManager::new(config);

        let steps = vec![
            ("Setup".to_string(), "Initialize project".to_string(), vec!["mkdir test".to_string()]),
            ("Build".to_string(), "Compile project".to_string(), vec!["cargo build".to_string()]),
        ];

        let plan_id = manager.create_plan(
            "Test Plan".to_string(),
            "A test plan".to_string(),
            steps,
        ).unwrap();

        let plan = manager.get_plan(&plan_id).unwrap();
        assert_eq!(plan.title, "Test Plan");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.status, PlanStatus::Draft);
    }

    #[test]
    fn test_plan_validation() {
        let config = PlanExecutionConfig::default();
        let mut manager = PlanManager::new(config);

        let steps = vec![
            ("Safe".to_string(), "Safe command".to_string(), vec!["ls -la".to_string()]),
            ("Unsafe".to_string(), "Unsafe command".to_string(), vec!["rm -rf /".to_string()]),
        ];

        let plan_id = manager.create_plan(
            "Test Plan".to_string(),
            "A test plan".to_string(),
            steps,
        ).unwrap();

        let warnings = manager.validate_plan(&plan_id).unwrap();
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("Unsafe command")));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let config = PlanExecutionConfig::default();
        let mut manager = PlanManager::new(config);

        let plan_id = manager.create_plan(
            "Test Plan".to_string(),
            "A test plan".to_string(),
            vec![],
        ).unwrap();

        // Add steps with circular dependency
        let step1 = manager.add_step(
            &plan_id,
            "Step 1".to_string(),
            "First step".to_string(),
            vec!["echo step1".to_string()],
            vec!["step_2".to_string()],
        ).unwrap();

        let step2 = manager.add_step(
            &plan_id,
            "Step 2".to_string(),
            "Second step".to_string(),
            vec!["echo step2".to_string()],
            vec![step1],
        ).unwrap();

        let warnings = manager.validate_plan(&plan_id).unwrap();
        assert!(warnings.iter().any(|w| w.contains("Circular dependency")));
    }

    #[tokio::test]
    async fn test_plan_execution() {
        let config = PlanExecutionConfig::default();
        let mut manager = PlanManager::new(config);

        let steps = vec![
            ("Step 1".to_string(), "First step".to_string(), vec!["echo hello".to_string()]),
        ];

        let plan_id = manager.create_plan(
            "Test Plan".to_string(),
            "A test plan".to_string(),
            steps,
        ).unwrap();

        manager.prepare_plan(&plan_id).unwrap();
        manager.start_plan(&plan_id).await.unwrap();

        let executed_step = manager.execute_next_step(&plan_id).await.unwrap();
        assert!(executed_step.is_some());

        let plan = manager.get_plan(&plan_id).unwrap();
        assert_eq!(plan.status, PlanStatus::Completed);
    }

    #[test]
    fn test_save_load_plan() {
        let temp_dir = TempDir::new().unwrap();
        let plan_path = temp_dir.path().join("test_plan.json");

        let config = PlanExecutionConfig::default();
        let mut manager = PlanManager::new(config);

        let steps = vec![
            ("Setup".to_string(), "Initialize".to_string(), vec!["mkdir test".to_string()]),
        ];

        let plan_id = manager.create_plan(
            "Test Plan".to_string(),
            "A test plan".to_string(),
            steps,
        ).unwrap();

        manager.save_plan(&plan_id, &plan_path).unwrap();
        assert!(plan_path.exists());

        let mut new_manager = PlanManager::new(PlanExecutionConfig::default());
        let loaded_id = new_manager.load_plan(&plan_path).unwrap();

        let loaded_plan = new_manager.get_plan(&loaded_id).unwrap();
        assert_eq!(loaded_plan.title, "Test Plan");
    }
}