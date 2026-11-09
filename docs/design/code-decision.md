# `vyapaar-agent` — Closed Decision Grammar & Parser Reference

Documents `vyapaar-agent-core/src/decision.rs`: the NoFX untrusted-AI closed decision
grammar. Every LLM completion on the trading decision surface is parsed into
`vyapaar_core::Decision` (`#[serde(deny_unknown_fields)]`), grounded against real-time
`PriceBand`s, and clamped by `vyapaar_risk::RiskEngine`.

## Decision Surface

| Decision Variant | Meaning |
|------------------|---------|
| `PlaceOrder` | Submit a new order (symbol, qty, price_limit, algo_id, side, order_type) |
| `ModifyOrder` | Amend an open order (new_qty, new_price, algo_id) |
| `CancelOrder` | Cancel a single order (order_id, algo_id) |
| `CancelAll` | Cancel all orders (algo_id) |
| `Hold` | No action (flat / wait) |

## `DecisionParser`

```rust
pub struct DecisionParser {
    pub allowed_universe: Option<HashSet<String>>,
    pub max_price_deviation_pct: Decimal,   // default ±20%
    pub strict_sebi_algo_format: bool,       // default true
}
```

### Responsibilities

1. **Closed-grammar parsing** (`parse_str`): strips markdown JSON fences, parses into
   `vyapaar_core::Decision` with `serde(deny_unknown_fields)` — any unauthorized field
   injected by the model yields `DecisionError::UnknownField` or `DecisionError::Json`.
2. **Lenient coercion**: numbers for `qty`/`price_limit`/`new_qty`/`new_price` are
   coerced to strings; `side` uppercased; `action` lowercased — one repair attempt
   before hard failure.
3. **Safety validation** (`validate_decision`):
   - Symbol non-empty and (if `allowed_universe` set) in universe.
   - Quantity strictly positive (`Decimal::is_positive`).
   - SEBI `algo_id` matches `^[A-Z0-9_]{4,32}$` (4–32 chars, uppercase/digits/underscore).
   - Limit price strictly positive.
   - **Price-band grounding**: limit must lie within `PriceBand::contains(limit)`.
   - **Price-collar grounding**: deviation from `reference_price` ≤ `max_price_deviation_pct`.
4. **Pre-trade risk gate** (`evaluate_with_risk`): submits the validated decision to
   `vyapaar_risk::RiskEngine::check_decision(decision, now)`; rejections surface as
   `DecisionError::RiskRejected`.

## Zero-Floats Note

All prices/quantities are `rust_decimal::Decimal` (via `vyapaar_core::Price`). No
`f32`/`f64` appears in the decision surface.

## Errors (`DecisionError`)

| Variant | Trigger |
|---------|---------|
| `UnknownField` | `deny_unknown_fields` rejected an unauthorized field |
| `Json` | Structural JSON parse failure |
| `EmptyAlgoId` | Algo ID empty after trim |
| `InvalidAlgoIdFormat` | Algo ID violates `^[A-Z0-9_]{4,32}$` |
| `EmptySymbol` | Order symbol empty |
| `SymbolNotInUniverse` | Symbol not in `allowed_universe` |
| `NonPositiveQuantity` | Qty ≤ 0 |
| `NonPositivePrice` | Price ≤ 0 |
| `PriceBandViolation` | Limit outside `PriceBand` |
| `UngroundedPrice` | Deviation from reference price > collar |
| `RiskRejected` | `RiskEngine::check_decision` refused |

## Related

- `vyapaar_core::Decision` — closed enum (kernel).
- `vyapaar_risk::RiskEngine` — pre-trade gate (gate subsystem).
- `vyapaar_agent_trader_loop::TraderLoop` — wires parser → provider → risk gate.
