//! `POST /v1/orders`, `GET /v1/orders[/{hash}]`, `POST /v1/orders/remove`,
//! `GET /v1/orders/matches`.
//!
//! All endpoints require JWT (auto-attached by the inner `JwtState` pre-hook).
//!
//! `cancel_orders_chunked` auto-splits at the documented 100 ids/call server
//! cap; results are concatenated into a single [`RemoveOrdersResponse`].

use crate::error::{PredictError, PredictResult};
use crate::openapi::codegen::{types as gen, ClientOrdersExt};
use crate::openapi::domain::enums::{OrderStatusFilter, OrderStrategy, ReservedBalancePolicy, SelfTradePreventionStrategy};
use crate::openapi::domain::{CreateOrderResponseData, MatchData, OrderData, Page, RemoveOrdersResponse};
use crate::openapi::OpenApiClient;
use crate::types::{MarketId, OrderHash, OrderId};

use alloy::primitives::U256;

/// Documented server cap for `POST /v1/orders/remove`.
const REMOVE_ORDERS_MAX_PER_CALL: usize = 100;

/// Optional fields for [`OpenApiClient::post_order`].
#[derive(Clone, Debug, Default)]
pub struct PostOrderOpts {
    pub slippage_bps: Option<String>,
    pub is_fill_or_kill: Option<bool>,
    pub is_post_only: Option<bool>,
    pub is_min_amount_out: Option<bool>,
    pub reserved_balance_policy: Option<ReservedBalancePolicy>,
    pub self_trade_prevention: Option<SelfTradePreventionStrategy>,
}

/// Optional fields for [`OpenApiClient::get_orders`].
#[derive(Clone, Debug, Default)]
pub struct GetOrdersOpts {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub status: Option<OrderStatusFilter>,
}

/// Optional fields for [`OpenApiClient::get_matches`].
#[derive(Clone, Debug, Default)]
pub struct GetMatchesOpts {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub category_id: Option<String>,
    pub is_signer_maker: Option<bool>,
    pub market_id: Option<MarketId>,
    pub min_value_usdt_wei: Option<String>,
    pub signer_address: Option<String>,
}

impl OpenApiClient {
    /// `POST /v1/orders` — create a single signed order.
    ///
    /// `contract` is the EIP-712 payload produced by `OrderBuilder`;
    /// `price_per_share_wei` is the probability scaled to 1e-18 units (e.g.
    /// `0.5` USDT/share → `500_000_000_000_000_000`). The matcher rejects
    /// human-readable decimals (`CreateOrderInvalidNumericValue`); sdk-python
    /// `OrderAmounts.price_per_share` is wei-typed for the same reason.
    pub async fn post_order(
        &self,
        contract: gen::ContractOrder,
        price_per_share_wei: U256,
        strategy: OrderStrategy,
        opts: PostOrderOpts,
    ) -> PredictResult<CreateOrderResponseData> {
        let data = gen::CreateOrderData {
            order: contract,
            price_per_share: price_per_share_wei.to_string(),
            strategy,
            slippage_bps: opts.slippage_bps,
            is_fill_or_kill: opts.is_fill_or_kill,
            is_post_only: opts.is_post_only,
            is_min_amount_out: opts.is_min_amount_out,
            reserved_balance_policy: opts.reserved_balance_policy,
            self_trade_prevention: opts.self_trade_prevention,
        };
        let body = gen::CreateOrderRequest { data };
        let resp = self.gen.post_orders().body(body).send().await?;
        Ok(resp.into_inner().data)
    }

    /// `GET /v1/orders/{hash}` — fetch a single order by its on-chain hash.
    pub async fn get_order(&self, hash: &OrderHash) -> PredictResult<OrderData> {
        let hash_str = hash.to_string();
        let resp = self.gen.get_orders_by_hash().hash(hash_str).send().await?;
        Ok(resp.into_inner().data)
    }

    /// `GET /v1/orders` — caller's orders (paginated).
    pub async fn get_orders(&self, opts: GetOrdersOpts) -> PredictResult<Page<OrderData>> {
        let mut req = self.gen.get_orders();
        if let Some(v) = opts.first {
            req = req.first(v);
        }
        if let Some(v) = opts.after {
            req = req.after(v);
        }
        if let Some(v) = opts.status {
            req = req.status(gen::OrderStatusFilter::from(v));
        }
        let resp = req.send().await?.into_inner();
        Ok(Page::new(resp.data, resp.cursor))
    }

    /// `GET /v1/orders/matches` — caller's match history (paginated).
    pub async fn get_matches(&self, opts: GetMatchesOpts) -> PredictResult<Page<MatchData>> {
        let mut req = self.gen.get_orders_matches();
        if let Some(v) = opts.first {
            req = req.first(v);
        }
        if let Some(v) = opts.after {
            req = req.after(v);
        }
        if let Some(v) = opts.category_id {
            req = req.category_id(v);
        }
        if let Some(v) = opts.is_signer_maker {
            req = req.is_signer_maker(v);
        }
        if let Some(v) = opts.market_id {
            req = req.market_id(v.0);
        }
        if let Some(v) = opts.min_value_usdt_wei {
            req = req.min_value_usdt_wei(v);
        }
        if let Some(v) = opts.signer_address {
            req = req.signer_address(v);
        }
        let resp = req.send().await?.into_inner();
        Ok(Page::new(resp.data, resp.cursor))
    }

    /// `POST /v1/orders/remove` — soft-cancel orders by id, auto-chunking at
    /// the documented 100 ids/call server cap.
    ///
    /// Does NOT broadcast an on-chain cancel. Returned `removed`/`noop`
    /// arrays are `Vec<OrderId>` (substituted at the codegen boundary by
    /// `with_conversion`); the chunked wrapper concatenates them across
    /// calls and preserves `success = true` only when every chunk
    /// succeeded.
    pub async fn cancel_orders_chunked(&self, ids: Vec<OrderId>) -> PredictResult<RemoveOrdersResponse> {
        if ids.len() <= REMOVE_ORDERS_MAX_PER_CALL {
            return self.cancel_orders_one_chunk(ids).await;
        }
        let mut acc = RemoveOrdersResponse {
            success: true,
            removed: Vec::new(),
            noop: Vec::new(),
        };
        for chunk in ids.chunks(REMOVE_ORDERS_MAX_PER_CALL) {
            let part = self.cancel_orders_one_chunk(chunk.to_vec()).await?;
            acc.success = acc.success && part.success;
            acc.removed.extend(part.removed);
            acc.noop.extend(part.noop);
        }
        Ok(acc)
    }

    async fn cancel_orders_one_chunk(&self, ids: Vec<OrderId>) -> PredictResult<RemoveOrdersResponse> {
        let body = gen::RemoveOrdersRequest {
            data: gen::RemoveOrdersData {
                ids: ids.into_iter().map(|id| id.0).collect(),
            },
        };
        let resp = self.gen.post_orders_remove().body(body).send().await?;
        Ok(resp.into_inner())
    }
}

// Silence unused-import warning until/unless we touch PredictError directly here.
const _: fn() = || {
    let _: PredictError;
};
