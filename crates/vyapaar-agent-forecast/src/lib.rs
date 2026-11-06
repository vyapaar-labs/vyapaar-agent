//! # व्यापार・予測 vyapaar-agent-forecast
//!
//! Time-series forecasting engines for the Vyapaar research loop and strategy
//! signals (plan 0003 Phase 1).
//!
//! Two families of forecasters, all deterministic:
//!
//! - **ML**: gradient-boosted regression trees over lag and rolling-window
//!   features of the input series.
//! - **Classical statistical**: ARIMA(p,d,q), ETS (SES / Holt /
//!   Holt-Winters), Theta, and a simplified TBATS.
//!
//! Scope boundary: this crate forecasts *raw series* — prices, volumes, macro
//! indicators — not strategy rules (`vyapaar-agent-genetic`) and not strategy
//! parameters (`vyapaar-runtime-enhancer`). A forecast produced here may feed
//! either of those as an input signal, but this crate has no dependency on
//! either.
//!
//! # Numeric policy
//!
//! The public API takes [`vyapaar_core::QuantSeries`] (exact `Decimal`) and
//! returns [`Decimal`] point forecasts and intervals. The statistical engines
//! compute in `f64` internally (matching the `vyapaar-agent-analytics`
//! convention for return/statistics math); no money/price/quantity arithmetic
//! crosses the crate boundary as a float.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unsafe_code)]

pub mod arima;
pub mod error;
pub mod ets;
pub mod features;
pub mod gradient_boost;
pub mod linalg;
pub mod tbats;
pub mod theta;

use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use vyapaar_core::{InstrumentId, QuantSeries};

pub use arima::{ArimaForecaster, ArimaOrder};
pub use error::ForecastError;
pub use ets::{EtsForecaster, EtsMethod};
pub use features::LagRollingFeatures;
pub use gradient_boost::{GradientBoostedRegressor, RegressionTree, RegressionTreeConfig};
pub use tbats::{TbatsConfig, TbatsForecaster};
pub use theta::ThetaForecaster;

/// Which forecast engine to use.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "params")]
pub enum ForecastModel {
    /// Theta method (SES on the θ=2 line + drift combination).
    #[default]
    Theta,
    /// Simple exponential smoothing.
    Ets,
    /// Additive Holt-Winters with a seasonal period.
    EtsHw {
        /// Seasonal period in observations.
        period: usize,
    },
    /// ARIMA(p, d, q).
    Arima {
        /// Autoregressive order.
        p: usize,
        /// Differencing order.
        d: usize,
        /// Moving-average order.
        q: usize,
    },
    /// Simplified TBATS with a seasonal period.
    Tbats {
        /// Seasonal period in observations.
        seasonal_period: usize,
    },
    /// Gradient-boosted trees over lag/rolling features.
    GradientBoost {
        /// Number of lag observations per feature row.
        lags: usize,
        /// Number of boosting rounds.
        n_estimators: usize,
        /// Maximum tree depth.
        max_depth: usize,
        /// Learning-rate shrinkage.
        learning_rate: f64,
    },
}

/// Configuration for a forecast run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ForecastConfig {
    /// Number of steps to forecast.
    pub horizon: usize,
    /// The forecast engine to use.
    #[serde(default)]
    pub model: ForecastModel,
    /// Two-sided prediction-interval probability in `(0, 1)` (e.g. `0.80`).
    #[serde(default = "default_interval_pct")]
    pub prediction_interval_pct: f64,
}

fn default_interval_pct() -> f64 {
    0.80
}

impl Default for ForecastConfig {
    fn default() -> Self {
        Self {
            horizon: 10,
            model: ForecastModel::Theta,
            prediction_interval_pct: 0.80,
        }
    }
}

/// Point forecasts at steps `1..=horizon`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointForecast {
    /// Forecast step numbers (1-based).
    pub steps: Vec<u32>,
    /// Point forecast values.
    pub values: Vec<Decimal>,
}

