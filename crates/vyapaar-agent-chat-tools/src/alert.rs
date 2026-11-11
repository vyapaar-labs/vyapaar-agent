//! Price-alert chat tools (§6.5, plan 0006 Phase 2).
//!
//! Longbridge's Alert category shape (add/delete/enable/disable/list) mapped onto this repo's
//! `ToolRisk` tiers: the list tool is read-only, the CRUD tool is
//! [`ToolRisk::LocalWrite`](vyapaar_agent_chat::permission::ToolRisk::LocalWrite) — platform-
//! managed state, never a real-money action (NoFX untrusted-AI principle, §5.5). Storage is the
//! user's `Profile.alerts` document; see the `AlertReader`/`AlertWriter` port docs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use vyapaar_agent_chat::permission::{ToolRisk, ToolTraits};
use vyapaar_agent_chat::tool::{ChatTool, PlanContext, ToolContext, ToolPlan};
use vyapaar_agent_core::{ToolError, ToolOutput, ToolSpec};
use vyapaar_core::Price;

use crate::market::parse_instrument;
use crate::ports::alert::{AlertReader, AlertStatus, AlertWriter, PriceAlert};

/// Workspace scope alerts fall in when the caller passes none — the same fallback
/// `GET /ws/chat?workspace=` uses when no workspace query param is given.
pub const DEFAULT_WORKSPACE: &str = "default";

/// Mints a ui-web-format alert id (`al_<unix_ms>_<counter>`); uniqueness within this process
/// comes from the counter, cross-process uniqueness from the millisecond timestamp (one writer
/// process per profiles directory).
fn new_alert_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let millis = Utc::now().timestamp_millis();
    format!("al_{millis}_{seq:06x}")
}

/// Today's UTC date, the `createdAt` format `vyapaar-ui-web`'s alerts store uses.
fn today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

/// Resolve the optional workspace input to a concrete scope.
fn workspace_of(input_workspace: Option<&str>) -> String {
    input_workspace
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .unwrap_or(DEFAULT_WORKSPACE)
        .to_string()
}

