//! Market read port and DTO contracts (§6.4).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use vyapaar_core::{InstrumentId, Price, PriceBand, Quantity};

use crate::ports::error::PortError;
use crate::ports::portfolio::DateRange;

/// Latest market quote for an instrument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketQuote {
    /// Instrument identifier.
    pub instrument: InstrumentId,
    /// Last traded price.
    pub last_price: Price,
    /// Best bid price.
    pub bid: Option<Price>,
    /// Best ask price.
    pub ask: Option<Price>,
    /// Last traded or cumulative volume.
    pub volume: Quantity,
    /// Quote observation timestamp.
    pub as_of: DateTime<Utc>,
}

/// OHLCV bar for market history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketBar {
    /// Bar start timestamp.
    pub timestamp: DateTime<Utc>,
    /// Open price.
    pub open: Price,
    /// High price.
    pub high: Price,
    /// Low price.
    pub low: Price,
    /// Close price.
    pub close: Price,
    /// Bar volume.
    pub volume: Quantity,
}

/// Historical series of bars with downsampling marker (§6.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketHistory {
    /// Instrument identifier.
    pub instrument: InstrumentId,
    /// Bars in chronological order.
    pub bars: Vec<MarketBar>,
    /// True if data was downsampled or truncated to cap limit (§6.5).
    pub downsampled: bool,
}

/// Set of technical indicators computed for an instrument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketIndicators {
    /// Instrument identifier.
    pub instrument: InstrumentId,
    /// Computed indicator values (e.g. "rsi_14" -> 62.5, "sma_20" -> 2450.0).
    pub values: HashMap<String, Decimal>,
    /// Snapshot timestamp.
    pub as_of: DateTime<Utc>,
}

/// Search hit when discovering instruments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentSearchResult {
    /// Canonical instrument identifier.
    pub instrument: InstrumentId,
    /// Human-readable scrip/security name.
    pub name: String,
    /// Trading venue / exchange.
    pub exchange: String,
    /// Market lot size if applicable.
    pub lot_size: Option<u64>,
}

/// Macro-economic series data points (§6.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroSeries {
    /// Canonical series identifier (e.g. "cpi", "rbi_repo_rate", "iip").
    pub series: String,
    /// Time-stamped metric values.
    pub points: Vec<(DateTime<Utc>, Decimal)>,
}

/// A single disclosed block or bulk deal (exchange disclosure feed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDeal {
    /// Instrument identifier.
    pub instrument: InstrumentId,
    /// Exchange that disclosed the deal (e.g. "NSE", "BSE").
    pub exchange: String,
    /// Disclosed client / trading-member name.
    pub client_name: String,
    /// Deal side: "buy" or "sell".
    pub side: String,
    /// Deal category: "block" or "bulk".
    pub category: String,
    /// Traded quantity.
    pub quantity: Quantity,
    /// Traded price.
    pub price: Price,
    /// Disclosure / trade date.
    pub trade_date: DateTime<Utc>,
}

/// Read-only market data port (§6.4).
///
/// Implemented by runtime application layer over market feeds and history stores.
#[async_trait]
pub trait MarketReader: Send + Sync + 'static {
    /// Fetch latest quote for an instrument.
    async fn quote(&self, instrument: &InstrumentId) -> Result<MarketQuote, PortError>;

    /// Fetch historical bars capped at 500 bars (downsampled if larger) (§6.5).
    async fn history(
        &self,
        instrument: &InstrumentId,
        range: DateRange,
        limit: Option<usize>,
    ) -> Result<MarketHistory, PortError>;

    /// Fetch requested technical indicators for an instrument.
    async fn indicators(
        &self,
        instrument: &InstrumentId,
        names: &[String],
    ) -> Result<MarketIndicators, PortError>;

    /// Search for instruments by query string (symbol or name).
    async fn search_instruments(
        &self,
        query: &str,
    ) -> Result<Vec<InstrumentSearchResult>, PortError>;

    /// Fetch macro-economic series data (e.g. CPI, IIP, RBI repo rate).
    async fn macro_series(&self, series: &str) -> Result<MacroSeries, PortError>;

    /// Fetch disclosed block/bulk deals for a date (defaults to latest trading day) or instrument.
    async fn block_deals(
        &self,
        instrument: Option<&InstrumentId>,
        date: Option<DateTime<Utc>>,
    ) -> Result<Vec<BlockDeal>, PortError>;

    /// Fetch pre-trade price bands for an instrument.
    async fn price_band(&self, instrument: &InstrumentId) -> Result<PriceBand, PortError>;
}
