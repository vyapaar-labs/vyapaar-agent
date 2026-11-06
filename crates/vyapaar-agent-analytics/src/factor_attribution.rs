//! Barra-style cross-sectional factor model and portfolio attribution
//! (plan 0003 Phase 2).
//!
//! Given a portfolio's holdings with style/sector/market factor exposures and
//! a factor-return panel, this module attributes portfolio *return* and
//! *risk* to factor exposures:
//!
//! - **Return attribution**: portfolio exposure to factor `k` is the
//!   weight-weighted mean of asset exposures; its contribution is that
//!   exposure times the factor's mean return. The remainder is the residual
//!   (specific) return.
//! - **Risk attribution**: with a factor covariance matrix `Σ` estimated from
//!   the panel and asset-specific volatilities, portfolio factor risk is
//!   `sqrt(x'Σx)` and each factor's contribution is its share of factor
//!   variance; specific risk is `sqrt(Σ w_i² σ_i²)`.
//!
//! Output renders into the `vyapaar/result/v1` [`DisplayBlock`] shapes used by
//! every MCP client: a metric grid plus a per-factor table.
//!
//! This is the factor-attribution half of Fincept's Portfolio Lab only; the
//! allocation half (HRP/Black-Litterman) lives in `vyapaar-ledger-allocation`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use vyapaar_agent_core::{DisplayBlock, MetricItem};

use crate::error::{AnalyticsError, Result};

/// A style-factor exposure for one asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleExposure {
    /// Factor name (e.g. "momentum", "value", "size").
    pub factor: String,
    /// Numeric exposure (z-score scale).
    pub value: f64,
}

/// Sector membership for one asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorExposure {
    /// Sector name (e.g. "Financials").
    pub sector: String,
}

/// One portfolio holding with its exposures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioHolding {
    /// Asset identifier (e.g. "NSE:TCS-EQ").
    pub asset: String,
    /// Portfolio weight (sums to 1 across holdings).
    pub weight: f64,
    /// Style factor exposures.
    pub style: Vec<StyleExposure>,
    /// Sector memberships (usually a single sector).
    pub sectors: Vec<SectorExposure>,
    /// Asset-specific (idiosyncratic) annualized volatility.
    pub specific_risk: f64,
}

/// Factor-return panel: one row per period, factor name → return.
pub type FactorReturnPanel = Vec<HashMap<String, f64>>;

/// Per-factor attribution of portfolio return and risk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorContribution {
    /// Factor name.
    pub factor: String,
    /// Portfolio-weighted exposure to the factor.
    pub exposure: f64,
    /// Mean factor return over the panel.
    pub avg_factor_return: f64,
    /// Return contribution `exposure × avg_factor_return`.
    pub return_contribution: f64,
    /// Share of factor variance contributed by this factor (percent).
    pub risk_contribution_pct: f64,
}

/// Complete factor-model attribution for a portfolio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorAttribution {
    /// Mean portfolio return over the panel.
    pub total_return: f64,
    /// Portfolio return explained by factor exposures.
    pub factor_return: f64,
    /// Portfolio return not explained by the factors (residual).
    pub residual_return: f64,
    /// Annualized total portfolio volatility.
    pub total_risk: f64,
    /// Annualized volatility attributable to factor exposures.
    pub factor_risk: f64,
    /// Annualized idiosyncratic (specific) volatility.
    pub specific_risk: f64,
    /// Per-factor contributions.
    pub factors: Vec<FactorContribution>,
    /// Number of periods in the factor panel.
    pub n_periods: usize,
    /// Number of holdings.
    pub n_assets: usize,
    /// Number of factors (style + sector).
    pub n_factors: usize,
}

