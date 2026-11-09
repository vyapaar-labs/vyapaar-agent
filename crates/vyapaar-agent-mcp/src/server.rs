//! MCP Server and JSON-RPC 2.0 message handling.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::McpError;
use crate::tools::{
    BacktestDispatcherService, DecisionExplainorService, ForecastService, IndicatorCatalogService,
    MetricsCalculatorService, SpecValidatorService, StrategyOptimizerService,
    list_tool_definitions,
};
use vyapaar_agent_core::{DataReferenceStore, DisplayBlock, MetricItem, VyapaarResultEnvelope};

/// JSON-RPC 2.0 message envelope.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum RpcMessage {
    Request {
        id: serde_json::Value,
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    Response {
        id: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RpcError>,
    },
    Notification {
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: msg.into(),
            data: None,
        }
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: msg.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {method}"),
            data: None,
        }
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: msg.into(),
            data: None,
        }
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: msg.into(),
            data: None,
        }
    }
}

/// Model Context Protocol quantitative server.
pub struct McpServer {
    pub indicator_catalog: IndicatorCatalogService,
    pub spec_validator: SpecValidatorService,
    pub backtest_dispatcher: BacktestDispatcherService,
    pub metrics_calculator: MetricsCalculatorService,
    pub strategy_optimizer: StrategyOptimizerService,
    pub decision_explainor: DecisionExplainorService,
    pub forecast: ForecastService,
    /// Ephemeral data reference store (`$vyapaar/ref/<uuid>`) for chaining
    /// large time-series between tools without token roundtrips.
    pub data_store: DataReferenceStore,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    /// Create a new default MCP server instance.
    pub fn new() -> Self {
        Self {
            indicator_catalog: IndicatorCatalogService::new(),
            spec_validator: SpecValidatorService::new(None),
            backtest_dispatcher: BacktestDispatcherService::new(),
            metrics_calculator: MetricsCalculatorService::new(),
            strategy_optimizer: StrategyOptimizerService::new(),
            decision_explainor: DecisionExplainorService::new(),
            forecast: ForecastService::new(),
            data_store: DataReferenceStore::new(),
        }
    }

    /// Handle an incoming JSON-RPC 2.0 message and generate a response.
    pub async fn handle_message(&self, message: &RpcMessage) -> Option<RpcMessage> {
        match message {
            RpcMessage::Request { id, method, params } => {
                let resp = self.dispatch(method, params.clone(), id.clone()).await;
                Some(resp)
            }
            RpcMessage::Notification { method, params } => {
                // Notifications do not return responses per JSON-RPC 2.0
                let _ = self.dispatch(method, params.clone(), json!(null)).await;
                None
            }
            RpcMessage::Response { .. } => None,
        }
    }

