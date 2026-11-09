//! Persistence port for scheduled jobs.
//!
//! The port is intentionally free of concrete database types so the scheduler can
//! be tested against an in-memory store and backed by SQLite in production.

use thiserror::Error;

use crate::jobs::spec::{Job, JobId, JobResult, JobSpec, JobStatus};

/// Categories of job store failures.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum JobStoreError {
    #[error("job not found: {0}")]
    NotFound(String),
    #[error("job already exists: {0}")]
    AlreadyExists(String),
    #[error("store unavailable: {0}")]
    Unavailable(String),
    #[error("data corrupt: {0}")]
    Corrupt(String),
    #[error("other: {0}")]
    Other(String),
}

impl JobStoreError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable(message.into())
    }
}

/// Asynchronous port for persisting scheduled jobs and their execution state.
#[async_trait::async_trait]
pub trait JobStore: Send + Sync + 'static {
    /// Insert a new job spec and return its id.
    async fn create_job(&self, spec: &JobSpec) -> Result<JobId, JobStoreError>;

    /// Fetch a persisted job by id.
    async fn get_job(&self, id: &JobId) -> Result<Option<Job>, JobStoreError>;

    /// List all persisted jobs.
    async fn list_jobs(&self) -> Result<Vec<Job>, JobStoreError>;

    /// Replace the status of a job.
    async fn update_status(&self, id: &JobId, status: JobStatus) -> Result<(), JobStoreError>;

    /// Set (or clear) the next scheduled run timestamp.
    async fn update_next_run(
        &self,
        id: &JobId,
        next_run: Option<String>,
    ) -> Result<(), JobStoreError>;

    /// Record the outcome of the most recent run.
    async fn record_run(&self, id: &JobId, result: &JobResult) -> Result<(), JobStoreError>;

    /// Increment the run counter and return the new value.
    async fn increment_runs(&self, id: &JobId) -> Result<u64, JobStoreError>;
}
