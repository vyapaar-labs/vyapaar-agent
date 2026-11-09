//! SQLite-backed [`JobStore`] (feature `sqlx-store`).
//!
//! Persists jobs at a caller-supplied SQLite URL. Job specs and results are stored
//! as JSON; queryable fields are denormalized into columns.

use crate::jobs::spec::{Job, JobId, JobResult, JobSpec, JobStatus};
use crate::jobs::store::{JobStore, JobStoreError};

/// SQLite-backed job store.
pub struct JobStoreSqlite {
    pool: sqlx::SqlitePool,
}

impl JobStoreSqlite {
    /// Connect to the SQLite database at `url` and run migrations.
    pub async fn connect(url: &str) -> Result<Self, JobStoreError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect(url)
            .await
            .map_err(|e| JobStoreError::unavailable(format!("connect: {e}")))?;
        Self::migrate(&pool).await?;
        Ok(Self { pool })
    }

    async fn migrate(pool: &sqlx::SqlitePool) -> Result<(), JobStoreError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                spec_json TEXT NOT NULL,
                status TEXT NOT NULL,
                next_run TEXT,
                runs INTEGER NOT NULL DEFAULT 0,
                last_run TEXT,
                last_result TEXT
            );
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| JobStoreError::unavailable(format!("migrate: {e}")))?;
        Ok(())
    }

    fn job_from_row(
        id: &str,
        spec_json: &str,
        status: &str,
        next_run: Option<String>,
        runs: i64,
        last_run: Option<String>,
        last_result: Option<String>,
    ) -> Result<Job, JobStoreError> {
        let mut spec: JobSpec = serde_json::from_str(spec_json)
            .map_err(|e| JobStoreError::Corrupt(format!("spec_json: {e}")))?;
        spec.status = serde_json::from_str(status)
            .map_err(|e| JobStoreError::Corrupt(format!("status: {e}")))?;
        spec.next_run = next_run;
        if spec.id.is_empty() {
            spec.id = id.to_string();
        }
        let last_result = match last_result {
            Some(s) => Some(
                serde_json::from_str(&s)
                    .map_err(|e| JobStoreError::Corrupt(format!("last_result: {e}")))?,
            ),
            None => None,
        };
        Ok(Job {
            spec,
            runs: runs as u64,
            last_run,
            last_result,
        })
    }
}