/// Symmetric prediction interval around the point forecast.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PredictionInterval {
    /// Lower bounds per step.
    pub lower: Vec<Decimal>,
    /// Upper bounds per step.
    pub upper: Vec<Decimal>,
}

/// Complete forecast output for one series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastOutput {
    /// The series this forecast is for, when run as part of a panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_key: Option<InstrumentId>,
    /// The engine used.
    pub model: ForecastModel,
    /// Requested horizon.
    pub horizon: usize,
    /// Prediction-interval probability used.
    pub prediction_interval_pct: f64,
    /// Point forecasts.
    pub point: PointForecast,
    /// Prediction interval.
    pub interval: Option<PredictionInterval>,
    /// Residual standard deviation of the fit (in series units).
    pub resid_std: Decimal,
}

/// A single series' forecast within a panel run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelForecast {
    /// Series identifier.
    pub series_key: InstrumentId,
    /// The forecast output for that series.
    pub output: ForecastOutput,
}

/// Fit a model and forecast a single series.
pub fn fit_and_forecast(
    series: &QuantSeries,
    config: &ForecastConfig,
) -> Result<ForecastOutput, ForecastError> {
    if config.horizon == 0 {
        return Err(ForecastError::InvalidConfig(
            "horizon must be at least 1".to_string(),
        ));
    }
    let pct = config.prediction_interval_pct;
    if !(pct > 0.0 && pct < 1.0) {
        return Err(ForecastError::InvalidConfig(
            "prediction_interval_pct must lie in (0, 1)".to_string(),
        ));
    }
    let y = quant_to_f64(series);
    let (point_values, resid_std) = forecast_values(&y, config, config.model)?;
    let interval = build_interval(&point_values, resid_std, pct, config.horizon);
    let point = PointForecast {
        steps: (1..=config.horizon).map(|s| s as u32).collect(),
        values: f64_to_decimals(&point_values),
    };

    Ok(ForecastOutput {
        series_key: None,
        model: config.model,
        horizon: config.horizon,
        prediction_interval_pct: pct,
        point,
        interval,
        resid_std: f64_to_decimal(resid_std),
    })
}

/// Fit and forecast each series in a panel.
///
/// Each series is fitted independently with the same configuration; a series
/// that is too short for its model raises an error (no silent dropping).
pub fn fit_and_forecast_panel(
    series: &[(InstrumentId, QuantSeries)],
    config: &ForecastConfig,
) -> Result<Vec<PanelForecast>, ForecastError> {
    series
        .iter()
        .map(|(key, s)| {
            let mut output = fit_and_forecast(s, config)?;
            output.series_key = Some(key.clone());
            Ok(PanelForecast {
                series_key: key.clone(),
                output,
            })
        })
        .collect()
}

/// Run the selected engine and return `(point_values, resid_std)`.
fn forecast_values(
    y: &[f64],
    config: &ForecastConfig,
    model: ForecastModel,
) -> Result<(Vec<f64>, f64), ForecastError> {
    let horizon = config.horizon;
    match model {
        ForecastModel::Theta => {
            let mut forecaster = ThetaForecaster::new();
            forecaster.fit(y)?;
            let point = forecaster.forecast(horizon)?;
            Ok((point, forecaster.resid_std))
        }
        ForecastModel::Ets => {
            let mut forecaster = EtsForecaster::new(EtsMethod::Ses);
            forecaster.fit(y)?;
            let point = forecaster.forecast(horizon)?;
            Ok((point, forecaster.resid_std))
        }
        ForecastModel::EtsHw { period } => {
            let mut forecaster = EtsForecaster::new(EtsMethod::HoltWinters { period });
            forecaster.fit(y)?;
            let point = forecaster.forecast(horizon)?;
            Ok((point, forecaster.resid_std))
        }
        ForecastModel::Arima { p, d, q } => {
            let mut forecaster = ArimaForecaster::new(ArimaOrder { p, d, q });
            forecaster.fit(y)?;
            let point = forecaster.forecast(horizon)?;
            Ok((point, forecaster.resid_std))
        }
        ForecastModel::Tbats { seasonal_period } => {
            let mut forecaster = TbatsForecaster::new(TbatsConfig {
                seasonal_period,
                harmonics: 2,
            });
            forecaster.fit(y)?;
            let point = forecaster.forecast(horizon)?;
            Ok((point, forecaster.resid_std))
        }
        ForecastModel::GradientBoost {
            lags,
            n_estimators,
            max_depth,
            learning_rate,
        } => forecast_gradient_boost(y, horizon, lags, n_estimators, max_depth, learning_rate),
    }
}

