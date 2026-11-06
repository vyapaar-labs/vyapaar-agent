//! Fama-MacBeth two-pass cross-sectional regression.
//!
//! First pass: time-series regression of each asset's returns on factor
//! returns to estimate factor betas. Second pass: per-period cross-sectional
//! regression of asset returns on the betas to estimate risk premia `λ_t`;
//! the point estimates are the time means and the t-stats use the time-series
//! standard errors (Fama-MacBeth 1973).

use crate::error::PanelError;
use crate::matrix::{mean, ols, std_dev};

/// Result of the Fama-MacBeth two-pass procedure.
#[derive(Debug, Clone, PartialEq)]
pub struct FamaMacBethResult {
    /// Time-series betas: `betas[asset][factor]`.
    pub betas: Vec<Vec<f64>>,
    /// Time-series intercepts per asset (the alpha estimates from pass one).
    pub alphas: Vec<f64>,
    /// Mean cross-sectional risk premia per factor.
    pub lambda: Vec<f64>,
    /// Fama-MacBeth t-stats per factor.
    pub t_stats: Vec<f64>,
    /// Mean absolute cross-sectional pricing error per period.
    pub mean_abs_alpha: f64,
    /// Number of periods.
    pub n_periods: usize,
    /// Number of assets.
    pub n_assets: usize,
    /// Number of factors.
    pub n_factors: usize,
}

/// Run the Fama-MacBeth two-pass procedure.
///
/// - `returns`: `T × N` matrix of asset returns (rows = periods).
/// - `factor_returns`: `T × K` matrix of factor returns.
///
/// Requires `T > K + 1` and `N > K + 1` so both passes are identified.
#[allow(clippy::needless_range_loop)]
pub fn fama_macbeth(
    returns: &[Vec<f64>],
    factor_returns: &[Vec<f64>],
) -> Result<FamaMacBethResult, PanelError> {
    let t = returns.len();
    if t == 0 {
        return Err(PanelError::InsufficientData { needed: 1, got: 0 });
    }
    let n = returns[0].len();
    if factor_returns.len() != t {
        return Err(PanelError::DimensionMismatch(
            "returns and factor_returns must have the same number of periods".to_string(),
        ));
    }
    let k = factor_returns[0].len();
    if t <= k + 1 {
        return Err(PanelError::InsufficientData {
            needed: k + 2,
            got: t,
        });
    }
    if n <= k + 1 {
        return Err(PanelError::InsufficientData {
            needed: k + 2,
            got: n,
        });
    }
    if returns
        .iter()
        .flatten()
        .chain(factor_returns.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(PanelError::NonFinite);
    }

    // First pass: time-series betas per asset.
    let mut betas = Vec::with_capacity(n);
    let mut alphas = Vec::with_capacity(n);
    for asset in 0..n {
        let y_ts: Vec<f64> = (0..t).map(|p| returns[p][asset]).collect();
        // Design: [1, f_1, ..., f_K].
        let x_ts: Vec<Vec<f64>> = (0..t)
            .map(|p| {
                let mut row = vec![1.0];
                row.extend_from_slice(&factor_returns[p]);
                row
            })
            .collect();
        let coef = ols(&x_ts, &y_ts)?;
        alphas.push(coef[0]);
        betas.push(coef[1..].to_vec());
    }

    // Second pass: cross-sectional regression per period.
    let mut lambda_series = vec![Vec::with_capacity(t); k];
    let mut alpha_series = Vec::with_capacity(t);
    for p in 0..t {
        let y_cs: Vec<f64> = (0..n).map(|asset| returns[p][asset]).collect();
        let x_cs: Vec<Vec<f64>> = (0..n)
            .map(|asset| {
                let mut row = vec![1.0];
                row.extend_from_slice(&betas[asset]);
                row
            })
            .collect();
        let coef = ols(&x_cs, &y_cs)?;
        // coef[0] is the zero-beta premium λ_0, coef[1..] are the risk premia.
        for (factor, &lambda_t) in coef[1..].iter().enumerate() {
            lambda_series[factor].push(lambda_t);
        }
        // Cross-sectional pricing errors given the premia.
        let fitted: Vec<f64> = x_cs
            .iter()
            .map(|row| row.iter().zip(&coef).map(|(a, b)| a * b).sum())
            .collect();
        let abs_err: f64 = (0..n).map(|i| (y_cs[i] - fitted[i]).abs()).sum::<f64>() / n as f64;
        alpha_series.push(abs_err);
    }

    let lambda: Vec<f64> = lambda_series.iter().map(|s| mean(s)).collect();
    let t_stats: Vec<f64> = (0..k)
        .map(|f| {
            let sd = std_dev(&lambda_series[f]);
            if sd > 0.0 {
                lambda[f] / (sd / (t as f64).sqrt())
            } else {
                0.0
            }
        })
        .collect();
    let mean_abs_alpha = mean(&alpha_series);

    Ok(FamaMacBethResult {
        betas,
        alphas,
        lambda,
        t_stats,
        mean_abs_alpha,
        n_periods: t,
        n_assets: n,
        n_factors: k,
    })
}
