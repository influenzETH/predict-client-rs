//! `GET /v1/positions` (caller's positions, JWT-auth) and
//! `GET /v1/positions/{address}` (third-party, API-KEY only).
//!
//! The two endpoints share the same query schema; both wrappers accept a
//! shared [`GetPositionsOpts`] options struct.

use crate::error::PredictResult;
use crate::openapi::codegen::ClientPositionsExt;
use crate::openapi::domain::enums::PositionSort;
use crate::openapi::domain::{Page, PositionData};
use crate::openapi::OpenApiClient;
use crate::types::MarketId;

use alloy::primitives::Address;

/// Optional fields shared by both `get_my_positions` and `get_positions`.
#[derive(Clone, Debug, Default)]
pub struct GetPositionsOpts {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub category_id: Option<String>,
    pub is_resolved: Option<bool>,
    pub market_id: Option<MarketId>,
    pub sort: Option<PositionSort>,
}

impl OpenApiClient {
    /// `GET /v1/positions` — caller's open + resolved positions (JWT-auth).
    pub async fn get_my_positions(&self, opts: GetPositionsOpts) -> PredictResult<Page<PositionData>> {
        let mut req = self.gen.get_positions();
        if let Some(v) = opts.first {
            req = req.first(v);
        }
        if let Some(v) = opts.after {
            req = req.after(v);
        }
        if let Some(v) = opts.category_id {
            req = req.category_id(v);
        }
        if let Some(v) = opts.is_resolved {
            req = req.is_resolved(v);
        }
        if let Some(v) = opts.market_id {
            req = req.market_id(v.0);
        }
        if let Some(v) = opts.sort {
            req = req.sort(crate::openapi::codegen::types::PositionSort::from(v));
        }
        let resp = req.send().await?.into_inner();
        Ok(Page::new(resp.data, resp.cursor))
    }

    /// `GET /v1/positions/{address}` — third-party Kernel-account positions
    /// (API-KEY only; no JWT required).
    pub async fn get_positions(&self, address: Address, opts: GetPositionsOpts) -> PredictResult<Page<PositionData>> {
        let mut req = self.gen.get_positions_by_address().address(address.to_string());
        if let Some(v) = opts.first {
            req = req.first(v);
        }
        if let Some(v) = opts.after {
            req = req.after(v);
        }
        if let Some(v) = opts.category_id {
            req = req.category_id(v);
        }
        if let Some(v) = opts.is_resolved {
            req = req.is_resolved(v);
        }
        if let Some(v) = opts.market_id {
            req = req.market_id(v.0);
        }
        if let Some(v) = opts.sort {
            req = req.sort(crate::openapi::codegen::types::PositionSort::from(v));
        }
        let resp = req.send().await?.into_inner();
        Ok(Page::new(resp.data, resp.cursor))
    }
}
