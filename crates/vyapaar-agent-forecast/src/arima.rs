//! ARIMA(p, d, q) forecaster.
//!
//! Estimation strategy (deterministic):
//! 1. Difference the series `d` times to stationarity.
//! 2. Fit the AR(p) coefficients by solving the Yule-Walker equations
//!    (a Toeplitz normal-equation system) via Gaussian elimination.
//! 3. Fit the MA(q) coefficients by conditional least squares: residual
//!    recursion with a numeric-gradient descent minimizing `Σ e_t²`
//!    (deterministic; starts at zeros and converges to a local optimum).
//! 4. Forecast recursively with future innovations set to zero, then invert
//!    the differencing back to level space.

use crate::error::ForecastError;
use crate::linalg::{mean, solve_linear_system, std_dev};

/// ARIMA order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArimaOrder {
    /// Autoregressive order.
    pub p: usize,
    /// Integration (differencing) order.
    pub d: usize,
    /// Moving-average order.
    pub q: usize,
}

/// A fitted ARIMA forecaster.
#[derive(Debug, Clone)]
pub struct ArimaForecaster {
    /// Model order.
    pub order: ArimaOrder,
    /// AR coefficients `φ[0..p]`.
    pub phi: Vec<f64>,
    /// MA coefficients `θ[0..q]`.
    pub theta: Vec<f64>,
    /// Intercept of the differenced model.
    pub intercept: f64,
    /// Residual standard deviation of the differenced fit.
    pub resid_std: f64,
    /// Last `p` differenced observations (for the forecast recursion).
    last_w: Vec<f64>,
    /// Last `q` residuals (for the forecast recursion).
    last_e: Vec<f64>,
    /// Original series tail for inverting differencing.
    last_orig: Vec<f64>,
}

impl ArimaForecaster {
    /// Create an untrained forecaster for the given order.
    pub fn new(order: ArimaOrder) -> Self {
        Self {
            order,
            phi: Vec::new(),
            theta: Vec::new(),
            intercept: 0.0,
            resid_std: 0.0,
            last_w: Vec::new(),
            last_e: Vec::new(),
            last_orig: Vec::new(),
        }
    }

    /// Fit the model to a series.
    pub fn fit(&mut self, y: &[f64]) -> Result<(), ForecastError> {
        if y.is_empty() || y.iter().any(|v| !v.is_finite()) {
            return Err(if y.is_empty() {
                ForecastError::InsufficientData { needed: 1, got: 0 }
            } else {
                ForecastError::NonFinite
            });
        }
        let p = self.order.p;
        let q = self.order.q;
        let d = self.order.d;

        // Difference d times.
        let mut w = y.to_vec();
        for _ in 0..d {
            w = w.windows(2).map(|win| win[1] - win[0]).collect();
        }
        if w.len() < p + q + 1 {
            return Err(ForecastError::InsufficientData {
                needed: p + q + 1,
                got: w.len(),
            });
        }

        // De-mean the differenced series.
        let w_mean = mean(&w);
        let wc: Vec<f64> = w.iter().map(|v| v - w_mean).collect();

        // AR(p) via Yule-Walker.
        let mut phi = vec![0.0; p];
        if p > 0 {
            phi = ar_yule_walker(&wc, p)?;
        }

        // MA(q) via conditional least squares on the residual recursion.
        let (theta, residuals) = ma_conditional_ls(&wc, &phi, q)?;

        let resid_std = if residuals.is_empty() {
            0.0
        } else {
            std_dev(&residuals)
        };

        self.phi = phi;
        self.theta = theta;
        self.intercept = w_mean;
        self.resid_std = resid_std;
        self.last_w = wc[wc.len().saturating_sub(p)..].to_vec();
        self.last_e = residuals[residuals.len().saturating_sub(q)..].to_vec();
        self.last_orig = y.to_vec();
        Ok(())
    }

    /// Produce `horizon` level-space forecasts.
    pub fn forecast(&self, horizon: usize) -> Result<Vec<f64>, ForecastError> {
        if horizon == 0 {
            return Ok(Vec::new());
        }
        let d = self.order.d;

        // Recursive ARMA forecast in (de-meaned) differenced space.
        let mut w_hist = self.last_w.clone();
        let mut e_hist = self.last_e.clone();
        let mut diff_fc = Vec::with_capacity(horizon);
        for _ in 0..horizon {
            let mut pred = 0.0;
            for (j, &phi_j) in self.phi.iter().enumerate() {
                pred += phi_j * w_hist[w_hist.len() - 1 - j];
            }
            for (k, &theta_k) in self.theta.iter().enumerate() {
                pred += theta_k * e_hist[e_hist.len() - 1 - k];
            }
            w_hist.push(pred);
            e_hist.push(0.0);
            diff_fc.push(pred + self.intercept);
        }

        if d == 0 {
            return Ok(diff_fc);
        }
        Ok(invert_diff(&self.last_orig, d, &diff_fc))
    }
}

