//! Errors raised by the forecast engines.

use thiserror::Error;

/// Errors produced by the forecasting engines.
#[derive(Debug, Error, Clone, PartialEq)]
#[non_exhaustive]
pub enum ForecastError {
    /// Not enough observations for the requested computation.
    #[error("insufficient data: needed {needed} observations, got {got}")]
    InsufficientData {
        /// Minimum number of observations required.
        needed: usize,
        /// Observations actually supplied.
        got: usize,
    },

    /// A configuration argument was invalid (zero horizon, negative window,
    /// non-finite interval probability, etc.).
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// A series contained a non-finite value (NaN / infinity).
    #[error("series must contain only finite values")]
    NonFinite,

    /// A division by zero occurred inside a model fit.
    #[error("division by zero in {model}: {detail}")]
    DivisionByZero {
        /// Model that hit the zero divisor.
        model: &'static str,
        /// Description of the failing denominator.
        detail: String,
    },

    /// A numerical estimation loop failed to converge.
    #[error("convergence failure in {model}")]
    Convergence {
        /// Model that failed to converge.
        model: &'static str,
    },
}
