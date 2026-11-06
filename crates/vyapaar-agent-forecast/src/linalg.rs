//! Internal numeric helpers for the forecast engines.
//!
//! Small, deterministic, dependency-free linear algebra and statistics over
//! `f64` (the forecast engines work in `f64` internally; the crate boundary
//! converts to/from [`vyapaar_core::QuantSeries`] / `Decimal`).

use crate::error::ForecastError;

/// Arithmetic mean of a non-empty slice.
pub(crate) fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Sample standard deviation of a non-empty slice.
pub(crate) fn std_dev(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }
    let m = mean(values);
    let var = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (n - 1) as f64;
    var.sqrt()
}

/// Solve the linear system `A x = b` by Gaussian elimination with partial
/// pivoting. Deterministic; returns [`ForecastError::DivisionByZero`] on a
/// singular matrix.
#[allow(clippy::needless_range_loop)]
pub(crate) fn solve_linear_system(a: &[Vec<f64>], b: &[f64]) -> Result<Vec<f64>, ForecastError> {
    let n = a.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    if b.len() != n || a.iter().any(|row| row.len() != n) {
        return Err(ForecastError::InvalidConfig(
            "linear system dimensions mismatch".to_string(),
        ));
    }

    let mut aug: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let mut row = a[i].clone();
            row.push(b[i]);
            row
        })
        .collect();

    for col in 0..n {
        // Partial pivoting
        let mut pivot = col;
        let mut pivot_mag = aug[col][col].abs();
        for row in (col + 1)..n {
            let mag = aug[row][col].abs();
            if mag > pivot_mag {
                pivot = row;
                pivot_mag = mag;
            }
        }
        if pivot_mag <= f64::EPSILON {
            return Err(ForecastError::DivisionByZero {
                model: "linalg",
                detail: format!("singular matrix (pivot at column {col} is zero)"),
            });
        }
        aug.swap(col, pivot);

        let inv_pivot = 1.0 / aug[col][col];
        for row in (col + 1)..n {
            let factor = aug[row][col] * inv_pivot;
            for c in col..=n {
                aug[row][c] -= factor * aug[col][c];
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut acc = aug[row][n];
        for c in (row + 1)..n {
            acc -= aug[row][c] * x[c];
        }
        x[row] = acc / aug[row][row];
    }
    Ok(x)
}

/// Ordinary least squares fit `y ≈ X β` via the normal equations.
///
/// `x` is the design matrix (one row per observation, one column per
/// regressor). Returns the coefficient vector. Requires at least as many
/// observations as regressors.
pub(crate) fn ols(x: &[Vec<f64>], y: &[f64]) -> Result<Vec<f64>, ForecastError> {
    let n = x.len();
    if n == 0 {
        return Err(ForecastError::InsufficientData { needed: 1, got: 0 });
    }
    let p = x[0].len();
    if n < p {
        return Err(ForecastError::InsufficientData { needed: p, got: n });
    }
    // X'X
    let mut xtx = vec![vec![0.0; p]; p];
    let mut xty = vec![0.0; p];
    for row in 0..n {
        for i in 0..p {
            xty[i] += x[row][i] * y[row];
            for j in 0..p {
                xtx[i][j] += x[row][i] * x[row][j];
            }
        }
    }
    solve_linear_system(&xtx, &xty)
}

/// Standard-normal quantile function (Acklam's rational approximation).
pub(crate) fn norm_ppf(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];

    let plow = 0.024_25;
    let phigh = 1.0 - plow;
    let p = p.clamp(1e-12, 1.0 - 1e-12);

    if p < plow {
        let q = (-2.0 * p.ln()).sqrt();
        let num = ((C[0] * q + C[1]) * q + C[2]) * q + C[3];
        let den = ((D[0] * q + D[1]) * q + D[2]) * q + D[3];
        num / den
    } else if p > phigh {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        let num = ((C[0] * q + C[1]) * q + C[2]) * q + C[3];
        let den = ((D[0] * q + D[1]) * q + D[2]) * q + D[3];
        -num / den
    } else {
        let q = p - 0.5;
        let r = q * q;
        let num = (((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4];
        let den = (((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4];
        q * num / den
    }
}
