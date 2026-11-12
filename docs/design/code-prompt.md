# `vyapaar-agent` — Prompt Assembly & Deterministic Hashing Reference

Documents `vyapaar-agent-core/src/prompt.rs`: versioned prompt assembly, context
serialization, and canonical SHA-256 prompt hashing for deterministic replay.

## `PromptAssembly`

```rust
pub struct PromptAssembly {
    pub cached_prefix: String,   // stable system prefix (cached across turns)
    pub turn_context: String,    // per-turn variable context
    pub placement: Placement,    // where each section goes
}
```

### `Placement`

Controls where a `PromptSection` is rendered: `Prefix` (cached, reused across turns) or
`Turn` (re-rendered each turn). Splitting cached prefix from turn context is what enables
prompt-cache hits.

## Hashing Functions

| Function | Signature | Purpose |
|----------|-----------|---------|
| `compute_prompt_hash` | `(&PromptAssembly) -> PromptHash` | Canonical hash of a full assembly |
| `hash_prompt_hex` | `(&str) -> String` | Hex SHA-256 of a raw prompt string |
| `sha256_hex` | `(&[u8]) -> String` | Low-level SHA-256 hex |
| `default_assemble` | `() -> PromptAssembly` | Default assembly with cached prefix vs turn context |

## Deterministic Replay Contract

Every prompt, system completion, seed, and temperature is SHA-256 hashed and archived
(`vyapaar-agent-replay`) for deterministic backtest replay. The hash of a `PromptAssembly`
must be stable for a given `cached_prefix` + `turn_context` pair (cache hit) and change when
either varies (cache miss).

## Note (gap 0007)

No cache-hit/cache-miss regression tests exist yet for prompt caching. See
`docs/gaps/0007-prompt-cache-regression-tests.md` (tracked by plan 0009).

## Related

- `vyapaar-agent-replay` — SHA-256 cassette store, `hash_request` canonical request hashing.
- `vyapaar-agent-prompt` — versioned prompt templates, chat base prompts, section renderers.
- `vyapaar-agent-trader-loop` — builds trading-decision system prompt via
  `render_trading_decision_system_prompt`.
