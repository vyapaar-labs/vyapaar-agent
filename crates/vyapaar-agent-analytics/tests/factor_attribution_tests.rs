//! Tests for the cross-sectional factor attribution (0003 Phase 2).

use std::collections::HashMap;

use vyapaar_agent_analytics::{
    PortfolioHolding, SectorExposure, StyleExposure, factor_attribution_blocks,
    fit_cross_sectional_factor_model,
};

fn holding(
    asset: &str,
    weight: f64,
    momentum: f64,
    size: f64,
    sector: &str,
    specific_risk: f64,
) -> PortfolioHolding {
    PortfolioHolding {
        asset: asset.to_string(),
        weight,
        style: vec![
            StyleExposure {
                factor: "momentum".to_string(),
                value: momentum,
            },
            StyleExposure {
                factor: "size".to_string(),
                value: size,
            },
        ],
        sectors: vec![SectorExposure {
            sector: sector.to_string(),
        }],
        specific_risk,
    }
}

fn panel(
    periods: usize,
    momentum_ret: f64,
    size_ret: f64,
    sector_ret: f64,
) -> Vec<HashMap<String, f64>> {
    (0..periods)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("momentum".to_string(), momentum_ret + (i as f64) * 0.0001);
            row.insert("size".to_string(), size_ret - (i as f64) * 0.0001);
            row.insert("sector_Financials".to_string(), sector_ret);
            row
        })
        .collect()
}

#[test]
fn test_factor_attribution_decomposes_return_and_risk() {
    let holdings = vec![
        holding("NSE:TCS-EQ", 0.6, 1.0, 0.5, "Financials", 0.10),
        holding("NSE:HDFCBANK-EQ", 0.4, -0.5, 1.0, "Financials", 0.12),
    ];
    let factor_returns = panel(60, 0.01, 0.005, 0.002);

    let attribution = fit_cross_sectional_factor_model(&holdings, &factor_returns, 252.0).unwrap();

    assert_eq!(attribution.n_assets, 2);
    assert_eq!(attribution.n_periods, 60);
    assert_eq!(attribution.n_factors, 3);

    // Portfolio momentum exposure = 0.6*1.0 + 0.4*(-0.5) = 0.4.
    let momentum = attribution
        .factors
        .iter()
        .find(|f| f.factor == "momentum")
        .expect("momentum factor");
    assert!((momentum.exposure - 0.4).abs() < 1e-9);

    // Portfolio size exposure = 0.6*0.5 + 0.4*1.0 = 0.7.
    let size = attribution
        .factors
        .iter()
        .find(|f| f.factor == "size")
        .expect("size factor");
    assert!((size.exposure - 0.7).abs() < 1e-9);

    // Factor return contribution = exposure * mean factor return.
    let momentum_mean: f64 = factor_returns.iter().map(|r| r["momentum"]).sum::<f64>() / 60.0;
    let size_mean: f64 = factor_returns.iter().map(|r| r["size"]).sum::<f64>() / 60.0;
    assert!((momentum.return_contribution - 0.4 * momentum_mean).abs() < 1e-12);
    assert!((size.return_contribution - 0.7 * size_mean).abs() < 1e-12);

    // Total = factor return + residual (residual small since data is exactly
    // factor-driven apart from small drift).
    assert!(
        (attribution.total_return - attribution.factor_return - attribution.residual_return).abs()
            < 1e-9
    );

    // Risk decomposition: total² ≈ factor² + specific².
    let total_sq = attribution.total_risk.powi(2);
    let factor_sq = attribution.factor_risk.powi(2);
    let specific_sq = attribution.specific_risk.powi(2);
    assert!((total_sq - (factor_sq + specific_sq)).abs() < 1e-9);

    // Risk contribution percentages sum to ~100%.
    let risk_sum: f64 = attribution
        .factors
        .iter()
        .map(|f| f.risk_contribution_pct)
        .sum();
    assert!(
        (risk_sum - 100.0).abs() < 1e-6,
        "risk shares sum to 100: {risk_sum}"
    );

    // Presentation blocks render.
    let blocks = factor_attribution_blocks(&attribution);
    assert_eq!(blocks.len(), 2);
    assert!(matches!(
        blocks[0],
        vyapaar_agent_core::DisplayBlock::MetricGrid { .. }
    ));
    assert!(matches!(
        blocks[1],
        vyapaar_agent_core::DisplayBlock::Table { .. }
    ));
}

#[test]
fn test_factor_attribution_single_asset_residual() {
    // A single asset with a factor that doesn't explain its return: the
    // residual should carry the unexplained mean return.
    let holdings = vec![holding(
        "NSE:RELIANCE-EQ",
        1.0,
        0.0,
        0.0,
        "Financials",
        0.15,
    )];
    // Sector return = 0, so with zero style exposure everything is residual.
    let factor_returns = panel(30, 0.0, 0.0, 0.0);
    let attribution = fit_cross_sectional_factor_model(&holdings, &factor_returns, 252.0).unwrap();
    assert!((attribution.factor_return).abs() < 1e-12);
    assert!((attribution.residual_return).abs() < 1e-12);
    // Total risk = specific risk for a fully residual position.
    assert!((attribution.factor_risk).abs() < 1e-12);
    assert!((attribution.total_risk - attribution.specific_risk).abs() < 1e-12);
}

#[test]
fn test_factor_attribution_rejects_missing_factor_in_panel() {
    let holdings = vec![holding("NSE:TCS-EQ", 1.0, 1.0, 0.0, "Financials", 0.10)];
    let mut row = HashMap::new();
    row.insert("momentum".to_string(), 0.01);
    row.insert("size".to_string(), 0.005);
    // Missing "sector_Financials".
    let factor_returns = vec![row];
    let err = fit_cross_sectional_factor_model(&holdings, &factor_returns, 252.0).unwrap_err();
    assert!(err.to_string().contains("missing factor"));
}

#[test]
fn test_factor_attribution_rejects_empty_inputs() {
    let err = fit_cross_sectional_factor_model(&[], &vec![HashMap::new()], 252.0).unwrap_err();
    assert!(err.to_string().contains("empty"));
}
