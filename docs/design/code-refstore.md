# `vyapaar-agent` — Data Reference Store Reference

Documents `vyapaar-agent-core/src/ref_store.rs`: the ephemeral in-memory data reference
store emitting `$vyapaar/ref/<uuid>` URIs.

## Purpose

To prevent huge token costs when chaining market quotes into technicals and portfolio
optimization, large time-series are cached here by value and only a compact reference URI is
returned to the caller. Downstream tools resolve the reference back to the full payload.

## `DataReferenceStore`

```rust
pub struct DataReferenceStore {
    inner: Arc<RwLock<HashMap<String, RefEntry>>>,
}
```

### Methods

| Method | Signature | Behavior |
|--------|-----------|----------|
| `new` | `() -> Self` | Empty store |
| `export` | `(reference_id, data: Value) -> String` | Persists payload, returns `$vyapaar/ref/<reference_id>` |
| `resolve` | `(&reference_uri) -> Option<Value>` | Rehydrates a previously exported payload |
| `len` | `() -> usize` | Cached entry count |
| `is_empty` | `() -> bool` | True when no entries |
| `remove` | `(&reference_uri) -> bool` | Remove entry by URI |
| `uris` | `() -> Vec<String>` | All cached reference URIs (discovery/introspection) |

## `RefEntry`

| Field | Type | Meaning |
|-------|------|---------|
| `uri` | `String` | Full `$vyapaar/ref/<reference_id>` URI |
| `data` | `serde_json::Value` | Cached payload |
| `size_bytes` | `usize` | Serialised payload size (telemetry / cap checks) |
| `created_at` | `DateTime<Utc>` | Creation timestamp |

## Zero-Floats Note

All stored values are arbitrary `serde_json::Value`s; any `Decimal`-derived numbers are
serialised to the model as JSON strings so precision survives the wire (matching the
`vyapaar-agent-core::result` precision contract `"exact_decimal_zero_floats"`).

## Note on Test Placement (gap 0001)

This file currently contains an inline `#[cfg(test)] mod tests` block (four unit tests),
which violates the workspace TDD rule forbidding in-file `#[test]` in production `src/`.
These tests are to be moved to `crates/vyapaar-agent-core/tests/ref_store_tests.rs`
(see `docs/plans/0002-tdd-cleanup-and-chat-tools.md`, gap 0001).

## Related

- `vyapaar-agent-core/src/result.rs` — `VyapaarResultEnvelope.refs` holds these URIs.
- `vyapaar-agent-mcp/src/server.rs` — `McpServer.data_store: DataReferenceStore`.
