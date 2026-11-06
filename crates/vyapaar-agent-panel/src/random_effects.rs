//! Random-effects panel regression (one-way, Swamy-Arora variance components).

use std::collections::HashMap;

use crate::error::PanelError;
use crate::matrix::{inverse, mean, ols};

/// Result of a random-effects (feasible GLS) regression.
#[derive(Debug, Clone, PartialEq)]
pub struct RandomEffectsResult {
    /// Slope coefficients (k).
    pub beta: Vec<f64>,
    /// Standard errors.
    pub se: Vec<f64>,
    /// `beta / se` per coefficient.
    pub t_stats: Vec<f64>,
    /// Estimated idiosyncratic error variance `σ_e²`.
    pub sigma2_e: f64,
    /// Estimated entity variance component `σ_u²`.
    pub sigma2_u: f64,
    /// Quasi-demeaning weight `θ`.
    pub theta: f64,
    /// R² on the quasi-demeaned fit.
    pub r_squared: f64,
    /// Number of observations.
    pub n_observations: usize,
    /// Number of entities.
    pub n_entities: usize,
    /// Number of regressors.
    pub k: usize,
}

/// Run one-way random-effects (feasible GLS) regression.
///
/// Variance components are estimated Swamy-Arora style: within (fixed-effects)
/// residual variance estimates `σ_e²`, and the between residual variance
/// identifies `σ_u²`. Requires a balanced panel structure — every entity must
/// appear at least twice — for stable variance-component estimates.
pub fn random_effects(
    entity: &[usize],
    y: &[f64],
    x: &[Vec<f64>],
) -> Result<RandomEffectsResult, PanelError> {
    let n = y.len();
    if n == 0 || entity.len() != n || x.len() != n {
        return Err(PanelError::DimensionMismatch(
            "entity/y/x lengths must agree".to_string(),
        ));
    }
    if y.iter().any(|v| !v.is_finite()) || x.iter().flatten().any(|v| !v.is_finite()) {
        return Err(PanelError::NonFinite);
    }
    let k = x[0].len();

    // Within (fixed-effects) estimator for sigma_e^2.
    let n_entities = entity
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    let y_em = entity_means(entity, y);
    let x_em: Vec<HashMap<usize, f64>> = (0..k)
        .map(|col| entity_means(entity, &x.iter().map(|row| row[col]).collect::<Vec<_>>()))
        .collect();
    let y_dm: Vec<f64> = y
        .iter()
        .enumerate()
        .map(|(i, v)| v - y_em.get(&entity[i]).copied().unwrap_or(0.0))
        .collect();
    let x_dm: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..k)
                .map(|col| x[i][col] - x_em[col].get(&entity[i]).copied().unwrap_or(0.0))
                .collect()
        })
        .collect();
    let beta_within = ols(&x_dm, &y_dm)?;
    let sse_within: f64 = (0..n)
        .map(|i| {
            let pred: f64 = x_dm[i].iter().zip(&beta_within).map(|(a, b)| a * b).sum();
            (y_dm[i] - pred).powi(2)
        })
        .sum();
    let df_within = n.saturating_sub(n_entities + k).max(1);
    let sigma2_e = sse_within / df_within as f64;

    // Between estimator for sigma_u^2.
    let mut y_bar = Vec::new();
    let mut x_bar: Vec<Vec<f64>> = Vec::new();
    let mut t_i = Vec::new();
    let mut entities: Vec<usize> = entity.to_vec();
    entities.sort_unstable();
    entities.dedup();
    for &e in &entities {
        y_bar.push(y_em.get(&e).copied().unwrap_or(0.0));
        x_bar.push(
            (0..k)
                .map(|col| x_em[col].get(&e).copied().unwrap_or(0.0))
                .collect(),
        );
        t_i.push(
            y.iter()
                .enumerate()
                .filter(|(i, _)| entity[*i] == e)
                .count() as f64,
        );
    }
    let n_groups = y_bar.len();
    let beta_between = ols(&x_bar, &y_bar)?;
    let sse_between: f64 = (0..n_groups)
        .map(|i| {
            let pred: f64 = x_bar[i].iter().zip(&beta_between).map(|(a, b)| a * b).sum();
            (y_bar[i] - pred).powi(2)
        })
        .sum();
    let df_between = n_groups.saturating_sub(k + 1).max(1);
    let t_bar: f64 = t_i.iter().sum::<f64>() / t_i.len().max(1) as f64;

    let sigma2_between = sse_between / df_between as f64;
    let sigma2_u = (sigma2_between - sigma2_e / t_bar).max(0.0);
    let theta = if sigma2_u > 0.0 {
        1.0 - (sigma2_e / (t_bar * sigma2_u + sigma2_e)).sqrt()
    } else {
        0.0
    };

    // Feasible GLS: quasi-demeaned OLS.
    let mut y_qd = Vec::with_capacity(n);
    let mut x_qd = Vec::with_capacity(n);
    for i in 0..n {
        let e = entity[i];
        let y_star = y[i] - theta * y_em.get(&e).copied().unwrap_or(0.0);
        let x_star: Vec<f64> = (0..k)
            .map(|col| x[i][col] - theta * x_em[col].get(&e).copied().unwrap_or(0.0))
            .collect();
        y_qd.push(y_star);
        x_qd.push(x_star);
    }
    let beta = ols(&x_qd, &y_qd)?;

    // Residual variance + standard errors on the quasi-demeaned fit.
    let sse: f64 = (0..n)
        .map(|i| {
            let pred: f64 = x_qd[i].iter().zip(&beta).map(|(a, b)| a * b).sum();
            (y_qd[i] - pred).powi(2)
        })
        .sum();
    let sigma2 = sse / n.saturating_sub(k).max(1) as f64;
    let mut xtx = vec![vec![0.0; k]; k];
    for row in &x_qd {
        for i in 0..k {
            for j in 0..k {
                xtx[i][j] += row[i] * row[j];
            }
        }
    }
    let xtx_inv = inverse(&xtx)?;
    let mut se = vec![0.0; k];
    for i in 0..k {
        se[i] = (sigma2 * xtx_inv[i][i]).max(0.0).sqrt();
    }
    let t_stats: Vec<f64> = beta
        .iter()
        .zip(&se)
        .map(|(b, s)| if *s > 0.0 { b / s } else { 0.0 })
        .collect();

    let y_mean = mean(&y_qd);
    let sst: f64 = y_qd.iter().map(|v| (v - y_mean).powi(2)).sum();
    let r_squared = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };

    Ok(RandomEffectsResult {
        beta,
        se,
        t_stats,
        sigma2_e,
        sigma2_u,
        theta,
        r_squared,
        n_observations: n,
        n_entities,
        k,
    })
}

/// Mean of `values` grouped by entity index.
fn entity_means(entity: &[usize], values: &[f64]) -> HashMap<usize, f64> {
    let mut sum: HashMap<usize, f64> = HashMap::new();
    let mut count: HashMap<usize, usize> = HashMap::new();
    for (i, &e) in entity.iter().enumerate() {
        *sum.entry(e).or_insert(0.0) += values[i];
        *count.entry(e).or_insert(0) += 1;
    }
    sum.into_iter()
        .map(|(e, s)| {
            let c = count.get(&e).copied().unwrap_or(1) as f64;
            (e, s / c)
        })
        .collect()
}
