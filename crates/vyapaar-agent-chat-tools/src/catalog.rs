use std::sync::Arc;
use vyapaar_agent_chat::tool::{ChatTool, ToolRegistry};
use vyapaar_agent_tools::{RegimeCheckTool, RiskInspectionTool, ScreenerQueryTool};

use crate::alert::{PriceAlertListTool, PriceAlertTool};
use crate::calc::ResearchCalcTool;
use crate::content::{FilingsSearchTool, NewsSearchTool};
use crate::lift::{lift_regime, lift_risk, lift_screener};
use crate::market::{
    BlockDealsTool, CorporateActionsTool, MacroSeriesTool, MarketHistoryTool, MarketIndicatorsTool,
    MarketQuoteTool, MarketSearchTool, OptionsChainTool,
};
use crate::portfolio::{
    OrdersViewTool, PnlViewTool, PortfolioViewTool, PositionsViewTool, TaxLotsViewTool,
};
use crate::ports::alert::{AlertReader, AlertWriter};
use crate::ports::content::ContentReader;
use crate::ports::market::MarketReader;
use crate::ports::portfolio::PortfolioReader;
use crate::ports::snapshot::SnapshotSource;

/// Bundle of all 19 Phase B read-only conversational tools (§6.5).
pub fn create_read_only_tools(
    portfolio: Arc<dyn PortfolioReader>,
    market: Arc<dyn MarketReader>,
    content: Arc<dyn ContentReader>,
    snapshots: Option<Arc<dyn SnapshotSource>>,
) -> Vec<Arc<dyn ChatTool>> {
    vec![
        // 1. Market tools (8)
        Arc::new(MarketQuoteTool::new(Arc::clone(&market))),
        Arc::new(MarketHistoryTool::new(Arc::clone(&market))),
        Arc::new(MarketIndicatorsTool::new(Arc::clone(&market))),
        Arc::new(MarketSearchTool::new(Arc::clone(&market))),
        Arc::new(MacroSeriesTool::new(Arc::clone(&market))),
        Arc::new(BlockDealsTool::new(Arc::clone(&market))),
        Arc::new(OptionsChainTool::new(Arc::clone(&market))),
        Arc::new(CorporateActionsTool::new(Arc::clone(&market))),
        // 2. Portfolio tools (5)
        Arc::new(PortfolioViewTool::new(
            Arc::clone(&portfolio),
            Some(Arc::clone(&market)),
        )),
        Arc::new(PositionsViewTool::new(
            Arc::clone(&portfolio),
            Some(Arc::clone(&market)),
        )),
        Arc::new(OrdersViewTool::new(Arc::clone(&portfolio))),
        Arc::new(TaxLotsViewTool::new(Arc::clone(&portfolio))),
        Arc::new(PnlViewTool::new(Arc::clone(&portfolio))),
        // 3. Lifted pure tools from vyapaar-agent-tools (3)
        Arc::new(lift_screener(
            Arc::new(ScreenerQueryTool),
            snapshots.clone(),
        )),
        Arc::new(lift_regime(Arc::new(RegimeCheckTool), snapshots.clone())),
        Arc::new(lift_risk(Arc::new(RiskInspectionTool), snapshots)),
        // 4. Research calculator (1)
        Arc::new(ResearchCalcTool::new()),
        // 5. News & filings content tools (2, plan 0006 Phase 3)
        Arc::new(NewsSearchTool::new(Arc::clone(&content))),
        Arc::new(FilingsSearchTool::new(content)),
    ]
}

/// Register all 19 Phase B read-only chat tools into a registry (§6.5).
pub fn register_read_only_tools(
    registry: &mut ToolRegistry,
    portfolio: Arc<dyn PortfolioReader>,
    market: Arc<dyn MarketReader>,
    content: Arc<dyn ContentReader>,
    snapshots: Option<Arc<dyn SnapshotSource>>,
) {
    let tools = create_read_only_tools(portfolio, market, content, snapshots);
    registry.register_all(tools);
}

/// Bundle of the 2 price-alert tools (§6.5, plan 0006 Phase 2): the list tool is read-only,
/// the CRUD tool is `ToolRisk::LocalWrite` — platform-managed state, not a read-only query,
/// so it lives outside [`create_read_only_tools`] and its all-`ReadOnly` catalogue-count
/// invariant.
pub fn create_alert_tools(
    reader: Arc<dyn AlertReader>,
    writer: Arc<dyn AlertWriter>,
) -> Vec<Arc<dyn ChatTool>> {
    vec![
        // 1. Price alerts (2: 1 read-only + 1 LocalWrite CRUD)
        Arc::new(PriceAlertListTool::new(reader)),
        Arc::new(PriceAlertTool::new(writer)),
    ]
}

/// Register the 2 price-alert tools into a registry (§6.5).
pub fn register_alert_tools(
    registry: &mut ToolRegistry,
    reader: Arc<dyn AlertReader>,
    writer: Arc<dyn AlertWriter>,
) {
    let tools = create_alert_tools(reader, writer);
    registry.register_all(tools);
}

use crate::ports::strategy::StrategyRunner;
use crate::strategy::{StrategyRunTool, StrategyTuneTool, TradeSuggestionTool};

/// Bundle of the 3 strategy advisory tools (§6.4).
pub fn create_strategy_tools(runner: Option<Arc<dyn StrategyRunner>>) -> Vec<Arc<dyn ChatTool>> {
    if let Some(r) = runner {
        vec![
            Arc::new(StrategyRunTool::with_runner(Arc::clone(&r))),
            Arc::new(StrategyTuneTool::with_runner(Arc::clone(&r))),
            Arc::new(TradeSuggestionTool::with_runner(r)),
        ]
    } else {
        vec![
            Arc::new(StrategyRunTool::new()),
            Arc::new(StrategyTuneTool::new()),
            Arc::new(TradeSuggestionTool::new()),
        ]
    }
}

/// Register the 3 strategy execution and trade suggestion tools into a registry (§6.4).
pub fn register_strategy_tools(
    registry: &mut ToolRegistry,
    runner: Option<Arc<dyn StrategyRunner>>,
) {
    let tools = create_strategy_tools(runner);
    registry.register_all(tools);
}
