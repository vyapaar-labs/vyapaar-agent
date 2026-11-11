//! # vyapaar-agent-chat-tools
//!
//! Conversational read-only tools and domain read ports for `vyapaar-agent` (§4, §6).
//!
//! Owns the `PortfolioReader`, `MarketReader`, `ContentReader`, and `SnapshotSource` ports, the
//! lifted pure tools via [`PureToolAdapter`], and model-facing tools for market, portfolio,
//! content, and research calculations.
//!
//! Architectural invariants:
//! - **Pure advisory (NoFX)**: chat tools never place orders or produce `Decision`s (§4.3).
//! - **Dependency rules (R1, R2, R4)**: depends on `vyapaar-ledger` for types only; zero dependency
//!   on `vyapaar-runtime`, `vyapaar-exec`, `vyapaar-market`, or `vyapaar-agent-trader-loop`.

pub mod alert;
pub mod calc;
pub mod catalog;
pub mod content;
pub mod lift;
pub mod market;
pub mod portfolio;
pub mod ports;

pub use alert::{
    AlertOperation, PriceAlertInput, PriceAlertListInput, PriceAlertListTool, PriceAlertTool,
};
pub use calc::{
    CagrArgs, CalcOperation, MarketSegment, PositionSizingArgs, ResearchCalcInput,
    ResearchCalcTool, ReturnMetricsArgs, StatutoryCostsArgs, TradeSide,
};
pub use catalog::{
    create_alert_tools, create_read_only_tools, register_alert_tools, register_read_only_tools,
};
pub use content::{FilingsSearchInput, FilingsSearchTool, NewsSearchInput, NewsSearchTool};
pub use lift::{PureToolAdapter, lift_regime, lift_risk, lift_screener};
pub use market::{
    BlockDealsInput, BlockDealsTool, CorporateActionsInput, CorporateActionsTool, MacroSeriesInput,
    MacroSeriesTool, MarketHistoryInput, MarketHistoryTool, MarketIndicatorsInput,
    MarketIndicatorsTool, MarketQuoteInput, MarketQuoteTool, MarketSearchInput, MarketSearchTool,
    OptionsChainInput, OptionsChainTool, parse_instrument,
};
pub use portfolio::{
    OrdersViewInput, OrdersViewTool, PnlViewInput, PnlViewTool, PortfolioViewInput,
    PortfolioViewTool, PositionsViewInput, PositionsViewTool, TaxLotsViewInput, TaxLotsViewTool,
};
pub use ports::{
    AccountSummary, AlertReader, AlertStatus, AlertWriter, BlockDeal, ContentReader,
    CorporateAction, DateRange, FilingRecord, InstrumentSearchResult, MacroSeries, MarketBar,
    MarketHistory, MarketIndicators, MarketQuote, MarketReader, NewsArticle, OptionContract,
    OptionsChain, OrderFilter, PortError, PortErrorKind, PortfolioReader, PriceAlert, SnapshotData,
    SnapshotNeeds, SnapshotSource,
};
pub mod strategy;

pub use catalog::{create_strategy_tools, register_strategy_tools};
pub use ports::strategy::{DefaultStrategyRunner, StrategyRunner};
pub use strategy::{
    StrategyRunInput, StrategyRunTool, StrategyTuneInput, StrategyTuneTool, TradeSuggestionInput,
    TradeSuggestionTool,
};