    /// Dispatch a JSON-RPC method call to the appropriate tool handler.
    async fn dispatch(
        &self,
        method: &str,
        params: serde_json::Value,
        id: serde_json::Value,
    ) -> RpcMessage {
        match method {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "logging": {}
                    },
                    "serverInfo": {
                        "name": "vyapaar-agent",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                });
                RpcMessage::Response {
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/list" => {
                let tools = list_tool_definitions();
                let result = json!({ "tools": tools });
                RpcMessage::Response {
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/call" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                match self.call_tool(name, arguments).await {
                    Ok(tool_result) => {
                        let result = json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&tool_result).unwrap_or_default()
                                }
                            ],
                            "structuredContent": tool_result
                        });
                        RpcMessage::Response {
                            id,
                            result: Some(result),
                            error: None,
                        }
                    }
                    Err(err) => RpcMessage::Response {
                        id,
                        result: None,
                        error: Some(RpcError::internal_error(err.to_string())),
                    },
                }
            }
            "ping" => RpcMessage::Response {
                id,
                result: Some(json!({})),
                error: None,
            },
            _ => RpcMessage::Response {
                id,
                result: None,
                error: Some(RpcError::method_not_found(method)),
            },
        }
    }

    /// Call an MCP quantitative tool by name.
    ///
    /// Every result is wrapped in the universal [`VyapaarResultEnvelope`]
    /// (`vyapaar/result/v1`) so all tools conform to the same result contract.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let data = match name {
            "get_indicator_catalog" => self.indicator_catalog.get_catalog_json(),
            "check_spec" => {
                let sm = arguments.get("strategy_manifest").ok_or_else(|| {
                    McpError::InvalidArguments("missing strategy_manifest".to_string())
                })?;
                self.spec_validator.validate(sm)?
            }
            "run_backtest" => {
                let sm = arguments.get("strategy_manifest").ok_or_else(|| {
                    McpError::InvalidArguments("missing strategy_manifest".to_string())
                })?;
                let rc = arguments
                    .get("run_config")
                    .ok_or_else(|| McpError::InvalidArguments("missing run_config".to_string()))?;
                self.backtest_dispatcher.dispatch(sm, rc)?
            }
            "evaluate_metrics" => {
                let tl = arguments
                    .get("trade_log")
                    .ok_or_else(|| McpError::InvalidArguments("missing trade_log".to_string()))?;
                let re = arguments.get("risk_engine_state");
                let cfg = arguments.get("config");
                self.metrics_calculator.compute(tl, re, cfg)?
            }
            "optimize_strategy" => {
                let sm = arguments.get("strategy_manifest").ok_or_else(|| {
                    McpError::InvalidArguments("missing strategy_manifest".to_string())
                })?;
                let oc = arguments.get("optimization_config").ok_or_else(|| {
                    McpError::InvalidArguments("missing optimization_config".to_string())
                })?;
                self.strategy_optimizer.optimize(sm, oc)?
            }
            "explain_decision" => {
                let decision = arguments
                    .get("decision")
                    .ok_or_else(|| McpError::InvalidArguments("missing decision".to_string()))?;
                let market_context = arguments.get("market_context").ok_or_else(|| {
                    McpError::InvalidArguments("missing market_context".to_string())
                })?;
                let risk_check_results = arguments.get("risk_check_results").ok_or_else(|| {
                    McpError::InvalidArguments("missing risk_check_results".to_string())
                })?;
                self.decision_explainor
                    .explain(decision, market_context, risk_check_results)?
            }
            "vyapaar_forecast_series" => {
                let envelope = self.forecast.forecast_series(&arguments)?;
                // `forecast_series` already returns a full envelope; serialize directly.
                return serde_json::to_value(envelope).map_err(McpError::Json);
            }
            "vyapaar_search_tools" => {
                let envelope = self.search_tools(&arguments)?;
                return serde_json::to_value(envelope).map_err(McpError::Json);
            }
            "vyapaar_describe_tool" => {
                let envelope = self.describe_tool(&arguments)?;
                return serde_json::to_value(envelope).map_err(McpError::Json);
            }
            "vyapaar_read_result" => {
                let envelope = self.read_result(&arguments)?;
                return serde_json::to_value(envelope).map_err(McpError::Json);
            }
            _ => return Err(McpError::MethodNotFound(format!("unknown tool: {name}"))),
        };

        // Wrap raw data payloads in the universal result envelope.
        let summary = match name {
            "get_indicator_catalog" => "Indicator catalog".to_string(),
            "check_spec" => "Strategy specification validation".to_string(),
            "run_backtest" => "Backtest dispatched".to_string(),
            "evaluate_metrics" => "Performance metrics computed".to_string(),
            "optimize_strategy" => "Strategy optimization started".to_string(),
            "explain_decision" => "Decision explanation".to_string(),
            _ => name.to_string(),
        };
        let envelope = VyapaarResultEnvelope::ok(summary, data);
        serde_json::to_value(envelope).map_err(McpError::Json)
    }

    // -----------------------------------------------------------------------
    // Two-Tier Gateway Discovery Protocol
    // -----------------------------------------------------------------------

    /// Search tools by natural language query, asset class, or analytical
    /// category. Returns a ranked list of tool names, categories, and concise
    /// 1-line taglines — the compact discovery surface used instead of
    /// advertising 100+ static schemas in `tools/list`.
    pub fn search_tools(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments("missing query".to_string()))?;
        let category = arguments.get("category").and_then(|v| v.as_str());

        let query_lower = query.to_lowercase();
        let tools = list_tool_definitions();

        let mut matches: Vec<serde_json::Value> = tools
            .into_iter()
            .filter(|tool| {
                let name = tool["name"].as_str().unwrap_or("").to_lowercase();
                let desc = tool["description"].as_str().unwrap_or("").to_lowercase();
                let cat = tool.get("category").and_then(|v| v.as_str()).unwrap_or("");
                let matches_query = name.contains(&query_lower) || desc.contains(&query_lower);
                // "all" means no category filter; otherwise match exact category.
                let matches_category = category.map_or(true, |c| c == "all" || cat == c);
                matches_query && matches_category
            })
            .map(|tool| {
                let tagline = tool["description"].as_str().unwrap_or("").to_string();
                let category = tool
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("all")
                    .to_string();
                serde_json::json!({
                    "name": tool["name"],
                    "category": category,
                    "tagline": tagline,
                })
            })
            .collect();

        // Rank exact name matches first, then prefix matches, then substring.
        matches.sort_by(|a, b| {
            let a_name = a["name"].as_str().unwrap_or("").to_lowercase();
            let b_name = b["name"].as_str().unwrap_or("").to_lowercase();
            let a_exact = a_name == query_lower;
            let b_exact = b_name == query_lower;
            let a_prefix = a_name.starts_with(&query_lower);
            let b_prefix = b_name.starts_with(&query_lower);
            b_exact.cmp(&a_exact).then(b_prefix.cmp(&a_prefix))
        });

        let envelope = VyapaarResultEnvelope::ok(
            format!("Found {} tool(s) matching '{}'", matches.len(), query),
            serde_json::json!({
                "query": query,
                "category": category.unwrap_or("all"),
                "count": matches.len(),
                "tools": matches,
            }),
        )
        .with_block(DisplayBlock::Table {
            title: "Tool Discovery Results".to_string(),
            columns: vec![
                "name".to_string(),
                "category".to_string(),
                "tagline".to_string(),
            ],
            rows: matches
                .iter()
                .map(|t| {
                    vec![
                        t["name"].as_str().unwrap_or("").to_string(),
                        t["category"].as_str().unwrap_or("all").to_string(),
                        t["tagline"].as_str().unwrap_or("").to_string(),
                    ]
                })
                .collect(),
        });

        serde_json::to_value(envelope).map_err(McpError::Json)
    }

    /// Return the full JSON-Schema, parameter constraints, mathematical
    /// definitions, and examples for a chosen tool.
    pub fn describe_tool(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let tool_name = arguments
            .get("tool_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments("missing tool_name".to_string()))?;

        let tools = list_tool_definitions();
        let tool = tools
            .into_iter()
            .find(|t| t["name"] == tool_name)
            .ok_or_else(|| McpError::MethodNotFound(format!("unknown tool: {tool_name}")))?;

        let envelope = VyapaarResultEnvelope::ok(
            format!("Tool schema for '{tool_name}'"),
            serde_json::json!({
                "tool_name": tool_name,
                "schema": tool,
            }),
        )
        .with_block(DisplayBlock::MetricGrid {
            items: vec![
                MetricItem {
                    label: "name".to_string(),
                    value: tool_name.to_string(),
                },
                MetricItem {
                    label: "description".to_string(),
                    value: tool["description"].as_str().unwrap_or("").to_string(),
                },
                MetricItem {
                    label: "category".to_string(),
                    value: tool
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or("all")
                        .to_string(),
                },
            ],
        });

        serde_json::to_value(envelope).map_err(McpError::Json)
    }

    /// Read a previously exported data reference by its `$vyapaar/ref/<uuid>`
    /// URI, with optional offset/limit pagination for array payloads.
    pub fn read_result(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let ref_id = arguments
            .get("ref_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments("missing ref_id".to_string()))?;
        let offset = arguments
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let data = self
            .data_store
            .resolve(ref_id)
            .ok_or_else(|| McpError::MethodNotFound(format!("unknown reference: {ref_id}")))?;

        let paginated = if let Some(limit) = limit {
            if let Some(arr) = data.as_array() {
                let end = (offset + limit).min(arr.len());
                if offset >= arr.len() {
                    serde_json::json!([])
                } else {
                    serde_json::json!(arr[offset..end])
                }
            } else {
                // Non-array payloads are returned whole; offset/limit ignored.
                data
            }
        } else {
            data
        };

        let envelope = VyapaarResultEnvelope::ok(
            format!("Reference '{ref_id}' resolved"),
            serde_json::json!({
                "ref_id": ref_id,
                "data": paginated,
            }),
        );

        serde_json::to_value(envelope).map_err(McpError::Json)
    }
}
