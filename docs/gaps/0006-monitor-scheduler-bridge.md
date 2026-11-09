# Gap 0006 — Runtime-Agent Monitor Scheduler Bridge Not Implemented

## What's Missing

Roadmap Phase D lists "Runtime-agent monitor scheduler bridge" as planned, but it does not exist.

There is no bridge between `vyapaar-runtime` (actor pipeline, scheduled jobs) and `vyapaar-agent` (autonomous research loop, trader loop) for running monitoring jobs or background agent tasks on a schedule.

## Why It Matters

Without this bridge, the agent's autonomous research and trader loops cannot be scheduled or monitored through the runtime's actor pipeline, limiting unattended operation.

## How to Close

1. Define monitor/scheduler ports in `vyapaar-agent-core::jobs` or `vyapaar-agent-core::ports` (not depending on `vyapaar-runtime-*` per rule R1 — invert through traits).
2. Implement the bridge in `vyapaar-runtime` (which may depend on agent per R1 inversion).
3. Add unit tests.

See `docs/plans/0003-persistent-memory-and-mcp-client.md` for the tracking plan.
