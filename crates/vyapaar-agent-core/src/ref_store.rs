//! Ephemeral in-memory data reference store emitting `$vyapaar/ref/<uuid>` URIs.
//!
//! To prevent huge token costs when chaining market quotes into technicals and
//! portfolio optimization, large time-series are cached here by value and only
//! a compact reference URI is returned to the caller. Downstream tools resolve
//! the reference back to the full payload.
//!
//! Zero-floats note: all stored values are arbitrary `serde_json::Value`s; any
//! `Decimal`-derived numbers are serialised to the model as JSON strings so
//! precision survives the wire (see `vyapaar-agent-core::result` precision
//! contract `"exact_decimal_zero_floats"`).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single cached reference entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefEntry {
    /// Full reference URI, e.g. `$vyapaar/ref/ohlcv_reliance_5m_20261106`.
    pub uri: String,
    /// Cached payload.
    pub data: Value,
    /// Size of the serialised payload in bytes (for telemetry / cap checks).
    pub size_bytes: usize,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// In-memory Arrow/SQLite-backed ephemeral cache keyed by
/// `$vyapaar/ref/<reference_id>`.
///
/// `export` persists a payload and returns its reference URI;
/// `resolve` rehydrates a previously-exported payload by URI.
#[derive(Debug, Default, Clone)]
pub struct DataReferenceStore {
    inner: Arc<RwLock<HashMap<String, RefEntry>>>,
}

impl DataReferenceStore {
    /// Create an empty reference store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Persist `data` under `reference_id` and return its reference URI
    /// (`$vyapaar/ref/<reference_id>`).
    pub fn export(&self, reference_id: impl Into<String>, data: Value) -> String {
        let reference_id = reference_id.into();
        let uri = format!("$vyapaar/ref/{reference_id}");
        let serialised = serde_json::to_string(&data).unwrap_or_default();
        let entry = RefEntry {
            uri: uri.clone(),
            data,
            size_bytes: serialised.len(),
            created_at: Utc::now(),
        };
        self.inner.write().unwrap().insert(uri.clone(), entry);
        uri
    }

    /// Rehydrate a previously exported payload by its reference URI.
    ///
    /// Returns `None` when the reference is unknown or has expired.
    pub fn resolve(&self, reference_uri: &str) -> Option<Value> {
        self.inner
            .read()
            .unwrap()
            .get(reference_uri)
            .map(|entry| entry.data.clone())
    }

    /// Return the number of entries currently cached.
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().len()
    }

    /// Return `true` when no entries are cached.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Remove a reference by URI. Returns `true` if an entry was removed.
    pub fn remove(&self, reference_uri: &str) -> bool {
        self.inner.write().unwrap().remove(reference_uri).is_some()
    }

    /// Iterate over all cached reference URIs (for discovery / introspection).
    pub fn uris(&self) -> Vec<String> {
        self.inner.read().unwrap().keys().cloned().collect()
    }
}

impl FromIterator<(String, Value)> for DataReferenceStore {
    fn from_iter<T: IntoIterator<Item = (String, Value)>>(iter: T) -> Self {
        let store = DataReferenceStore::new();
        for (id, data) in iter {
            store.export(id, data);
        }
        store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_and_resolve_round_trip() {
        let store = DataReferenceStore::new();
        let uri = store.export(
            "ohlcv_reliance_5m",
            serde_json::json!({"instrument": "NSE:RELIANCE-EQ", "close": "2500.00"}),
        );
        assert_eq!(uri, "$vyapaar/ref/ohlcv_reliance_5m");
        assert!(store.resolve(&uri).is_some());
        let resolved = store.resolve(&uri).unwrap();
        assert_eq!(resolved["instrument"], "NSE:RELIANCE-EQ");
        assert_eq!(resolved["close"], "2500.00");
    }

    #[test]
    fn unknown_reference_resolves_to_none() {
        let store = DataReferenceStore::new();
        assert!(store.resolve("$vyapaar/ref/does-not-exist").is_none());
        assert!(store.is_empty());
    }

    #[test]
    fn export_overwrites_same_id() {
        let store = DataReferenceStore::new();
        let uri = store.export("key", serde_json::json!({"v": 1}));
        assert_eq!(store.len(), 1);
        store.export("key", serde_json::json!({"v": 2}));
        assert_eq!(store.len(), 1);
        assert_eq!(store.resolve(&uri).unwrap()["v"], 2);
    }

    #[test]
    fn remove_and_uris() {
        let store = DataReferenceStore::new();
        store.export("a", serde_json::json!({"v": 1}));
        store.export("b", serde_json::json!({"v": 2}));
        assert_eq!(store.len(), 2);
        let mut uris = store.uris();
        uris.sort();
        assert_eq!(
            uris,
            vec!["$vyapaar/ref/a".to_string(), "$vyapaar/ref/b".to_string()]
        );
        assert!(store.remove("$vyapaar/ref/a"));
        assert_eq!(store.len(), 1);
        assert!(store.resolve("$vyapaar/ref/a").is_none());
        assert!(store.resolve("$vyapaar/ref/b").is_some());
    }
}
