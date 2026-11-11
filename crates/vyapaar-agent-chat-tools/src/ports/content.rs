//! Content read port: news articles and regulatory filings (plan 0006 Phase 3).
//!
//! Modeled on longbridge's Content/Search category and Fincept's "News & alternative
//! data" toolset. Upstream state: regulatory filings are ingested in `vyapaar-data`'s
//! Python service (closed gap `vyapaar-data/docs/gaps/0003`, read via
//! `vyapaar_data.service.filings_reader.LakehouseFilingsReader`) but have no Rust-side
//! bridge yet; news has no data source anywhere. Implementors must answer
//! `PortError::unavailable` with a gap citation for what they cannot serve — never an
//! empty success (see `vyapaar-runtime/docs/gaps/0007` for why that distinction matters).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vyapaar_core::InstrumentId;

use crate::ports::error::PortError;

/// A financial news article hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsArticle {
    /// Stable article identifier (source-specific).
    pub id: String,
    /// Headline.
    pub title: String,
    /// Publisher / source name (e.g. "Economic Times").
    pub source: String,
    /// Canonical article URL, if resolvable.
    pub url: Option<String>,
    /// Publication timestamp.
    pub published_at: DateTime<Utc>,
    /// Short summary or lede, if provided.
    pub summary: Option<String>,
    /// Free-form instrument symbols/tags the article mentions (non-canonical — news
    /// mentions are not validated scrip references, unlike `FilingRecord::instrument`).
    pub instruments: Vec<String>,
}

/// One disclosed regulatory filing (SEBI/BSE/NSE equivalent of EDGAR).
///
/// Mirrors `vyapaar_data.domain.filings.Filing`: `filing_type` is the canonical
/// `FilingType` value — one of "corporate_action", "shareholding", "insider_trade",
/// "fund_holding", "annual_report", "quarterly_result", "other".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilingRecord {
    /// Instrument the filing concerns.
    pub instrument: InstrumentId,
    /// Filing / disclosure date (midnight UTC when only a calendar date exists).
    pub filing_date: DateTime<Utc>,
    /// Canonical filing category (see struct doc).
    pub filing_type: String,
    /// Filing title / subject.
    pub title: String,
    /// Source document URL, if available.
    pub source_url: Option<String>,
}

/// Read-only content port (plan 0006 Phase 3).
///
/// Implemented by runtime application adapters; chat tools depend only on this trait.
#[async_trait]
pub trait ContentReader: Send + Sync + 'static {
    /// Search news articles, optionally by free-text keyword, mentioned instrument
    /// tag, and capped at `limit` (implementor-chosen default when `None`).
    async fn news_search(
        &self,
        query: Option<&str>,
        instrument: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<NewsArticle>, PortError>;

    /// Search regulatory filings, optionally filtered by instrument and filing type,
    /// capped at `limit` (implementor-chosen default when `None`).
    async fn filings(
        &self,
        instrument: Option<&InstrumentId>,
        filing_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<FilingRecord>, PortError>;
}
