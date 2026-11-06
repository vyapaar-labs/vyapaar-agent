//! Exponential smoothing forecasters (ETS).
//!
//! Implements three deterministic ETS variants:
//! - simple exponential smoothing (`Ses`),
//! - Holt's linear trend (`Holt`),
//! - additive Holt-Winters with seasonality (`HoltWinters { period }`).
//!
//! Smoothing parameters are chosen by an exhaustive grid search minimizing
//! one-step-ahead squared error — deterministic, no optimizer state.

use crate::error::ForecastError;
use crate::linalg::{mean, std_dev};

/// Which exponential smoothing method to fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtsMethod {
    /// Simple exponential smoothing (no trend, no seasonality).
    Ses,
    /// Holt's linear trend.
    Holt,
    /// Additive Holt-Winters with a seasonal period in observations.
    HoltWinters {
        /// Seasonal period in observations (e.g. 5 for a weekly cycle).
        period: usize,
    },
}

/// A fitted exponential-smoothing forecaster.
#[derive(Debug, Clone)]
pub struct EtsForecaster {
    /// The smoothing method.
    pub method: EtsMethod,
    /// Level smoothing weight `α` (grid-selected).
    pub alpha: f64,
    /// Trend smoothing weight `β` (Holt / Holt-Winters).
    pub beta: f64,
    /// Seasonal smoothing weight `γ` (Holt-Winters).
    pub gamma: f64,
    /// Final smoothed level.
    level: f64,
    /// Final smoothed trend.
    trend: f64,
    /// Seasonal components indexed `0..period` (Holt-Winters).
    seasonals: Vec<f64>,
    /// Seasonal period (Holt-Winters).
    period: usize,
    /// Number of fitted observations (for seasonal index alignment).
    n_obs: usize,
    /// Residual standard deviation (one-step-ahead fit errors).
    pub resid_std: f64,
}

impl EtsForecaster {
    /// Create an untrained forecaster for the given method.
    pub fn new(method: EtsMethod) -> Self {
        Self {
            method,
            alpha: 0.5,
            beta: 0.1,
            gamma: 0.1,
            level: 0.0,
            trend: 0.0,
            seasonals: Vec::new(),
            period: 0,
            n_obs: 0,
            resid_std: 0.0,
        }
    }

    /// Fit the smoothing model to a series.
    pub fn fit(&mut self, y: &[f64]) -> Result<(), ForecastError> {
        if y.is_empty() || y.iter().any(|v| !v.is_finite()) {
            return Err(if y.is_empty() {
                ForecastError::InsufficientData { needed: 1, got: 0 }
            } else {
                ForecastError::NonFinite
            });
        }
        self.n_obs = y.len();

        match self.method {
            EtsMethod::Ses => self.fit_ses(y)?,
            EtsMethod::Holt => self.fit_holt(y)?,
            EtsMethod::HoltWinters { period } => self.fit_holt_winters(y, period)?,
        }
        Ok(())
    }

    /// Produce `horizon` step forecasts.
    pub fn forecast(&self, horizon: usize) -> Result<Vec<f64>, ForecastError> {
        if horizon == 0 {
            return Ok(Vec::new());
        }
        match self.method {
            EtsMethod::Ses => Ok(vec![self.level; horizon]),
            EtsMethod::Holt => Ok((0..horizon)
                .map(|h| self.level + (h as f64 + 1.0) * self.trend)
                .collect()),
            EtsMethod::HoltWinters { .. } => {
                if self.period == 0 {
                    return Err(ForecastError::InvalidConfig(
                        "Holt-Winters period not fitted".to_string(),
                    ));
                }
                Ok((0..horizon)
                    .map(|h| {
                        let s_idx = (self.n_obs + h) % self.period;
                        self.level + (h as f64 + 1.0) * self.trend + self.seasonals[s_idx]
                    })
                    .collect())
            }
        }
    }

    fn fit_ses(&mut self, y: &[f64]) -> Result<(), ForecastError> {
        let mut best_alpha = 0.5;
        let mut best_sse = f64::INFINITY;
        let mut best_level = mean(y);
        let mut best_resid = Vec::new();
        for alpha in grid() {
            let (levels, sse, resid) = ses_smooth(alpha, y);
            if sse < best_sse {
                best_alpha = alpha;
                best_sse = sse;
                best_level = *levels.last().unwrap_or(&mean(y));
                best_resid = resid;
            }
        }
        self.alpha = best_alpha;
        self.beta = 0.0;
        self.gamma = 0.0;
        self.level = best_level;
        self.trend = 0.0;
        self.resid_std = std_dev(&best_resid);
        Ok(())
    }

