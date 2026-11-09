//! Autonomous job scheduler: cron evaluation, multi-dimensional budget enforcement,
//! and the human approval gate (§2.1 of plan 0002).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::jobs::cron::CronExpr;
use crate::jobs::spec::{JobId, JobResult, JobStatus};
use crate::jobs::store::JobStore;

/// Errors produced by the job scheduler.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum JobError {
    #[error("cron error: {0}")]
    Cron(String),
    #[error("store error: {0}")]
    Store(String),
    #[error("budget exhausted: {0}")]
    BudgetExhausted(String),
    #[error("approval required: {0}")]
    ApprovalRequired(String),
    #[error("not found: {0}")]
    NotFound(String),
}

/// Action the scheduler decides a due job should take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobTickAction {
    /// Job is due and cleared to execute.
    Execute { id: JobId },
    /// Job requires human approval before it may execute.
    PendingApproval {
        id: JobId,
        action_id: String,
        details: String,
    },
    /// Job is due but skipped (budget exhausted or paused).
    Skipped { id: JobId, reason: String },
}

/// Human approval gate for jobs that are not read-only.
///
/// Any tool call proposing a portfolio rebalance or touching orders switches a job
/// to `PendingApproval`; execution halts until confirmed.
#[async_trait::async_trait]
pub trait ApprovalGate: Send + Sync + 'static {
    /// Request approval for an action; returns `true` if granted.
    async fn request_approval(&self, action_id: &str, details: &str) -> Result<bool, JobError>;

    /// Return whether an action has already been approved.
    async fn is_approved(&self, action_id: &str) -> bool;
}

/// Auto-approves every request. Suitable for read-only/advisory jobs and tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct PermissiveApprovalGate;

#[async_trait::async_trait]
impl ApprovalGate for PermissiveApprovalGate {
    async fn request_approval(&self, _action_id: &str, _details: &str) -> Result<bool, JobError> {
        Ok(true)
    }
    async fn is_approved(&self, _action_id: &str) -> bool {
        true
    }
}

/// Rejects every request. Safe default for production until an operator wires a
/// real confirmation channel (`vyapaar jobs approve <id>`).
#[derive(Debug, Default, Clone, Copy)]
pub struct ManualApprovalGate;

#[async_trait::async_trait]
impl ApprovalGate for ManualApprovalGate {
    async fn request_approval(&self, _action_id: &str, _details: &str) -> Result<bool, JobError> {
        Ok(false)
    }
    async fn is_approved(&self, _action_id: &str) -> bool {
        false
    }
}

/// Clock abstraction injected into the scheduler for temporal determinism.
pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// Evaluates cron schedules against a clock, enforces budgets, and gates execution.
pub struct JobScheduler<G = PermissiveApprovalGate> {
    gate: G,
    clock: Clock,
}

impl JobScheduler<PermissiveApprovalGate> {
    /// Construct a scheduler with a permissive (auto-approving) gate and the real clock.
    pub fn permissive() -> Self {
        Self::with_clock(PermissiveApprovalGate, Arc::new(Utc::now))
    }
}

impl<G: ApprovalGate> JobScheduler<G> {
    /// Construct a scheduler with the given approval gate and real clock.
    pub fn with_clock(gate: G, clock: Clock) -> Self {
        Self { gate, clock }
    }

    /// Gate accessor.
    pub fn gate(&self) -> &G {
        &self.gate
    }

