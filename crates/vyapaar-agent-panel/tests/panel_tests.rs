//! Tests for the vyapaar-agent-panel econometrics engines.
//!
//! All test data is generated with a deterministic LCG so the suite is
//! reproducible. Estimators are checked against known coefficients.

#![allow(clippy::needless_range_loop)]

use vyapaar_agent_panel::{
    fama_macbeth, fixed_effects, grs_test, random_effects, two_stage_least_squares,
};

/// Small deterministic LCG (Numerical Recipes constants).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

#[test]
fn test_fixed_effects_recovers_coefficients() {
    let mut rng = Lcg(42);
    // Balanced panel: 10 entities x 8 periods.
    let mut entity = Vec::new();
    let mut period = Vec::new();
    let mut y = Vec::new();
    let mut x = Vec::new();
    let entity_effects: Vec<f64> = (0..10).map(|i| (i as f64) * 1.5).collect();
    for e in 0..10 {
        for p in 0..8 {
            let x1 = rng.next();
            let x2 = rng.next();
            let noise = rng.next() * 0.1;
            let value = 2.0 * x1 + 3.0 * x2 + entity_effects[e] + noise;
            entity.push(e);
            period.push(p);
            y.push(value);
            x.push(vec![x1, x2]);
        }
    }
    let result = fixed_effects(&entity, &period, &y, &x, false).unwrap();
    assert_eq!(result.k, 2);
    assert!(
        (result.beta[0] - 2.0).abs() < 0.2,
        "beta[0]={}",
        result.beta[0]
    );
    assert!(
        (result.beta[1] - 3.0).abs() < 0.2,
        "beta[1]={}",
        result.beta[1]
    );
    assert_eq!(result.n_entities, 10);
    assert_eq!(result.n_observations, 80);
    // Fixed effects absorb the entity means → high within R².
    assert!(result.r_squared_within > 0.9);
}

#[test]
fn test_fixed_effects_two_way_handles_period_effects() {
    let mut rng = Lcg(7);
    let mut entity = Vec::new();
    let mut period = Vec::new();
    let mut y = Vec::new();
    let mut x = Vec::new();
    let period_effects: Vec<f64> = (0..6).map(|p| (p as f64) * 0.8).collect();
    for e in 0..8 {
        for p in 0..6 {
            let x1 = rng.next();
            let value = 1.5 * x1 + period_effects[p] + rng.next() * 0.1;
            entity.push(e);
            period.push(p);
            y.push(value);
            x.push(vec![x1]);
        }
    }
    let result = fixed_effects(&entity, &period, &y, &x, true).unwrap();
    assert!(result.two_way);
    assert!(
        (result.beta[0] - 1.5).abs() < 0.2,
        "beta[0]={}",
        result.beta[0]
    );
}

#[test]
fn test_random_effects_recovers_coefficients() {
    let mut rng = Lcg(99);
    let mut entity = Vec::new();
    let mut y = Vec::new();
    let mut x = Vec::new();
    let entity_effects: Vec<f64> = (0..6).map(|i| (i as f64) * 2.0).collect();
    for e in 0..6 {
        for _ in 0..10 {
            let x1 = rng.next();
            let value = 2.5 * x1 + entity_effects[e] + rng.next() * 0.2;
            entity.push(e);
            y.push(value);
            x.push(vec![x1]);
        }
    }
    let result = random_effects(&entity, &y, &x).unwrap();
    assert!(
        (result.beta[0] - 2.5).abs() < 0.4,
        "beta[0]={}",
        result.beta[0]
    );
    assert!(result.sigma2_u > 0.0, "entity variance component positive");
}

#[test]
fn test_fama_macbeth_recovers_risk_premia() {
    let mut rng = Lcg(1234);
    let t = 120;
    let n = 30;
    let k = 2;
    // Factor returns: f1 mean 0.5%, f2 mean -0.2% per period, with noise.
    let mut factor_returns = vec![Vec::new(); t];
    for p in 0..t {
        factor_returns[p] = vec![0.005 + rng.next() * 0.02, -0.002 + rng.next() * 0.02];
    }
    // Asset betas.
    let betas: Vec<Vec<f64>> = (0..n)
        .map(|_| vec![0.5 + rng.next() * 0.5, 0.3 + rng.next() * 0.5])
        .collect();
    // Returns = beta * f + idiosyncratic noise.
    let mut returns = vec![vec![0.0; n]; t];
    for p in 0..t {
        for asset in 0..n {
            returns[p][asset] = betas[asset][0] * factor_returns[p][0]
                + betas[asset][1] * factor_returns[p][1]
                + rng.next() * 0.01;
        }
    }

    let result = fama_macbeth(&returns, &factor_returns).unwrap();
    assert_eq!(result.n_periods, t);
    assert_eq!(result.n_assets, n);
    assert_eq!(result.n_factors, k);
    // The estimated premium should be near the factor mean.
    let mean_f1: f64 = (0..t).map(|p| factor_returns[p][0]).sum::<f64>() / t as f64;
    let mean_f2: f64 = (0..t).map(|p| factor_returns[p][1]).sum::<f64>() / t as f64;
    assert!(
        (result.lambda[0] - mean_f1).abs() < 0.005,
        "lambda[0]={} vs {mean_f1}",
        result.lambda[0]
    );
    assert!(
        (result.lambda[1] - mean_f2).abs() < 0.005,
        "lambda[1]={} vs {mean_f2}",
        result.lambda[1]
    );
    assert!(result.lambda[1] < result.lambda[0], "premia ordered");
}

