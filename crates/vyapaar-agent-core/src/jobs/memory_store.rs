//! In-memory [`JobStore`] implementation for tests and as a fallback.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::jobs::spec::{Job, JobId, JobResult, JobSpec, JobStatus};
use crate::jobs::store::{JobStore, JobStoreError};

#[derive(Default, Clone)]
pub struct InMemoryJobStore {
    jobs: Arc<RwLock<HashMap<JobId, Job>>>,
}

impl InMemoryJobStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl JobStore for InMemoryJobStore {
    async fn create_job(&self, spec: &JobSpec) -> Result<JobId, JobStoreError> {
        let mut jobs = self.jobs.write().unwrap();
        if jobs.contains_key(&spec.id) {
            return Err(JobStoreError::AlreadyExists(spec.id.clone()));
        }
        jobs.insert(spec.id.clone(), Job::from_spec(spec.clone()));
        Ok(spec.id.clone())
    }

    async fn get_job(&self, id: &JobId) -> Result<Option<Job>, JobStoreError> {
        Ok(self.jobs.read().unwrap().get(id).cloned())
    }

    async fn list_jobs(&self) -> Result<Vec<Job>, JobStoreError> {
        Ok(self.jobs.read().unwrap().values().cloned().collect())
    }

    async fn update_status(&self, id: &JobId, status: JobStatus) -> Result<(), JobStoreError> {
        let mut jobs = self.jobs.write().unwrap();
        match jobs.get_mut(id) {
            Some(j) => {
                j.spec.status = status;
                Ok(())
            }
            None => Err(JobStoreError::NotFound(id.clone())),
        }
    }

    async fn update_next_run(
        &self,
        id: &JobId,
        next_run: Option<String>,
    ) -> Result<(), JobStoreError> {
        let mut jobs = self.jobs.write().unwrap();
        match jobs.get_mut(id) {
            Some(j) => {
                j.spec.next_run = next_run;
                Ok(())
            }
            None => Err(JobStoreError::NotFound(id.clone())),
        }
    }

    async fn record_run(&self, id: &JobId, result: &JobResult) -> Result<(), JobStoreError> {
        let mut jobs = self.jobs.write().unwrap();
        match jobs.get_mut(id) {
            Some(j) => {
                j.last_run = Some(now_iso());
                j.last_result = Some(result.clone());
                Ok(())
            }
            None => Err(JobStoreError::NotFound(id.clone())),
        }
    }

    async fn increment_runs(&self, id: &JobId) -> Result<u64, JobStoreError> {
        let mut jobs = self.jobs.write().unwrap();
        match jobs.get_mut(id) {
            Some(j) => {
                j.runs += 1;
                Ok(j.runs)
            }
            None => Err(JobStoreError::NotFound(id.clone())),
        }
    }
}

/// Current RFC3339 IST timestamp (used by the in-memory store).
pub(crate) fn now_iso() -> String {
    use crate::jobs::cron::IST;
    use chrono::TimeZone;
    IST.timestamp_opt(chrono::Utc::now().timestamp(), 0)
        .single()
        .expect("valid IST")
        .to_rfc3339()
}