    /// Evaluate every active job against `now` and return the actions to take.
    ///
    /// Due jobs that require approval are moved to `PendingApproval`; due jobs that
    /// are cleared are returned as `Execute`. Jobs whose budget is exhausted are
    /// failed and skipped.
    pub async fn tick(
        &self,
        now: DateTime<Utc>,
        store: &dyn JobStore,
    ) -> Result<Vec<JobTickAction>, JobError> {
        let mut actions = Vec::new();
        for job in store
            .list_jobs()
            .await
            .map_err(|e| JobError::Store(e.to_string()))?
        {
            let spec = &job.spec;
            if spec.status != JobStatus::Active {
                continue;
            }
            let schedule = match &spec.schedule {
                Some(s) => s.clone(),
                None => continue, // manual/one-off jobs are not auto-ticked
            };
            let cron = CronExpr::parse(&schedule).map_err(|e| JobError::Cron(e.to_string()))?;

            // Determine the next scheduled run, computing it from the creation time
            // when the job has never been ticked.
            let next_run = match &spec.next_run {
                Some(nr) => nr.clone(),
                None => {
                    let created = spec
                        .created_at
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| (self.clock)());
                    let next = cron.next_after(created).map(|dt| dt.to_rfc3339());
                    let _ = store
                        .update_next_run(&spec.id, next.clone())
                        .await
                        .map_err(|e| JobError::Store(e.to_string()));
                    next.unwrap_or_default()
                }
            };

            let due = next_run
                .parse::<DateTime<Utc>>()
                .map(|nr| nr <= now)
                .unwrap_or(false);
            if !due {
                continue;
            }

            // Budget: max_turns.
            if job.runs >= spec.budget.max_turns as u64 {
                let reason = format!("max_turns {} reached", spec.budget.max_turns);
                let _ = store
                    .update_status(
                        &spec.id,
                        JobStatus::Failed(format!("budget exhausted: {reason}")),
                    )
                    .await
                    .map_err(|e| JobError::Store(e.to_string()));
                actions.push(JobTickAction::Skipped {
                    id: spec.id.clone(),
                    reason,
                });
                continue;
            }

            // Non-read-only jobs need explicit human approval.
            if !spec.read_only {
                let action_id = format!("approve-{}", spec.id);
                let details = format!(
                    "Job '{}' proposes non-read-only execution: {}",
                    spec.id, spec.goal
                );
                let approved = self.gate.is_approved(&action_id).await;
                if !approved {
                    let _ = store
                        .update_status(
                            &spec.id,
                            JobStatus::PendingApproval {
                                action_id: action_id.clone(),
                                details: details.clone(),
                            },
                        )
                        .await
                        .map_err(|e| JobError::Store(e.to_string()));
                    actions.push(JobTickAction::PendingApproval {
                        id: spec.id.clone(),
                        action_id,
                        details,
                    });
                    continue;
                }
            }

            actions.push(JobTickAction::Execute {
                id: spec.id.clone(),
            });
        }
        Ok(actions)
    }

    /// Record that `id` has finished executing with `result`, advance its `next_run`,
    /// and increment its run counter.
    pub async fn record_execution(
        &self,
        store: &dyn JobStore,
        id: &JobId,
        result: &JobResult,
    ) -> Result<(), JobError> {
        store
            .record_run(id, result)
            .await
            .map_err(|e| JobError::Store(e.to_string()))?;
        let new_runs = store
            .increment_runs(id)
            .await
            .map_err(|e| JobError::Store(e.to_string()))?;
        let _ = new_runs; // run counter is source of truth for max_turns enforcement

        let job = store
            .get_job(id)
            .await
            .map_err(|e| JobError::Store(e.to_string()))?
            .ok_or_else(|| JobError::NotFound(id.clone()))?;

        if let Some(sched) = &job.spec.schedule {
            let cron = CronExpr::parse(sched).map_err(|e| JobError::Cron(e.to_string()))?;
            let now = (self.clock)();
            let next = cron.next_after(now).map(|dt| dt.to_rfc3339());
            let _ = store
                .update_next_run(id, next)
                .await
                .map_err(|e| JobError::Store(e.to_string()));
        }
        Ok(())
    }
}

impl<G: ApprovalGate> JobScheduler<G> {
    /// Convenience: execute `tick` then record each `Execute` action as successful.
    /// `PendingApproval` and `Skipped` actions are recorded but not executed.
    pub async fn tick_and_record(
        &self,
        now: DateTime<Utc>,
        store: &dyn JobStore,
    ) -> Result<Vec<JobTickAction>, JobError> {
        let actions = self.tick(now, store).await?;
        for action in &actions {
            match action {
                JobTickAction::Execute { id } => {
                    let _ = self.record_execution(store, id, &JobResult::Success).await;
                }
                JobTickAction::PendingApproval { id, .. } => {
                    let _ = self
                        .record_execution(store, id, &JobResult::PendingApproval)
                        .await;
                }
                JobTickAction::Skipped { id, .. } => {
                    let _ = self
                        .record_execution(store, id, &JobResult::BudgetExhausted)
                        .await;
                }
            }
        }
        Ok(actions)
    }
}