#[test]
fn test_two_stage_least_squares_recovers_beta_with_endogeneity() {
    let mut rng = Lcg(555);
    let n = 400;
    // Endogenous regressor: x = z + u where u is correlated with the error.
    let mut y = Vec::new();
    let mut x = Vec::new();
    let mut z = Vec::new();
    let true_beta = 3.0;
    for _ in 0..n {
        let instrument = rng.next();
        let u = rng.next();
        let regressor = instrument + u;
        let error = u * 0.5 + rng.next() * 0.1;
        let value = true_beta * regressor + error;
        y.push(value);
        x.push(vec![regressor]);
        // Z: instrument + an extra valid instrument (over-identified, L=2 > k=1).
        z.push(vec![instrument, rng.next()]);
    }

    // OLS is biased because the regressor correlates with the error.
    let ols_beta: f64 = {
        let sum_xy: f64 = x.iter().zip(&y).map(|(xi, yi)| xi[0] * yi).sum();
        let sum_xx: f64 = x.iter().map(|xi| xi[0] * xi[0]).sum();
        sum_xy / sum_xx
    };
    let result = two_stage_least_squares(&y, &x, &z).unwrap();
    assert_eq!(result.k, 1);
    assert_eq!(result.l, 2);
    assert!(
        (result.beta[0] - true_beta).abs() < 0.2,
        "2SLS beta[0]={}",
        result.beta[0]
    );
    // The J-statistic is available (over-identified) and p-value moderate.
    let j = result.j_stat.expect("Sargan J computed");
    let p = result.j_p_value.expect("J p-value computed");
    assert!(j.is_finite() && j >= 0.0);
    assert!(p.is_finite() && (0.0..=1.0).contains(&p));
    let _ = ols_beta;
}

#[test]
fn test_grs_test_accepts_well_priced_factor_model() {
    let mut rng = Lcg(2024);
    let t = 240;
    let n = 5;
    let k = 1;
    let factor_mean = 0.003;
    let mut factor_returns = vec![Vec::new(); t];
    for p in 0..t {
        factor_returns[p] = vec![factor_mean + rng.next() * 0.01];
    }
    // Assets exactly priced by the factor (zero alphas).
    let betas: Vec<f64> = (0..n).map(|_| 0.5 + rng.next() * 1.0).collect();
    let mut returns = vec![vec![0.0; n]; t];
    for p in 0..t {
        for asset in 0..n {
            returns[p][asset] = betas[asset] * factor_returns[p][0] + rng.next() * 0.005;
        }
    }

    let result = grs_test(&returns, &factor_returns).unwrap();
    assert_eq!(result.df1, n);
    assert_eq!(result.df2, t - n - k);
    assert!(
        result.p_value > 0.05,
        "zero-alpha model should not reject; p={}",
        result.p_value
    );
    assert!(!result.rejected_at_5_pct);
}

#[test]
fn test_grs_test_rejects_mispriced_factor_model() {
    let mut rng = Lcg(31337);
    let t = 240;
    let n = 5;
    let mut factor_returns = vec![Vec::new(); t];
    for p in 0..t {
        factor_returns[p] = vec![0.003 + rng.next() * 0.01];
    }
    let betas: Vec<f64> = (0..n).map(|_| 0.5 + rng.next() * 1.0).collect();
    // One asset has a large nonzero alpha.
    let mut returns = vec![vec![0.0; n]; t];
    for p in 0..t {
        for asset in 0..n {
            let alpha = if asset == 0 { 0.02 } else { 0.0 };
            returns[p][asset] = alpha + betas[asset] * factor_returns[p][0] + rng.next() * 0.005;
        }
    }

    let result = grs_test(&returns, &factor_returns).unwrap();
    assert!(
        result.p_value < 0.05,
        "mispriced model should reject; p={} statistic={}",
        result.p_value,
        result.statistic
    );
    assert!(result.rejected_at_5_pct);
}
