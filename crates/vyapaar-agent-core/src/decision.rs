//! Closed LLM Decision Parser and Safety Validation Harness.
//!
//! Language models are untrusted for direct execution.
//! This module strictly enforces:
//! - Closed enum parsing using [`vyapaar-core::Decision`] with `deny_unknown_fields`.
//! - Strict symbol universe verification (rejects unknown / undefined instruments).
//! - Strictly positive quantities and limit prices (zero float arithmetic).
//! - Mandatory SEBI `algo_id` format matching `^[A-Z0-9_]{4,32}$`.
//! - Grounding checks: asserts limit orders sit within instrument's active [`PriceBand`]
//!   and/or price collar bounds.
//! - Direct, mandatory pre-trade evaluation against [`vyapaar_risk::RiskEngine`].

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use vyapaar_core::{Decision, Price, PriceBand};
use vyapaar_risk::RiskEngine;

use crate::error::DecisionError;

/// Format specification for SEBI algo identifiers.
pub const SEBI_ALGO_ID_REGEX: &str = "^[A-Z0-9_]{4,32}$";

/// Checks if an algo ID strictly matches SEBI Retail Algo circular rules (`^[A-Z0-9_]{4,32}$`).
pub fn is_valid_sebi_algo_id(algo_id: &str) -> bool {
    (4..=32).contains(&algo_id.len())
        && algo_id
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Strict parser and safety validator for AI/LLM decisions.
#[derive(Debug, Clone)]
pub struct DecisionParser {
    /// Allowed trading universe symbols (if None, any non-empty symbol is allowed)
    pub allowed_universe: Option<HashSet<String>>,
    /// Maximum allowed deviation percentage for limit prices relative to market reference price.
    /// Default is 20.0 (i.e. ±20% price collar).
    pub max_price_deviation_pct: Decimal,
    /// Whether to strictly enforce the uppercase SEBI algo ID pattern (`^[A-Z0-9_]{4,32}$`).
    /// Defaults to true.
    pub strict_sebi_algo_format: bool,
}

impl Default for DecisionParser {
    fn default() -> Self {
        Self {
            allowed_universe: None,
            max_price_deviation_pct: dec!(20.00),
            strict_sebi_algo_format: true,
        }
    }
}

impl DecisionParser {
    /// Create a new decision parser with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an explicit allowed trading universe (e.g. NIFTY 100 constituent symbols).
    pub fn with_universe(mut self, symbols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let set: HashSet<String> = symbols.into_iter().map(Into::into).collect();
        self.allowed_universe = Some(set);
        self
    }

    /// Set maximum allowed limit price deviation from reference price (in percent).
    pub fn with_max_price_deviation(mut self, pct: Decimal) -> Self {
        self.max_price_deviation_pct = pct;
        self
    }

    /// Set whether strict SEBI algo ID format (`^[A-Z0-9_]{4,32}$`) is enforced.
    pub fn with_strict_sebi_algo_format(mut self, strict: bool) -> Self {
        self.strict_sebi_algo_format = strict;
        self
    }

    /// Parse raw LLM output text into [`vyapaar-core::Decision`], stripping markdown fences.
    ///
    /// Enforces `deny_unknown_fields`: any unauthorized field injected by the model
    /// will result in an immediate [`DecisionError::UnknownField`] or [`DecisionError::Json`].
    pub fn parse_str(&self, raw: &str) -> Result<Decision, DecisionError> {
        let cleaned = extract_json_payload(raw);
        if let Ok(decision) = serde_json::from_str::<Decision>(cleaned) {
            return Ok(decision);
        }

        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(cleaned) {
            let mut modified = false;
            if let Some(side_val) = val.get_mut("side") {
                if let Some(s) = side_val.as_str() {
                    let upper = s.to_uppercase();
                    if upper != s {
                        *side_val = serde_json::Value::String(upper);
                        modified = true;
                    }
                }
            }
            if let Some(action_val) = val.get_mut("action") {
                if let Some(a) = action_val.as_str() {
                    let lower = a.to_lowercase();
                    if lower != a {
                        *action_val = serde_json::Value::String(lower);
                        modified = true;
                    }
                }
            }
            if let Some(qty_val) = val.get_mut("qty") {
                if qty_val.is_number() {
                    *qty_val = serde_json::Value::String(qty_val.to_string());
                    modified = true;
                }
            }
            if let Some(price_val) = val.get_mut("price_limit") {
                if price_val.is_number() {
                    *price_val = serde_json::Value::String(price_val.to_string());
                    modified = true;
                }
            }
            if let Some(new_qty_val) = val.get_mut("new_qty") {
                if new_qty_val.is_number() {
                    *new_qty_val = serde_json::Value::String(new_qty_val.to_string());
                    modified = true;
                }
            }
            if let Some(new_price_val) = val.get_mut("new_price") {
                if new_price_val.is_number() {
                    *new_price_val = serde_json::Value::String(new_price_val.to_string());
                    modified = true;
                }
            }
            if modified {
                if let Ok(dec) = serde_json::from_value::<Decision>(val) {
                    return Ok(dec);
                }
            }
        }

        let decision: Decision = serde_json::from_str(cleaned).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("unknown field") {
                DecisionError::UnknownField(msg)
            } else {
                DecisionError::Json(msg)
            }
        })?;

        Ok(decision)
    }

    pub fn validate_algo_id(&self, algo_id: &str) -> Result<(), DecisionError> {
        let trimmed = algo_id.trim();
        if trimmed.is_empty() {
            return Err(DecisionError::EmptyAlgoId);
        }

        if self.strict_sebi_algo_format && !is_valid_sebi_algo_id(trimmed) {
            return Err(DecisionError::InvalidAlgoIdFormat {
                algo_id: trimmed.to_string(),
                expected_format: SEBI_ALGO_ID_REGEX,
            });
        }

        Ok(())
    }

    /// Perform strict safety validation and grounding check on a parsed [`Decision`].
    pub fn validate_decision(
        &self,
        decision: &Decision,
        reference_price: Option<Price>,
        price_band: Option<&PriceBand>,
    ) -> Result<(), DecisionError> {
        // Enforce core domain invariants
        if let Err(e) = decision.validate() {
            return Err(DecisionError::Json(e.to_string()));
        }

        match decision {
            Decision::PlaceOrder {
                symbol,
                qty,
                price_limit,
                algo_id,
                ..
            } => {
                let trimmed_sym = symbol.trim();
                if trimmed_sym.is_empty() {
                    return Err(DecisionError::EmptySymbol);
                }

                // Universe validation
                if let Some(universe) = &self.allowed_universe {
                    if !universe.contains(trimmed_sym) {
                        return Err(DecisionError::SymbolNotInUniverse(trimmed_sym.to_string()));
                    }
                }

                // Quantity must be strictly positive
                if !qty.is_positive() {
                    return Err(DecisionError::NonPositiveQuantity(qty.0));
                }

                // Algo ID validation
                self.validate_algo_id(algo_id)?;

                // Limit price validation & grounding check
                if let Some(limit) = price_limit {
                    if !limit.is_positive() {
                        return Err(DecisionError::NonPositivePrice(*limit));
                    }

                    // Scrip-wise price band grounding check
                    if let Some(band) = price_band {
                        if !band.contains(*limit) {
                            return Err(DecisionError::PriceBandViolation {
                                price: *limit,
                                band: *band,
                            });
                        }
                    }

                    // Price collar grounding check against market reference price
                    if let Some(ref_price) = reference_price {
                        if ref_price.is_positive() {
                            let diff = (limit.0 - ref_price.0).abs();
                            let deviation_pct = (diff / ref_price.0) * dec!(100.00);
                            if deviation_pct > self.max_price_deviation_pct {
                                return Err(DecisionError::UngroundedPrice {
                                    limit: *limit,
                                    reference: ref_price,
                                    deviation_pct: deviation_pct.round_dp(2),
                                    max_pct: self.max_price_deviation_pct,
                                });
                            }
                        }
                    }
                }

                Ok(())
            }
            Decision::ModifyOrder {
                new_qty,
                new_price,
                algo_id,
                ..
            } => {
                self.validate_algo_id(algo_id)?;

                if let Some(qty) = new_qty {
                    if !qty.is_positive() {
                        return Err(DecisionError::NonPositiveQuantity(qty.0));
                    }
                }

                if let Some(price) = new_price {
                    if !price.is_positive() {
                        return Err(DecisionError::NonPositivePrice(*price));
                    }
                    if let Some(band) = price_band {
                        if !band.contains(*price) {
                            return Err(DecisionError::PriceBandViolation {
                                price: *price,
                                band: *band,
                            });
                        }
                    }
                }

                Ok(())
            }
            Decision::CancelOrder { algo_id, .. } => {
                if let Some(id) = algo_id {
                    self.validate_algo_id(id)?;
                }
                Ok(())
            }
            Decision::CancelAll { algo_id, .. } => {
                if let Some(id) = algo_id {
                    self.validate_algo_id(id)?;
                }
                Ok(())
            }
            Decision::Hold { .. } => Ok(()),
        }
    }

    /// Parse raw LLM output and perform full safety validation.
    pub fn parse_and_validate(
        &self,
        raw: &str,
        reference_price: Option<Price>,
        price_band: Option<&PriceBand>,
    ) -> Result<Decision, DecisionError> {
        let decision = self.parse_str(raw)?;
        self.validate_decision(&decision, reference_price, price_band)?;
        Ok(decision)
    }

    /// Submit a validated decision to [`vyapaar_risk::RiskEngine`] pre-trade gate.
    pub fn evaluate_with_risk(
        &self,
        decision: &Decision,
        risk_engine: &mut RiskEngine,
        now: DateTime<Utc>,
    ) -> Result<(), DecisionError> {
        risk_engine
            .check_decision(decision, now)
            .map_err(DecisionError::RiskRejected)
    }
}

/// Helper to strip markdown JSON fences: ```json ... ``` or extract outer `{ ... }`.
pub fn extract_json_payload(raw: &str) -> &str {
    let s = raw.trim();
    let text = if let Some(stripped) = s.strip_prefix("```json") {
        if let Some(end) = stripped.find("```") {
            stripped[..end].trim()
        } else {
            stripped.trim()
        }
    } else if let Some(stripped) = s.strip_prefix("```") {
        if let Some(end) = stripped.find("```") {
            stripped[..end].trim()
        } else {
            stripped.trim()
        }
    } else {
        s
    };

    // Extract first balanced { ... } object if present
    if let Some(start) = text.find("{") {
        let mut depth = 0;
        let mut in_str = false;
        let mut escape = false;
        for (idx, ch) in text[start..].char_indices() {
            if escape {
                escape = false;
                continue;
            }
            match ch {
                '\\' if in_str => escape = true,
                '"' => in_str = !in_str,
                '{' if !in_str => depth += 1,
                '}' if !in_str => {
                    depth -= 1;
                    if depth == 0 {
                        return &text[start..=start + idx];
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = text.rfind("}") {
            if start <= end {
                return &text[start..=end];
            }
        }
    }

    text
}