//! Auth + small response wrappers + generic [`Page<T>`] cursor pagination.
//!
//! Per Plan C: `AuthMessageData`, `AuthTokenData`, and `RemoveOrdersResponse`
//! are re-exported verbatim from the codegen.
//!
//! - `RemoveOrdersResponse.removed` / `.noop` are strengthened to
//!   `Vec<crate::types::OrderId>` inside the codegen via
//!   `with_conversion` (see `predictfun/build.rs`); no per-batch glue lives
//!   in this file.
//! - `AuthMessageData` / `AuthTokenData` only carry plain `String` fields —
//!   no field-level strengthening is meaningful, so they are pure
//!   re-exports.
//!
//! [`Page<T>`] is the only domain-only type retained here: it is the
//! cross-endpoint cursor-pagination wrapper used by the `markets` /
//! `orders` / `positions` / `matches` REST helpers, and has no analogue
//! inside the codegen (each codegen `*Response` carries a sibling
//! `cursor: Option<String>` next to a per-endpoint `data` array).

use crate::error::PredictError;
use crate::types::OrderId;
use std::str::FromStr;

pub use crate::openapi::codegen::types::{AuthMessageData, AuthTokenData, RemoveOrdersResponse};

// ---------------------------------------------------------------------------
// Page<T> — generic cursor-paginated response
// ---------------------------------------------------------------------------

/// One page from a cursor-paginated endpoint. `cursor` is `Some` iff there
/// is a next page; pass it back as the `after` query param.
#[derive(Clone, Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub cursor: Option<String>,
}

impl<T> Page<T> {
    /// Construct from raw `(items, cursor)` after collecting/converting from
    /// a codegen `*Response` struct (which always exposes a `data` and
    /// optional `cursor` field).
    pub fn new(items: Vec<T>, cursor: Option<String>) -> Self {
        Self { items, cursor }
    }

    /// Convert each item with a fallible map; preserves the cursor.
    pub fn try_map<U, F>(self, mut f: F) -> Result<Page<U>, PredictError>
    where
        F: FnMut(T) -> Result<U, PredictError>,
    {
        let items = self.items.into_iter().map(&mut f).collect::<Result<Vec<_>, _>>()?;
        Ok(Page { items, cursor: self.cursor })
    }
}

// ---------------------------------------------------------------------------
// Helpers — parse opaque cursor strings into typed ids when needed.
// ---------------------------------------------------------------------------

/// Parse an arbitrary `&str` cursor into [`OrderId`]. Returned for symmetry
/// with the typed-id newtypes; predict.fun cursors are opaque server tokens.
pub fn parse_order_id(s: &str) -> OrderId {
    // Infallible.
    OrderId::from_str(s).unwrap()
}
