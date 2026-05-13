//! Orderbook domain types — α path (slim domain wrapper).
//!
//! `OrderbookData.bids` / `.asks` arrive over the wire as `Vec<Vec<f64>>`
//! where each inner vec is positionally `[price, size]`. typify cannot
//! express that positional → named-struct reshape via `with_conversion`
//! (the codegen `Vec<Vec<f64>>` shape and a hypothetical
//! `Vec<OrderbookLevel{price, size}>` shape have no exact-equality
//! `SchemaObject` match), so the boundary `try_from` lives here as the
//! permitted Plan-C exception.
//!
//! All other id-shaped / numeric-string / enum fields on
//! `OrderbookData` and `LastOrderSettled` are already strengthened
//! inside the codegen via `with_conversion` (`market_id: MarketId`,
//! `id: OrderId`, `price: Decimal` left as `String` — see note below) so
//! this file only handles the `Vec<Vec<f64>>` → `Vec<OrderbookLevel>`
//! reshape and the `Decimal` `FromStr` parse for the lone
//! `LastOrderSettled.price` string field.
//!
//! Re-exported for callers that need the raw codegen view:
//! `pub use crate::openapi::codegen::types::OrderbookData`.

use crate::error::{PredictError, PredictResult};
use crate::openapi::codegen::types as gen;
use crate::openapi::domain::enums::{OutcomeSide, QuoteType};
use crate::types::{MarketId, OrderId};
use rust_decimal::Decimal;
use std::str::FromStr;

pub use crate::openapi::codegen::types::OrderbookData;

/// One side of one price level. Renamed from the previous `Level` to
/// disambiguate from `gen::PriceLevel` and similar codegen names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderbookLevel {
    pub price: Decimal,
    pub size: Decimal,
}

/// Parsed orderbook for a single market.
///
/// `bids` are descending in price; `asks` are ascending. Both are always on
/// the YES side (per predict.fun convention — the NO ask is `1 - YES bid`).
#[derive(Clone, Debug)]
pub struct Orderbook {
    pub market_id: MarketId,
    pub update_timestamp_ms: i64,
    pub bids: Vec<OrderbookLevel>,
    pub asks: Vec<OrderbookLevel>,
    pub last_order_settled: Option<LastOrderSettled>,
}

/// Last filled order on this market (snapshot-time auxiliary info).
#[derive(Clone, Debug)]
pub struct LastOrderSettled {
    pub id: OrderId,
    pub kind: String,
    pub market_id: MarketId,
    pub outcome: OutcomeSide,
    pub price: Decimal,
    pub side: QuoteType,
}

// ---------------------------------------------------------------------------
// Conversion: gen → domain (Plan-C-impossible reshape only)
// ---------------------------------------------------------------------------

fn pair_to_level(pair: &[f64], side_label: &'static str, market_id: MarketId) -> PredictResult<OrderbookLevel> {
    if pair.len() != 2 {
        return Err(PredictError::Other(format!(
            "orderbook market_id={} {} level has {} f64s, expected exactly [price, size]",
            market_id,
            side_label,
            pair.len()
        )));
    }
    let price =
        Decimal::try_from(pair[0]).map_err(|e| PredictError::Other(format!("orderbook market_id={} {} price f64→Decimal: {}", market_id, side_label, e)))?;
    let size =
        Decimal::try_from(pair[1]).map_err(|e| PredictError::Other(format!("orderbook market_id={} {} size f64→Decimal: {}", market_id, side_label, e)))?;
    Ok(OrderbookLevel { price, size })
}

impl TryFrom<gen::OrderbookData> for Orderbook {
    type Error = PredictError;

    fn try_from(g: gen::OrderbookData) -> Result<Self, Self::Error> {
        let market_id = g.market_id;
        let bids = g.bids.iter().map(|p| pair_to_level(p, "bid", market_id)).collect::<Result<Vec<_>, _>>()?;
        let asks = g.asks.iter().map(|p| pair_to_level(p, "ask", market_id)).collect::<Result<Vec<_>, _>>()?;
        let last_order_settled = g.last_order_settled.map(LastOrderSettled::try_from).transpose()?;
        Ok(Orderbook {
            market_id,
            update_timestamp_ms: g.update_timestamp_ms,
            bids,
            asks,
            last_order_settled,
        })
    }
}

impl TryFrom<gen::LastOrderSettled> for LastOrderSettled {
    type Error = PredictError;

    fn try_from(g: gen::LastOrderSettled) -> Result<Self, Self::Error> {
        // `price` stays `String` in the codegen (no `with_conversion` tag —
        // it's the only price-decimal field on this schema and adding a
        // dedicated tag for one site is overkill). Parse here.
        let price = Decimal::from_str(&g.price).map_err(|e| PredictError::Other(format!("LastOrderSettled.price '{}' parse: {}", g.price, e)))?;
        Ok(LastOrderSettled {
            id: g.id,
            kind: g.kind,
            market_id: g.market_id,
            outcome: g.outcome.into(),
            price,
            side: g.side.into(),
        })
    }
}
