//! Integration tests for the autonomous job engine (`vyapaar-agent-core::jobs`).
//!
//! All tests live here (parallel to `src/`) per the TDD / parallel test directory rule.
//! No `#[test]` blocks appear in production source files.

use chrono::Datelike;
use chrono::Timelike;
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use rust_decimal::Decimal;

use vyapaar_agent_core::jobs::{
    ApprovalGate, CronExpr, JobBudget, JobResult, JobScheduler, JobSpec, JobStatus, JobTickAction,
    ManualApprovalGate, PermissiveApprovalGate,
};
use vyapaar_agent_core::jobs::{InMemoryJobStore, JobStore, JobStoreError};

type JobId = String;

/// Construct a `DateTime<Utc>` whose IST wall-clock is `y-mo-d h:mi` (IST = UTC+05:30).
fn at_ist(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
    let ist = FixedOffset::east_opt(5 * 3600 + 30 * 60)
        .expect("IST offset")
        .with_ymd_and_hms(y, mo, d, h, mi, 0)
        .single()
        .expect("valid IST");
    ist.to_utc()
}

fn active_job(id: &str, schedule: &str) -> JobSpec {
    JobSpec::new(id.to_string(), "daily scanner".to_string())
        .with_schedule(schedule.to_string())
        .with_created_at(at_ist(2026, 11, 9, 0, 0).to_rfc3339())
}

#[tokio::test]
async fn test_cron_is_due_weekdays_at_nine_ist() {
    let cron = CronExpr::parse("0 9 * * 1-5").expect("valid cron");
    // Monday 2026-11-09 09:00 IST is due.
    assert!(cron.is_due(at_ist(2026, 11, 9, 9, 0)));
    // One minute before is not due.
    assert!(!cron.is_due(at_ist(2026, 11, 9, 8, 59)));
    // Sunday is not due.
    assert!(!cron.is_due(at_ist(2026, 11, 8, 9, 0)));
    // Saturday is not due.
    assert!(!cron.is_due(at_ist(2026, 11, 7, 9, 0)));
}

#[tokio::test]
async fn test_cron_next_after_skips_weekend() {
    let cron = CronExpr::parse("0 9 * * 1-5").expect("valid cron");
    // Friday 2026-11-06 09:00 IST (strictly after) -> next is Monday 2026-11-09 09:00 IST.
    let friday = at_ist(2026, 11, 6, 9, 0);
    let next = cron.next_after(friday).expect("next run exists");
    let next_ist = next.with_timezone(&FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap());
    assert_eq!(next_ist.weekday(), chrono::Weekday::Mon);
    assert_eq!(
        (next_ist.year(), next_ist.month(), next_ist.day()),
        (2026, 11, 9)
    );
    assert_eq!((next_ist.hour(), next_ist.minute()), (9, 0));
}

#[tokio::test]
async fn test_cron_step_and_list_fields() {
    let cron = CronExpr::parse("*/15 9,10 * * 1-5").expect("valid cron");
    assert!(cron.is_due(at_ist(2026, 11, 9, 9, 0)));
    assert!(cron.is_due(at_ist(2026, 11, 9, 9, 15)));
    assert!(!cron.is_due(at_ist(2026, 11, 9, 9, 14)));
    assert!(cron.is_due(at_ist(2026, 11, 9, 10, 0)));
    assert!(!cron.is_due(at_ist(2026, 11, 9, 11, 0)));
}

#[tokio::test]
async fn test_cron_invalid() {
    assert!(CronExpr::parse("0 9 * *").is_err());
    assert!(CronExpr::parse("60 9 * * 1-5").is_err());
    assert!(CronExpr::parse("0 25 * * 1-5").is_err());
    assert!(CronExpr::parse("0 9 * * 8").is_err());
}

#[tokio::test]
async fn test_in_memory_store_crud() {
    let store = InMemoryJobStore::new();
    let spec = active_job("a1", "0 9 * * 1-5");
    let id = store.create_job(&spec).await.expect("create job");
    assert_eq!(id.as_str(), "a1");

    let job = store
        .get_job(&id)
        .await
        .expect("get job")
        .expect("job exists");
    assert_eq!(job.spec.id, "a1");
    assert_eq!(job.spec.status, JobStatus::Active);
    assert_eq!(job.runs, 0);

    let jobs = store.list_jobs().await.expect("list jobs");
    assert_eq!(jobs.len(), 1);

    // Missing job.
    let missing = JobId::from("nope");
    assert!(
        store
            .get_job(&missing)
            .await
            .expect("get missing")
            .is_none()
    );

    // Duplicate create fails.
    let res = store.create_job(&spec).await;
    assert!(matches!(res, Err(JobStoreError::AlreadyExists(_))));
}

#[tokio::test]
async fn test_tick_due_read_only_job_executes() {
    let store = InMemoryJobStore::new();
    let id = JobId::from("a1");
    store
        .create_job(&active_job(&id, "0 9 * * 1-5"))
        .await
        .expect("create");

    let scheduler = JobScheduler::permissive();
    let now = at_ist(2026, 11, 9, 9, 0); // Monday 09:00 IST -> due
    let actions = scheduler.tick(now, &store).await.expect("tick");
    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions[0],
        JobTickAction::Execute { id: ref job_id } if job_id == &id
    ));
}

