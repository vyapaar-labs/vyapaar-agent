//! Autonomous scheduled job engine (§2.1 of plan 0002).
//!
//! Provides the job domain model (`JobSpec`, `JobBudget`, `JobStatus`, `Job`),
//! the `JobStore` persistence port, the `JobScheduler` (cron evaluation, budget
//!
//! enforcement, approval gate), and an in-memory store. A SQLite-backed store is
//! available behind the `sqlx-store` feature.

pub mod cron;
pub mod memory_store;
pub mod scheduler;
pub mod spec;
pub mod store;

#[cfg(feature = "sqlx-store")]
pub mod sqlite_store;

pub use cron::CronExpr;
pub use memory_store::InMemoryJobStore;
pub use scheduler::{
    ApprovalGate, JobError, JobScheduler, JobTickAction, ManualApprovalGate, PermissiveApprovalGate,
};
pub use spec::{Job, JobBudget, JobId, JobResult, JobSpec, JobStatus};
pub use store::{JobStore, JobStoreError};
