//! Theta forecaster (Assimakopoulos & Nikolopoulos, M3-competition winner).
//!
//! Deterministic implementation following the well-known `thetaf` recipe:
//! 1. Fit a linear trend to the whole series (time-index regression).
//! 2. Build the θ=2 theta line `y_t + b·(t−1)` and smooth it with simple
//!    exponential smoothing (α = 0.5).
//! 3. Fit a drift line (linear regression) to the second half of the series.
//! 4. Combine: half the SES forecast of the θ=2 line, half the extrapolated
//!    drift line.

use crate::error::ForecastError;
use crate::linalg::{ols, std_dev};

/// A fitted Theta forecaster.
#[derive(Debug, Clone)]
pub struct ThetaForecaster {
    /// SES level of the θ=2 line.
    ses_level: f64,
    /// Drift-line intercept (second-half regression).
    drift_intercept: f64,
    /// Drift-line slope.
    drift_slope: f64,
    /// Number of fitted observations.
    n: usize,
    /// Residual standard deviation around the whole-series trend line.
    pub resid_std: f64,
}

impl ThetaForecaster {
    /// Create an untrained Theta forecaster.
    pub fn new() -> Self {
        Self {
            ses_level: 0.0,
            drift_intercept: 0.0,
            drift_slope: 0.0,
            n: 0,
            resid_std: 0.0,
        }
    }

    /// Fit the Theta model to a series (requires at least 3 observations).
    pub fn fit(&mut self, y: &[f64]) -> Result<(), ForecastError> {
        if y.len() < 3 {
            return Err(ForecastError::InsufficientData {
                needed: 3,
                got: y.len(),
            });
        }
        if y.iter().any(|v| !v.is_finite()) {
            return Err(ForecastError::NonFinite);
        }
        let n = y.len();
        let t: Vec<f64> = (0..n).map(|i| i as f64).collect();

        // Whole-series linear trend.
        let x: Vec<Vec<f64>> = t.iter().map(|&ti| vec![1.0, ti]).collect();
        let coef = ols(&x, y)?;
        let intercept = coef[0];
        let slope = coef[1];

        // θ=2 theta line and its SES forecast (α = 0.5).
        let theta_line: Vec<f64> = y
            .iter()
            .enumerate()
            .map(|(i, &v)| v + slope * i as f64)
            .collect();
        let ses_level = ses_forecast(0.5, &theta_line);

        // Drift line from the second half.
        let half = n / 2;
        let t_half: Vec<f64> = (half..n).map(|i| i as f64).collect();
        let x_half: Vec<Vec<f64>> = t_half.iter().map(|&ti| vec![1.0, ti]).collect();
        let coef_half = ols(&x_half, &y[half..])?;

        // Residuals around the whole-series trend (for the interval).
        let residuals: Vec<f64> = y
            .iter()
            .enumerate()
            .map(|(i, &v)| v - (intercept + slope * i as f64))
            .collect();

        self.ses_level = ses_level;
        self.drift_intercept = coef_half[0];
        self.drift_slope = coef_half[1];
        self.n = n;
        self.resid_std = std_dev(&residuals);
        Ok(())
    }

    /// Produce `horizon` step forecasts.
    pub fn forecast(&self, horizon: usize) -> Result<Vec<f64>, ForecastError> {
        if self.n == 0 {
            return Err(ForecastError::InvalidConfig(
                "Theta forecaster not fitted".to_string(),
            ));
        }
        Ok((0..horizon)
            .map(|k| {
                let t_future = (self.n - 1 + k) as f64;
                0.5 * self.ses_level + 0.5 * (self.drift_intercept + self.drift_slope * t_future)
            })
            .collect())
    }
}

impl Default for ThetaForecaster {
    fn default() -> Self {
        Self::new()
    }
}

/// Final smoothed level of a series under simple exponential smoothing.
fn ses_forecast(alpha: f64, y: &[f64]) -> f64 {
    let mut level = y[0];
    for &yi in y.iter().skip(1) {
        level = alpha * yi + (1.0 - alpha) * level;
    }
    level
}
