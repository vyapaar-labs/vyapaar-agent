//! Volatility modeling and GARCH(1,1) parameter estimation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{AnalyticsError, Result};
use crate::math::{mean, variance};

/// Configuration for fitting a GARCH model.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GarchConfig {
    /// ARCH lag order (default 1).
    pub p: usize,
    /// GARCH lag order (default 1).
    pub q: usize,
    /// Annualization factor (e.g. 252.0 for daily trading sessions).
    pub annualization: f64,
    /// Forecast horizon in steps (e.g. 5 steps ahead).
    pub forecast_horizon: usize,
}

impl Default for GarchConfig {
    fn default() -> Self {
        Self {
            p: 1,
            q: 1,
            annualization: 252.0,
            forecast_horizon: 5,
        }
    }
}

/// Result of fitting a GARCH(1,1) volatility model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GarchResult {
    /// Constant variance intercept ($\omega > 0$).
    pub omega: f64,
    /// ARCH innovation parameter ($\alpha \ge 0$).
    pub alpha: f64,
    /// GARCH persistence parameter ($\beta \ge 0$).
    pub beta: f64,
    /// Persistence of the process ($\alpha + \beta$).
    pub persistence: f64,
    /// Half-life of a volatility shock in bars ($\ln(0.5) / \ln(\alpha + \beta)$).
    pub half_life: Option<f64>,
    /// Unconditional sample variance ($\sigma^2 = \omega / (1 - \alpha - \beta)$).
    pub unconditional_variance: f64,
    /// Unconditional volatility (standard deviation per period).
    pub unconditional_volatility: f64,
    /// Annualized unconditional volatility ($\sigma \times \sqrt{\text{annualization}}$).
    pub annualized_unconditional_volatility: f64,
    /// Latest in-sample conditional volatility ($\sigma_T$).
    pub latest_conditional_volatility: f64,
    /// Latest in-sample annualized conditional volatility ($\sigma_T \times \sqrt{\text{annualization}}$).
    pub latest_annualized_volatility: f64,
    /// Maximized Gaussian log-likelihood of the fitted model.
    pub log_likelihood: f64,
    /// Akaike Information Criterion ($2k - 2\ln L$).
    pub aic: f64,
    /// Bayesian Information Criterion ($k\ln(T) - 2\ln L$).
    pub bic: f64,
    /// In-sample conditional volatilities ($\sigma_t$).
    pub conditional_volatilities: Vec<f64>,
    /// Multi-step ahead volatility forecasts ($\sigma_{T+h}$).
    pub forecast_volatilities: Vec<f64>,
    /// Annualized multi-step ahead volatility forecasts ($\sigma_{T+h} \times \sqrt{\text{annualization}}$).
    pub annualized_forecast_volatilities: Vec<f64>,
}