/// Fit a cross-sectional factor model and attribute portfolio return/risk.
///
/// - `holdings`: portfolio positions with exposures (weights need not sum to
///   exactly 1; they are normalized).
/// - `factor_returns`: a panel of per-period factor returns. Every factor that
///   appears in `holdings` must appear in every period's row; extra factors
///   are allowed and simply contribute zero exposure.
/// - `annualization_factor`: periods per year used to annualize risk
///   (e.g. 252 for daily returns).
pub fn fit_cross_sectional_factor_model(
    holdings: &[PortfolioHolding],
    factor_returns: &FactorReturnPanel,
    annualization_factor: f64,
) -> Result<FactorAttribution> {
    if holdings.is_empty() {
        return Err(AnalyticsError::EmptyInput);
    }
    if factor_returns.is_empty() {
        return Err(AnalyticsError::InsufficientData { needed: 1, got: 0 });
    }
    if annualization_factor <= 0.0 || !annualization_factor.is_finite() {
        return Err(AnalyticsError::InsufficientData { needed: 1, got: 0 });
    }

    let n_periods = factor_returns.len();
    let n_assets = holdings.len();

    // Factor universe = union of style factors + sectors.
    let mut factor_set: HashSet<String> = HashSet::new();
    for holding in holdings {
        for style in &holding.style {
            factor_set.insert(style.factor.clone());
        }
        for sector in &holding.sectors {
            factor_set.insert(format!("sector_{}", sector.sector));
        }
    }
    let factors: Vec<String> = {
        let mut sorted: Vec<String> = factor_set.into_iter().collect();
        sorted.sort();
        sorted
    };
    let n_factors = factors.len();
    let factor_index: HashMap<&str, usize> = factors
        .iter()
        .enumerate()
        .map(|(i, f)| (f.as_str(), i))
        .collect();

    // Validate the panel contains every factor each period.
    for (period_idx, row) in factor_returns.iter().enumerate() {
        for factor in &factors {
            if !row.contains_key(factor) {
                return Err(AnalyticsError::TradeMismatch(format!(
                    "period {period_idx} missing factor '{factor}'"
                )));
            }
        }
    }

    // Normalize weights and build the asset × factor exposure matrix.
    let total_weight: f64 = holdings.iter().map(|h| h.weight).sum();
    let normalized_weight = if total_weight.abs() > f64::EPSILON {
        total_weight
    } else {
        1.0
    };
    let mut exposures = vec![vec![0.0; n_factors]; n_assets];
    for (asset, holding) in holdings.iter().enumerate() {
        for style in &holding.style {
            if let Some(&idx) = factor_index.get(style.factor.as_str()) {
                exposures[asset][idx] = style.value;
            }
        }
        for sector in &holding.sectors {
            let name = format!("sector_{}", sector.sector);
            if let Some(&idx) = factor_index.get(name.as_str()) {
                exposures[asset][idx] = 1.0;
            }
        }
    }

    // Portfolio-weighted factor exposure.
    let mut portfolio_exposure = vec![0.0; n_factors];
    for asset in 0..n_assets {
        let w = holdings[asset].weight / normalized_weight;
        for k in 0..n_factors {
            portfolio_exposure[k] += w * exposures[asset][k];
        }
    }

    // Mean factor returns and the factor covariance matrix.
    let mut mean_factor_return = vec![0.0; n_factors];
    for k in 0..n_factors {
        mean_factor_return[k] = factor_returns
            .iter()
            .map(|row| row[&factors[k]])
            .sum::<f64>()
            / n_periods as f64;
    }
    let mut cov = vec![vec![0.0; n_factors]; n_factors];
    for i in 0..n_factors {
        for j in 0..n_factors {
            let mut sum = 0.0;
            for row in factor_returns {
                sum += (row[&factors[i]] - mean_factor_return[i])
                    * (row[&factors[j]] - mean_factor_return[j]);
            }
            cov[i][j] = sum / n_periods.saturating_sub(1).max(1) as f64;
        }
    }

    // Predicted portfolio returns per period.
    let mut predicted_portfolio_returns = Vec::with_capacity(n_periods);
    for row in factor_returns {
        let mut predicted = 0.0;
        for k in 0..n_factors {
            predicted += portfolio_exposure[k] * row[&factors[k]];
        }
        predicted_portfolio_returns.push(predicted);
    }
    let total_return = predicted_portfolio_returns.iter().sum::<f64>() / n_periods as f64;

    // Factor + specific variance (annualized).
    let mut factor_variance = 0.0;
    for i in 0..n_factors {
        let mut row_sum = 0.0;
        for j in 0..n_factors {
            row_sum += portfolio_exposure[j] * cov[i][j];
        }
        factor_variance += portfolio_exposure[i] * row_sum;
    }
    factor_variance = factor_variance.max(0.0) * annualization_factor;
    let factor_risk = factor_variance.sqrt();

    let specific_variance: f64 = holdings
        .iter()
        .map(|h| (h.weight / normalized_weight).powi(2) * h.specific_risk.powi(2))
        .sum::<f64>();
    let specific_risk = specific_variance.sqrt();
    let total_risk = (factor_variance + specific_variance).sqrt();

    // Per-factor return contributions and variance shares.
    let mut factors_contrib: Vec<FactorContribution> = Vec::with_capacity(n_factors);
    for k in 0..n_factors {
        let mut factor_var_share = 0.0;
        for j in 0..n_factors {
            factor_var_share += portfolio_exposure[j] * cov[k][j];
        }
        factor_var_share *= portfolio_exposure[k];
        let contribution_pct = if factor_variance > 0.0 {
            factor_var_share * annualization_factor / factor_variance * 100.0
        } else {
            0.0
        };
        factors_contrib.push(FactorContribution {
            factor: factors[k].clone(),
            exposure: portfolio_exposure[k],
            avg_factor_return: mean_factor_return[k],
            return_contribution: portfolio_exposure[k] * mean_factor_return[k],
            risk_contribution_pct: contribution_pct,
        });
    }

    let factor_return: f64 = factors_contrib.iter().map(|f| f.return_contribution).sum();
    let residual_return = total_return - factor_return;

    Ok(FactorAttribution {
        total_return,
        factor_return,
        residual_return,
        total_risk,
        factor_risk,
        specific_risk,
        factors: factors_contrib,
        n_periods,
        n_assets,
        n_factors,
    })
}

