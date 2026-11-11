//! Domain read ports owned by `vyapaar-agent-chat-tools` (§6.2, §6.4).
//!
//! Enforces compile-time read-only safety: chat tools hold references to these
//! traits and cannot access mutators or execution systems.

pub mod alert;
pub mod content;
pub mod error;
pub mod market;
pub mod portfolio;
pub mod snapshot;

pub use alert::{AlertReader, AlertStatus, AlertWriter, PriceAlert};
pub use content::{ContentReader, FilingRecord, NewsArticle};
pub use error::{PortError, PortErrorKind};
pub use market::{
    BlockDeal, CorporateAction, InstrumentSearchResult, MacroSeries, MarketBar, MarketHistory,
    MarketIndicators, MarketQuote, MarketReader, OptionContract, OptionsChain,
};
pub use portfolio::{AccountSummary, DateRange, OrderFilter, PortfolioReader};
pub use snapshot::{SnapshotData, SnapshotNeeds, SnapshotSource};
pub mod strategy;
pub use strategy::{DefaultStrategyRunner, StrategyRunner};
