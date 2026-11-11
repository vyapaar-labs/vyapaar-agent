//! Price-alert storage port and DTO contracts (§6.4).
//!
//! Alerts are platform-managed user state (`ToolRisk::LocalWrite`'s "watchlists, monitors"
//! category), never real-money actions. The wire shape mirrors `vyapaar-ui-web`'s
//! `alerts-store.ts::AlertMeta` exactly — both writers share the same opaque `Profile.alerts`
//! JSON array (the `vyapaar-runtime/docs/gaps/0004-*.md` profile-schema precedent), so the
//! fields are camelCase and the target price stays a decimal string.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyapaar_core::Price;

use crate::ports::error::PortError;

/// Lifecycle status of a price alert (matches `vyapaar-ui-web`'s `AlertStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    /// Watching for the price cross.
    Active,
    /// The cross fired; set only by `vyapaar-ui-web`'s evaluation loop, never by chat tools.
    Triggered,
    /// Monitoring disabled by the user.
    Paused,
}

/// One price-alert rule.
///
/// Deliberately tolerant on deserialization (no `deny_unknown_fields`): newer clients may add
/// fields, and the array is shared with another writer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceAlert {
    /// Stable alert id (ui-web format family: `al_<unix_ms>_<suffix>`), minted by the caller.
    pub id: String,
    /// Workspace the alert belongs to (ui-web scopes alerts per workspace).
    pub workspace_id: String,
    /// Instrument identifier as stored (e.g. "NSE:RELIANCE"); validated at creation.
    pub instrument_id: String,
    /// Decimal-safe target price (string on the wire).
    pub price: Price,
    /// Lifecycle status.
    pub status: AlertStatus,
    /// Creation date ("yyyy-mm-dd"); an opaque string so foreign writers can't break the read.
    pub created_at: String,
    /// When the alert triggered, if it did (set by ui-web's evaluation loop only).
    pub triggered_at: Option<String>,
}

/// Read port for the price-alert rules stored on the user's profile (§6.4).
#[async_trait]
pub trait AlertReader: Send + Sync + 'static {
    /// All alerts, optionally scoped to one workspace id.
    async fn list_alerts(&self, workspace: Option<&str>) -> Result<Vec<PriceAlert>, PortError>;
}

/// Write port for price-alert CRUD — platform-managed state (`ToolRisk::LocalWrite`, §5.5).
///
/// Implementors must apply each change as one atomic read-modify-write against the alert store:
/// `vyapaar-ui-web` writes the same documents through the `/profile` routes, so a change must
/// never silently drop alerts the writer didn't see.
#[async_trait]
pub trait AlertWriter: Send + Sync + 'static {
    /// Persist a fully-formed alert (the caller mints `id`, sets `status: Active`, fills
    /// `created_at`). Fails with [`PortErrorKind::InvalidArgument`] when an alert with the same
    /// id already exists — ids are caller-owned and never overwritten.
    ///
    /// [`PortErrorKind`]: crate::ports::error::PortErrorKind
    async fn add_alert(&self, alert: PriceAlert) -> Result<PriceAlert, PortError>;

    /// Set an alert's status with ui-web parity: a `Triggered` alert is left unchanged by both
    /// directions (enable/disable on a fired alert is a no-op, exactly as `alerts-store.ts`'s
    /// toggle is). Returns the stored alert afterwards, or `None` when the id is unknown.
    async fn set_alert_status(
        &self,
        id: &str,
        status: AlertStatus,
    ) -> Result<Option<PriceAlert>, PortError>;

    /// Remove an alert regardless of status. Returns the removed alert, or `None` when the id
    /// is unknown.
    async fn delete_alert(&self, id: &str) -> Result<Option<PriceAlert>, PortError>;
}
