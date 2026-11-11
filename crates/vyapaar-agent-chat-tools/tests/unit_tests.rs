use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;
use vyapaar_agent_chat::event::{CallId, TurnId};
use vyapaar_agent_chat::permission::ToolRisk;
use vyapaar_agent_chat::tool::{ChatTool, ToolContext, ToolRegistry};
use vyapaar_agent_chat_tools::calc::{
    CagrArgs, CalcOperation, MarketSegment, PositionSizingArgs, ResearchCalcInput,
    ResearchCalcTool, ReturnMetricsArgs, StatutoryCostsArgs, TradeSide,
};
use vyapaar_agent_chat_tools::catalog::register_read_only_tools;
use vyapaar_agent_chat_tools::ports::alert::{AlertReader, AlertStatus, AlertWriter, PriceAlert};
use vyapaar_agent_chat_tools::ports::content::{ContentReader, FilingRecord, NewsArticle};
use vyapaar_agent_chat_tools::ports::error::PortError;
use vyapaar_agent_chat_tools::ports::market::{
    BlockDeal, InstrumentSearchResult, MacroSeries, MarketBar, MarketHistory, MarketIndicators,
    MarketQuote, MarketReader,
};
use vyapaar_agent_chat_tools::ports::portfolio::{
    AccountSummary, DateRange, OrderFilter, PortfolioReader,
};
use vyapaar_agent_chat_tools::ports::snapshot::{SnapshotData, SnapshotNeeds, SnapshotSource};
use vyapaar_agent_chat_tools::{create_alert_tools, register_alert_tools};
use vyapaar_agent_core::ToolError;
use vyapaar_core::{
    AccountId, AlgoId, ClientOrderId, Exchange, InstrumentId, Money, OrderSide, OrderStatus,
    OrderType, PositionSide, Price, PriceBand, Quantity, TimeInForce,
};
use vyapaar_ledger::repository::OrderRecord;
use vyapaar_ledger::{PositionRecord, TaxLot};

// ——————————————————————————————————————————————————————————————————————————
// Mocks for Testing Ports
// ——————————————————————————————————————————————————————————————————————————

struct MockMarketReader {
    fail_quotes: bool,
}

#[async_trait]
impl MarketReader for MockMarketReader {
    async fn quote(&self, instrument: &InstrumentId) -> Result<MarketQuote, PortError> {
        if self.fail_quotes {
            return Err(PortError::unavailable("market feed offline"));
        }
        Ok(MarketQuote {
            instrument: instrument.clone(),
            last_price: Price(dec!(2500.0)),
            bid: Some(Price(dec!(2499.5))),
            ask: Some(Price(dec!(2500.5))),
            volume: Quantity(dec!(100000.0)),
            as_of: Utc::now(),
        })
    }

    async fn history(
        &self,
        instrument: &InstrumentId,
        _range: DateRange,
        limit: Option<usize>,
    ) -> Result<MarketHistory, PortError> {
        let count = limit.unwrap_or(10);
        let mut bars = Vec::new();
        for i in 0..count {
            bars.push(MarketBar {
                timestamp: Utc::now() - chrono::Duration::minutes((count - i) as i64),
                open: Price(dec!(2490.0)),
                high: Price(dec!(2510.0)),
                low: Price(dec!(2485.0)),
                close: Price(dec!(2500.0)),
                volume: Quantity(dec!(1000.0)),
            });
        }
        Ok(MarketHistory {
            instrument: instrument.clone(),
            bars,
            downsampled: false,
        })
    }

    async fn indicators(
        &self,
        instrument: &InstrumentId,
        names: &[String],
    ) -> Result<MarketIndicators, PortError> {
        let mut values = HashMap::new();
        for name in names {
            if name == "rsi_14" {
                values.insert(name.clone(), dec!(62.45));
            } else if name == "sma_20" {
                values.insert(name.clone(), dec!(2480.0));
            } else {
                values.insert(name.clone(), dec!(100.0));
            }
        }
        Ok(MarketIndicators {
            instrument: instrument.clone(),
            values,
            as_of: Utc::now(),
        })
    }

    async fn search_instruments(
        &self,
        query: &str,
    ) -> Result<Vec<InstrumentSearchResult>, PortError> {
        if query.to_uppercase().contains("RELIANCE") {
            Ok(vec![InstrumentSearchResult {
                instrument: InstrumentId::new(Exchange::Nse, "RELIANCE"),
                name: "Reliance Industries Limited".to_string(),
                exchange: "NSE".to_string(),
                lot_size: Some(1),
            }])
        } else {
            Ok(vec![])
        }
    }

    async fn macro_series(&self, series: &str) -> Result<MacroSeries, PortError> {
        Ok(MacroSeries {
            series: series.to_string(),
            points: vec![
                (Utc::now() - chrono::Duration::days(30), dec!(5.4)),
                (Utc::now(), dec!(5.1)),
            ],
        })
    }

