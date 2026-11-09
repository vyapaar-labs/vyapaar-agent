//! Job specifications, budgets, statuses, and persisted job records for the
//! autonomous scheduled job scheduler (§2.1 of plan 0002).

use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Opaque 8-character hex job identifier (e.g. `"a3f81c9b"`).
pub type JobId = String;

/// A scheduled autonomous job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSpec {
    /// 8-character hex id.
    pub id: JobId,
    /// Natural language goal.
    pub goal: String,
    /// Standard 5-field cron (e.g. `"0 9 * * 1-5"`), or `None` for manual/one-off.
    pub schedule: Option<String>,
    pub budget: JobBudget,
    /// Defaults to `true` (NoFX advisory-only; no order placement).
    pub read_only: bool,
    pub status: JobStatus,
    /// RFC3339 IST creation timestamp.
    pub created_at: String,
    /// Next scheduled run (RFC3339 IST), computed by the scheduler.
    pub next_run: Option<String>,
}

/// Multi-dimensional budget enforcement for a job run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobBudget {
    /// Maximum autonomous turns per run. Default: 20.
    pub max_turns: usize,
    /// Hard limit on statutory / data cost in INR.
    pub max_inr: Option<Decimal>,
    /// Hard token cap per run.
    pub max_tokens: Option<usize>,
    /// Maximum tool calls per run to prevent tool-call loops.
    pub max_tool_calls: Option<usize>,
}

impl Default for JobBudget {
    fn default() -> Self {
        Self {
            max_turns: 20,
            max_inr: None,
            max_tokens: None,
            max_tool_calls: None,
        }
    }
}

/// Lifecycle status of a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Active,
    Paused,
    PendingApproval { action_id: String, details: String },
    Completed,
    Failed(String),
}

/// Persisted job record with execution history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub spec: JobSpec,
    /// Number of completed runs.
    pub runs: u64,
    /// Last run timestamp (RFC3339 IST).
    pub last_run: Option<String>,
    /// Last run result.
    pub last_result: Option<JobResult>,
}

/// Outcome of a single job run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobResult {
    Success,
    BudgetExhausted,
    PendingApproval,
    Failed(String),
}

impl JobSpec {
    /// Create a new active, read-only job with the given id and goal.
    pub fn new(id: impl Into<String>, goal: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            goal: goal.into(),
            schedule: None,
            budget: JobBudget::default(),
            read_only: true,
            status: JobStatus::Active,
            created_at: Utc::now().to_rfc3339(),
            next_run: None,
        }
    }

    /// Attach a standard 5-field cron schedule.
    pub fn with_schedule(mut self, cron: impl Into<String>) -> Self {
        self.schedule = Some(cron.into());
        self
    }

    /// Replace the job budget.
    pub fn with_budget(mut self, budget: JobBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Set whether the job is read-only (advisory) or may place orders.
    pub fn read_only(mut self, flag: bool) -> Self {
        self.read_only = flag;
        self
    }

    /// Override the RFC3339 IST creation timestamp (useful for deterministic tests).
    pub fn with_created_at(mut self, created_at: impl Into<String>) -> Self {
        self.created_at = created_at.into();
        self
    }
}

impl Job {
    /// Build a persisted job record from a spec.
    pub fn from_spec(spec: JobSpec) -> Self {
        Self {
            runs: 0,
            last_run: None,
            last_result: None,
            spec,
        }
    }
}
