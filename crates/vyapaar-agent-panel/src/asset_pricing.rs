//! Factor asset-pricing tests (Gibbons–Ross–Shanken joint alpha test).

use crate::error::PanelError;
use crate::matrix::{incomplete_beta, inverse, ols};

/// Result of the GRS joint test of pricing-error significance.
#[derive(Debug, Clone, PartialEq)]
pub struct GrsTestResult {
    /// The GRS F-statistic.
    pub statistic: f64,
    /// Numerator degrees of freedom (number of assets `N`).
    pub df1: usize,
    /// Denominator degrees of freedom (`T − N − K`).
    pub df2: usize,
    /// F-distribution p-value of the statistic.
    pub p_value: f64,
    /// `true` when `p_value < 0.05`.
    pub rejected_at_5_pct: bool,
    /// Estimated pricing errors (time-series intercepts) per asset.
    pub alphas: Vec<f64>,
    /// Number of periods.
    pub n_periods: usize,
}

/// Run the GRS test.
///
/// - `returns`: `T × N` matrix of asset excess returns.
/// - `factor_returns`: `T × K` matrix of factor returns.
///
/// Tests `H0: α = 0` for the K-factor model using the statistic
/// `GRS = (T−N−K)/N · α'Σ⁻¹α / (1 + μ'Ω⁻¹μ) ~ F(N, T−N−K)` under the null.
/// Requires `T > N + K + 1`.
pub fn grs_test(
    returns: &[Vec<f64>],
    factor_returns: &[Vec<f64>],
) -> Result<GrsTestResult, PanelError> {
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
    if t <= n + k + 1 {
        return Err(PanelError::InsufficientData {
            needed: n + k + 2,
            got: t,
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

    // Time-series regressions: r_i,t = α_i + β_i·f_t + ε_i,t.
    let mut alphas = vec![0.0; n];
    let mut betas = vec![vec![0.0; k]; n];
    let mut residuals = vec![vec![0.0; t]; n];
    for asset in 0..n {
        let y_ts: Vec<f64> = (0..t).map(|p| returns[p][asset]).collect();
        let x_ts: Vec<Vec<f64>> = (0..t)
            .map(|p| {
                let mut row = vec![1.0];
                row.extend_from_slice(&factor_returns[p]);
                row
            })
            .collect();
        let coef = ols(&x_ts, &y_ts)?;
        alphas[asset] = coef[0];
        betas[asset] = coef[1..].to_vec();
        for p in 0..t {
            let pred: f64 = x_ts[p].iter().zip(&coef).map(|(a, b)| a * b).sum();
            residuals[asset][p] = y_ts[p] - pred;
        }
    }

    // Residual covariance Σ = e'e / (T − K − 1).
    let df_resid = (t - k - 1) as f64;
    let mut sigma = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            sigma[i][j] = (0..t)
                .map(|p| residuals[i][p] * residuals[j][p])
                .sum::<f64>()
                / df_resid;
        }
    }
    let sigma_inv = inverse(&sigma)?;

    // Factor mean vector μ and covariance Ω.
    let mu_f: Vec<f64> = (0..k)
        .map(|f| (0..t).map(|p| factor_returns[p][f]).sum::<f64>() / t as f64)
        .collect();
    let mut omega = vec![vec![0.0; k]; k];
    for f1 in 0..k {
        for f2 in 0..k {
            omega[f1][f2] = (0..t)
                .map(|p| (factor_returns[p][f1] - mu_f[f1]) * (factor_returns[p][f2] - mu_f[f2]))
                .sum::<f64>()
                / (t - 1) as f64;
        }
    }
    let omega_inv = inverse(&omega)?;

    // GRS statistic.
    let alpha_sigma_inv_alpha: f64 = (0..n)
        .map(|i| {
            let row_sum: f64 = (0..n).map(|j| alphas[j] * sigma_inv[i][j]).sum::<f64>();
            alphas[i] * row_sum
        })
        .sum();
    let mu_omega_mu: f64 = (0..k)
        .map(|i| {
            let row_sum: f64 = (0..k).map(|j| mu_f[j] * omega_inv[i][j]).sum::<f64>();
            mu_f[i] * row_sum
        })
        .sum();
    let denominator = 1.0 + mu_omega_mu;
    if denominator <= 0.0 {
        return Err(PanelError::Singular {
            model: "grs",
            detail: "non-positive GRS denominator".to_string(),
        });
    }
    let statistic = (t - n - k) as f64 / n as f64 * alpha_sigma_inv_alpha / denominator;
    let statistic = statistic.max(0.0);

    let df1 = n;
    let df2 = t - n - k;
    // F survival: P(F > x) with (df1, df2) = 1 − I_{df1·x/(df1·x+df2)}(df1/2, df2/2).
    let f_p_value = {
        let ratio = df1 as f64 * statistic / (df1 as f64 * statistic + df2 as f64);
        let cdf = incomplete_beta(df1 as f64 / 2.0, df2 as f64 / 2.0, ratio);
        1.0 - cdf
    };
    let p_value = f_p_value.clamp(0.0, 1.0);

    Ok(GrsTestResult {
        statistic,
        df1,
        df2,
        p_value,
        rejected_at_5_pct: p_value < 0.05,
        alphas,
        n_periods: t,
    })
}
