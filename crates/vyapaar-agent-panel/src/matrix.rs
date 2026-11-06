//! Small deterministic linear-algebra helpers for the panel engines.
//!
//! All computation is `f64` (research-grade econometrics on return series,
//! matching the `vyapaar-agent-analytics` convention); no money/price/quantity
//! arithmetic flows through these helpers.

use crate::error::PanelError;

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
    if values.len() < 2 {
        return 0.0;
    }
    let m = mean(values);
    let var = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    var.sqrt()
}

/// Solve `A x = b` by Gaussian elimination with partial pivoting.
#[allow(clippy::needless_range_loop)]
pub(crate) fn solve(a: &[Vec<f64>], b: &[f64]) -> Result<Vec<f64>, PanelError> {
    let n = a.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    if b.len() != n || a.iter().any(|row| row.len() != n) {
        return Err(PanelError::DimensionMismatch(
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
            return Err(PanelError::Singular {
                model: "linalg",
                detail: format!("zero pivot at column {col}"),
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

/// Ordinary least squares `y ≈ X β` via the normal equations.
///
/// `x` rows are observations, columns regressors; requires `n ≥ p`.
pub(crate) fn ols(x: &[Vec<f64>], y: &[f64]) -> Result<Vec<f64>, PanelError> {
    if x.is_empty() || x.len() != y.len() {
        return Err(PanelError::InsufficientData {
            needed: 1,
            got: x.len(),
        });
    }
    let p = x[0].len();
    if x.len() < p {
        return Err(PanelError::InsufficientData {
            needed: p,
            got: x.len(),
        });
    }
    let mut xtx = vec![vec![0.0; p]; p];
    let mut xty = vec![0.0; p];
    for row in 0..x.len() {
        for i in 0..p {
            xty[i] += x[row][i] * y[row];
            for j in 0..p {
                xtx[i][j] += x[row][i] * x[row][j];
            }
        }
    }
    solve(&xtx, &xty)
}

/// Transpose a matrix.
pub(crate) fn transpose(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    if a.is_empty() {
        return Vec::new();
    }
    let rows = a.len();
    let cols = a[0].len();
    (0..cols)
        .map(|c| (0..rows).map(|r| a[r][c]).collect())
        .collect()
}

/// Matrix product `A (m×k) × B (k×n)`.
pub(crate) fn mat_mul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, PanelError> {
    if a.is_empty() {
        return Ok(Vec::new());
    }
    let k = a[0].len();
    if b.is_empty() || b.len() != k {
        return Err(PanelError::DimensionMismatch(
            "matrix product inner dimensions mismatch".to_string(),
        ));
    }
    let n = b[0].len();
    let mut out = vec![vec![0.0; n]; a.len()];
    for i in 0..a.len() {
        for l in 0..k {
            let a_il = a[i][l];
            for j in 0..n {
                out[i][j] += a_il * b[l][j];
            }
        }
    }
    Ok(out)
}

/// Matrix inverse by Gauss-Jordan elimination.
#[allow(clippy::needless_range_loop)]
pub(crate) fn inverse(a: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, PanelError> {
    let n = a.len();
    if n == 0 {
        return Err(PanelError::DimensionMismatch(
            "empty matrix has no inverse".to_string(),
        ));
    }
    if a.iter().any(|row| row.len() != n) {
        return Err(PanelError::DimensionMismatch(
            "inverse requires a square matrix".to_string(),
        ));
    }
    let mut aug: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let mut row = a[i].clone();
            row.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            row
        })
        .collect();

    for col in 0..n {
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
            return Err(PanelError::Singular {
                model: "linalg",
                detail: format!("singular matrix (zero pivot at column {col})"),
            });
        }
        aug.swap(col, pivot);
        let inv_pivot = 1.0 / aug[col][col];
        for c in 0..2 * n {
            aug[col][c] *= inv_pivot;
        }
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = aug[row][col];
            for c in 0..2 * n {
                aug[row][c] -= factor * aug[col][c];
            }
        }
    }

    Ok(aug.iter().map(|row| row[n..].to_vec()).collect())
}

/// Vector dot product.
#[allow(dead_code)]
pub(crate) fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// The regularized incomplete beta function (Numerical Recipes `betai`).
///
/// Used for F-distribution survival probabilities in the GRS test.
pub(crate) fn incomplete_beta(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_factor = gamma_ln(a + b) - gamma_ln(a) - gamma_ln(b) + a * x.ln() + b * (1.0 - x).ln();
    if x < (a + 1.0) / (a + b + 2.0) {
        (ln_factor.exp() * betacf(a, b, x) / a).min(1.0)
    } else {
        (1.0 - (ln_factor.exp() * betacf(b, a, 1.0 - x) / b)).clamp(0.0, 1.0)
    }
}

/// Continued-fraction evaluation for the incomplete beta function.
fn betacf(a: f64, b: f64, x: f64) -> f64 {
    const MAX_ITER: usize = 200;
    const EPS: f64 = 3.0e-12;
    const FP_MIN: f64 = 1.0e-300;

    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FP_MIN {
        d = FP_MIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAX_ITER {
        let m2 = 2 * m;
        let mut aa = m as f64 * (b - m as f64) * x / ((qam + m2 as f64) * (a + m2 as f64));
        d = 1.0 + aa * d;
        if d.abs() < FP_MIN {
            d = FP_MIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FP_MIN {
            c = FP_MIN;
        }
        d = 1.0 / d;
        h *= d * c;
        aa = -(a + m as f64) * (qab + m as f64) * x / ((a + m2 as f64) * (qap + m2 as f64));
        d = 1.0 + aa * d;
        if d.abs() < FP_MIN {
            d = FP_MIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FP_MIN {
            c = FP_MIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

/// Natural log of the gamma function (Lanczos approximation).
fn gamma_ln(xx: f64) -> f64 {
    let cof = [
        76.180_091_729_471_46,
        -86.505_320_329_416_77,
        24.014_098_240_830_91,
        -1.231_739_572_450_155,
        0.120_865_097_386_617_9,
        -5.395_239_384_953e-5,
    ];
    let x = xx;
    let mut y = xx;
    let tmp = x + 5.5;
    let tmp = (x + 0.5) * tmp.ln() - tmp;
    let mut ser = 1.000_000_000_190_015;
    for &c in cof.iter() {
        y += 1.0;
        ser += c / y;
    }
    -tmp + (2.506_628_274_631_000_5 * ser / x).ln()
}

/// Regularized lower incomplete gamma `P(a, x)`.
pub(crate) fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        // Series representation.
        let mut term = 1.0 / a;
        let mut sum = term;
        let mut ap = a;
        for _ in 0..1000 {
            ap += 1.0;
            term *= x / ap;
            sum += term;
            if term.abs() < sum.abs() * 1.0e-14 {
                break;
            }
        }
        sum * (-x + a * x.ln() - gamma_ln(a)).exp()
    } else {
        // Continued-fraction representation: P = 1 - Q.
        1.0 - gamma_q_cf(a, x)
    }
}

/// Regularized upper incomplete gamma `Q(a, x)`.
pub(crate) fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        1.0 - gamma_p(a, x)
    } else {
        gamma_q_cf(a, x)
    }
}

/// Upper incomplete gamma via continued fraction (Lentz's algorithm).
fn gamma_q_cf(a: f64, x: f64) -> f64 {
    const FP_MIN: f64 = 1.0e-300;
    const EPS: f64 = 3.0e-14;

    let mut b = x + 1.0 - a;
    let mut c = 1.0 / FP_MIN;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..=1000 {
        let an = -i as f64 * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < FP_MIN {
            d = FP_MIN;
        }
        c = b + an / c;
        if c.abs() < FP_MIN {
            c = FP_MIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    (-x + a * x.ln() - gamma_ln(a)).exp() * h
}

/// Chi-square survival probability `P(X > x)` for `df` degrees of freedom.
pub(crate) fn chi_square_sf(df: f64, x: f64) -> f64 {
    gamma_q(df / 2.0, x / 2.0)
}
