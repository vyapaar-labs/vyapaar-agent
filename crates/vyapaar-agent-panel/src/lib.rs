//! # व्यापार・パネル vyapaar-agent-panel
//!
//! Research-grade panel econometrics (plan 0003 Phase 3):
//!
//! - **Fixed-effects** and **random-effects** panel regression over
//!   security-return panels.
//! - **Fama-MacBeth** two-pass cross-sectional regression.
//! - **Instrumental-variables / GMM** estimation via two-stage least squares
//!   (2SLS) with a Sargan overidentification test.
//! - **Factor asset-pricing tests** (Gibbons–Ross–Shanken joint alpha test).
//!
//! Scope boundary: this is research-grade econometrics for validating factor
//! models on a research desk; nothing in the live pipeline calls it on a hot
//! path. It has the lowest implementation priority of the three 0003 engines
//! and depends only on [`vyapaar_core`] (per the plan) — the numeric kernels
//! are self-contained `f64` linear algebra on return series, matching the
//! `vyapaar-agent-analytics` convention.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unsafe_code)]

pub mod asset_pricing;
pub mod error;
pub mod fama_macbeth;
pub mod fixed_effects;
pub mod gmm;
pub mod matrix;
pub mod random_effects;

pub use asset_pricing::{GrsTestResult, grs_test};
pub use error::PanelError;
pub use fama_macbeth::{FamaMacBethResult, fama_macbeth};
pub use fixed_effects::{FixedEffectsResult, fixed_effects};
pub use gmm::{TwoStageLeastSquaresResult, two_stage_least_squares};
pub use random_effects::{RandomEffectsResult, random_effects};
