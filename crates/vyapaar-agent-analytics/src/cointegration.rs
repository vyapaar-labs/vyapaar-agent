//! Engle-Granger two-step cointegration test and pairs-trading spread analysis.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{AnalyticsError, Result};
use crate::math::mean;

/// Results from Engle-Granger cointegration test.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CointegrationResult {
    /// OLS hedge ratio ($\beta$) of series $Y$ on series $X$.
    pub hedge_ratio: f64,
    /// OLS intercept ($\alpha$).
    pub intercept: f64,
    /// Augmented Dickey-Fuller $t$-statistic on regression residuals.
    pub adf_stat: f64,
    /// Estimated asymptotic $p$-value of the cointegration test.
    pub p_value: f64,
    /// 1% critical value for cointegration test with constant (2 series).
    pub crit_value_1pct: f64,
    /// 5% critical value for cointegration test with constant (2 series).
    pub crit_value_5pct: f64,
    /// 10% critical value for cointegration test with constant (2 series).
    pub crit_value_10pct: f64,
    /// Whether the pair is cointegrated at the 5% significance level.
    pub is_cointegrated: bool,
    /// Half-life of mean reversion in bars (if mean-reverting).
    pub half_life: Option<f64>,
    /// Current z-score of the spread on the latest observation.
    pub current_z_score: f64,
}

/// Tests for cointegration between two time series using the Engle-Granger two-step procedure.
///
/// 1. Fits OLS regression $Y_t = \alpha + \beta X_t + \epsilon_t$.
/// 2. Tests the stationarity of residuals $\epsilon_t$ using the Dickey-Fuller test:
///    $\Delta \epsilon_t = \gamma \epsilon_{t-1} + u_t$.
/// 3. Computes critical values from MacKinnon response surface estimates and spread z-score.
pub fn test_engle_granger(series_x: &[f64], series_y: &[f64]) -> Result<CointegrationResult> {
    if series_x.is_empty() || series_y.is_empty() {
        return Err(AnalyticsError::EmptyInput);
    }
    if series_x.len() != series_y.len() {
        return Err(AnalyticsError::TradeMismatch(format!(
            "series length mismatch: {} != {}",
            series_x.len(),
            series_y.len()
        )));
    }
    let n = series_x.len();
    if n < 8 {
        return Err(AnalyticsError::InsufficientData { needed: 8, got: n });
    }

    // Step 1: OLS regression Y = alpha + beta * X
    let mean_x = mean(series_x);
    let mean_y = mean(series_y);

    let mut var_x = 0.0;
    let mut cov_xy = 0.0;
    for i in 0..n {
        let dx = series_x[i] - mean_x;
        let dy = series_y[i] - mean_y;
        var_x += dx * dx;
        cov_xy += dx * dy;
    }

    if var_x <= 1e-12 {
        return Err(AnalyticsError::ZeroVariance);
    }

    let hedge_ratio = cov_xy / var_x;
    let intercept = mean_y - hedge_ratio * mean_x;

    // Compute residuals e_t = Y_t - (alpha + beta * X_t)
    let residuals: Vec<f64> = series_x
        .iter()
        .zip(series_y.iter())
        .map(|(&x, &y)| y - (intercept + hedge_ratio * x))
        .collect();

    // Step 2: ADF test on residuals without constant (residuals already mean ~ 0)
    // Delta e_t = gamma * e_{t-1} + u_t
    let mut sum_xy = 0.0;
    let mut sum_xx = 0.0;
    let num_pairs = n - 1;

    for t in 1..n {
        let lag_e = residuals[t - 1];
        let diff_e = residuals[t] - lag_e;
        sum_xy += lag_e * diff_e;
        sum_xx += lag_e * lag_e;
    }

    let gamma = if sum_xx > 1e-14 { sum_xy / sum_xx } else { 0.0 };

    let mut ssr = 0.0;
    for t in 1..n {
        let lag_e = residuals[t - 1];
        let diff_e = residuals[t] - lag_e;
        let u = diff_e - gamma * lag_e;
        ssr += u * u;
    }

    let dof = (num_pairs.saturating_sub(1)).max(1) as f64;
    let s2 = ssr / dof;
    let se_gamma = if sum_xx > 1e-14 && s2 > 0.0 {
        (s2 / sum_xx).sqrt()
    } else {
        0.0
    };

    let adf_stat = if se_gamma > 1e-12 {
        gamma / se_gamma
    } else if gamma < 0.0 {
        -50.0
    } else {
        0.0
    };

    // MacKinnon (2010) critical values for cointegration with constant (N = 2 variables)
    // C(p, N) = beta_inf + beta_1/T + beta_2/T^2
    let t_obs = n as f64;
    let crit_1pct = -3.9001 - 10.419 / t_obs - 24.2 / (t_obs * t_obs);
    let crit_5pct = -3.3377 - 5.448 / t_obs - 7.5 / (t_obs * t_obs);
    let crit_10pct = -3.0462 - 3.421 / t_obs - 2.8 / (t_obs * t_obs);

    let is_cointegrated = adf_stat < crit_5pct;

    // Approximate p-value based on critical thresholds
    let p_value = if adf_stat <= crit_1pct {
        let excess = (crit_1pct - adf_stat).max(0.0);
        (0.01 * (-excess * 0.5).exp()).clamp(0.0001, 0.01)
    } else if adf_stat <= crit_5pct {
        0.01 + 0.04 * (adf_stat - crit_1pct) / (crit_5pct - crit_1pct)
    } else if adf_stat <= crit_10pct {
        0.05 + 0.05 * (adf_stat - crit_5pct) / (crit_10pct - crit_5pct)
    } else {
        let excess = (adf_stat - crit_10pct).max(0.0);
        (0.10 + 0.90 * (1.0 - (-excess * 0.4).exp())).clamp(0.10, 0.9999)
    };

    // Half-life: lambda = -ln(1 + gamma), half_life = ln(2) / lambda
    let half_life = if gamma < 0.0 {
        if 1.0 + gamma > 0.0 {
            let lambda = -(1.0 + gamma).ln();
            if lambda > 1e-12 {
                Some((2.0_f64.ln() / lambda).max(0.01))
            } else {
                None
            }
        } else {
            Some(0.0) // Instantaneous mean reversion
        }
    } else {
        None // Non-stationary / explosive
    };

    // Current z-score of spread
    let res_mean = mean(&residuals);
    let res_var = residuals
        .iter()
        .map(|&r| (r - res_mean).powi(2))
        .sum::<f64>()
        / (n - 1) as f64;
    let res_std = res_var.sqrt();

    let current_spread = residuals.last().copied().unwrap_or(0.0);
    let current_z_score = if res_std > 1e-12 {
        (current_spread - res_mean) / res_std
    } else {
        0.0
    };

    Ok(CointegrationResult {
        hedge_ratio,
        intercept,
        adf_stat,
        p_value,
        crit_value_1pct: crit_1pct,
        crit_value_5pct: crit_5pct,
        crit_value_10pct: crit_10pct,
        is_cointegrated,
        half_life,
        current_z_score,
    })
}
