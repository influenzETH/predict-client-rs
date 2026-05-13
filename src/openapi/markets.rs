//! `GET /v1/markets` and `GET /v1/markets/{id}`.
//!
//! Both endpoints accept optional auth (work with API-KEY only); JWT is
//! still attached when present, harmlessly.
//!
//! The Market sub-tree is treated as a black-box display payload — the
//! domain layer re-exports the codegen types verbatim (see
//! `domain::market`), so the wrappers return [`MarketWithStats`] / `Market`
//! directly. Callers that need typed [`MarketId`] / [`ConditionId`] /
//! [`TokenId`] convert at the call site.
//!
//! Pagination uses the shared [`Page<T>`] envelope.

use crate::error::PredictResult;
use crate::openapi::codegen::ClientMarketsExt;
use crate::openapi::domain::enums::{MarketSort, MarketStatusFilter, MarketVariant};
use crate::openapi::domain::market::MarketWithStats;
use crate::openapi::domain::Page;
use crate::openapi::OpenApiClient;
use crate::types::MarketId;

/// Optional fields for [`OpenApiClient::get_markets`].
///
/// `tag_ids` is a comma-separated list of integer tag ids (the OpenAPI spec
/// declares it as a single string, not an array).
#[derive(Clone, Debug, Default)]
pub struct GetMarketsOpts {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub has_active_rewards: Option<bool>,
    pub include_stats: Option<bool>,
    pub is_boosted: Option<bool>,
    pub market_variant: Option<MarketVariant>,
    pub sort: Option<MarketSort>,
    pub status: Option<MarketStatusFilter>,
    pub tag_ids: Option<String>,
}

impl OpenApiClient {
    /// `GET /v1/markets/{id}` — single-market metadata + (optional) stats.
    pub async fn get_market(&self, id: MarketId, include_stats: Option<bool>) -> PredictResult<MarketWithStats> {
        let mut req = self.gen.get_markets_by_id().id(id.0);
        if let Some(v) = include_stats {
            req = req.include_stats(v);
        }
        Ok(req.send().await?.into_inner().data)
    }

    /// `GET /v1/markets` — paginated market index.
    pub async fn get_markets(&self, opts: GetMarketsOpts) -> PredictResult<Page<MarketWithStats>> {
        let mut req = self.gen.get_markets();
        if let Some(v) = opts.first {
            req = req.first(v);
        }
        if let Some(v) = opts.after {
            req = req.after(v);
        }
        if let Some(v) = opts.has_active_rewards {
            req = req.has_active_rewards(v);
        }
        if let Some(v) = opts.include_stats {
            req = req.include_stats(v);
        }
        if let Some(v) = opts.is_boosted {
            req = req.is_boosted(v);
        }
        if let Some(v) = opts.market_variant {
            req = req.market_variant(crate::openapi::codegen::types::MarketVariant::from(v));
        }
        if let Some(v) = opts.sort {
            req = req.sort(crate::openapi::codegen::types::MarketSort::from(v));
        }
        if let Some(v) = opts.status {
            req = req.status(crate::openapi::codegen::types::MarketStatusFilter::from(v));
        }
        if let Some(v) = opts.tag_ids {
            req = req.tag_ids(v);
        }
        let resp = req.send().await?.into_inner();
        Ok(Page::new(resp.data, resp.cursor))
    }

    /// Paginate the entire `GET /v1/markets` cursor and collect every
    /// [`MarketWithStats`].
    ///
    /// `base_opts.first` is overridden to 100 (the server cap mirrors
    /// GraphQL's `first` cap). `base_opts.after` is the starting cursor
    /// (typically `None`); subsequent pages use the server-issued cursor.
    /// All other filter fields are passed through verbatim on every page.
    ///
    /// `progress` is invoked after each page with
    /// `(page_number, page_count, cumulative_total)`, mirroring
    /// `crate::graphql::GraphQLClient::query_all_markets`.
    pub async fn get_all_markets(&self, mut base_opts: GetMarketsOpts, mut progress: impl FnMut(usize, usize, usize)) -> PredictResult<Vec<MarketWithStats>> {
        base_opts.first = Some(100);
        let mut after: Option<String> = base_opts.after.take();
        let mut all: Vec<MarketWithStats> = Vec::new();
        let mut page = 0usize;

        loop {
            page += 1;
            let mut opts = base_opts.clone();
            opts.after = after.clone();
            let resp = self.get_markets(opts).await?;
            let n = resp.items.len();
            all.extend(resp.items);
            progress(page, n, all.len());
            match resp.cursor {
                Some(c) if !c.is_empty() => after = Some(c),
                _ => break,
            }
        }

        Ok(all)
    }
}
