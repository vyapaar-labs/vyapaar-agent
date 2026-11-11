//! Content inspection chat tools: news search and regulatory filings search
//! (§6.5, plan 0006 Phase 3).

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use vyapaar_agent_chat::permission::{ToolRisk, ToolTraits};
use vyapaar_agent_chat::tool::{ChatTool, PlanContext, ToolContext, ToolPlan};
use vyapaar_agent_core::{ToolError, ToolOutput, ToolSpec};

use crate::market::parse_instrument;
use crate::ports::content::ContentReader;

/// Maximum number of content items returned per call (token budget cap, §6.5).
pub const MAX_CONTENT_LIMIT: usize = 50;

/// Default number of content items when the caller passes no limit.
pub const DEFAULT_CONTENT_LIMIT: usize = 20;

fn effective_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_CONTENT_LIMIT)
        .min(MAX_CONTENT_LIMIT)
}

// ——————————————————————————————————————————————————————————————————————————
// 1. news_search
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `news_search`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewsSearchInput {
    /// Free-text keyword filter (e.g. "reliance", "rate cut"). Omit for all recent articles.
    pub query: Option<String>,
    /// Instrument mention filter (e.g. "NSE:RELIANCE"). Free-form; matches article tags.
    pub instrument: Option<String>,
    /// Maximum number of articles to return (default 20, cap 50).
    pub limit: Option<usize>,
}

/// Chat tool searching financial news articles (§6.5).
pub struct NewsSearchTool {
    reader: Arc<dyn ContentReader>,
    spec: ToolSpec,
}

impl NewsSearchTool {
    /// Create new news search tool.
    pub fn new(reader: Arc<dyn ContentReader>) -> Self {
        let schema = schemars::schema_for!(NewsSearchInput);
        let spec = ToolSpec::new(
            "news_search",
            "Search financial news articles by keyword or instrument mention.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { reader, spec }
    }
}

#[async_trait]
impl ChatTool for NewsSearchTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let mut target = String::from("all recent articles");
        if let Some(q) = input.get("query").and_then(Value::as_str) {
            target = format!("articles matching `{q}`");
        }
        if let Some(inst) = input.get("instrument").and_then(Value::as_str) {
            target = format!("{target} mentioning `{inst}`");
        }
        Ok(ToolPlan::new(format!("Search news: {target}")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: NewsSearchInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "news_search".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let articles = self
            .reader
            .news_search(
                args.query.as_deref(),
                args.instrument.as_deref(),
                args.limit,
            )
            .await
            .map_err(|e| e.into_tool_error("news_search"))?;

        let articles_json: Vec<Value> = articles
            .into_iter()
            .take(effective_limit(args.limit))
            .map(|a| {
                serde_json::json!({
                    "id": a.id,
                    "title": a.title,
                    "source": a.source,
                    "url": a.url,
                    "published_at": a.published_at,
                    "summary": a.summary,
                    "instruments": a.instruments,
                })
            })
            .collect();

        Ok(ToolOutput::ok(
            "news_search",
            serde_json::json!({
                "articles": articles_json,
                "count": articles_json.len(),
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 2. filings_search
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `filings_search`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FilingsSearchInput {
    /// Instrument symbol filter (e.g. "NSE:RELIANCE", "TCS"). Omit for all instruments.
    pub instrument: Option<String>,
    /// Filing-type filter — one of "corporate_action", "shareholding", "insider_trade",
    /// "fund_holding", "annual_report", "quarterly_result", "other". Omit for all types.
    pub filing_type: Option<String>,
    /// Maximum number of filings to return (default 20, cap 50).
    pub limit: Option<usize>,
}

/// Chat tool searching disclosed regulatory filings (§6.5).
pub struct FilingsSearchTool {
    reader: Arc<dyn ContentReader>,
    spec: ToolSpec,
}

impl FilingsSearchTool {
    /// Create new filings search tool.
    pub fn new(reader: Arc<dyn ContentReader>) -> Self {
        let schema = schemars::schema_for!(FilingsSearchInput);
        let spec = ToolSpec::new(
            "filings_search",
            "Search disclosed regulatory filings for an instrument or filing type.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { reader, spec }
    }
}

#[async_trait]
impl ChatTool for FilingsSearchTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let mut target = String::from("all instruments");
        if let Some(inst) = input.get("instrument").and_then(Value::as_str) {
            target = format!("`{inst}`");
        }
        if let Some(ty) = input.get("filing_type").and_then(Value::as_str) {
            target = format!("{target} of type `{ty}`");
        }
        Ok(ToolPlan::new(format!(
            "Search disclosed filings for {target}"
        )))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: FilingsSearchInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "filings_search".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let inst = args
            .instrument
            .as_deref()
            .map(parse_instrument)
            .transpose()?;
        let filings = self
            .reader
            .filings(inst.as_ref(), args.filing_type.as_deref(), args.limit)
            .await
            .map_err(|e| e.into_tool_error("filings_search"))?;

        let filings_json: Vec<Value> = filings
            .into_iter()
            .take(effective_limit(args.limit))
            .map(|f| {
                serde_json::json!({
                    "instrument": f.instrument.to_string(),
                    "filing_date": f.filing_date,
                    "filing_type": f.filing_type,
                    "title": f.title,
                    "source_url": f.source_url,
                })
            })
            .collect();

        Ok(ToolOutput::ok(
            "filings_search",
            serde_json::json!({
                "filings": filings_json,
                "count": filings_json.len(),
            }),
        ))
    }
}
