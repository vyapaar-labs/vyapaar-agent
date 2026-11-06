//! Lag and rolling-window feature builder for the gradient-boosted forecaster.

use crate::error::ForecastError;

/// Deterministic lag + rolling-stat feature extractor.
///
/// For a series index `t`, the feature vector is:
/// - the last `lags` observations (`y[t-1] … y[t-lags]`),
/// - for each window `w`, the rolling mean and sample stdev of
///   `y[t-w … t-1]`.
///
/// The minimum usable index is the largest window (or `lags`, whichever is
/// larger); earlier indices have insufficient history.
#[derive(Debug, Clone)]
pub struct LagRollingFeatures {
    /// Number of lag observations.
    pub lags: usize,
    /// Rolling windows (each ≥ 2) whose mean/stdev become features.
    pub windows: Vec<usize>,
}

impl LagRollingFeatures {
    /// Create a feature builder.
    pub fn new(lags: usize, windows: Vec<usize>) -> Result<Self, ForecastError> {
        if lags < 1 {
            return Err(ForecastError::InvalidConfig(
                "lags must be at least 1".to_string(),
            ));
        }
        if windows.iter().any(|&w| w < 2) {
            return Err(ForecastError::InvalidConfig(
                "rolling windows must be at least 2".to_string(),
            ));
        }
        Ok(Self { lags, windows })
    }

    /// Number of observations required before features can be built.
    pub fn min_required(&self) -> usize {
        self.windows
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .max(self.lags)
    }

    /// Number of features per row: `lags + 2 * windows`.
    pub fn feature_count(&self) -> usize {
        self.lags + 2 * self.windows.len()
    }

    /// Build the feature vector for series index `t` (`t` 0-based).
    ///
    /// `t` may equal `series.len()` to build features for the *next* step
    /// from the series tail (used by recursive multi-step forecasting).
    /// Returns `None` when `t < min_required` or `t > series.len()`.
    pub fn build(&self, series: &[f64], index: usize) -> Option<Vec<f64>> {
        let min_req = self.min_required();
        if index < min_req || index > series.len() {
            return None;
        }
        let mut features = Vec::with_capacity(self.feature_count());
        for lag in 1..=self.lags {
            features.push(series[index - lag]);
        }
        for &window in &self.windows {
            let slice = &series[index - window..index];
            let mean = slice.iter().sum::<f64>() / window as f64;
            let var = if window > 1 {
                slice.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (window - 1) as f64
            } else {
                0.0
            };
            features.push(mean);
            features.push(var.sqrt());
        }
        Some(features)
    }

    /// Build the supervised design matrix `(X, y)` from a series.
    ///
    /// Row `i` has features from index `min_required + i` and target
    /// `y[min_required + i]`.
    pub fn build_matrix(&self, series: &[f64]) -> (Vec<Vec<f64>>, Vec<f64>) {
        let mut x = Vec::new();
        let mut y = Vec::new();
        for i in self.min_required()..series.len() {
            if let Some(features) = self.build(series, i) {
                x.push(features);
                y.push(series[i]);
            }
        }
        (x, y)
    }
}