#[async_trait::async_trait]
impl JobStore for JobStoreSqlite {
    async fn create_job(&self, spec: &JobSpec) -> Result<JobId, JobStoreError> {
        let spec_json = serde_json::to_string(spec)
            .map_err(|e| JobStoreError::Corrupt(format!("serialize spec: {e}")))?;
        let status_json = serde_json::to_string(&spec.status)
            .map_err(|e| JobStoreError::Corrupt(format!("serialize status: {e}")))?;
        let result = sqlx::query(
            r#"
            INSERT INTO jobs (id, spec_json, status, next_run, runs, last_run, last_result)
            VALUES (?1, ?2, ?3, ?4, 0, NULL, NULL)
            "#,
        )
        .bind(&spec.id)
        .bind(&spec_json)
        .bind(&status_json)
        .bind(&spec.next_run)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                JobStoreError::AlreadyExists(spec.id.clone())
            } else {
                JobStoreError::unavailable(format!("insert: {e}"))
            }
        })?;
        if result.rows_affected() == 0 {
            return Err(JobStoreError::Unavailable(
                "insert affected no rows".to_string(),
            ));
        }
        Ok(spec.id.clone())
    }

    async fn get_job(&self, id: &JobId) -> Result<Option<Job>, JobStoreError> {
        let row = sqlx::query(
            r#"SELECT id, spec_json, status, next_run, runs, last_run, last_result FROM jobs WHERE id = ?1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| JobStoreError::unavailable(format!("select: {e}")))?;
        match row {
            Some(r) => {
                let id: String = r
                    .try_get("id")
                    .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
                let spec_json: String = r
                    .try_get("spec_json")
                    .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
                let status: String = r
                    .try_get("status")
                    .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
                let next_run: Option<String> = r
                    .try_get("next_run")
                    .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
                let runs: i64 = r
                    .try_get("runs")
                    .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
                let last_run: Option<String> = r
                    .try_get("last_run")
                    .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
                let last_result: Option<String> = r
                    .try_get("last_result")
                    .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
                Ok(Some(Self::job_from_row(
                    &id,
                    &spec_json,
                    &status,
                    next_run,
                    runs,
                    last_run,
                    last_result,
                )?))
            }
            None => Ok(None),
        }
    }

    async fn list_jobs(&self) -> Result<Vec<Job>, JobStoreError> {
        let rows = sqlx::query(
            r#"SELECT id, spec_json, status, next_run, runs, last_run, last_result FROM jobs"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| JobStoreError::unavailable(format!("list: {e}")))?;
        let mut jobs = Vec::with_capacity(rows.len());
        for r in rows {
            let id: String = r
                .try_get("id")
                .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
            let spec_json: String = r
                .try_get("spec_json")
                .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
            let status: String = r
                .try_get("status")
                .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
            let next_run: Option<String> = r
                .try_get("next_run")
                .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
            let runs: i64 = r
                .try_get("runs")
                .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
            let last_run: Option<String> = r
                .try_get("last_run")
                .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
            let last_result: Option<String> = r
                .try_get("last_result")
                .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
            jobs.push(Self::job_from_row(
                &id,
                &spec_json,
                &status,
                next_run,
                runs,
                last_run,
                last_result,
            )?);
        }
        Ok(jobs)
    }

    async fn update_status(&self, id: &JobId, status: JobStatus) -> Result<(), JobStoreError> {
        let status_json = serde_json::to_string(&status)
            .map_err(|e| JobStoreError::Corrupt(format!("serialize status: {e}")))?;
        let result = sqlx::query(r#"UPDATE jobs SET status = ?2 WHERE id = ?1"#)
            .bind(id)
            .bind(&status_json)
            .execute(&self.pool)
            .await
            .map_err(|e| JobStoreError::unavailable(format!("update status: {e}")))?;
        if result.rows_affected() == 0 {
            return Err(JobStoreError::NotFound(id.clone()));
        }
        Ok(())
    }

    async fn update_next_run(
        &self,
        id: &JobId,
        next_run: Option<String>,
    ) -> Result<(), JobStoreError> {
        let result = sqlx::query(r#"UPDATE jobs SET next_run = ?2 WHERE id = ?1"#)
            .bind(id)
            .bind(&next_run)
            .execute(&self.pool)
            .await
            .map_err(|e| JobStoreError::unavailable(format!("update next_run: {e}")))?;
        if result.rows_affected() == 0 {
            return Err(JobStoreError::NotFound(id.clone()));
        }
        Ok(())
    }

    async fn record_run(&self, id: &JobId, result: &JobResult) -> Result<(), JobStoreError> {
        let result_json = serde_json::to_string(result)
            .map_err(|e| JobStoreError::Corrupt(format!("serialize result: {e}")))?;
        let now = chrono::Utc::now().to_rfc3339();
        let result =
            sqlx::query(r#"UPDATE jobs SET last_run = ?2, last_result = ?3 WHERE id = ?1"#)
                .bind(id)
                .bind(&now)
                .bind(&result_json)
                .execute(&self.pool)
                .await
                .map_err(|e| JobStoreError::unavailable(format!("record run: {e}")))?;
        if result.rows_affected() == 0 {
            return Err(JobStoreError::NotFound(id.clone()));
        }
        Ok(())
    }

    async fn increment_runs(&self, id: &JobId) -> Result<u64, JobStoreError> {
        let _result = sqlx::query(r#"UPDATE jobs SET runs = runs + 1 WHERE id = ?1"#)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| JobStoreError::unavailable(format!("increment runs: {e}")))?;
        let row = sqlx::query(r#"SELECT runs FROM jobs WHERE id = ?1"#)
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| JobStoreError::unavailable(format!("read runs: {e}")))?;
        let runs: i64 = row
            .try_get("runs")
            .map_err(|e| JobStoreError::Corrupt(e.to_string()))?;
        Ok(runs as u64)
    }
}
