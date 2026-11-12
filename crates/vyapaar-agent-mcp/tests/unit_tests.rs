//! Integration and Golden Conformance tests for vyapaar-agent-mcp.

use std::fs;
use std::sync::Arc;

use serde_json::json;
use vyapaar_agent_mcp::frontier::ManifestGenerator;
use vyapaar_agent_mcp::server::{McpServer, RpcMessage};
use vyapaar_agent_mcp::tools::list_tool_definitions;
use vyapaar_agent_mcp::transport::{create_router, run_stdio_transport};

#[tokio::test]
async fn test_mcp_initialize() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(1),
        method: "initialize".to_string(),
        params: json!({}),
    };

    let resp = server
        .handle_message(&msg)
        .await
        .expect("expected response");
    if let RpcMessage::Response { id, result, error } = resp {
        assert_eq!(id, json!(1));
        assert!(error.is_none());
        let res = result.expect("result should exist");
        assert_eq!(res["protocolVersion"], "2024-11-05");
        assert_eq!(res["serverInfo"]["name"], "vyapaar-agent");
        assert!(res["capabilities"]["tools"].is_object());
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_mcp_tools_list() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!("req-2"),
        method: "tools/list".to_string(),
        params: json!({}),
    };

    let resp = server
        .handle_message(&msg)
        .await
        .expect("expected response");
    if let RpcMessage::Response { id, result, error } = resp {
        assert_eq!(id, json!("req-2"));
        assert!(error.is_none());
        let res = result.expect("result should exist");
        let tools = res["tools"].as_array().expect("tools array");
        assert!(tools.len() >= 6);

        let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

        assert!(tool_names.contains(&"get_indicator_catalog"));
        assert!(tool_names.contains(&"check_spec"));
        assert!(tool_names.contains(&"run_backtest"));
        assert!(tool_names.contains(&"evaluate_metrics"));
        assert!(tool_names.contains(&"optimize_strategy"));
        assert!(tool_names.contains(&"explain_decision"));
        assert!(tool_names.contains(&"vyapaar_forecast_series"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_get_indicator_catalog() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(3),
        method: "tools/call".to_string(),
        params: json!({
            "name": "get_indicator_catalog",
            "arguments": {}
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        let data = &structured["data"];
        let indicators = data["indicators"].as_array().expect("indicators array");
        assert!(indicators.len() >= 10);

        let names: Vec<&str> = indicators
            .iter()
            .filter_map(|i| i["name"].as_str())
            .collect();

        // Verify catalog contains core verified indicators
        assert!(names.contains(&"SMA"));
        assert!(names.contains(&"EMA"));
        assert!(names.contains(&"RSI"));
        assert!(names.contains(&"MACD"));
        assert!(names.contains(&"CPR"));
        assert!(names.contains(&"VWAP"));
        assert!(names.contains(&"ATR"));
        assert!(names.contains(&"SUPERTREND"));
        assert!(names.contains(&"BOLLINGER_BANDS"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_check_spec_valid_and_invalid() {
    let server = McpServer::new();

    // 1. Valid specification
    let valid_spec = json!({
        "name": "TEST_MOMENTUM_ALGO",
        "driving_timeframe": { "aggregation": "minute", "interval": 5 },
        "warmup_bars": 50,
        "indicators": [
            {
                "name": "RSI",
                "params": { "length": 14.0 }
            },
            {
                "name": "SMA",
                "params": { "length": 50.0 }
            }
        ]
    });

    let msg = RpcMessage::Request {
        id: json!(4),
        method: "tools/call".to_string(),
        params: json!({
            "name": "check_spec",
            "arguments": {
                "strategy_manifest": valid_spec
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        let data = &structured["data"];
        assert_eq!(data["status"], "valid");
    } else {
        panic!("unexpected response type");
    }

    // 2. Invalid specification with unapproved hallucinated indicator
    let invalid_spec = json!({
        "name": "HALLUCINATED_ALGO",
        "indicators": [
            {
                "name": "SECRET_QUANTUM_ORACLE",
                "params": { "alpha": 99.0 }
            }
        ]
    });

    let msg_invalid = RpcMessage::Request {
        id: json!(5),
        method: "tools/call".to_string(),
        params: json!({
            "name": "check_spec",
            "arguments": {
                "strategy_manifest": invalid_spec
            }
        }),
    };

    let resp_invalid = server.handle_message(&msg_invalid).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp_invalid {
        assert!(error.is_some());
        let err = error.unwrap();
        assert!(
            err.message.contains("SECRET_QUANTUM_ORACLE") || err.message.contains("validation")
        );
    } else {
        panic!("expected validation error");
    }
}

#[tokio::test]
async fn test_tool_run_backtest() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(6),
        method: "tools/call".to_string(),
        params: json!({
            "name": "run_backtest",
            "arguments": {
                "strategy_manifest": {
                    "name": "NIFTY_TREND_V1"
                },
                "run_config": {
                    "seed": 1337,
                    "account": {
                        "starting_cash": 2500000.0,
                        "currency": "INR"
                    },
                    "execution": {
                        "apply_statutory_costs": true,
                        "t_plus_1_settlement": true
                    }
                }
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        let data = &structured["data"];
        assert_eq!(data["status"], "queued");
        assert_eq!(data["strategy_name"], "NIFTY_TREND_V1");
        assert_eq!(data["statutory_costs_enforced"], true);
        assert_eq!(data["settlement_timing"], "T+1");
        assert!(data["backtest_id"].is_string());
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_evaluate_metrics() {
    let server = McpServer::new();

    let trade_log = json!([
        { "timestamp": "2026-01-01T09:15:00Z", "instrument": "RELIANCE.NSE", "side": "BUY", "quantity": 100, "price": 2500.0, "pnl": 1500.0 },
        { "timestamp": "2026-01-02T09:15:00Z", "instrument": "RELIANCE.NSE", "side": "SELL", "quantity": 100, "price": 2515.0, "pnl": -500.0 },
        { "timestamp": "2026-01-03T09:15:00Z", "instrument": "TCS.NSE", "side": "BUY", "quantity": 50, "price": 3800.0, "pnl": 2200.0 },
        { "timestamp": "2026-01-04T09:15:00Z", "instrument": "INFY.NSE", "side": "BUY", "quantity": 75, "price": 1800.0, "pnl": 800.0 }
    ]);

    let msg = RpcMessage::Request {
        id: json!(7),
        method: "tools/call".to_string(),
        params: json!({
            "name": "evaluate_metrics",
            "arguments": {
                "trade_log": trade_log,
                "config": {
                    "risk_free_rate": 0.06,
                    "periods_per_year": 252,
                    "num_trials": 5
                }
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        let data = &structured["data"];
        assert_eq!(data["total_trades"], 4);
        assert_eq!(data["win_rate"], 75.0);
        assert!(data["sharpe"].as_f64().is_some());
        assert!(data["sortino"].as_f64().is_some());
        assert!(data["max_drawdown"].as_f64().is_some());
        assert!(data["dsr"].as_f64().is_some());
        assert!(data["pbo"].as_f64().is_some());
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_optimize_strategy() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(8),
        method: "tools/call".to_string(),
        params: json!({
            "name": "optimize_strategy",
            "arguments": {
                "strategy_manifest": { "name": "CPR_BREAKOUT" },
                "optimization_config": {
                    "algorithm": "genetic",
                    "trials": 50,
                    "seed": 99
                }
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        let data = &structured["data"];
        assert_eq!(data["status"], "started");
        assert_eq!(data["algorithm"], "genetic");
        assert_eq!(data["total_trials"], 50);
        assert!(
            data["job_id"]
                .as_str()
                .unwrap()
                .contains("sweep_genetic_50_99")
        );
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_explain_decision() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(9),
        method: "tools/call".to_string(),
        params: json!({
            "name": "explain_decision",
            "arguments": {
                "decision": {
                    "action": "BUY",
                    "symbol": "HDFCBANK.NSE",
                    "qty": 50.0,
                    "price_limit": 1650.0,
                    "algo_id": "SEBI_ALGO_HDFC_001"
                },
                "market_context": {
                    "price": 1648.5,
                    "timestamp": "2026-10-26T10:00:00Z"
                },
                "risk_check_results": [
                    { "rule": "PriceBandCheck", "passed": true },
                    { "rule": "MaxParticipationLimit", "passed": true },
                    { "rule": "DailyDrawdownLimit", "passed": true }
                ]
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        let data = &structured["data"];
        assert_eq!(data["sebi_algo_id"], "SEBI_ALGO_HDFC_001");
        assert_eq!(data["passed_pre_trade_risk"], true);
        assert!(
            data["explanation"]
                .as_str()
                .unwrap()
                .contains("HDFCBANK.NSE")
        );
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_forecast_series() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(10),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_forecast_series",
            "arguments": {
                "series": [100.0, 101.2, 100.7, 102.4, 103.1, 104.0],
                "horizon": 3,
                "model": "theta"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        assert_eq!(structured["data"]["model"]["kind"], "theta");
        assert_eq!(structured["data"]["horizon"], 3);
        assert_eq!(structured["data"]["point"]["steps"], json!([1, 2, 3]));
        assert_eq!(
            structured["data"]["point"]["values"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let interval = &structured["data"]["interval"];
        assert_eq!(interval["lower"].as_array().unwrap().len(), 3);
        assert_eq!(interval["upper"].as_array().unwrap().len(), 3);
        let blocks = structured["blocks"].as_array().unwrap();
        assert!(blocks.iter().any(|b| b["type"] == "metric_grid"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_forecast_series_rejects_bad_model() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(11),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_forecast_series",
            "arguments": {
                "series": [1.0, 2.0, 3.0],
                "model": "black_magic"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp {
        assert!(error.is_some(), "unknown model must error");
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_stdio_transport() {
    let server = Arc::new(McpServer::new());

    let input_lines = format!(
        "{}\n{}\n",
        serde_json::to_string(&json!({
            "id": 101,
            "method": "initialize",
            "params": {}
        }))
        .unwrap(),
        serde_json::to_string(&json!({
            "id": 102,
            "method": "ping",
            "params": {}
        }))
        .unwrap(),
    );

    let reader = std::io::Cursor::new(input_lines.into_bytes());
    let mut writer = Vec::new();

    run_stdio_transport(server, reader, &mut writer)
        .await
        .expect("stdio transport execution");

    let output_str = String::from_utf8(writer).expect("valid utf8");
    let lines: Vec<&str> = output_str.trim().split('\n').collect();
    assert_eq!(lines.len(), 2);

    let resp1: serde_json::Value = serde_json::from_str(lines[0]).expect("json 1");
    assert_eq!(resp1["id"], 101);
    assert_eq!(resp1["result"]["serverInfo"]["name"], "vyapaar-agent");

    let resp2: serde_json::Value = serde_json::from_str(lines[1]).expect("json 2");
    assert_eq!(resp2["id"], 102);
}

#[tokio::test]
async fn test_sse_and_http_transport() {
    let server = Arc::new(McpServer::new());
    let router = create_router(server);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();

    // 1. Test /health
    let health_resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("health request");
    assert_eq!(health_resp.status(), reqwest::StatusCode::OK);

    // 2. Test /rpc JSON-RPC call
    let rpc_resp = client
        .post(format!("http://{addr}/rpc"))
        .json(&json!({
            "id": "http-1",
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .expect("rpc request");

    assert_eq!(rpc_resp.status(), reqwest::StatusCode::OK);
    let rpc_body: serde_json::Value = rpc_resp.json().await.expect("json body");
    assert_eq!(rpc_body["id"], "http-1");
    let tools = rpc_body["result"]["tools"].as_array().expect("tools array");
    assert!(tools.len() >= 6);

    // 3. Test /sse endpoint returns endpoint event
    let sse_resp = client
        .get(format!("http://{addr}/sse"))
        .send()
        .await
        .expect("sse request");
    assert_eq!(sse_resp.status(), reqwest::StatusCode::OK);
    let content_type = sse_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));

    server_handle.abort();
}

#[tokio::test]
async fn test_frontier_manifest_generation() {
    let tmp_dir = std::env::temp_dir().join(format!("mcp_manifests_{}", uuid::Uuid::new_v4()));
    let paths = ManifestGenerator::write_all(
        &tmp_dir,
        "/usr/local/bin/vyapaar-agent-mcp",
        &["--transport", "stdio"],
        Some("http://127.0.0.1:8000/sse"),
    )
    .expect("write manifests");

    assert!(paths.claude_desktop_path.exists());
    assert!(paths.cursor_path.exists());
    assert!(paths.gemini_path.exists());

    // Verify Claude Desktop format
    let claude_str = fs::read_to_string(&paths.claude_desktop_path).unwrap();
    let claude_val: serde_json::Value = serde_json::from_str(&claude_str).unwrap();
    assert_eq!(
        claude_val["mcpServers"]["vyapaar-agent"]["command"],
        "/usr/local/bin/vyapaar-agent-mcp"
    );

    // Verify Cursor format
    let cursor_str = fs::read_to_string(&paths.cursor_path).unwrap();
    let cursor_val: serde_json::Value = serde_json::from_str(&cursor_str).unwrap();
    assert_eq!(
        cursor_val["mcpServers"]["vyapaar-agent"]["url"],
        "http://127.0.0.1:8000/sse"
    );

    // Verify Gemini CLI format
    let gemini_str = fs::read_to_string(&paths.gemini_path).unwrap();
    let gemini_val: serde_json::Value = serde_json::from_str(&gemini_str).unwrap();
    assert_eq!(
        gemini_val["mcp_servers"]["vyapaar-agent"]["command"],
        "/usr/local/bin/vyapaar-agent-mcp"
    );

    let _ = fs::remove_dir_all(tmp_dir);
}

#[test]
fn test_golden_mcp_tools_schema_conformance() {
    // Verify all quantitative tool definitions in vyapaar-agent-mcp conform to MCP tool schema
    let quantitative_tools = list_tool_definitions();
    assert!(quantitative_tools.len() >= 6);

    for tool in &quantitative_tools {
        let name = tool["name"].as_str().expect("name");
        let desc = tool["description"].as_str().expect("description");
        assert!(!name.is_empty());
        assert!(!desc.is_empty());
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
}

// ---------------------------------------------------------------------------
// Gateway Discovery Protocol tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gateway_search_tools() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(100),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_search_tools",
            "arguments": {
                "query": "backtest",
                "category": "all"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none(), "expected no error, got: {:?}", error);
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert_eq!(data["query"], "backtest");
        assert_eq!(data["category"], "all");
        assert!(data["count"].as_u64().unwrap() >= 1);
        let tools = data["tools"].as_array().expect("tools array");
        assert!(
            tools
                .iter()
                .any(|t| t["name"].as_str().unwrap().contains("backtest"))
        );
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_gateway_search_tools_filters_by_category() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(101),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_search_tools",
            "arguments": {
                "query": "forecast",
                "category": "forecast"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let data = &res["structuredContent"]["data"];
        assert_eq!(data["category"], "forecast");
        let tools = data["tools"].as_array().expect("tools array");
        assert!(
            tools
                .iter()
                .any(|t| t["category"].as_str() == Some("forecast"))
        );
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_gateway_describe_tool() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(102),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_describe_tool",
            "arguments": {
                "tool_name": "vyapaar_forecast_series"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert_eq!(data["tool_name"], "vyapaar_forecast_series");
        assert!(data["schema"]["inputSchema"].is_object());
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_gateway_describe_tool_unknown() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(103),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_describe_tool",
            "arguments": {
                "tool_name": "nonexistent_tool"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp {
        assert!(error.is_some());
    } else {
        panic!("expected error for unknown tool");
    }
}

#[tokio::test]
async fn test_gateway_read_result() {
    let server = McpServer::new();

    // First export a reference via the data store directly
    let uri = server.data_store.export(
        "ohlcv_test",
        serde_json::json!({"instrument": "NSE:RELIANCE-EQ", "close": "2500.00"}),
    );

    let msg = RpcMessage::Request {
        id: json!(104),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_read_result",
            "arguments": {
                "ref_id": uri,
                "offset": 0,
                "limit": 10
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert_eq!(data["ref_id"], uri);
        assert_eq!(data["data"]["instrument"], "NSE:RELIANCE-EQ");
        assert_eq!(data["data"]["close"], "2500.00");
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_gateway_read_result_unknown_ref() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(105),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_read_result",
            "arguments": {
                "ref_id": "$vyapaar/ref/does-not-exist"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp {
        assert!(error.is_some());
    } else {
        panic!("expected error for unknown reference");
    }
}

// ---------------------------------------------------------------------------
// Plan 0007 Track 1 Phase 1 Analytics Engine MCP Tools
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_tool_monte_carlo() {
    let server = McpServer::new();
    let returns = vec![
        0.01, -0.005, 0.02, -0.01, 0.015, -0.008, 0.012, 0.003, -0.004, 0.009, 0.014, -0.002,
        0.008, -0.015, 0.022, 0.005, -0.006, 0.011, 0.004, -0.003,
    ];
    let msg = RpcMessage::Request {
        id: json!(201),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_monte_carlo",
            "arguments": {
                "returns": returns,
                "num_sims": 150,
                "horizon": 25
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none(), "unexpected error: {:?}", error);
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert_eq!(data["simulations_run"], 150);
        assert!(data["median_max_drawdown"].as_f64().unwrap() >= 0.0);
        assert!(data["drawdown_95"].as_f64().unwrap() >= 0.0);
        assert!(data["prob_loss"].as_f64().unwrap() >= 0.0);

        // Check MetricGrid
        let blocks = structured["blocks"].as_array().expect("blocks");
        assert!(blocks.iter().any(|b| b["type"] == "metric_grid"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_cointegration() {
    let server = McpServer::new();
    let n = 80;
    let mut series_x = Vec::with_capacity(n);
    let mut series_y = Vec::with_capacity(n);
    let mut x = 100.0;
    for i in 0..n {
        let step = ((i as f64 * 0.17).sin() * 2.0).round();
        x += step;
        let noise = (i as f64 * 0.73).cos() * 0.5;
        let y = 1.5 * x + 20.0 + noise;
        series_x.push(x);
        series_y.push(y);
    }

    let msg = RpcMessage::Request {
        id: json!(202),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_cointegration",
            "arguments": {
                "series_x": series_x,
                "series_y": series_y
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none(), "unexpected error: {:?}", error);
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert!((data["hedge_ratio"].as_f64().unwrap() - 1.5).abs() < 0.15);
        assert!(data["is_cointegrated"].as_bool().unwrap());
        assert!(data["adf_stat"].as_f64().unwrap() < 0.0);

        // Check MetricGrid
        let blocks = structured["blocks"].as_array().expect("blocks");
        assert!(blocks.iter().any(|b| b["type"] == "metric_grid"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_walk_forward() {
    let server = McpServer::new();
    let mut returns = Vec::with_capacity(60);
    for i in 0..60 {
        returns.push(if i % 2 == 0 { 0.02 } else { -0.005 });
    }

    let msg = RpcMessage::Request {
        id: json!(203),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_walk_forward",
            "arguments": {
                "returns": returns,
                "train_size": 36,
                "test_size": 24
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none(), "unexpected error: {:?}", error);
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert!(data["is_annual_return"].as_f64().unwrap() > 0.0);
        assert!(data["oos_annual_return"].as_f64().unwrap() > 0.0);
        assert!(data["wfe"].as_f64().unwrap() > 0.0);

        // Check MetricGrid
        let blocks = structured["blocks"].as_array().expect("blocks");
        assert!(blocks.iter().any(|b| b["type"] == "metric_grid"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_factor_attribution_regression() {
    let server = McpServer::new();
    let n = 40;
    let mut mkt = Vec::with_capacity(n);
    let mut smb = Vec::with_capacity(n);
    let mut asset = Vec::with_capacity(n);

    for i in 0..n {
        let f1 = (i as f64 * 0.2).sin() * 0.02;
        let f2 = (i as f64 * 0.5).cos() * 0.01;
        let r = 0.001 + 1.2 * f1 + 0.5 * f2 + ((i as f64).sin() * 0.001);
        mkt.push(f1);
        smb.push(f2);
        asset.push(r);
    }

    let msg = RpcMessage::Request {
        id: json!(204),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_factor_attribution",
            "arguments": {
                "asset_returns": asset,
                "factor_returns": {
                    "market": mkt,
                    "size": smb
                },
                "annualization_factor": 252.0
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none(), "unexpected error: {:?}", error);
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert_eq!(data["n_factors"], 2);
        assert_eq!(data["n_periods"], n);

        // Check MetricGrid and Table blocks
        let blocks = structured["blocks"].as_array().expect("blocks");
        assert!(blocks.iter().any(|b| b["type"] == "metric_grid"));
        assert!(blocks.iter().any(|b| b["type"] == "table"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_gateway_search_tools_analytics_category() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(205),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_search_tools",
            "arguments": {
                "query": "monte",
                "category": "analytics"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let data = &res["structuredContent"]["data"];
        assert_eq!(data["category"], "analytics");
        let tools = data["tools"].as_array().expect("tools array");
        assert!(tools.iter().any(|t| t["name"] == "vyapaar_monte_carlo"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_gateway_describe_tool_analytics() {
    let server = McpServer::new();
    for tool_name in &[
        "vyapaar_monte_carlo",
        "vyapaar_cointegration",
        "vyapaar_walk_forward",
        "vyapaar_factor_attribution",
    ] {
        let msg = RpcMessage::Request {
            id: json!(206),
            method: "tools/call".to_string(),
            params: json!({
                "name": "vyapaar_describe_tool",
                "arguments": {
                    "tool_name": tool_name
                }
            }),
        };

        let resp = server.handle_message(&msg).await.expect("response");
        if let RpcMessage::Response { result, error, .. } = resp {
            assert!(error.is_none());
            let res = result.unwrap();
            let data = &res["structuredContent"]["data"];
            assert_eq!(data["tool_name"], *tool_name);
            assert!(data["schema"]["inputSchema"].is_object());
        } else {
            panic!("unexpected response type");
        }
    }
}

// ---------------------------------------------------------------------------
// Plan 0007 Track 1 Phase 2: Portfolio Optimization & Allocation Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_tool_portfolio_optimize_mean_variance() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(301),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_portfolio_optimize",
            "arguments": {
                "symbols": ["ASSET_A", "ASSET_B"],
                "expected_returns": [0.10, 0.12],
                "covariance_matrix": [
                    [1.0, 0.5],
                    [0.5, 1.0]
                ],
                "objective": "mean_variance",
                "risk_free_rate": 0.0
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none(), "unexpected error: {:?}", error);
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["schema"], "vyapaar/result/v1");
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert_eq!(data["objective"], "mean_variance");

        let weights = data["weights"].as_array().expect("weights array");
        assert_eq!(weights.len(), 2);

        let w_a = weights[0]["weight"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let w_b = weights[1]["weight"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((w_a + w_b - 1.0).abs() < 1e-4);
        assert!(w_a > 0.0 && w_b > 0.0);
        // Asset B has higher expected return with identical variance, so w_b > w_a
        assert!(w_b > w_a);

        let exp_ret = data["expected_return"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!(exp_ret > 0.10 && exp_ret < 0.12);

        let vol = data["volatility"].as_str().unwrap().parse::<f64>().unwrap();
        assert!(vol > 0.0);

        let sharpe = data["sharpe_ratio"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!(sharpe > 0.0);

        let eff = data["effective_assets"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((1.0..=2.0).contains(&eff));

        // Check DisplayBlocks
        let blocks = structured["blocks"].as_array().expect("blocks");
        assert!(blocks.iter().any(|b| b["type"] == "metric_grid"));
        assert!(blocks.iter().any(|b| b["type"] == "table"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_portfolio_optimize_risk_parity() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(302),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_portfolio_optimize",
            "arguments": {
                "symbols": ["LOW_VOL", "HIGH_VOL"],
                "covariance_matrix": [
                    [0.04, 0.0],
                    [0.0, 0.16]
                ],
                "objective": "risk_parity"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none(), "unexpected error: {:?}", error);
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["status"], "ok");
        let data = &structured["data"];
        assert_eq!(data["objective"], "risk_parity");

        let weights = data["weights"].as_array().expect("weights array");
        assert_eq!(weights.len(), 2);

        let w_low = weights[0]["weight"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let w_high = weights[1]["weight"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((w_low + w_high - 1.0).abs() < 1e-4);
        // Inverse volatility on diag(0.04, 0.16) -> vol 0.2 vs 0.4 -> 2/3 vs 1/3
        assert!((w_low - 2.0 / 3.0).abs() < 0.02);
        assert!((w_high - 1.0 / 3.0).abs() < 0.02);

        let vol = data["volatility"].as_str().unwrap().parse::<f64>().unwrap();
        assert!(vol > 0.0);
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_portfolio_optimize_weight_bounds() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(303),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_portfolio_optimize",
            "arguments": {
                "symbols": ["ASSET_A", "ASSET_B"],
                "covariance_matrix": [
                    [0.04, 0.0],
                    [0.0, 0.16]
                ],
                "objective": "risk_parity",
                "weight_bounds": [
                    { "instrument": "ASSET_A", "min": 0.0, "max": 0.50 }
                ]
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none(), "unexpected error: {:?}", error);
        let res = result.unwrap();
        let data = &res["structuredContent"]["data"];
        let weights = data["weights"].as_array().expect("weights array");
        let w_a = weights[0]["weight"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        // Asset A unconstrained would be ~0.667, but clamped to <= 0.50
        assert!(w_a <= 0.50001);
        let w_b = weights[1]["weight"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((w_a + w_b - 1.0).abs() < 1e-4);
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_portfolio_optimize_hrp_unimplemented() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(304),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_portfolio_optimize",
            "arguments": {
                "symbols": ["ASSET_A", "ASSET_B"],
                "covariance_matrix": [
                    [0.04, 0.0],
                    [0.0, 0.16]
                ],
                "objective": "hrp"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp {
        assert!(error.is_some(), "expected error for hrp");
        let err = error.unwrap();
        assert!(err.message.contains("not implemented") || err.message.contains("Phase 2"));
    } else {
        panic!("expected error response for unimplemented hrp");
    }
}

#[tokio::test]
async fn test_tool_portfolio_optimize_validation_errors() {
    let server = McpServer::new();

    // 1. Missing covariance matrix
    let msg1 = RpcMessage::Request {
        id: json!(305),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_portfolio_optimize",
            "arguments": {
                "objective": "mean_variance"
            }
        }),
    };
    let resp1 = server.handle_message(&msg1).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp1 {
        assert!(error.is_some());
    } else {
        panic!("expected error");
    }

    // 2. Missing expected_returns for mean_variance
    let msg2 = RpcMessage::Request {
        id: json!(306),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_portfolio_optimize",
            "arguments": {
                "covariance_matrix": [[1.0, 0.0], [0.0, 1.0]],
                "objective": "mean_variance"
            }
        }),
    };
    let resp2 = server.handle_message(&msg2).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp2 {
        assert!(error.is_some());
    } else {
        panic!("expected error");
    }

    // 3. Dimension mismatch between matrix and expected_returns
    let msg3 = RpcMessage::Request {
        id: json!(307),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_portfolio_optimize",
            "arguments": {
                "expected_returns": [0.10],
                "covariance_matrix": [[1.0, 0.0], [0.0, 1.0]],
                "objective": "mean_variance"
            }
        }),
    };
    let resp3 = server.handle_message(&msg3).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp3 {
        assert!(error.is_some());
    } else {
        panic!("expected error");
    }
}

#[tokio::test]
async fn test_gateway_search_tools_allocation_category() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(308),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_search_tools",
            "arguments": {
                "query": "optimize",
                "category": "allocation"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let data = &res["structuredContent"]["data"];
        assert_eq!(data["category"], "allocation");
        let tools = data["tools"].as_array().expect("tools array");
        assert!(
            tools
                .iter()
                .any(|t| t["name"] == "vyapaar_portfolio_optimize")
        );
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_gateway_describe_tool_allocation() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(309),
        method: "tools/call".to_string(),
        params: json!({
            "name": "vyapaar_describe_tool",
            "arguments": {
                "tool_name": "vyapaar_portfolio_optimize"
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let data = &res["structuredContent"]["data"];
        assert_eq!(data["tool_name"], "vyapaar_portfolio_optimize");
        assert_eq!(data["schema"]["category"], "allocation");
        assert!(data["schema"]["inputSchema"].is_object());
    } else {
        panic!("unexpected response type");
    }
}