/// Fits a GARCH(1,1) model using Gaussian quasi-maximum likelihood with variance targeting.
///
/// In standard variance-targeted GARCH(1,1), the unconditional variance $\sigma^2$ is set
/// equal to the sample variance $s^2$, which locks $\omega = s^2(1 - \alpha - \beta)$.
/// The parameter space $(\alpha, \beta)$ is optimized over the simplex:
/// $\alpha \ge 0$, $\beta \ge 0$, $\alpha + \beta < 1$
/// to maximize the Gaussian conditional log-likelihood.
pub fn fit_garch11(returns: &[f64], config: &GarchConfig) -> Result<GarchResult> {
    if returns.is_empty() {
        return Err(AnalyticsError::EmptyInput);
    }
    let n = returns.len();
    if n < 10 {
        return Err(AnalyticsError::InsufficientData {
            needed: 10,
            got: n,
        });
    }
    if config.p != 1 || config.q != 1 {
        return Err(AnalyticsError::TradeMismatch(format!(
            "only GARCH(1,1) is supported, got p={}, q={}",
            config.p, config.q
        )));
    }

    let sample_var = variance(returns);
    if sample_var <= 1e-12 || !sample_var.is_finite() {
        return Err(AnalyticsError::ZeroVariance);
    }

    let mu = mean(returns);
    let eps: Vec<f64> = returns.iter().map(|&r| r - mu).collect();
    let eps_sq: Vec<f64> = eps.iter().map(|&e| e * e).collect();

    // Objective function: Negative Log-Likelihood given (alpha, beta)
    let nll = |alpha: f64, beta: f64| -> f64 {
        if alpha < 1e-6 || beta < 1e-6 || (alpha + beta) >= 0.9999 {
            let mut penalty = 1e8;
            if alpha < 1e-6 {
                penalty += (1e-6 - alpha) * 1e7;
            }
            if beta < 1e-6 {
                penalty += (1e-6 - beta) * 1e7;
            }
            if (alpha + beta) >= 0.9999 {
                penalty += (alpha + beta - 0.9999) * 1e7;
            }
            return penalty;
        }

        let omega = sample_var * (1.0 - alpha - beta);
        let mut total_nll = 0.0;
        let mut sigma_sq = sample_var;

        for &e_sq in &eps_sq {
            if sigma_sq <= 1e-14 || !sigma_sq.is_finite() {
                return 1e9;
            }
            // 0.5 * (ln(sigma^2) + eps^2 / sigma^2)
            total_nll += 0.5 * (sigma_sq.ln() + e_sq / sigma_sq);

            // sigma_{t+1}^2 = omega + alpha * eps_t^2 + beta * sigma_t^2
            sigma_sq = omega + alpha * e_sq + beta * sigma_sq;
        }

        total_nll
    };

    // Candidate initial seeds across the stationary triangle
    let candidate_seeds = [
        (0.08, 0.88),
        (0.05, 0.90),
        (0.12, 0.82),
        (0.03, 0.94),
        (0.15, 0.75),
        (0.20, 0.65),
        (0.10, 0.70),
    ];

    let mut best_seed = candidate_seeds[0];
    let mut best_seed_nll = nll(best_seed.0, best_seed.1);
    for &seed in &candidate_seeds[1..] {
        let val = nll(seed.0, seed.1);
        if val < best_seed_nll {
            best_seed_nll = val;
            best_seed = seed;
        }
    }

    // Nelder-Mead simplex optimization in 2D
    let p0 = best_seed;
    let p1 = (
        (best_seed.0 + 0.03).min(0.4),
        (best_seed.1 - 0.03).max(0.1),
    );
    let p2 = (
        (best_seed.0 - 0.02).max(0.01),
        (best_seed.1 + 0.01).min(0.95),
    );

    let mut simplex = [(p0, nll(p0.0, p0.1)), (p1, nll(p1.0, p1.1)), (p2, nll(p2.0, p2.1))];

    for _ in 0..200 {
        // Sort vertices so simplex[0] is best, simplex[2] is worst
        simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let best = simplex[0].0;
        let worst = simplex[2].0;
        let second_worst = simplex[1].0;

        let diff_01 = (best.0 - second_worst.0).hypot(best.1 - second_worst.1);
        let diff_02 = (best.0 - worst.0).hypot(best.1 - worst.1);
        if diff_01 < 1e-6 && diff_02 < 1e-6 {
            break;
        }

        // Centroid of best two points
        let xo = (0.5 * (best.0 + second_worst.0), 0.5 * (best.1 + second_worst.1));

        // Reflection
        let xr = (2.0 * xo.0 - worst.0, 2.0 * xo.1 - worst.1);
        let fxr = nll(xr.0, xr.1);

        if fxr < simplex[1].1 && fxr >= simplex[0].1 {
            simplex[2] = (xr, fxr);
            continue;
        }

        // Expansion
        if fxr < simplex[0].1 {
            let xe = (xo.0 + 2.0 * (xr.0 - xo.0), xo.1 + 2.0 * (xr.1 - xo.1));
            let fxe = nll(xe.0, xe.1);
            if fxe < fxr {
                simplex[2] = (xe, fxe);
            } else {
                simplex[2] = (xr, fxr);
            }
            continue;
        }

        // Contraction
        if fxr < simplex[2].1 {
            // Outside contraction
            let xc = (xo.0 + 0.5 * (xr.0 - xo.0), xo.1 + 0.5 * (xr.1 - xo.1));
            let fxc = nll(xc.0, xc.1);
            if fxc <= fxr {
                simplex[2] = (xc, fxc);
                continue;
            }
        } else {
            // Inside contraction
            let xcc = (xo.0 - 0.5 * (xo.0 - worst.0), xo.1 - 0.5 * (xo.1 - worst.1));
            let fxcc = nll(xcc.0, xcc.1);
            if fxcc < simplex[2].1 {
                simplex[2] = (xcc, fxcc);
                continue;
            }
        }

        // Shrink
        let x1 = (0.5 * (simplex[1].0.0 + best.0), 0.5 * (simplex[1].0.1 + best.1));
        let x2 = (0.5 * (simplex[2].0.0 + best.0), 0.5 * (simplex[2].0.1 + best.1));
        simplex[1] = (x1, nll(x1.0, x1.1));
        simplex[2] = (x2, nll(x2.0, x2.1));
    }

    simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let (opt_alpha, opt_beta) = simplex[0].0;
    let min_nll = simplex[0].1;

    let persistence = opt_alpha + opt_beta;
    let omega = sample_var * (1.0 - persistence);
    let unconditional_variance = sample_var;
    let unconditional_volatility = sample_var.sqrt();
    let annualized_unconditional_volatility =
        (unconditional_variance * config.annualization).sqrt();

    let half_life = if persistence > 0.0 && persistence < 1.0 {
        Some(0.5f64.ln() / persistence.ln())
    } else {
        None
    };

    // Calculate conditional volatilities in-sample
    let mut conditional_volatilities = Vec::with_capacity(n);
    let mut sigma_sq = sample_var;
    for &e_sq in &eps_sq {
        conditional_volatilities.push(sigma_sq.sqrt());
        sigma_sq = omega + opt_alpha * e_sq + opt_beta * sigma_sq;
    }

    let latest_conditional_volatility = *conditional_volatilities.last().unwrap_or(&unconditional_volatility);
    let latest_annualized_volatility = (latest_conditional_volatility * latest_conditional_volatility * config.annualization).sqrt();

    // Out-of-sample forward forecasts: sigma_{T+h}^2 = V_L + (alpha+beta)^h * (sigma_T^2 - V_L)
    let latest_sigma_sq = sigma_sq; // next step 1-step forecast variance
    let mut forecast_volatilities = Vec::with_capacity(config.forecast_horizon);
    let mut annualized_forecast_volatilities = Vec::with_capacity(config.forecast_horizon);

    for h in 0..config.forecast_horizon {
        let f_sigma_sq = unconditional_variance
            + persistence.powi(h as i32) * (latest_sigma_sq - unconditional_variance);
        let f_vol = f_sigma_sq.max(0.0).sqrt();
        forecast_volatilities.push(f_vol);
        annualized_forecast_volatilities.push((f_vol * f_vol * config.annualization).sqrt());
    }

    let total_log_lik = -0.5 * (n as f64) * (2.0 * std::f64::consts::PI).ln() - min_nll;
    let k = 3.0; // omega, alpha, beta
    let aic = 2.0 * k - 2.0 * total_log_lik;
    let bic = k * (n as f64).ln() - 2.0 * total_log_lik;

    Ok(GarchResult {
        omega,
        alpha: opt_alpha,
        beta: opt_beta,
        persistence,
        half_life,
        unconditional_variance,
        unconditional_volatility,
        annualized_unconditional_volatility,
        latest_conditional_volatility,
        latest_annualized_volatility,
        log_likelihood: total_log_lik,
        aic,
        bic,
        conditional_volatilities,
        forecast_volatilities,
        annualized_forecast_volatilities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_returns_rejected() {
        let res = fit_garch11(&[], &GarchConfig::default());
        assert_eq!(res, Err(AnalyticsError::EmptyInput));
    }

    #[test]
    fn test_insufficient_data_rejected() {
        let res = fit_garch11(&[0.01, -0.01, 0.02], &GarchConfig::default());
        assert_eq!(
            res,
            Err(AnalyticsError::InsufficientData {
                needed: 10,
                got: 3
            })
        );
    }

    #[test]
    fn test_zero_variance_rejected() {
        let res = fit_garch11(&[0.01; 20], &GarchConfig::default());
        assert_eq!(res, Err(AnalyticsError::ZeroVariance));
    }

    #[test]
    fn test_garch11_fitting_synthetic_clustering() {
        // Synthetic time series with clear volatility clustering
        let mut returns = Vec::new();
        for i in 0..100 {
            let scale = if (30..60).contains(&i) { 0.03 } else { 0.008 };
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            returns.push(sign * scale);
        }

        let config = GarchConfig {
            p: 1,
            q: 1,
            annualization: 252.0,
            forecast_horizon: 5,
        };

        let result = fit_garch11(&returns, &config).expect("garch fit succeeded");
        assert!(result.alpha > 0.0);
        assert!(result.beta > 0.0);
        assert!(result.persistence > 0.0 && result.persistence < 1.0);
        assert!(result.omega > 0.0);
        assert!(result.unconditional_variance > 0.0);
        assert!(result.half_life.is_some());
        assert_eq!(result.forecast_volatilities.len(), 5);
        assert_eq!(result.annualized_forecast_volatilities.len(), 5);
        assert_eq!(result.conditional_volatilities.len(), returns.len());
    }
}