#[tokio::test]
async fn test_tick_not_due_skips() {
    let store = InMemoryJobStore::new();
    let id = JobId::from("a1");
    store
        .create_job(&active_job(&id, "0 9 * * 1-5"))
        .await
        .expect("create");

    let scheduler = JobScheduler::permissive();
    // Sunday 09:00 IST -> not due (weekday cron).
    let now = at_ist(2026, 11, 8, 9, 0);
    let actions = scheduler.tick(now, &store).await.expect("tick");
    assert!(actions.is_empty(), "expected no actions on non-due time");
}

#[tokio::test]
async fn test_tick_budget_exhausted_fails() {
    let store = InMemoryJobStore::new();
    let id = JobId::from("a1");
    let mut spec = active_job(&id, "0 9 * * 1-5");
    spec.budget = JobBudget {
        max_turns: 1,
        ..Default::default()
    };
    store.create_job(&spec).await.expect("create");
    // Pre-record one run so the budget is exhausted.
    let _ = store.increment_runs(&id).await.expect("increment");

    let scheduler = JobScheduler::permissive();
    let now = at_ist(2026, 11, 9, 9, 0);
    let actions = scheduler.tick(now, &store).await.expect("tick");
    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions[0],
        JobTickAction::Skipped { id: ref job_id, .. } if job_id == &id
    ));
    let job = store.get_job(&id).await.expect("get").expect("job");
    assert!(matches!(job.spec.status, JobStatus::Failed(_)));
}

#[tokio::test]
async fn test_tick_non_read_only_requires_approval_manual_gate() {
    let store = InMemoryJobStore::new();
    let id = JobId::from("a1");
    let mut spec = active_job(&id, "0 9 * * 1-5");
    spec.read_only = false;
    store.create_job(&spec).await.expect("create");

    let scheduler = JobScheduler::with_clock(ManualApprovalGate, std::sync::Arc::new(Utc::now));
    let now = at_ist(2026, 11, 9, 9, 0);
    let actions = scheduler.tick(now, &store).await.expect("tick");
    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions[0],
        JobTickAction::PendingApproval { id: ref job_id, .. } if job_id == &id
    ));
    let job = store.get_job(&id).await.expect("get").expect("job");
    assert!(matches!(job.spec.status, JobStatus::PendingApproval { .. }));
}

#[tokio::test]
async fn test_tick_non_read_only_permissive_gate_executes() {
    let store = InMemoryJobStore::new();
    let id = JobId::from("a1");
    let mut spec = active_job(&id, "0 9 * * 1-5");
    spec.read_only = false;
    store.create_job(&spec).await.expect("create");

    let scheduler = JobScheduler::permissive();
    let now = at_ist(2026, 11, 9, 9, 0);
    let actions = scheduler.tick(now, &store).await.expect("tick");
    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions[0],
        JobTickAction::Execute { id: ref job_id } if job_id == &id
    ));
}

#[tokio::test]
async fn test_record_execution_advances_next_run_and_runs() {
    let store = InMemoryJobStore::new();
    let id = JobId::from("a1");
    store
        .create_job(&active_job(&id, "0 9 * * 1-5"))
        .await
        .expect("create");

    let scheduler = JobScheduler::permissive();
    let now = at_ist(2026, 11, 9, 9, 0);
    let actions = scheduler.tick(now, &store).await.expect("tick");
    assert_eq!(actions.len(), 1);

    scheduler
        .record_execution(&store, &id, &JobResult::Success)
        .await
        .expect("record execution");

    let job = store.get_job(&id).await.expect("get").expect("job");
    assert_eq!(job.runs, 1);
    assert!(job.last_run.is_some());
    assert!(job.spec.next_run.is_some());
}

#[tokio::test]
async fn test_approval_gate_implementations() {
    let permissive = PermissiveApprovalGate;
    assert!(permissive.is_approved("any").await);
    let manual = ManualApprovalGate;
    assert!(!manual.is_approved("any").await);
}

#[tokio::test]
async fn test_job_spec_defaults() {
    let spec = JobSpec::new("x".to_string(), "g".to_string());
    assert_eq!(spec.id, "x");
    assert!(spec.read_only);
    assert!(matches!(spec.status, JobStatus::Active));
    assert_eq!(spec.budget.max_turns, 20);
    assert!(spec.schedule.is_none());
    assert!(spec.next_run.is_none());
}

#[tokio::test]
async fn test_tick_paused_job_skipped() {
    let store = InMemoryJobStore::new();
    let id = JobId::from("a1");
    let mut spec = active_job(&id, "0 9 * * 1-5");
    spec.status = JobStatus::Paused;
    store.create_job(&spec).await.expect("create");

    let scheduler = JobScheduler::permissive();
    let now = at_ist(2026, 11, 9, 9, 0);
    let actions = scheduler.tick(now, &store).await.expect("tick");
    assert!(actions.is_empty(), "paused job must not be ticked");
}

// Decimal budget sanity (zero-floats policy).
#[tokio::test]
async fn test_budget_uses_decimal() {
    let _b = JobBudget {
        max_turns: 5,
        max_inr: Some(Decimal::new(1000, 2)),
        max_tokens: Some(10_000),
        max_tool_calls: Some(50),
    };
    assert_eq!(JobBudget::default().max_turns, 20);
}