/// Render an attribution into the universal `vyapaar/result/v1` presentation
/// blocks: a metric grid plus a per-factor table.
pub fn factor_attribution_blocks(attribution: &FactorAttribution) -> Vec<DisplayBlock> {
    let metrics = vec![
        MetricItem {
            label: "total_return".to_string(),
            value: format!("{:.4}%", attribution.total_return * 100.0),
        },
        MetricItem {
            label: "factor_return".to_string(),
            value: format!("{:.4}%", attribution.factor_return * 100.0),
        },
        MetricItem {
            label: "residual_return".to_string(),
            value: format!("{:.4}%", attribution.residual_return * 100.0),
        },
        MetricItem {
            label: "total_risk".to_string(),
            value: format!("{:.2}%", attribution.total_risk * 100.0),
        },
        MetricItem {
            label: "factor_risk".to_string(),
            value: format!("{:.2}%", attribution.factor_risk * 100.0),
        },
        MetricItem {
            label: "specific_risk".to_string(),
            value: format!("{:.2}%", attribution.specific_risk * 100.0),
        },
    ];

    let rows: Vec<Vec<String>> = attribution
        .factors
        .iter()
        .map(|f| {
            vec![
                f.factor.clone(),
                format!("{:.3}", f.exposure),
                format!("{:.4}%", f.avg_factor_return * 100.0),
                format!("{:.4}%", f.return_contribution * 100.0),
                format!("{:.2}%", f.risk_contribution_pct),
            ]
        })
        .collect();

    vec![
        DisplayBlock::MetricGrid { items: metrics },
        DisplayBlock::Table {
            title: "Factor attribution".to_string(),
            columns: vec![
                "factor".to_string(),
                "exposure".to_string(),
                "avg_return".to_string(),
                "return_contribution".to_string(),
                "risk_share_pct".to_string(),
            ],
            rows,
        },
    ]
}
