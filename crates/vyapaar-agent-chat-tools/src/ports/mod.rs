//! Domain read ports owned by `vyapaar-agent-chat-tools` (§6.2, §6.4).
//!
//! Enforces compile-time read-only safety: chat tools hold references to these
//! traits and cannot access mutators or execution systems.

pub mod error;
pub mod market;
pub mod portfolio;
pub mod snapshot;

pub use error::{PortError, PortErrorKind};
pub use market::{
    InstrumentSearchResult, MacroSeries, MarketBar, MarketHistory, MarketIndicators, MarketQuote,
    MarketReader,
};
pub use portfolio::{AccountSummary, DateRange, OrderFilter, PortfolioReader};
pub use snapshot::{SnapshotData, SnapshotNeeds, SnapshotSource};
