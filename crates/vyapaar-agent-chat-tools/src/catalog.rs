use std::sync::Arc;
use vyapaar_agent_chat::tool::{ChatTool, ToolRegistry};
use vyapaar_agent_tools::{RegimeCheckTool, RiskInspectionTool, ScreenerQueryTool};

use crate::calc::ResearchCalcTool;
use crate::lift::{lift_regime, lift_risk, lift_screener};
use crate::market::{
    MacroSeriesTool, MarketHistoryTool, MarketIndicatorsTool, MarketQuoteTool, MarketSearchTool,
};
use crate::portfolio::{
    OrdersViewTool, PnlViewTool, PortfolioViewTool, PositionsViewTool, TaxLotsViewTool,
};
use crate::ports::market::MarketReader;
use crate::ports::portfolio::PortfolioReader;
use crate::ports::snapshot::SnapshotSource;

/// Bundle of all 14 Phase B read-only conversational tools (§6.5).
pub fn create_read_only_tools(
    portfolio: Arc<dyn PortfolioReader>,
    market: Arc<dyn MarketReader>,
    snapshots: Option<Arc<dyn SnapshotSource>>,
) -> Vec<Arc<dyn ChatTool>> {
    vec![
        // 1. Market tools (5)
        Arc::new(MarketQuoteTool::new(Arc::clone(&market))),
        Arc::new(MarketHistoryTool::new(Arc::clone(&market))),
        Arc::new(MarketIndicatorsTool::new(Arc::clone(&market))),
        Arc::new(MarketSearchTool::new(Arc::clone(&market))),
        Arc::new(MacroSeriesTool::new(Arc::clone(&market))),
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
    ]
}

/// Register all 14 Phase B read-only chat tools into a registry (§6.5).
pub fn register_read_only_tools(
    registry: &mut ToolRegistry,
    portfolio: Arc<dyn PortfolioReader>,
    market: Arc<dyn MarketReader>,
    snapshots: Option<Arc<dyn SnapshotSource>>,
) {
    let tools = create_read_only_tools(portfolio, market, snapshots);
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
pub fn register_strategy_tools(registry: &mut ToolRegistry, runner: Option<Arc<dyn StrategyRunner>>) {
    let tools = create_strategy_tools(runner);
    registry.register_all(tools);
}
