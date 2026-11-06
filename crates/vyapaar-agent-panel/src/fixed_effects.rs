//! Fixed-effects panel regression (one-way entity, or two-way entity + period).

use std::collections::HashMap;

use crate::error::PanelError;
use crate::matrix::{inverse, mean, ols};

/// Result of a fixed-effects regression.
#[derive(Debug, Clone, PartialEq)]
pub struct FixedEffectsResult {
    /// Slope coefficients (k).
    pub beta: Vec<f64>,
    /// Heteroskedasticity-consistent-ish standard errors from the within fit.
    pub se: Vec<f64>,
    /// `beta / se` per coefficient.
    pub t_stats: Vec<f64>,
    /// Within R² (on demeaned data).
    pub r_squared_within: f64,
    /// Overall R² (on original data, using within-predicted fit).
    pub r_squared_overall: f64,
    /// Number of distinct entities.
    pub n_entities: usize,
    /// Number of observations.
    pub n_observations: usize,
    /// Number of regressors.
    pub k: usize,
    /// Whether period effects were also absorbed (two-way).
    pub two_way: bool,
}

/// Run fixed-effects (within) regression on a panel.
///
/// `data` carries numeric entity / period indices per observation. With
/// `two_way = true` the period means are also removed from the
/// entity-demeaned data.
pub fn fixed_effects(
    entity: &[usize],
    period: &[usize],
    y: &[f64],
    x: &[Vec<f64>],
    two_way: bool,
) -> Result<FixedEffectsResult, PanelError> {
    let n = y.len();
    if n == 0 || entity.len() != n || period.len() != n || x.len() != n {
        return Err(PanelError::DimensionMismatch(
            "entity/period/y/x lengths must agree".to_string(),
        ));
    }
    if y.iter().any(|v| !v.is_finite()) || x.iter().flatten().any(|v| !v.is_finite()) {
        return Err(PanelError::NonFinite);
    }
    let k = x[0].len();

    let (y_dm, x_dm) = demean_panel(entity, period, y, x, two_way)?;
    let beta = ols(&x_dm, &y_dm)?;

    // Within residuals / R².
    let fitted: Vec<f64> = x_dm
        .iter()
        .map(|row| row.iter().zip(&beta).map(|(a, b)| a * b).sum())
        .collect();
    let sse: f64 = y_dm.iter().zip(&fitted).map(|(a, b)| (a - b).powi(2)).sum();
    let y_mean = mean(&y_dm);
    let sst: f64 = y_dm.iter().map(|v| (v - y_mean).powi(2)).sum();
    let r_squared_within = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };

    // Overall R² against the raw series using entity-demeaned fit + entity mean.
    let entity_mean_y: HashMap<usize, f64> = entity_means(entity, y);
    let overall_sse: f64 = (0..n)
        .map(|i| {
            let pred = fitted[i] + entity_mean_y.get(&entity[i]).copied().unwrap_or(0.0);
            (y[i] - pred).powi(2)
        })
        .sum();
    let y_total_mean = mean(y);
    let overall_sst: f64 = y.iter().map(|v| (v - y_total_mean).powi(2)).sum();
    let r_squared_overall = if overall_sst > 0.0 {
        1.0 - overall_sse / overall_sst
    } else {
        0.0
    };

    // Degrees of freedom: n - n_entities - k (one-way), n - n_entities - n_periods - k (two-way).
    let n_entities = entity
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    let df = if two_way {
        let n_periods = period
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        n.saturating_sub(n_entities + n_periods + k)
    } else {
        n.saturating_sub(n_entities + k)
    };
    let sigma2 = if df > 0 { sse / df as f64 } else { 0.0 };

    // Covariance from the within design matrix.
    let mut xtx = vec![vec![0.0; k]; k];
    for row in &x_dm {
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

    Ok(FixedEffectsResult {
        beta,
        se,
        t_stats,
        r_squared_within,
        r_squared_overall,
        n_entities,
        n_observations: n,
        k,
        two_way,
    })
}

/// Entity-demean (and optionally period-demean) the panel.
fn demean_panel(
    entity: &[usize],
    period: &[usize],
    y: &[f64],
    x: &[Vec<f64>],
    two_way: bool,
) -> Result<(Vec<f64>, Vec<Vec<f64>>), PanelError> {
    let k = x[0].len();
    let y_em = entity_means(entity, y);
    let x_em: Vec<HashMap<usize, f64>> = (0..k)
        .map(|col| entity_means(entity, &x.iter().map(|row| row[col]).collect::<Vec<_>>()))
        .collect();

    let mut y_dm: Vec<f64> = y
        .iter()
        .enumerate()
        .map(|(i, v)| v - y_em.get(&entity[i]).copied().unwrap_or(0.0))
        .collect();
    let mut x_dm: Vec<Vec<f64>> = (0..x.len())
        .map(|i| {
            (0..k)
                .map(|col| x[i][col] - x_em[col].get(&entity[i]).copied().unwrap_or(0.0))
                .collect()
        })
        .collect();

    if two_way {
        // Period-demean the already entity-demeaned data.
        let y_pm = period_means(period, &y_dm);
        let x_pm: Vec<HashMap<usize, f64>> = (0..k)
            .map(|col| period_means(period, &x_dm.iter().map(|row| row[col]).collect::<Vec<_>>()))
            .collect();
        for i in 0..y_dm.len() {
            y_dm[i] -= y_pm.get(&period[i]).copied().unwrap_or(0.0);
            for col in 0..k {
                x_dm[i][col] -= x_pm[col].get(&period[i]).copied().unwrap_or(0.0);
            }
        }
    }
    Ok((y_dm, x_dm))
}

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

/// Mean of `values` grouped by period index.
fn period_means(period: &[usize], values: &[f64]) -> HashMap<usize, f64> {
    let mut sum: HashMap<usize, f64> = HashMap::new();
    let mut count: HashMap<usize, usize> = HashMap::new();
    for (i, &p) in period.iter().enumerate() {
        *sum.entry(p).or_insert(0.0) += values[i];
        *count.entry(p).or_insert(0) += 1;
    }
    sum.into_iter()
        .map(|(p, s)| {
            let c = count.get(&p).copied().unwrap_or(1) as f64;
            (p, s / c)
        })
        .collect()
}
