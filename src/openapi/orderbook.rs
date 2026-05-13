//! `GET /v1/markets/{id}/orderbook` and `GET /v1/markets/orderbooks?ids=…`.
//!
//! Both endpoints require JWT (auto-attached by the inner `JwtState`).
//!
//! `get_orderbooks_chunked` auto-splits at the documented 100 ids/call cap;
//! results are merged into one flat `Vec<Orderbook>`. Markets without a book
//! are silently omitted by the server.

use crate::error::PredictResult;
use crate::openapi::codegen::ClientMarketsExt;
use crate::openapi::domain::Orderbook;
use crate::openapi::OpenApiClient;
use crate::types::MarketId;

/// Server cap for `GET /v1/markets/orderbooks?ids=…`.
const ORDERBOOKS_MAX_PER_CALL: usize = 100;

impl OpenApiClient {
    /// `GET /v1/markets/{id}/orderbook` — single-market book snapshot.
    pub async fn get_orderbook(&self, market_id: MarketId) -> PredictResult<Orderbook> {
        let resp = self.gen.get_markets_by_id_orderbook().id(market_id.0).send().await?;
        resp.into_inner().data.try_into()
    }

    /// `GET /v1/markets/orderbooks?ids=…` — bulk fetch (≤100 per call;
    /// auto-chunked transparently).
    pub async fn get_orderbooks_chunked(&self, ids: &[MarketId]) -> PredictResult<Vec<Orderbook>> {
        if ids.len() <= ORDERBOOKS_MAX_PER_CALL {
            return self.get_orderbooks_one_chunk(ids).await;
        }
        let mut acc = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(ORDERBOOKS_MAX_PER_CALL) {
            let part = self.get_orderbooks_one_chunk(chunk).await?;
            acc.extend(part);
        }
        Ok(acc)
    }

    async fn get_orderbooks_one_chunk(&self, ids: &[MarketId]) -> PredictResult<Vec<Orderbook>> {
        let raw_ids: Vec<i64> = ids.iter().map(|m| m.0).collect();
        let resp = self.gen.get_markets_orderbooks().ids(raw_ids).send().await?;
        resp.into_inner().data.into_iter().map(Orderbook::try_from).collect()
    }
}
