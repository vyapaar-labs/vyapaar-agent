//! Instrumental-variables / GMM estimation via two-stage least squares (2SLS).

use crate::error::PanelError;
use crate::matrix::{chi_square_sf, inverse, mat_mul, solve, transpose};

/// Result of a 2SLS / IV regression.
#[derive(Debug, Clone, PartialEq)]
pub struct TwoStageLeastSquaresResult {
    /// Coefficient estimates `β` (k regressors).
    pub beta: Vec<f64>,
    /// Standard errors.
    pub se: Vec<f64>,
    /// `beta / se` per coefficient.
    pub t_stats: Vec<f64>,
    /// Number of observations.
    pub n: usize,
    /// Number of regressors.
    pub k: usize,
    /// Number of instruments.
    pub l: usize,
    /// Sargan / Hansen overidentification J statistic (`None` when exactly
    /// identified, i.e. `l == k`).
    pub j_stat: Option<f64>,
    /// Chi-square survival probability of the J statistic.
    pub j_p_value: Option<f64>,
}

/// Run 2SLS: `y = X β + ε` with instruments `Z` (each row one observation).
///
/// Endogenous regressors may be instrumented via the `l ≥ k` columns of `Z`
/// (which should include any exogenous regressors alongside the instruments).
/// When `l > k`, the Sargan overidentification statistic tests instrument
/// validity: `J = n·(e'P_Z e)/(e'e) ~ χ²(l − k)` under the null.
pub fn two_stage_least_squares(
    y: &[f64],
    x: &[Vec<f64>],
    z: &[Vec<f64>],
) -> Result<TwoStageLeastSquaresResult, PanelError> {
    let n = y.len();
    if n == 0 || x.len() != n || z.len() != n {
        return Err(PanelError::DimensionMismatch(
            "y/x/z lengths must agree".to_string(),
        ));
    }
    if y.iter().any(|v| !v.is_finite())
        || x.iter().flatten().any(|v| !v.is_finite())
        || z.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(PanelError::NonFinite);
    }
    let k = x[0].len();
    let l = z[0].len();
    if l < k {
        return Err(PanelError::DimensionMismatch(format!(
            "not enough instruments: l = {l} < k = {k}"
        )));
    }

    // First stage: X_hat = Z (Z'Z)^{-1} Z' X.
    let zt = transpose(z);
    let ztz = mat_mul(&zt, z)?;
    let ztz_inv = inverse(&ztz)?;
    let zt_x = mat_mul(&zt, x)?;
    let beta_first = mat_mul(&ztz_inv, &zt_x)?; // l × k
    let z_beta_first = mat_mul(z, &beta_first)?; // n × k = X_hat

    // Second stage: β = (X_hat'X)^{-1} X_hat' y.
    let x_hat_t = transpose(&z_beta_first);
    let x_hat_x = mat_mul(&x_hat_t, x)?;
    let x_hat_y: Vec<f64> = x_hat_t
        .iter()
        .map(|row| row.iter().zip(y).map(|(a, b)| a * b).sum())
        .collect();
    let beta = solve(&x_hat_x, &x_hat_y)?;

    // Residuals and error variance.
    let residuals: Vec<f64> = (0..n)
        .map(|i| {
            let pred: f64 = x[i].iter().zip(&beta).map(|(a, b)| a * b).sum();
            y[i] - pred
        })
        .collect();
    let sse: f64 = residuals.iter().map(|e| e * e).sum();
    let sigma2 = sse / n.saturating_sub(k).max(1) as f64;

    // Covariance: Var(β) = σ² (X'P_Z X)^{-1}, with P_Z = Z(Z'Z)^{-1}Z'.
    let pz_x = z_beta_first.clone(); // X_hat = P_Z X
    let xt_pz_x: Vec<Vec<f64>> = {
        let xt = transpose(x);
        mat_mul(&xt, &pz_x)?
    };
    let cov = inverse(&xt_pz_x)?;
    let mut se = vec![0.0; k];
    for i in 0..k {
        se[i] = (sigma2 * cov[i][i]).max(0.0).sqrt();
    }
    let t_stats: Vec<f64> = beta
        .iter()
        .zip(&se)
        .map(|(b, s)| if *s > 0.0 { b / s } else { 0.0 })
        .collect();

    // Sargan J statistic when over-identified.
    let (j_stat, j_p_value) = if l > k {
        // e' P_Z e = e' Z (Z'Z)^{-1} Z' e.
        let ze: Vec<f64> = zt
            .iter()
            .map(|row| row.iter().zip(&residuals).map(|(a, b)| a * b).sum())
            .collect();
        let ztz_inv_ze = solve(&ztz, &ze)?;
        let e_pz_e: f64 = ze.iter().zip(&ztz_inv_ze).map(|(a, b)| a * b).sum();
        let j = if sse > 0.0 {
            n as f64 * e_pz_e / sse
        } else {
            0.0
        };
        let df = (l - k) as f64;
        let p_value = chi_square_sf(df, j);
        (Some(j), Some(p_value))
    } else {
        (None, None)
    };

    Ok(TwoStageLeastSquaresResult {
        beta,
        se,
        t_stats,
        n,
        k,
        l,
        j_stat,
        j_p_value,
    })
}
