//! Domain-facing aliases for every wire-level enum the 13 wrappers touch.
//!
//! These are **transparent `pub use` re-exports** of the codegen enums.
//! The wire shape, variant set, and `#[serde(rename = "...")]` strings
//! match 1:1 with the OpenAPI spec, so a hand-written mirror layer would
//! be pure duplication. The module is kept (rather than deleted) so that
//! domain code, request-param wrappers (`*Opts`), `MarketIndex`, and CLI
//! call sites can keep their stable
//! `use crate::openapi::domain::enums::{...}` import paths and remain
//! agnostic to the codegen module path.
//!
//! If a future spec change requires *non-trivial* divergence (renaming,
//! variant subsetting, custom serde shape), replace the relevant
//! `pub use` with a hand-written enum + `From`/`Into` bridge here — the
//! abstraction boundary is preserved.

use crate::openapi::codegen::types as gen;

pub use gen::{
    FeeType, MarketSort, MarketStatus, MarketStatusFilter, MarketTradingStatus, MarketVariant, OrderStatus, OrderStatusFilter, OrderStrategy, OutcomeSide,
    OutcomeStatus, PositionSort, QuoteType, ReservedBalancePolicy, SelfTradePreventionStrategy,
};