    async fn block_deals(
        &self,
        instrument: Option<&InstrumentId>,
        date: Option<DateTime<Utc>>,
    ) -> Result<Vec<BlockDeal>, PortError> {
        if self.fail_quotes {
            return Err(PortError::unavailable(
                "block/bulk deal disclosures are not yet ingested",
            ));
        }
        let inst = instrument
            .cloned()
            .unwrap_or_else(|| InstrumentId::new(Exchange::Nse, "RELIANCE"));
        Ok(vec![BlockDeal {
            instrument: inst,
            exchange: "NSE".to_string(),
            client_name: "MORGAN STANLEY ASIA SINGAPORE PTE".to_string(),
            side: "buy".to_string(),
            category: "block".to_string(),
            quantity: Quantity(dec!(500000.0)),
            price: Price(dec!(2500.0)),
            trade_date: date.unwrap_or_else(Utc::now),
        }])
    }

    async fn price_band(&self, _instrument: &InstrumentId) -> Result<PriceBand, PortError> {
        Ok(PriceBand::new(
            Price(dec!(2250.0)),
            Price(dec!(2750.0)),
            Price(dec!(2500.0)),
        ))
    }
}

struct MockContentReader {
    fail_content: bool,
}

#[async_trait]
impl ContentReader for MockContentReader {
    async fn news_search(
        &self,
        query: Option<&str>,
        instrument: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<NewsArticle>, PortError> {
        if self.fail_content {
            return Err(PortError::unavailable("news content feed offline"));
        }
        let articles = vec![NewsArticle {
            id: "news_001".to_string(),
            title: "Reliance quarterly results beat estimates".to_string(),
            source: "Economic Times".to_string(),
            url: Some("https://example.com/news_001".to_string()),
            published_at: Utc::now(),
            summary: Some("Q2 results above street expectations".to_string()),
            instruments: vec!["NSE:RELIANCE".to_string()],
        }];
        let take = limit.unwrap_or(articles.len());
        let articles: Vec<NewsArticle> = articles
            .into_iter()
            .filter(|a| query.is_none_or(|q| a.title.to_lowercase().contains(&q.to_lowercase())))
            .filter(|a| {
                instrument
                    .is_none_or(|i| a.instruments.iter().any(|tag| tag.eq_ignore_ascii_case(i)))
            })
            .take(take)
            .collect();
        Ok(articles)
    }

    async fn filings(
        &self,
        instrument: Option<&InstrumentId>,
        filing_type: Option<&str>,
        _limit: Option<usize>,
    ) -> Result<Vec<FilingRecord>, PortError> {
        if self.fail_content {
            return Err(PortError::unavailable("filings source offline"));
        }
        let inst = instrument
            .cloned()
            .unwrap_or_else(|| InstrumentId::new(Exchange::Nse, "RELIANCE"));
        let filings = vec![FilingRecord {
            instrument: inst,
            filing_date: Utc::now(),
            filing_type: "quarterly_result".to_string(),
            title: "Quarterly results filing".to_string(),
            source_url: Some("https://example.com/filing".to_string()),
        }];
        let filings: Vec<FilingRecord> = filings
            .into_iter()
            .filter(|f| filing_type.is_none_or(|t| f.filing_type == t))
            .collect();
        Ok(filings)
    }
}

struct MockPortfolioReader;

#[async_trait]
impl PortfolioReader for MockPortfolioReader {
    async fn accounts(&self) -> Result<Vec<AccountSummary>, PortError> {
        Ok(vec![AccountSummary {
            account_id: AccountId::new("ACC-01"),
            algo_id: Some(AlgoId::new("ALPHA-1")),
            cash: Money::inr(dec!(500000.0)),
            status: "active".to_string(),
            updated_at: Utc::now(),
        }])
    }

    async fn positions(&self, account: &AccountId) -> Result<Vec<PositionRecord>, PortError> {
        if account.as_str() != "ACC-01" {
            return Ok(vec![]);
        }
        Ok(vec![PositionRecord {
            account_id: account.clone(),
            instrument: InstrumentId::new(Exchange::Nse, "RELIANCE"),
            side: PositionSide::Long,
            quantity: Quantity(dec!(100.0)),
            avg_price: Price(dec!(2400.0)),
            realized_pnl: Money::inr(dec!(5000.0)),
            unrealized_pnl: Money::inr(dec!(10000.0)),
            lots: vec![TaxLot {
                lot_id: "LOT-01".to_string(),
                account_id: account.clone(),
                instrument: InstrumentId::new(Exchange::Nse, "RELIANCE"),
                side: PositionSide::Long,
                entry_price: Price(dec!(2400.0)),
                quantity: Quantity(dec!(100.0)),
                remaining_quantity: Quantity(dec!(100.0)),
                entry_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                created_at: Utc::now(),
            }],
            updated_at: Utc::now(),
        }])
    }