    fn fit_holt(&mut self, y: &[f64]) -> Result<(), ForecastError> {
        let mut best = (0.5, 0.1);
        let mut best_sse = f64::INFINITY;
        let mut best_state = (mean(y), 0.0);
        let mut best_resid = Vec::new();
        for &alpha in &grid() {
            for &beta in &grid() {
                let (level, trend, sse, resid) = holt_smooth(alpha, beta, y);
                if sse < best_sse {
                    best = (alpha, beta);
                    best_sse = sse;
                    best_state = (level, trend);
                    best_resid = resid;
                }
            }
        }
        self.alpha = best.0;
        self.beta = best.1;
        self.gamma = 0.0;
        self.level = best_state.0;
        self.trend = best_state.1;
        self.resid_std = std_dev(&best_resid);
        Ok(())
    }

    fn fit_holt_winters(&mut self, y: &[f64], period: usize) -> Result<(), ForecastError> {
        if period < 2 {
            return Err(ForecastError::InvalidConfig(
                "Holt-Winters period must be at least 2".to_string(),
            ));
        }
        if y.len() < period {
            return Err(ForecastError::InsufficientData {
                needed: period,
                got: y.len(),
            });
        }
        let mut best = (0.5, 0.1, 0.1);
        let mut best_sse = f64::INFINITY;
        let mut best_state = (mean(y), 0.0, vec![0.0; period]);
        let mut best_resid = Vec::new();
        for &alpha in &grid() {
            for &beta in &grid() {
                for &gamma in &grid() {
                    let (level, trend, seasonals, sse, resid) =
                        holt_winters_smooth(alpha, beta, gamma, period, y);
                    if sse < best_sse {
                        best = (alpha, beta, gamma);
                        best_sse = sse;
                        best_state = (level, trend, seasonals);
                        best_resid = resid;
                    }
                }
            }
        }
        self.alpha = best.0;
        self.beta = best.1;
        self.gamma = best.2;
        self.level = best_state.0;
        self.trend = best_state.1;
        self.seasonals = best_state.2;
        self.period = period;
        self.resid_std = std_dev(&best_resid);
        Ok(())
    }
}

/// Grid of candidate smoothing weights.
fn grid() -> Vec<f64> {
    (1..=19).map(|i| i as f64 / 20.0).collect()
}

/// Simple exponential smoothing: returns levels, SSE, and one-step residuals.
fn ses_smooth(alpha: f64, y: &[f64]) -> (Vec<f64>, f64, Vec<f64>) {
    let mut level = y[0];
    let mut levels = vec![level];
    let mut sse = 0.0;
    let mut resid = Vec::new();
    for &yi in y.iter().skip(1) {
        let pred = level;
        let e = yi - pred;
        sse += e * e;
        resid.push(e);
        level = alpha * yi + (1.0 - alpha) * level;
        levels.push(level);
    }
    (levels, sse, resid)
}

/// Holt's linear trend: returns `(level, trend, sse, residuals)`.
fn holt_smooth(alpha: f64, beta: f64, y: &[f64]) -> (f64, f64, f64, Vec<f64>) {
    let mut level = y[0];
    let mut trend = if y.len() > 1 { y[1] - y[0] } else { 0.0 };
    let mut sse = 0.0;
    let mut resid = Vec::new();
    for &yi in y.iter().skip(1) {
        let pred = level + trend;
        let e = yi - pred;
        sse += e * e;
        resid.push(e);
        let new_level = alpha * yi + (1.0 - alpha) * (level + trend);
        trend = beta * (new_level - level) + (1.0 - beta) * trend;
        level = new_level;
    }
    (level, trend, sse, resid)
}

/// Additive Holt-Winters: returns `(level, trend, seasonals, sse, residuals)`.
fn holt_winters_smooth(
    alpha: f64,
    beta: f64,
    gamma: f64,
    period: usize,
    y: &[f64],
) -> (f64, f64, Vec<f64>, f64, Vec<f64>) {
    let base = mean(&y[..period.min(y.len())]);
    let mut seasonals: Vec<f64> = y[..period.min(y.len())].iter().map(|v| v - base).collect();
    while seasonals.len() < period {
        seasonals.push(0.0);
    }
    let mut level = y[0];
    let mut trend = if y.len() > period {
        (y[period] - y[0]) / period as f64
    } else {
        0.0
    };
    let mut sse = 0.0;
    let mut resid = Vec::new();
    for (i, &yi) in y.iter().enumerate() {
        let s_idx = i % period;
        if i > 0 {
            let pred = level + trend + seasonals[s_idx];
            let e = yi - pred;
            sse += e * e;
            resid.push(e);
        }
        let new_level = alpha * (yi - seasonals[s_idx]) + (1.0 - alpha) * (level + trend);
        trend = beta * (new_level - level) + (1.0 - beta) * trend;
        seasonals[s_idx] = gamma * (yi - new_level) + (1.0 - gamma) * seasonals[s_idx];
        level = new_level;
    }
    (level, trend, seasonals, sse, resid)
}