/// Solve the Yule-Walker equations for AR(p) coefficients.
fn ar_yule_walker(series: &[f64], p: usize) -> Result<Vec<f64>, ForecastError> {
    // Sample autocovariances r[0..=p].
    let n = series.len();
    let mut r = vec![0.0; p + 1];
    for k in 0..=p {
        r[k] = (0..n - k).map(|i| series[i] * series[i + k]).sum::<f64>() / n as f64;
    }
    if r[0].abs() <= f64::EPSILON {
        return Ok(vec![0.0; p]);
    }
    // Build the Toeplitz system Σ_j φ_j r[|i-j|] = r[i], i = 1..=p.
    let mut a = vec![vec![0.0; p]; p];
    let mut b = vec![0.0; p];
    for i in 0..p {
        for j in 0..p {
            a[i][j] = r[(i as isize - j as isize).unsigned_abs()];
        }
        b[i] = r[i + 1];
    }
    solve_linear_system(&a, &b)
}

/// Conditional least squares for MA(q) coefficients given AR(p) coefficients.
///
/// Returns `(theta, residuals)`. Uses a deterministic numeric-gradient
/// descent on the residual recursion `e_t = w_t − Σφ w − Σθ e`.
fn ma_conditional_ls(
    series: &[f64],
    phi: &[f64],
    q: usize,
) -> Result<(Vec<f64>, Vec<f64>), ForecastError> {
    if q == 0 {
        // No MA terms: residuals from the AR fit directly.
        let mut residuals = Vec::new();
        for t in 0..series.len() {
            let mut pred = 0.0;
            for (j, &phi_j) in phi.iter().enumerate() {
                if t > j {
                    pred += phi_j * series[t - 1 - j];
                }
            }
            residuals.push(series[t] - pred);
        }
        return Ok((Vec::new(), residuals));
    }

    let mut theta = vec![0.0; q];
    let mut sse = residual_sse(series, phi, &theta);
    let mut step = 0.1;
    let mut grad = vec![0.0; q];
    let mut iter = 0;
    const MAX_ITER: usize = 1000;
    while iter < MAX_ITER && step > 1e-8 {
        // Numeric gradient.
        let eps = 1e-6;
        for k in 0..q {
            let mut up = theta.clone();
            let mut down = theta.clone();
            up[k] += eps;
            down[k] -= eps;
            let sse_up = residual_sse(series, phi, &up);
            let sse_down = residual_sse(series, phi, &down);
            grad[k] = (sse_up - sse_down) / (2.0 * eps);
        }
        // Line search: shrink step until SSE decreases.
        let mut candidate = theta.clone();
        let mut candidate_sse = sse;
        let mut improved = false;
        for _ in 0..30 {
            let mut trial = theta.clone();
            for k in 0..q {
                trial[k] -= step * grad[k];
            }
            let trial_sse = residual_sse(series, phi, &trial);
            if trial_sse < candidate_sse {
                candidate = trial;
                candidate_sse = trial_sse;
                improved = true;
                break;
            }
            step *= 0.5;
        }
        if !improved {
            break;
        }
        let improvement = sse - candidate_sse;
        theta = candidate;
        sse = candidate_sse;
        if improvement.abs() < 1e-10 {
            break;
        }
        iter += 1;
    }

    let residuals = residuals_of(series, phi, &theta);
    Ok((theta, residuals))
}

/// Squared error of the residual recursion for given AR/MA coefficients.
fn residual_sse(series: &[f64], phi: &[f64], theta: &[f64]) -> f64 {
    residuals_of(series, phi, theta).iter().map(|e| e * e).sum()
}

/// Compute the residual recursion `e_t = w_t − Σφ w_{t−1−j} − Σθ e_{t−1−k}`.
fn residuals_of(series: &[f64], phi: &[f64], theta: &[f64]) -> Vec<f64> {
    let mut e = vec![0.0; series.len()];
    for t in 0..series.len() {
        let mut pred = 0.0;
        for (j, &phi_j) in phi.iter().enumerate() {
            if t > j {
                pred += phi_j * series[t - 1 - j];
            }
        }
        for (k, &theta_k) in theta.iter().enumerate() {
            if t > k {
                pred += theta_k * e[t - 1 - k];
            }
        }
        e[t] = series[t] - pred;
    }
    e
}

/// Invert `d` orders of differencing to reconstruct level-space forecasts.
fn invert_diff(original: &[f64], d: usize, diff_fc: &[f64]) -> Vec<f64> {
    let mut levels = original.to_vec();
    let mut current = diff_fc.to_vec();
    for _ in 0..d {
        let base = *levels.last().unwrap_or(&0.0);
        let mut acc = base;
        let next: Vec<f64> = current
            .iter()
            .map(|w| {
                acc += w;
                acc
            })
            .collect();
        levels.extend(next.clone());
        current = next;
    }
    let n_out = diff_fc.len();
    levels[levels.len() - n_out..].to_vec()
}
