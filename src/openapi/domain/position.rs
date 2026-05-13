//! Position re-export — Plan C boundary types.
//!
//! Under Plan C, the four numeric-string fields on `PositionData`
//! (`amount`, `valueUsd`, `averageBuyPriceUsd`, `pnlUsd`) are
//! type-strengthened **inside the codegen itself** via
//! `progenitor::GenerationSettings::with_conversion` (see
//! `predictfun/build.rs`):
//!
//! - `amount: alloy::primitives::U256` — CTF share-wei (18 decimals)
//! - `value_usd` / `average_buy_price_usd` / `pnl_usd: rust_decimal::Decimal` — USD
//!
//! `Outcome.on_chain_id: TokenId` propagates from the same Plan C
//! mechanism. The whole `PositionData` is therefore re-exported
//! verbatim under the spec name; no domain-side rename or mirror layer
//! remains.

use crate::openapi::domain::enums::OutcomeStatus;

pub use crate::openapi::codegen::types::PositionData;

/// Convenience: outcome resolution status (Won / Lost) if this position
/// is on a resolved market. Free function (rather than method) because
/// `PositionData` is an external (codegen) type and we cannot add inherent
/// impls to it from this crate's domain layer.
pub fn outcome_status(p: &PositionData) -> Option<OutcomeStatus> {
    p.outcome.status.map(OutcomeStatus::from)
}
