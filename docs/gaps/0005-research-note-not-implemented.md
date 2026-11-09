# Gap 0005 — `ResearchNote` in `vyapaar-agent-report` Not Implemented

## What's Missing

`vyapaar-agent-report` implements `TearsheetReport` and a Typst/Markdown source generator consuming `vyapaar-core` `Decimal` metrics. Roadmap Phase E lists `ResearchNote` in `vyapaar-agent-report` as planned, but it does not exist.

There is no structured research-note type for capturing hypothesis, evidence, and verification outcome in a renderable report format.

## Why It Matters

Without `ResearchNote`, the autonomous research loop (`vyapaar-agent-research`) cannot emit structured, renderable research notes tying a generated hypothesis to its walk-forward verification result and DSR/PBO statistics.

## How to Close

1. Define a `ResearchNote` type in `vyapaar-agent-report` (hypothesis, evidence refs, verification outcome, metrics).
2. Integrate with `vyapaar-agent-research` output so verified hypotheses produce `ResearchNote`s.
3. Add Typst/Markdown rendering for research notes.
4. Add unit tests in `crates/vyapaar-agent-report/tests/`.

See `docs/plans/0003-persistent-memory-and-mcp-client.md` for the tracking plan.
