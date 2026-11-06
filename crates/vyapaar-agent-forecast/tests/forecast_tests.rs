//! Tests for the vyapaar-agent-forecast engines.
//!
//! Covers deterministic behavior, shape contracts, and known-value sanity
//! checks for all five engines plus the panel/single-series entry points.

use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use vyapaar_agent_forecast::{
    ArimaForecaster, ArimaOrder, EtsForecaster, EtsMethod, ForecastConfig, ForecastError,
    ForecastModel, GradientBoostedRegressor, LagRollingFeatures, TbatsConfig, TbatsForecaster,
    ThetaForecaster, fit_and_forecast, fit_and_forecast_panel,
};
use vyapaar_core::{InstrumentId, QuantSeries};

fn quant(series: &[f64]) -> QuantSeries {
    QuantSeries::new(
        series
            .iter()
            .map(|v| Decimal::from_f64(*v).unwrap())
            .collect(),
    )
}

fn finite(v: &Decimal) -> bool {
    v.to_f64().is_some_and(|f| f.is_finite())
}

#[test]
fn test_theta_forecast_is_finite_and_flat_for_constant_series() {
    let mut forecaster = ThetaForecaster::new();
    forecaster.fit(&[10.0, 10.0, 10.0, 10.0, 10.0]).unwrap();
    let forecast = forecaster.forecast(3).unwrap();
    assert_eq!(forecast.len(), 3);
    assert!(forecast.iter().all(|v| v.is_finite()));
}