    async fn tax_lots(
        &self,
        account: &AccountId,
        _instrument: &InstrumentId,
    ) -> Result<Vec<TaxLot>, PortError> {
        Ok(vec![TaxLot {
            lot_id: "LOT-01".to_string(),
            account_id: account.clone(),
            instrument: InstrumentId::new(Exchange::Nse, "RELIANCE"),
            side: PositionSide::Long,
            entry_price: Price(dec!(2400.0)),
            quantity: Quantity(dec!(100.0)),
            remaining_quantity: Quantity(dec!(100.0)),
            entry_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            created_at: Utc::now(),
        }])
    }

    async fn cash(&self, _account: &AccountId) -> Result<Vec<Money>, PortError> {
        Ok(vec![Money::inr(dec!(500000.0))])
    }

    async fn orders(
        &self,
        account: &AccountId,
        _filter: OrderFilter,
    ) -> Result<Vec<OrderRecord>, PortError> {
        Ok(vec![OrderRecord {
            client_order_id: ClientOrderId::new("ORD-101"),
            broker_order_id: Some("BROKER-101".to_string()),
            algo_id: Some(AlgoId::new("ALPHA-1")),
            account_id: account.clone(),
            instrument: InstrumentId::new(Exchange::Nse, "RELIANCE"),
            side: OrderSide::Buy,
            order_type: OrderType::Limit(Price(dec!(2400.0))),
            time_in_force: TimeInForce::Day,
            price: Some(Price(dec!(2400.0))),
            quantity: Quantity(dec!(100.0)),
            filled_quantity: Quantity(dec!(100.0)),
            avg_fill_price: Some(Price(dec!(2400.0))),
            status: OrderStatus::Filled,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }])
    }

