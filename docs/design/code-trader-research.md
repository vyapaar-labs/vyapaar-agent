# `vyapaar-agent` — Trader Loop & Research Loop Reference

Documents `vyapaar-agent-trader-loop/` and `vyapaar-agent-research/`.

## `TraderLoop` (`vyapaar-agent-trader-loop/src/lib.rs`)

Async trader loop per strategy instance. Coordinates the closed-grammar decision loop:

```
build prompt (vyapaar-agent-prompt) → call LlmProvider → parse via DecisionParser
→ validate (universe, qty, price, algo_id, price band, collar) → RiskEngine gate
```

```rust
pub struct TraderConfig {
    pub name: String,
    pub universe: Vec<String>,
    pub llm: Arc<dyn LlmProvider>,
    pub risk_limits: RiskLimits,
}

pub struct TraderLoop {
    pub config: TraderConfig,
    pub parser: DecisionParser,
}
```

### Methods

- `build_prompt()` — renders the trading-decision system prompt for this loop's name +
  universe.
- `evaluate_decision(&self, decision, risk_engine, now)` — runs parser validation then the
  `vyapaar_risk::RiskEngine` pre-trade gate (nothing bypasses straight to the risk gate).
- `run_decision(reference_price, price_band)` — one closed-grammar turn: build prompt →
  generate → parse + validate structurally.

### Errors (`TraderError`)

- `Llm(LlmError)` — LLM transport or parse failure.
- `Decision(DecisionError)` — closed-grammar validation failure.

## Research Loop (`vyapaar-agent-research/`)

Autonomous hypothesis generation, factor discovery, and walk-forward experiment loop:

| Module | Purpose |
|--------|---------|
| `backtest.rs` | Backtest execution for research |
| `enhancer.rs` | Automatic strategy enhancement (GA / Bayesian / WFO / DSR) |
| `hypothesis.rs` | Hypothesis generation |
| `llm.rs` | LLM-assisted research (`ResearchLlm`) |
| `manifest.rs` | Experiment manifest |
| `verification.rs` | Hypothesis/strategy verification driver |
| `verify/mod.rs` | Static strategy verifier (contains inline `#[cfg(test)]` — see gap 0001) |

## Untrusted AI Principle

LLMs are untrusted signal proposers. Chat turns never produce a trade or bypass the
`DecisionValidator → PriceBand → RiskEngine` pipeline. Every decision is clamped before
reaching execution buffers.

## Related

- `vyapaar-agent-core::decision` — `DecisionParser`, closed grammar.
- `vyapaar-agent-core::prompt` — prompt assembly.
- `vyapaar-agent-llm` — `LlmProvider`, `ScriptedFakeLlm`, `OpenAiCompatibleProvider`.
- `vyapaar-runtime::backtest` — backtest engine target of `run_backtest`.
