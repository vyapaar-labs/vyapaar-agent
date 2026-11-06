//! Portfolio read port and DTO contracts (§6.2).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vyapaar_core::{AccountId, AlgoId, InstrumentId, Money, OrderStatus};
use vyapaar_ledger::PositionRecord;
use vyapaar_ledger::TaxLot;
use vyapaar_ledger::repository::OrderRecord;

use crate::ports::error::PortError;

/// Date range query for portfolio metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DateRange {
    /// Start of date range (inclusive).
    pub start: Option<DateTime<Utc>>,
    /// End of date range (exclusive).
    pub end: Option<DateTime<Utc>>,
}

impl DateRange {
    /// Construct a new date range.
    pub fn new(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Self {
        Self { start, end }
    }
}

/// Summary of an account's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSummary {
    /// Canonical account identifier.
    pub account_id: AccountId,
    /// Optional strategy/algo tag.
    pub algo_id: Option<AlgoId>,
    /// Instantaneous cash balance.
    pub cash: Money,
    /// Account status string.
    pub status: String,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Filter for order queries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderFilter {
    /// Filter by instrument.
    pub instrument: Option<InstrumentId>,
    /// Filter by order status.
    pub status: Option<OrderStatus>,
    /// Restrict only to active (unfilled / partially filled) orders.
    pub active_only: bool,
    /// Maximum count of records to return.
    pub limit: Option<usize>,
}

/// Read-only portfolio inspection port (§6.2).
///
/// Implemented by runtime application layer over ledger repositories.
/// Provides read-only access to accounts, positions, tax lots, cash, orders, and realized PnL.
#[async_trait]
pub trait PortfolioReader: Send + Sync + 'static {
    /// Retrieve all account summaries.
    async fn accounts(&self) -> Result<Vec<AccountSummary>, PortError>;

    /// Retrieve all open positions for a specific account.
    async fn positions(&self, account: &AccountId) -> Result<Vec<PositionRecord>, PortError>;

    /// Retrieve FIFO tax lots for a given account and instrument.
    async fn tax_lots(
        &self,
        account: &AccountId,
        instrument: &InstrumentId,
    ) -> Result<Vec<TaxLot>, PortError>;

    /// Retrieve cash balance for an account.
    async fn cash(&self, account: &AccountId) -> Result<Vec<Money>, PortError>;

    /// Retrieve orders matching an optional filter.
    async fn orders(
        &self,
        account: &AccountId,
        filter: OrderFilter,
    ) -> Result<Vec<OrderRecord>, PortError>;

    /// Retrieve realized profit and loss for an account across an optional date range.
    async fn realized_pnl(&self, account: &AccountId, range: DateRange)
    -> Result<Money, PortError>;
}