/// Recursive multi-step forecast with the gradient-boosted tree ensemble.
fn forecast_gradient_boost(
    y: &[f64],
    horizon: usize,
    lags: usize,
    n_estimators: usize,
    max_depth: usize,
    learning_rate: f64,
) -> Result<(Vec<f64>, f64), ForecastError> {
    let feature_builder =
        LagRollingFeatures::new(lags, vec![lags.max(2), lags.saturating_mul(2).max(3)])?;
    let (x, yt) = feature_builder.build_matrix(y);
    if x.len() < 2 {
        return Err(ForecastError::InsufficientData {
            needed: feature_builder.min_required() + 2,
            got: y.len(),
        });
    }
    let mut model = GradientBoostedRegressor::new(n_estimators, learning_rate, max_depth, 2)?;
    model.fit(&x, &yt)?;

    // In-sample residual standard deviation.
    let preds = model.predict_rows(&x);
    let resid_std = std_dev_of(&preds, &yt);

    // Recursive forecast: append each prediction and rebuild features.
    let mut augmented = y.to_vec();
    let mut point = Vec::with_capacity(horizon);
    for _ in 0..horizon {
        let features = feature_builder
            .build(&augmented, augmented.len())
            .ok_or_else(|| {
                ForecastError::InvalidConfig("feature index out of range".to_string())
            })?;
        let pred = model.predict(&features);
        augmented.push(pred);
        point.push(pred);
    }
    Ok((point, resid_std))
}

/// Standard deviation of residuals `y - y_hat`.
fn std_dev_of(preds: &[f64], y: &[f64]) -> f64 {
    if preds.is_empty() {
        return 0.0;
    }
    let n = preds.len() as f64;
    let sse: f64 = preds
        .iter()
        .zip(y.iter())
        .map(|(p, yv)| (yv - p).powi(2))
        .sum();
    (sse / (n - 1.0)).max(0.0).sqrt()
}

/// Build a symmetric normal prediction interval around the point forecast.
fn build_interval(
    point: &[f64],
    resid_std: f64,
    pct: f64,
    horizon: usize,
) -> Option<PredictionInterval> {
    if point.len() != horizon || resid_std <= 0.0 {
        return None;
    }
    let z = linalg::norm_ppf(0.5 + pct / 2.0);
    let mut lower = Vec::with_capacity(horizon);
    let mut upper = Vec::with_capacity(horizon);
    for (step, &value) in point.iter().enumerate() {
        let half_width = z * resid_std * ((step + 1) as f64).sqrt();
        lower.push(f64_to_decimal(value - half_width));
        upper.push(f64_to_decimal(value + half_width));
    }
    Some(PredictionInterval { lower, upper })
}

/// Convert an exact [`QuantSeries`] to internal `f64` working values.
fn quant_to_f64(series: &QuantSeries) -> Vec<f64> {
    series
        .as_slice()
        .iter()
        .filter_map(|d| d.to_f64())
        .collect()
}

/// Convert internal `f64` values back to exact [`Decimal`] outputs.
fn f64_to_decimals(values: &[f64]) -> Vec<Decimal> {
    values.iter().map(|v| f64_to_decimal(*v)).collect()
}

/// Convert one internal `f64` value back to exact [`Decimal`].
fn f64_to_decimal(value: f64) -> Decimal {
    Decimal::from_f64(value).unwrap_or_default()
}