fn invalid(tool: &str, message: impl Into<String>) -> ToolError {
    ToolError::InvalidArguments {
        tool: tool.to_string(),
        message: message.into(),
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 1. price_alert_list (read-only)
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `price_alert_list`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PriceAlertListInput {
    /// Filter by instrument symbol (e.g. "NSE:RELIANCE", "TCS"). Omit for all instruments.
    pub instrument: Option<String>,
    /// Filter by lifecycle status ("active", "triggered", "paused"). Omit for all statuses.
    pub status: Option<AlertStatus>,
    /// Workspace scope (e.g. "main"). Omit for the default workspace.
    pub workspace: Option<String>,
}

/// Chat tool listing the user's stored price alerts (read-only).
pub struct PriceAlertListTool {
    reader: Arc<dyn AlertReader>,
    spec: ToolSpec,
}

impl PriceAlertListTool {
    /// Create new price-alert list tool.
    pub fn new(reader: Arc<dyn AlertReader>) -> Self {
        let schema = schemars::schema_for!(PriceAlertListInput);
        let spec = ToolSpec::new(
            "price_alert_list",
            "List the stored price alerts, optionally filtered by instrument or status.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { reader, spec }
    }
}

#[async_trait]
impl ChatTool for PriceAlertListTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let workspace = workspace_of(input.get("workspace").and_then(Value::as_str));
        Ok(ToolPlan::new(format!(
            "List price alerts for workspace `{workspace}`"
        )))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: PriceAlertListInput = serde_json::from_value(input).map_err(|e| {
            invalid(
                "price_alert_list",
                format!("schema validation failure: {e}"),
            )
        })?;

        let workspace = workspace_of(args.workspace.as_deref());
        let mut alerts = self
            .reader
            .list_alerts(Some(&workspace))
            .await
            .map_err(|e| e.into_tool_error("price_alert_list"))?;

        if let Some(raw) = args.instrument.as_deref() {
            if raw.trim().is_empty() {
                return Err(invalid(
                    "price_alert_list",
                    "instrument filter cannot be empty",
                ));
            }
            let want = parse_instrument(raw)?.to_string();
            alerts.retain(|a| a.instrument_id.eq_ignore_ascii_case(&want));
        }
        if let Some(status) = args.status {
            alerts.retain(|a| a.status == status);
        }

        let count = alerts.len();
        Ok(ToolOutput::ok(
            "price_alert_list",
            serde_json::json!({
                "workspace": workspace,
                "alerts": alerts,
                "count": count,
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 2. price_alert (LocalWrite CRUD)
// ——————————————————————————————————————————————————————————————————————————

/// CRUD operation of `price_alert`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlertOperation {
    /// Create an active alert for an instrument at a target price.
    Add,
    /// Remove an alert by id (any status).
    Delete,
    /// Resume monitoring a paused alert (no-op on a triggered one).
    Enable,
    /// Stop monitoring an active alert (no-op on a triggered one).
    Disable,
}

/// Input arguments for `price_alert`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PriceAlertInput {
    /// CRUD operation: "add", "delete", "enable", or "disable".
    pub operation: AlertOperation,
    /// Workspace scope (e.g. "main"). Omit for the default workspace.
    pub workspace: Option<String>,
    /// Instrument symbol — required for `add` (e.g. "NSE:RELIANCE").
    pub instrument: Option<String>,
    /// Target price as a decimal string — required for `add` (e.g. "2500.50").
    pub price: Option<String>,
    /// Alert id — required for `delete`, `enable`, and `disable`.
    pub id: Option<String>,
}

/// Chat tool creating, removing, enabling, and disabling the user's price alerts
/// (`ToolRisk::LocalWrite`: platform-managed state, prompted for confirmation).
pub struct PriceAlertTool {
    writer: Arc<dyn AlertWriter>,
    spec: ToolSpec,
}

impl PriceAlertTool {
    /// Create new price-alert CRUD tool.
    pub fn new(writer: Arc<dyn AlertWriter>) -> Self {
        let schema = schemars::schema_for!(PriceAlertInput);
        let spec = ToolSpec::new(
            "price_alert",
            "Add, delete, enable, or disable a stored price alert for an instrument.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { writer, spec }
    }
}

#[async_trait]
impl ChatTool for PriceAlertTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::LocalWrite)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let op = match input.get("operation").and_then(Value::as_str) {
            Some("add") => {
                let inst = input
                    .get("instrument")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                format!("Create price alert for `{inst}`")
            }
            Some("delete") => format!(
                "Delete price alert `{}`",
                input.get("id").and_then(Value::as_str).unwrap_or("unknown")
            ),
            Some("enable") => format!(
                "Enable price alert `{}`",
                input.get("id").and_then(Value::as_str).unwrap_or("unknown")
            ),
            Some("disable") => format!(
                "Disable price alert `{}`",
                input.get("id").and_then(Value::as_str).unwrap_or("unknown")
            ),
            _ => "Run a price-alert CRUD operation".to_string(),
        };
        Ok(ToolPlan::new(op))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: PriceAlertInput = serde_json::from_value(input)
            .map_err(|e| invalid("price_alert", format!("schema validation failure: {e}")))?;

        let workspace = workspace_of(args.workspace.as_deref());
        let alert = match args.operation {
            AlertOperation::Add => self.add(&workspace, args).await?,
            AlertOperation::Delete => self.delete(&args.id).await?,
            AlertOperation::Enable => self.set_status(&args.id, AlertStatus::Active).await?,
            AlertOperation::Disable => self.set_status(&args.id, AlertStatus::Paused).await?,
        };

        Ok(ToolOutput::ok(
            "price_alert",
            serde_json::json!({ "alert": alert }),
        ))
    }
}

impl PriceAlertTool {
    /// `add`: validate instrument + price, mint the id, persist an active alert.
    async fn add(&self, workspace: &str, args: PriceAlertInput) -> Result<PriceAlert, ToolError> {
        let raw_instrument = args
            .instrument
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("price_alert", "`add` requires an `instrument`"))?;
        let instrument = parse_instrument(raw_instrument)?;

        let raw_price = args
            .price
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("price_alert", "`add` requires a decimal-string `price`"))?;
        let price =
            Price::from_rupees(raw_price).map_err(|e| invalid("price_alert", e.to_string()))?;

        let alert = PriceAlert {
            id: new_alert_id(),
            workspace_id: workspace.to_string(),
            instrument_id: instrument.to_string(),
            price,
            status: AlertStatus::Active,
            created_at: today(),
            triggered_at: None,
        };
        self.writer
            .add_alert(alert.clone())
            .await
            .map_err(|e| e.into_tool_error("price_alert"))
    }

    /// `enable`/`disable`: set the status, mapping an unknown id to an explicit not-found error.
    async fn set_status(
        &self,
        id: &Option<String>,
        status: AlertStatus,
    ) -> Result<PriceAlert, ToolError> {
        let id = alert_id(id, "price_alert")?;
        self.writer
            .set_alert_status(&id, status)
            .await
            .map_err(|e| e.into_tool_error("price_alert"))?
            .ok_or_else(|| {
                let e = crate::ports::error::PortError::not_found(format!(
                    "no price alert with id `{id}`"
                ));
                e.into_tool_error("price_alert")
            })
    }

    /// `delete`: remove the alert, mapping an unknown id to an explicit not-found error.
    async fn delete(&self, id: &Option<String>) -> Result<PriceAlert, ToolError> {
        let id = alert_id(id, "price_alert")?;
        self.writer
            .delete_alert(&id)
            .await
            .map_err(|e| e.into_tool_error("price_alert"))?
            .ok_or_else(|| {
                let e = crate::ports::error::PortError::not_found(format!(
                    "no price alert with id `{id}`"
                ));
                e.into_tool_error("price_alert")
            })
    }
}

/// Validate the optional alert-id input into a concrete id.
fn alert_id(id: &Option<String>, tool: &str) -> Result<String, ToolError> {
    id.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(tool, "`delete`, `enable`, and `disable` require an `id`"))
}
