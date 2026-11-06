//! Simplified TBATS forecaster (Box-Cox + Fourier seasonal + smoothing).
//!
//! TBATS (De Livera, Hyndman & Snyder 2011) combines Box-Cox transformation,
//! Fourier-seasonal terms, an ARMA error, trend, and seasonality. This is a
//! deliberately simplified, deterministic subset of that model:
//!
//! - Box-Cox: λ = 0 (log) when every observation is positive, identity otherwise.
//! - Seasonality: `K` Fourier pairs at the configured period, fitted by OLS.
//! - Trend/level: simple exponential smoothing on the de-seasonalised series.
//!
//! The full TBATS ARMA-error and multiple-seasonality machinery is out of
//! scope for the crate's first slice; the model is documented as such so
//! short-history and periodic series still get a usable seasonal forecast.

use crate::error::ForecastError;
use crate::linalg::{ols, std_dev};

/// Configuration for the simplified TBATS model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TbatsConfig {
    /// Seasonal period in observations (e.g. 5 for a weekly cycle of dailies).
    pub seasonal_period: usize,
    /// Number of Fourier harmonic pairs (clamped to `period / 2`).
    pub harmonics: usize,
}

impl Default for TbatsConfig {
    fn default() -> Self {
        Self {
            seasonal_period: 5,
            harmonics: 2,
        }
    }
}

/// A fitted simplified TBATS forecaster.
#[derive(Debug, Clone)]
pub struct TbatsForecaster {
    /// Model configuration.
    pub config: TbatsConfig,
    /// Effective number of Fourier pairs.
    k: usize,
    /// Whether the Box-Cox transform was log.
    use_log: bool,
    /// Smoothed level of the de-seasonalised series.
    level: f64,
    /// Fourier cosine coefficients.
    cos_beta: Vec<f64>,
    /// Fourier sine coefficients.
    sin_beta: Vec<f64>,
    /// Number of fitted observations.
    n: usize,
    /// Residual standard deviation in transform space.
    pub resid_std: f64,
}

impl TbatsForecaster {
    /// Create an untrained forecaster.
    pub fn new(config: TbatsConfig) -> Self {
        Self {
            config,
            k: 0,
            use_log: false,
            level: 0.0,
            cos_beta: Vec::new(),
            sin_beta: Vec::new(),
            n: 0,
            resid_std: 0.0,
        }
    }

    /// Fit the model to a series.
    pub fn fit(&mut self, y: &[f64]) -> Result<(), ForecastError> {
        let m = self.config.seasonal_period;
        if m < 2 {
            return Err(ForecastError::InvalidConfig(
                "TBATS seasonal period must be at least 2".to_string(),
            ));
        }
        if y.len() < 2 * m {
            return Err(ForecastError::InsufficientData {
                needed: 2 * m,
                got: y.len(),
            });
        }
        if y.iter().any(|v| !v.is_finite()) {
            return Err(ForecastError::NonFinite);
        }

        let use_log = y.iter().all(|&v| v > 0.0);
        let yt: Vec<f64> = if use_log {
            y.iter().map(|v| v.ln()).collect()
        } else {
            y.to_vec()
        };
        let n = yt.len();
        let k = self.config.harmonics.max(1).min(m / 2).max(1);

        // Design matrix: [intercept, cos₁, sin₁, …, cos_K, sin_K].
        let x: Vec<Vec<f64>> = (0..n)
            .map(|t| {
                let mut row = vec![1.0];
                for h in 1..=k {
                    let omega = 2.0 * std::f64::consts::PI * h as f64 / m as f64;
                    row.push((omega * t as f64).cos());
                    row.push((omega * t as f64).sin());
                }
                row
            })
            .collect();

        let beta = ols(&x, &yt)?;
        let intercept = beta[0];
        let mut cos_beta = Vec::with_capacity(k);
        let mut sin_beta = Vec::with_capacity(k);
        for h in 0..k {
            cos_beta.push(beta[1 + 2 * h]);
            sin_beta.push(beta[2 + 2 * h]);
        }

        // De-seasonalise (remove Fourier + intercept) and smooth the level.
        let deseasonalised: Vec<f64> = (0..n)
            .map(|t| {
                let mut seasonal = 0.0;
                for h in 0..k {
                    let omega = 2.0 * std::f64::consts::PI * (h + 1) as f64 / m as f64;
                    seasonal += cos_beta[h] * (omega * t as f64).cos()
                        + sin_beta[h] * (omega * t as f64).sin();
                }
                yt[t] - intercept - seasonal
            })
            .collect();

        let level = ses_level(&deseasonalised);

        // Residuals in transform space around the full fit.
        let fitted: Vec<f64> = (0..n)
            .map(|t| {
                let mut seasonal = 0.0;
                for h in 0..k {
                    let omega = 2.0 * std::f64::consts::PI * (h + 1) as f64 / m as f64;
                    seasonal += cos_beta[h] * (omega * t as f64).cos()
                        + sin_beta[h] * (omega * t as f64).sin();
                }
                intercept + seasonal + level
            })
            .collect();
        let residuals: Vec<f64> = (0..n).map(|t| yt[t] - fitted[t]).collect();

        self.k = k;
        self.use_log = use_log;
        self.level = level;
        self.cos_beta = cos_beta;
        self.sin_beta = sin_beta;
        self.n = n;
        self.resid_std = std_dev(&residuals);
        Ok(())
    }

    /// Produce `horizon` step forecasts in the original scale.
    pub fn forecast(&self, horizon: usize) -> Result<Vec<f64>, ForecastError> {
        if self.n == 0 {
            return Err(ForecastError::InvalidConfig(
                "TBATS forecaster not fitted".to_string(),
            ));
        }
        let m = self.config.seasonal_period;
        let k = self.k;
        let mut out = Vec::with_capacity(horizon);
        for step in 0..horizon {
            let t = (self.n + step) as f64;
            let mut seasonal = 0.0;
            for h in 0..k {
                let omega = 2.0 * std::f64::consts::PI * (h + 1) as f64 / m as f64;
                seasonal +=
                    self.cos_beta[h] * (omega * t).cos() + self.sin_beta[h] * (omega * t).sin();
            }
            let value = self.level + seasonal;
            out.push(if self.use_log { value.exp() } else { value });
        }
        Ok(out)
    }
}

/// Final smoothed level under simple exponential smoothing (α = 0.3).
fn ses_level(series: &[f64]) -> f64 {
    let mut level = series[0];
    for &v in series.iter().skip(1) {
        level = 0.3 * v + 0.7 * level;
    }
    level
}