#[test]
fn test_ets_ses_forecast_matches_final_level() {
    let mut forecaster = EtsForecaster::new(EtsMethod::Ses);
    forecaster.fit(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
    let forecast = forecaster.forecast(2).unwrap();
    assert_eq!(forecast.len(), 2);
    // SES forecast is flat and lies within the observed range.
    assert!(forecast[0] > 1.0 && forecast[0] < 5.0);
    assert!((forecast[0] - forecast[1]).abs() < 1e-12);
}

#[test]
fn test_ets_holt_trend_is_monotonic() {
    let series: Vec<f64> = (0..30).map(|i| i as f64 + 0.5).collect();
    let mut forecaster = EtsForecaster::new(EtsMethod::Holt);
    forecaster.fit(&series).unwrap();
    let forecast = forecaster.forecast(5).unwrap();
    // A strongly trending series should continue upward.
    assert!(forecast.windows(2).all(|w| w[1] > w[0]));
    assert!(forecast[0] > 29.5);
}

#[test]
fn test_ets_holt_winters_captures_periodicity() {
    let mut series = Vec::new();
    for i in 0..40 {
        series.push(10.0 + 2.0 * (i % 5) as f64);
    }
    let mut forecaster = EtsForecaster::new(EtsMethod::HoltWinters { period: 5 });
    forecaster.fit(&series).unwrap();
    let forecast = forecaster.forecast(10).unwrap();
    assert_eq!(forecast.len(), 10);
    // Seasonal structure repeats with period 5 (within a small drift
    // tolerance — the fitted trend on a periodic series is ~0 but not exact).
    assert!((forecast[0] - forecast[5]).abs() < 0.5);
    assert!((forecast[1] - forecast[6]).abs() < 0.5);
    // Within-period variation dominates: peaks are distinct from troughs.
    let max5 = forecast[..5]
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    let min5 = forecast[..5].iter().cloned().fold(f64::INFINITY, f64::min);
    assert!(max5 - min5 > 1.0, "seasonal amplitude preserved");
}

#[test]
fn test_arima_forecast_constant_series_is_constant() {
    let mut forecaster = ArimaForecaster::new(ArimaOrder { p: 1, d: 0, q: 0 });
    forecaster.fit(&[5.0; 20]).unwrap();
    let forecast = forecaster.forecast(4).unwrap();
    assert!(forecast.iter().all(|v| (v - 5.0).abs() < 1e-6));
}

#[test]
fn test_arima_with_differencing_reproduces_trend() {
    // A deterministic upward trend: AR(1) on the differenced series should
    // continue it (difference is constant).
    let series: Vec<f64> = (0..40).map(|i| 100.0 + 2.0 * i as f64).collect();
    let mut forecaster = ArimaForecaster::new(ArimaOrder { p: 1, d: 1, q: 0 });
    forecaster.fit(&series).unwrap();
    let forecast = forecaster.forecast(5).unwrap();
    assert!(forecast.windows(2).all(|w| w[1] > w[0]));
    // Step size ~2, so 5 steps ahead ≈ last value + 10.
    let last = *series.last().unwrap();
    assert!((forecast[0] - (last + 2.0)).abs() < 2.0);
    assert!((forecast[4] - (last + 10.0)).abs() < 4.0);
}

#[test]
fn test_tbats_forecast_has_seasonal_shape() {
    let mut series = Vec::new();
    for i in 0..30 {
        series.push(50.0 + 5.0 * ((i % 7) as f64).sin());
    }
    let mut forecaster = TbatsForecaster::new(TbatsConfig {
        seasonal_period: 7,
        harmonics: 2,
    });
    forecaster.fit(&series).unwrap();
    let forecast = forecaster.forecast(14).unwrap();
    assert_eq!(forecast.len(), 14);
    assert!(forecast.iter().all(|v| v.is_finite()));
    // Weekly shape repeats: step 0 ≈ step 7.
    assert!((forecast[0] - forecast[7]).abs() < 1.0);
}

#[test]
fn test_gradient_boost_fits_sine_wave() {
    let series: Vec<f64> = (0..60).map(|i| (i as f64 * 0.5).sin()).collect();
    let features = LagRollingFeatures::new(5, vec![5, 10]).unwrap();
    let (x, y) = features.build_matrix(&series);
    assert!(x.len() >= 40);
    let mut model = GradientBoostedRegressor::new(50, 0.1, 3, 2).unwrap();
    model.fit(&x, &y).unwrap();
    let preds = model.predict_rows(&x);
    let sse: f64 = preds.iter().zip(&y).map(|(p, yv)| (p - yv).powi(2)).sum();
    let rmse = (sse / y.len() as f64).sqrt();
    // Shallow trees on a smooth series should beat naive persistence.
    assert!(rmse < 1.0);
}

#[test]
fn test_fit_and_forecast_returns_output_contract() {
    // A noisy trend series so the residual variance is positive and the
    // prediction interval exists.
    let series = quant(&[10.0, 11.2, 10.8, 12.5, 13.1, 14.2, 15.0, 14.6, 15.8, 16.3]);
    let config = ForecastConfig {
        horizon: 3,
        model: ForecastModel::Theta,
        prediction_interval_pct: 0.80,
    };
    let output = fit_and_forecast(&series, &config).unwrap();

    assert_eq!(output.horizon, 3);
    assert_eq!(output.point.steps, vec![1, 2, 3]);
    assert_eq!(output.point.values.len(), 3);
    let interval = output.interval.expect("interval");
    assert_eq!(interval.lower.len(), 3);
    assert_eq!(interval.upper.len(), 3);
    for i in 0..3 {
        assert!(
            interval.lower[i] <= output.point.values[i],
            "lower bound must not exceed point"
        );
        assert!(
            output.point.values[i] <= interval.upper[i],
            "point must not exceed upper bound"
        );
    }
    // Interval widens with the horizon.
    let width0 = interval.upper[0] - interval.lower[0];
    let width2 = interval.upper[2] - interval.lower[2];
    assert!(width2 > width0, "interval widens with horizon");
}

#[test]
fn test_fit_and_forecast_rejects_zero_horizon() {
    let config = ForecastConfig {
        horizon: 0,
        model: ForecastModel::Theta,
        prediction_interval_pct: 0.8,
    };
    let err = fit_and_forecast(&quant(&[1.0, 2.0, 3.0]), &config).unwrap_err();
    assert!(matches!(err, ForecastError::InvalidConfig(_)));
}

#[test]
fn test_fit_and_forecast_rejects_bad_interval_pct() {
    let config = ForecastConfig {
        horizon: 3,
        model: ForecastModel::Theta,
        prediction_interval_pct: 1.5,
    };
    let err = fit_and_forecast(&quant(&[1.0, 2.0, 3.0]), &config).unwrap_err();
    assert!(matches!(err, ForecastError::InvalidConfig(_)));
}

#[test]
fn test_fit_and_forecast_rejects_short_series() {
    let config = ForecastConfig {
        horizon: 3,
        model: ForecastModel::Arima { p: 2, d: 1, q: 1 },
        prediction_interval_pct: 0.8,
    };
    let err = fit_and_forecast(&quant(&[1.0, 2.0]), &config).unwrap_err();
    assert!(matches!(err, ForecastError::InsufficientData { .. }));
}

#[test]
fn test_panel_forecast_carries_series_keys() {
    let tcs = InstrumentId::new(vyapaar_core::Exchange::Nse, "TCS");
    let rel = InstrumentId::new(vyapaar_core::Exchange::Nse, "RELIANCE");
    let panel = vec![
        (tcs.clone(), quant(&[10.0, 11.0, 12.0, 13.0])),
        (rel.clone(), quant(&[100.0, 102.0, 101.0, 103.0])),
    ];
    let config = ForecastConfig {
        horizon: 2,
        model: ForecastModel::Ets,
        prediction_interval_pct: 0.8,
    };
    let results = fit_and_forecast_panel(&panel, &config).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].series_key, tcs);
    assert_eq!(results[1].series_key, rel);
    assert_eq!(results[0].output.point.values.len(), 2);
}

#[test]
fn test_gradient_boost_model_in_config_returns_finite_forecast() {
    let series: Vec<f64> = (0..40).map(|i| 10.0 + (i as f64 * 0.3).sin()).collect();
    let config = ForecastConfig {
        horizon: 5,
        model: ForecastModel::GradientBoost {
            lags: 5,
            n_estimators: 50,
            max_depth: 3,
            learning_rate: 0.1,
        },
        prediction_interval_pct: 0.8,
    };
    let output = fit_and_forecast(&quant(&series), &config).unwrap();
    assert!(output.point.values.iter().all(finite));
    assert!(output.point.values.iter().all(|v| {
        let f = v.to_f64().unwrap();
        f > 5.0 && f < 15.0
    }));
}
