//! Errors raised by the panel econometrics engines.

use thiserror::Error;

/// Errors produced by panel regression, IV/GMM, and asset-pricing tests.
#[derive(Debug, Error, Clone, PartialEq)]
#[non_exhaustive]
pub enum PanelError {
    /// Not enough observations for the requested estimator.
    #[error("insufficient data: needed {needed}, got {got}")]
    InsufficientData {
        /// Minimum observations required.
        needed: usize,
        /// Observations actually supplied.
        got: usize,
    },

    /// A value was non-finite (NaN / infinity).
    #[error("input must contain only finite values")]
    NonFinite,

    /// A matrix was singular (zero pivot) during estimation.
    #[error("singular matrix in {model}: {detail}")]
    Singular {
        /// Estimator that hit the singular matrix.
        model: &'static str,
        /// Description of the failing matrix.
        detail: String,
    },

    /// Input dimension mismatch.
    #[error("dimension mismatch: {0}")]
    DimensionMismatch(String),
}