    async fn realized_pnl(
        &self,
        _account: &AccountId,
        _range: DateRange,
    ) -> Result<Money, PortError> {
        Ok(Money::inr(dec!(25000.0)))
    }
}

struct MockSnapshotSource;

#[async_trait]
impl SnapshotSource for MockSnapshotSource {
    async fn snapshot(&self, _needs: &SnapshotNeeds) -> Result<SnapshotData, PortError> {
        Ok(SnapshotData {
            as_of: Utc::now(),
            universe: Some(vec!["RELIANCE".to_string(), "TCS".to_string()]),
            regime: Some("trending".to_string()),
            risk_metrics: HashMap::new(),
        })
    }
}

/// In-memory alert store implementing both alert port traits over one shared `Vec` — the same
/// contract the runtime's profile-backed store implements (duplicate ids rejected; a
/// `Triggered` alert is never flipped by enable/disable).
struct MockAlertStore {
    alerts: std::sync::Mutex<Vec<PriceAlert>>,
}

impl MockAlertStore {
    fn new() -> Self {
        Self {
            alerts: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl AlertReader for MockAlertStore {
    async fn list_alerts(&self, workspace: Option<&str>) -> Result<Vec<PriceAlert>, PortError> {
        let all = self.alerts.lock().unwrap().clone();
        Ok(match workspace {
            Some(w) => all.into_iter().filter(|a| a.workspace_id == w).collect(),
            None => all,
        })
    }
}

#[async_trait]
impl AlertWriter for MockAlertStore {
    async fn add_alert(&self, alert: PriceAlert) -> Result<PriceAlert, PortError> {
        let mut alerts = self.alerts.lock().unwrap();
        if alerts.iter().any(|a| a.id == alert.id) {
            return Err(PortError::invalid_argument(format!(
                "an alert with id `{}` already exists",
                alert.id
            )));
        }
        alerts.push(alert.clone());
        Ok(alert)
    }

    async fn set_alert_status(
        &self,
        id: &str,
        status: AlertStatus,
    ) -> Result<Option<PriceAlert>, PortError> {
        let mut alerts = self.alerts.lock().unwrap();
        Ok(match alerts.iter_mut().find(|a| a.id == id) {
            Some(alert) => {
                if alert.status != AlertStatus::Triggered {
                    alert.status = status;
                }
                Some(alert.clone())
            }
            None => None,
        })
    }

    async fn delete_alert(&self, id: &str) -> Result<Option<PriceAlert>, PortError> {
        let mut alerts = self.alerts.lock().unwrap();
        let removed = alerts
            .iter()
            .position(|a| a.id == id)
            .map(|i| alerts.remove(i));
        Ok(removed)
    }
}

fn dummy_tool_context() -> ToolContext {
    let (tx, _) = mpsc::channel(16);
    ToolContext::new(None, TurnId(1), CallId::new("call-1"), tx)
}

fn parse_dec(val: &serde_json::Value) -> f64 {
    if let Some(s) = val.as_str() {
        s.parse::<f64>().unwrap_or(0.0)
    } else {
        val.as_f64().unwrap_or(0.0)
    }
}

// ——————————————————————————————————————————————————————————————————————————
// Unit Tests
// ——————————————————————————————————————————————————————————————————————————

#[tokio::test]
async fn test_research_calc_cagr() {
    let tool = ResearchCalcTool::new();
    let ctx = dummy_tool_context();

    let input = serde_json::to_value(ResearchCalcInput {
        operation: CalcOperation::Cagr,
        cagr: Some(CagrArgs {
            start_value: dec!(100000.0),
            end_value: dec!(200000.0),
            years: dec!(3.0),
        }),
        position_sizing: None,
        statutory_costs: None,
        return_metrics: None,
    })
    .unwrap();

    let out = tool.execute(input, ctx).await.unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["operation"], "cagr");
    let cagr = parse_dec(&out.payload["cagr_percent"]);
    // (2)^(1/3) - 1 ≈ 25.9921%
    assert!((cagr - 25.9921).abs() < 0.01);
}

#[tokio::test]
async fn test_research_calc_position_sizing() {
    let tool = ResearchCalcTool::new();
    let ctx = dummy_tool_context();

    let input = serde_json::to_value(ResearchCalcInput {
        operation: CalcOperation::PositionSizing,
        cagr: None,
        position_sizing: Some(PositionSizingArgs {
            capital: dec!(500000.0),
            risk_percent: dec!(1.0), // 1%
            entry_price: dec!(2500.0),
            stop_loss: dec!(2450.0), // Rs 50 risk per share
            lot_size: Some(10),
        }),
        statutory_costs: None,
        return_metrics: None,
    })
    .unwrap();

    let out = tool.execute(input, ctx).await.unwrap();
    assert!(out.ok);
    assert_eq!(parse_dec(&out.payload["risk_capital"]), 5000.0);
    assert_eq!(parse_dec(&out.payload["per_share_risk"]), 50.0);
    // 5000 / 50 = 100 shares (multiple of 10 lot size)
    assert_eq!(out.payload["quantity"], 100);
}

#[tokio::test]
async fn test_research_calc_statutory_costs() {
    let tool = ResearchCalcTool::new();
    let ctx = dummy_tool_context();

    let input = serde_json::to_value(ResearchCalcInput {
        operation: CalcOperation::StatutoryCosts,
        cagr: None,
        position_sizing: None,
        statutory_costs: Some(StatutoryCostsArgs {
            segment: MarketSegment::Delivery,
            side: TradeSide::Buy,
            notional: dec!(100000.0),
            brokerage: Some(Decimal::ZERO),
        }),
        return_metrics: None,
    })
    .unwrap();

    let out = tool.execute(input, ctx).await.unwrap();
    assert!(out.ok);
    assert_eq!(parse_dec(&out.payload["charges"]["stt"]), 100.0); // 0.1%
    assert_eq!(parse_dec(&out.payload["charges"]["stamp_duty"]), 15.0); // 0.015%
    let total = parse_dec(&out.payload["total_cost"]);
    assert!(total > 118.0 && total < 125.0);
}

#[tokio::test]
async fn test_research_calc_return_metrics() {
    let tool = ResearchCalcTool::new();
    let ctx = dummy_tool_context();

    let input = serde_json::to_value(ResearchCalcInput {
        operation: CalcOperation::ReturnMetrics,
        cagr: None,
        position_sizing: None,
        statutory_costs: None,
        return_metrics: Some(ReturnMetricsArgs {
            cost_basis: dec!(100000.0),
            current_value: dec!(120000.0),
            cash_flow: Some(dec!(5000.0)),
            days_held: Some(365),
        }),
    })
    .unwrap();

    let out = tool.execute(input, ctx).await.unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["operation"], "return_metrics");
    assert_eq!(parse_dec(&out.payload["capital_gain"]), 20000.0);
    assert_eq!(parse_dec(&out.payload["total_profit"]), 25000.0);
    assert_eq!(parse_dec(&out.payload["return_percent"]), 25.0);
}

#[tokio::test]
async fn test_market_tools() {
    let market = Arc::new(MockMarketReader { fail_quotes: false });
    let mut registry = ToolRegistry::new();
    let portfolio = Arc::new(MockPortfolioReader);
    let snapshot = Arc::new(MockSnapshotSource);

    register_read_only_tools(
        &mut registry,
        portfolio,
        market,
        Arc::new(MockContentReader {
            fail_content: false,
        }),
        Some(snapshot),
    );

    // 1. market_quote
    let quote_tool = registry
        .get("market_quote")
        .expect("market_quote registered");
    let out = quote_tool
        .execute(
            serde_json::json!({ "instrument": "NSE:RELIANCE" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(parse_dec(&out.payload["last_price"]), 2500.0);

    // 2. market_history
    let hist_tool = registry
        .get("market_history")
        .expect("market_history registered");
    let out = hist_tool
        .execute(
            serde_json::json!({ "instrument": "NSE:RELIANCE", "limit": 5 }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 5);

    // 3. market_indicators
    let ind_tool = registry
        .get("market_indicators")
        .expect("market_indicators registered");
    let out = ind_tool
        .execute(
            serde_json::json!({ "instrument": "NSE:RELIANCE", "indicators": ["rsi_14", "sma_20"] }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["indicators"]["rsi_14"], "62.45");

    // 4. market_search
    let search_tool = registry
        .get("market_search")
        .expect("market_search registered");
    let out = search_tool
        .execute(
            serde_json::json!({ "query": "RELIANCE" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 1);

    // 5. macro_series
    let macro_tool = registry
        .get("macro_series")
        .expect("macro_series registered");
    let out = macro_tool
        .execute(serde_json::json!({ "series": "cpi" }), dummy_tool_context())
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["series"], "cpi");
}

#[tokio::test]
async fn test_block_deals_tools() {
    let market = Arc::new(MockMarketReader { fail_quotes: false });
    let mut registry = ToolRegistry::new();
    register_read_only_tools(
        &mut registry,
        Arc::new(MockPortfolioReader),
        market,
        Arc::new(MockContentReader {
            fail_content: false,
        }),
        None,
    );

    // 1. block_deals — success, no filters (defaults to latest trading day).
    let tool = registry.get("block_deals").expect("block_deals registered");
    assert_eq!(tool.traits().risk, ToolRisk::ReadOnly);
    let out = tool
        .execute(serde_json::json!({}), dummy_tool_context())
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 1);
    assert_eq!(out.payload["deals"][0]["instrument"], "NSE:RELIANCE");
    assert_eq!(out.payload["deals"][0]["side"], "buy");
    assert_eq!(out.payload["deals"][0]["category"], "block");
    assert_eq!(parse_dec(&out.payload["deals"][0]["price"]), 2500.0);
    assert_eq!(parse_dec(&out.payload["deals"][0]["quantity"]), 500000.0);

    // 2. block_deals — instrument + date filters parsed through to the port.
    let out = tool
        .execute(
            serde_json::json!({
                "instrument": "NSE:TCS",
                "date": "2026-11-10T10:00:00Z"
            }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["deals"][0]["instrument"], "NSE:TCS");
    assert!(out.payload["deals"][0]["trade_date"].is_string());

    // 3. block_deals — honest unavailability when no disclosure source exists.
    let failing = Arc::new(MockMarketReader { fail_quotes: true });
    let mut fail_registry = ToolRegistry::new();
    register_read_only_tools(
        &mut fail_registry,
        Arc::new(MockPortfolioReader),
        failing,
        Arc::new(MockContentReader {
            fail_content: false,
        }),
        None,
    );
    let err = fail_registry
        .get("block_deals")
        .unwrap()
        .execute(serde_json::json!({}), dummy_tool_context())
        .await
        .unwrap_err();
    assert!(
        matches!(err, ToolError::Execution(..)),
        "unavailable port must surface as ToolError::Execution, got {err:?}"
    );
}

#[tokio::test]
async fn test_content_tools() {
    let content = Arc::new(MockContentReader {
        fail_content: false,
    });
    let mut registry = ToolRegistry::new();
    register_read_only_tools(
        &mut registry,
        Arc::new(MockPortfolioReader),
        Arc::new(MockMarketReader { fail_quotes: false }),
        content,
        None,
    );

    // 1. news_search — both tools registered and read-only.
    let news = registry.get("news_search").expect("news_search registered");
    assert_eq!(news.traits().risk, ToolRisk::ReadOnly);
    let filings = registry
        .get("filings_search")
        .expect("filings_search registered");
    assert_eq!(filings.traits().risk, ToolRisk::ReadOnly);

    // 2. news_search — success, no filters.
    let out = news
        .execute(serde_json::json!({}), dummy_tool_context())
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 1);
    assert_eq!(out.payload["articles"][0]["id"], "news_001");
    assert_eq!(
        out.payload["articles"][0]["title"],
        "Reliance quarterly results beat estimates"
    );
    assert_eq!(out.payload["articles"][0]["instruments"][0], "NSE:RELIANCE");
    assert_eq!(out.payload["articles"][0]["source"], "Economic Times");
    assert!(out.payload["articles"][0]["published_at"].is_string());

    // 3. news_search — query + instrument-mention filters parsed through to the port.
    let out = news
        .execute(
            serde_json::json!({
                "query": "results",
                "instrument": "NSE:RELIANCE",
                "limit": 5
            }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 1);

    // 4. news_search — non-matching query yields an empty result, not an error.
    let out = news
        .execute(
            serde_json::json!({ "query": "unrelated" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 0);

    // 5. filings_search — success with type filter.
    let out = filings
        .execute(
            serde_json::json!({
                "instrument": "NSE:TCS",
                "filing_type": "quarterly_result"
            }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 1);
    assert_eq!(out.payload["filings"][0]["instrument"], "NSE:TCS");
    assert_eq!(out.payload["filings"][0]["filing_type"], "quarterly_result");
    assert!(out.payload["filings"][0]["filing_date"].is_string());

    // 6. filings_search — non-matching type filter yields an empty result, not an error.
    let out = filings
        .execute(
            serde_json::json!({ "filing_type": "insider_trade" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 0);

    // 7. filings_search — unknown exchange rejected as invalid arguments.
    let err = filings
        .execute(
            serde_json::json!({ "instrument": "BOGUS:RELIANCE" }),
            dummy_tool_context(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ToolError::InvalidArguments { .. }),
        "unknown exchange must surface as ToolError::InvalidArguments, got {err:?}"
    );

    // 8. Content tools — honest unavailability when no content source exists.
    let failing = Arc::new(MockContentReader { fail_content: true });
    let mut fail_registry = ToolRegistry::new();
    register_read_only_tools(
        &mut fail_registry,
        Arc::new(MockPortfolioReader),
        Arc::new(MockMarketReader { fail_quotes: false }),
        failing,
        None,
    );
    for name in ["news_search", "filings_search"] {
        let err = fail_registry
            .get(name)
            .unwrap()
            .execute(serde_json::json!({}), dummy_tool_context())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ToolError::Execution(..)),
            "unavailable content port must surface as ToolError::Execution, got {err:?}"
        );
    }

    // 9. news_search — deny_unknown_fields rejects unexpected arguments.
    let err = registry
        .get("news_search")
        .unwrap()
        .execute(serde_json::json!({ "bogus": true }), dummy_tool_context())
        .await
        .unwrap_err();
    assert!(
        matches!(err, ToolError::InvalidArguments { .. }),
        "unknown field must be rejected, got {err:?}"
    );
}

#[tokio::test]
async fn test_alert_tools() {
    let store = Arc::new(MockAlertStore::new());
    let reader: Arc<dyn AlertReader> = store.clone();
    let writer: Arc<dyn AlertWriter> = store.clone();
    let mut registry = ToolRegistry::new();
    register_alert_tools(&mut registry, reader, writer);

    // 1. Risk tiers: the list tool is read-only, the CRUD tool is LocalWrite.
    let list = registry
        .get("price_alert_list")
        .expect("price_alert_list registered");
    assert_eq!(list.traits().risk, ToolRisk::ReadOnly);
    let crud = registry.get("price_alert").expect("price_alert registered");
    assert_eq!(crud.traits().risk, ToolRisk::LocalWrite);

    // 2. list on an empty store → count 0.
    let out = list
        .execute(serde_json::json!({}), dummy_tool_context())
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 0);

    // 3. add → active alert with a minted id, decimal-string price, default workspace.
    let out = crud
        .execute(
            serde_json::json!({
                "operation": "add",
                "instrument": "NSE:RELIANCE",
                "price": "2500.50"
            }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    let alert = &out.payload["alert"];
    let added_id = alert["id"].as_str().unwrap().to_string();
    assert!(added_id.starts_with("al_"));
    assert_eq!(alert["instrumentId"], "NSE:RELIANCE");
    assert_eq!(alert["price"], "2500.50");
    assert_eq!(alert["status"], "active");
    assert_eq!(alert["workspaceId"], "default");
    assert_eq!(alert["triggeredAt"], serde_json::Value::Null);

    // 4. list sees it; instrument + status filters and workspace scoping work.
    let out = list
        .execute(serde_json::json!({}), dummy_tool_context())
        .await
        .unwrap();
    assert_eq!(out.payload["count"], 1);
    let out = list
        .execute(
            serde_json::json!({ "instrument": "RELIANCE" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert_eq!(out.payload["count"], 1);
    let out = list
        .execute(
            serde_json::json!({ "instrument": "NSE:TCS" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert_eq!(out.payload["count"], 0);
    let out = list
        .execute(
            serde_json::json!({ "status": "active" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert_eq!(out.payload["count"], 1);
    let out = list
        .execute(
            serde_json::json!({ "workspace": "other" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert_eq!(out.payload["count"], 0);

    // 5. disable → paused; enable → active.
    let out = crud
        .execute(
            serde_json::json!({ "operation": "disable", "id": added_id }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert_eq!(out.payload["alert"]["status"], "paused");
    let out = crud
        .execute(
            serde_json::json!({ "operation": "enable", "id": added_id }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert_eq!(out.payload["alert"]["status"], "active");

    // 6. enable/disable on a triggered alert is a no-op (port contract, ui-web parity).
    store.alerts.lock().unwrap()[0].status = AlertStatus::Triggered;
    let out = crud
        .execute(
            serde_json::json!({ "operation": "disable", "id": added_id }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert_eq!(out.payload["alert"]["status"], "triggered");

    // 7. delete → removed; deleting again is an explicit not-found error.
    let out = crud
        .execute(
            serde_json::json!({ "operation": "delete", "id": added_id }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert_eq!(out.payload["alert"]["id"], added_id.as_str());
    let err = crud
        .execute(
            serde_json::json!({ "operation": "delete", "id": added_id }),
            dummy_tool_context(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ToolError::Execution(..)),
        "unknown id must surface as ToolError::Execution, got {err:?}"
    );

    // 8. Input validation.
    let err = crud
        .execute(
            serde_json::json!({ "operation": "add", "price": "10" }),
            dummy_tool_context(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidArguments { .. }));
    let err = crud
        .execute(
            serde_json::json!({ "operation": "add", "instrument": "NSE:RELIANCE", "price": "-1" }),
            dummy_tool_context(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidArguments { .. }));
    let err = crud
        .execute(
            serde_json::json!({ "operation": "delete" }),
            dummy_tool_context(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidArguments { .. }));
    let err = list
        .execute(
            serde_json::json!({ "instrument": "NSE:RELIANCE", "extra": true }),
            dummy_tool_context(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidArguments { .. }));

    // 9. The alert bundle is exactly 2 tools (list + CRUD).
    let reader: Arc<dyn AlertReader> = Arc::new(MockAlertStore::new());
    let writer: Arc<dyn AlertWriter> = Arc::new(MockAlertStore::new());
    assert_eq!(create_alert_tools(reader, writer).len(), 2);
}

#[tokio::test]
async fn test_portfolio_tools() {
    let market = Arc::new(MockMarketReader { fail_quotes: false });
    let portfolio = Arc::new(MockPortfolioReader);
    let content = Arc::new(MockContentReader {
        fail_content: false,
    });
    let mut registry = ToolRegistry::new();

    register_read_only_tools(&mut registry, portfolio, market, content, None);

    // 1. portfolio_view
    let pv_tool = registry
        .get("portfolio_view")
        .expect("portfolio_view registered");
    let out = pv_tool
        .execute(
            serde_json::json!({ "account": "ACC-01" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["total_accounts"], 1);
    // Cost basis: 100 * 2400 = 240,000; Market value: 100 * 2500 = 250,000; Unrealized = 10,000
    assert_eq!(
        parse_dec(&out.payload["accounts"][0]["cost_basis"]),
        240000.0
    );
    assert_eq!(
        parse_dec(&out.payload["accounts"][0]["valuation"]["market_value"]),
        250000.0
    );
    assert_eq!(
        parse_dec(&out.payload["accounts"][0]["valuation"]["unrealized_pnl"]),
        10000.0
    );

    // 2. positions_view
    let pos_tool = registry
        .get("positions_view")
        .expect("positions_view registered");
    let out = pos_tool
        .execute(
            serde_json::json!({ "account": "ACC-01" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 1);
    assert_eq!(parse_dec(&out.payload["positions"][0]["quantity"]), 100.0);

    // 3. orders_view
    let orders_tool = registry.get("orders_view").expect("orders_view registered");
    let out = orders_tool
        .execute(
            serde_json::json!({ "account": "ACC-01" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 1);
    assert_eq!(out.payload["orders"][0]["client_order_id"], "ORD-101");

    // 4. taxlots_view
    let taxlots_tool = registry
        .get("taxlots_view")
        .expect("taxlots_view registered");
    let out = taxlots_tool
        .execute(
            serde_json::json!({ "account": "ACC-01", "instrument": "NSE:RELIANCE" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["count"], 1);
    assert_eq!(out.payload["lots"][0]["lot_id"], "LOT-01");

    // 5. pnl_view
    let pnl_tool = registry.get("pnl_view").expect("pnl_view registered");
    let out = pnl_tool
        .execute(
            serde_json::json!({ "account": "ACC-01" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(parse_dec(&out.payload["realized_pnl"]), 25000.0);
}

#[tokio::test]
async fn test_portfolio_graceful_degradation_without_quotes() {
    let market = Arc::new(MockMarketReader { fail_quotes: true });
    let portfolio = Arc::new(MockPortfolioReader);
    let mut registry = ToolRegistry::new();

    let content = Arc::new(MockContentReader {
        fail_content: false,
    });
    register_read_only_tools(&mut registry, portfolio, market, content, None);

    let pv_tool = registry.get("portfolio_view").unwrap();
    let out = pv_tool
        .execute(
            serde_json::json!({ "account": "ACC-01" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();

    assert!(out.ok);
    assert_eq!(out.payload["accounts"][0]["valuation"], "unavailable");
    assert_eq!(
        parse_dec(&out.payload["accounts"][0]["cost_basis"]),
        240000.0
    );
}

#[tokio::test]
async fn test_lifted_pure_tools() {
    let market = Arc::new(MockMarketReader { fail_quotes: false });
    let portfolio = Arc::new(MockPortfolioReader);
    let snapshot = Arc::new(MockSnapshotSource);
    let mut registry = ToolRegistry::new();

    register_read_only_tools(
        &mut registry,
        portfolio,
        market,
        Arc::new(MockContentReader {
            fail_content: false,
        }),
        Some(snapshot),
    );

    // 1. screener_scan
    let screener = registry.get("screener_scan").expect("screener_scan lifted");
    assert_eq!(screener.traits().risk, ToolRisk::ReadOnly);
    let out = screener
        .execute(
            serde_json::json!({ "universe": ["INFY", "TCS"], "metric": "rsi_14" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["universe_size"], 2);

    // 2. regime_check
    let regime = registry.get("regime_check").expect("regime_check lifted");
    assert_eq!(regime.traits().risk, ToolRisk::ReadOnly);
    let out = regime
        .execute(
            serde_json::json!({ "regime": "trending" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["regime"], "trending");

    // 3. risk_inspect
    let risk = registry.get("risk_inspect").expect("risk_inspect lifted");
    assert_eq!(risk.traits().risk, ToolRisk::ReadOnly);
    let out = risk
        .execute(
            serde_json::json!({ "metric": "price_band" }),
            dummy_tool_context(),
        )
        .await
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["metric"], "price_band");
}

#[tokio::test]
async fn test_total_read_only_catalogue_count() {
    let market = Arc::new(MockMarketReader { fail_quotes: false });
    let portfolio = Arc::new(MockPortfolioReader);
    let content = Arc::new(MockContentReader {
        fail_content: false,
    });
    let mut registry = ToolRegistry::new();

    register_read_only_tools(&mut registry, portfolio, market, content, None);
    let specs = registry.specs();
    assert_eq!(specs.len(), 17);

    for spec in &specs {
        let tool = registry.get(&spec.name).unwrap();
        assert_eq!(
            tool.traits().risk,
            ToolRisk::ReadOnly,
            "Tool `{}` must have ReadOnly risk level",
            spec.name
        );
    }
}

#[tokio::test]
async fn test_strategy_chat_tools_execution() {
    use vyapaar_agent_chat_tools::catalog::{create_strategy_tools, register_strategy_tools};
    use vyapaar_agent_chat_tools::strategy::{
        StrategyRunTool, StrategyTuneTool, TradeSuggestionTool,
    };

    // 1. StrategyRunTool
    let run_tool = StrategyRunTool::new();
    assert_eq!(run_tool.spec().name, "strategy_run");
    assert_eq!(run_tool.traits().risk, ToolRisk::ReadOnly);

    let run_out = run_tool
        .execute(
            serde_json::json!({
                "strategy": "turtle_breakout",
                "symbol": "NSE:NIFTY50-INDEX",
                "capital": "1000000"
            }),
            dummy_tool_context(),
        )
        .await
        .expect("strategy_run should succeed");

    assert!(run_out.ok);
    assert_eq!(run_out.payload["strategy"], "turtle_breakout");
    assert_eq!(run_out.payload["total_trades"], 12);
    assert!(run_out.payload["net_pnl"].is_number() || run_out.payload["net_pnl"].is_string());

    // 2. StrategyTuneTool
    let tune_tool = StrategyTuneTool::new();
    assert_eq!(tune_tool.spec().name, "strategy_tune");
    assert_eq!(tune_tool.traits().risk, ToolRisk::ReadOnly);

    let tune_out = tune_tool
        .execute(
            serde_json::json!({
                "strategy": "turtle_breakout",
                "symbol": "NSE:NIFTY50-INDEX",
                "metric": "sharpe"
            }),
            dummy_tool_context(),
        )
        .await
        .expect("strategy_tune should succeed");

    assert!(tune_out.ok);
    assert_eq!(tune_out.payload["strategy"], "turtle_breakout");
    assert!(tune_out.payload["tuned_params"].is_object());
    assert!(
        tune_out.payload["overfitting_verdict"]
            .as_str()
            .unwrap()
            .contains("Overfitting Risk")
    );

    // 3. TradeSuggestionTool
    let sugg_tool = TradeSuggestionTool::new();
    assert_eq!(sugg_tool.spec().name, "strategy_suggest");
    assert_eq!(sugg_tool.traits().risk, ToolRisk::ReadOnly);

    let sugg_out = sugg_tool
        .execute(
            serde_json::json!({
                "strategy": "turtle_breakout",
                "symbol": "NSE:NIFTY50-INDEX",
                "risk_pct": "1.0"
            }),
            dummy_tool_context(),
        )
        .await
        .expect("strategy_suggest should succeed");

    assert!(sugg_out.ok);
    assert_eq!(sugg_out.payload["signal"]["bias"], "BUY / LONG");
    assert!(sugg_out.payload["sizing"]["quantity"].as_u64().unwrap() > 0);
    assert!(
        sugg_out.payload["statutory_charges"]["total_charges"].is_number()
            || sugg_out.payload["statutory_charges"]["total_charges"].is_string()
    );

    // 4. Registration in ToolRegistry
    let mut reg = ToolRegistry::new();
    register_strategy_tools(&mut reg, None);
    assert!(reg.get("strategy_run").is_some());
    assert!(reg.get("strategy_tune").is_some());
    assert!(reg.get("strategy_suggest").is_some());

    let bundle = create_strategy_tools(None);
    assert_eq!(bundle.len(), 3);
}
