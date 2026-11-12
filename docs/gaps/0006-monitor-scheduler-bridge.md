# Gap 0006 — Runtime-Agent Monitor Scheduler Bridge Not Implemented

## What's Missing

Roadmap Phase D listed "Runtime-agent monitor scheduler bridge" as planned.

**Implemented so far**: the agent-side scheduling engine exists in
`vyapaar-agent-core/src/jobs/` — cron evaluation (`cron.rs`), multi-dimensional budget
enforcement (`spec.rs`), an explicit human-approval gate, an in-memory store
(`memory_store.rs`), a trait store (`store.rs`), and a SQLite-backed store
(`sqlite_store.rs`).

**Missing**: the bridge between `vyapaar-runtime` (actor pipeline, scheduled jobs) and
`vyapaar-agent` (autonomous research loop, trader loop) for running monitoring jobs or
background agent tasks on a schedule through the runtime's actor pipeline.

## Why It Matters

Without this bridge, the agent's autonomous research and trader loops cannot be scheduled
or monitored through the runtime's actor pipeline, limiting unattended operation.

## How to Close

1. Confirm the agent-side scheduler ports in `vyapaar-agent-core::jobs` / `::ports` satisfy
   the monitor use case (inverted through traits — no `vyapaar-runtime-*` dependency per R1).
2. Implement the bridge in `vyapaar-runtime` (which may depend on agent per R1 inversion).
3. Add unit tests.

See `docs/plans/0009-persistent-memory-mcp-client-and-phase-e.md` for the tracking plan.
