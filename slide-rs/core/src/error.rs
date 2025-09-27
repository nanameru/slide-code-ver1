// codex-1互換のエラーハンドリング実装

use std::time::Duration;
use serde::{Deserialize, Serialize};

pub type Result<T> = anyhow::Result<T>;

// For compatibility with codex-1
pub use anyhow::Error;

// codex-1互換のプラン型定義
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlanType {
    Known(KnownPlan),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KnownPlan {
    Free,
    Plus,
    Pro,
    Team,
    Business,
    Enterprise,
    Edu,
}

// codex-1互換のエラー型（完全実装）
#[derive(Debug, thiserror::Error)]
pub enum CodexErr {
    /// ストリーム中断エラー（リトライ対象）
    /// オプションでリトライ遅延時間を指定可能
    #[error("stream disconnected before completion: {0}")]
    Stream(String, Option<Duration>),
    
    /// Ctrl-C割り込み（致命的エラー、リトライしない）
    #[error("interrupted (Ctrl-C)")]
    Interrupted,
    
    /// 使用量制限エラー（致命的エラー、リトライしない）
    #[error("{0}")]
    UsageLimitReached(UsageLimitReachedError),
    
    /// プラン不適合エラー（致命的エラー、リトライしない）
    #[error("To use Codex with your ChatGPT plan, upgrade to Plus: https://openai.com/chatgpt/pricing.")]
    UsageNotIncluded,
    
    /// 環境変数不足エラー（致命的エラー、リトライしない）
    #[error("{0}")]
    EnvVar(EnvVarError),
    
    /// 内部エージェント死亡
    #[error("Internal agent died")]
    InternalAgentDied,
    
    /// その他のエラー（リトライ対象）
    #[error("Other error: {0}")]
    Other(String),
}

// codex-1互換のUsageLimitReachedError実装
#[derive(Debug, Clone)]
pub struct UsageLimitReachedError {
    pub plan_type: Option<PlanType>,
    pub resets_in_seconds: Option<u64>,
}

impl std::fmt::Display for UsageLimitReachedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.plan_type.as_ref() {
            Some(PlanType::Known(KnownPlan::Plus)) => format!(
                "You've hit your usage limit. Upgrade to Pro (https://openai.com/chatgpt/pricing){}",
                retry_suffix_after_or(self.resets_in_seconds)
            ),
            Some(PlanType::Known(KnownPlan::Team)) | Some(PlanType::Known(KnownPlan::Business)) => {
                format!(
                    "You've hit your usage limit. To get more access now, send a request to your admin{}",
                    retry_suffix_after_or(self.resets_in_seconds)
                )
            }
            Some(PlanType::Known(KnownPlan::Free)) => {
                "To use Codex with your ChatGPT plan, upgrade to Plus: https://openai.com/chatgpt/pricing."
                    .to_string()
            }
            Some(PlanType::Known(KnownPlan::Pro))
            | Some(PlanType::Known(KnownPlan::Enterprise))
            | Some(PlanType::Known(KnownPlan::Edu)) => format!(
                "You've hit your usage limit.{}",
                retry_suffix(self.resets_in_seconds)
            ),
            Some(PlanType::Unknown(_)) | None => format!(
                "You've hit your usage limit.{}",
                retry_suffix(self.resets_in_seconds)
            ),
        };

        write!(f, "{message}")
    }
}

// codex-1互換のEnvVarError実装
#[derive(Debug, Clone)]
pub struct EnvVarError {
    /// Name of the environment variable that is missing.
    pub var: String,

    /// Optional instructions to help the user get a valid value for the
    /// variable and set it.
    pub instructions: Option<String>,
}

impl std::fmt::Display for EnvVarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Missing environment variable: `{}`.", self.var)?;
        if let Some(instructions) = &self.instructions {
            write!(f, " {instructions}")?;
        }
        Ok(())
    }
}

// ヘルパー関数（codex-1互換）
fn retry_suffix(resets_in_seconds: Option<u64>) -> String {
    if let Some(secs) = resets_in_seconds {
        let reset_duration = format_reset_duration(secs);
        format!(" Try again in {reset_duration}.")
    } else {
        " Try again later.".to_string()
    }
}

fn retry_suffix_after_or(resets_in_seconds: Option<u64>) -> String {
    if let Some(secs) = resets_in_seconds {
        let reset_duration = format_reset_duration(secs);
        format!(" or try again in {reset_duration}.")
    } else {
        " or try again later.".to_string()
    }
}

fn format_reset_duration(total_secs: u64) -> String {
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let minutes = (total_secs % 3_600) / 60;

    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        let unit = if days == 1 { "day" } else { "days" };
        parts.push(format!("{days} {unit}"));
    }
    if hours > 0 {
        let unit = if hours == 1 { "hour" } else { "hours" };
        parts.push(format!("{hours} {unit}"));
    }
    if minutes > 0 {
        let unit = if minutes == 1 { "minute" } else { "minutes" };
        parts.push(format!("{minutes} {unit}"));
    }

    if parts.is_empty() {
        return "less than a minute".to_string();
    }

    match parts.len() {
        1 => parts[0].clone(),
        2 => format!("{} {}", parts[0], parts[1]),
        _ => format!("{} {} {}", parts[0], parts[1], parts[2]),
    }
}

impl CodexErr {
    /// codex-1互換のdowncast_refメソッド
    pub fn downcast_ref<T: std::any::Any>(&self) -> Option<&T> {
        (self as &dyn std::any::Any).downcast_ref::<T>()
    }
}

pub type CodexResult<T> = std::result::Result<T, CodexErr>;