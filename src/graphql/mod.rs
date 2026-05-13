//! GraphQL endpoint wrappers for predict.fun.
//!
//! The official GraphQL endpoint (`https://graphql.predict.fun/graphql`)
//! exposes richer market data than the REST API — most importantly
//! `category.onChainId`, the on-chain NegRisk `marketId` required for
//! `convertPositions`.
//!
//! This module is built on [`cynic`] (3.13). Schema is the SDL dump in
//! `predictfun/graphql/schema.graphql`, regenerated via:
//!
//! ```bash
//! cargo install cynic-cli --version 3.13.2
//! cynic introspect https://graphql.predict.fun/graphql \
//!   -o predictfun/graphql/schema.graphql
//! ```
//!
//! Custom scalars (`Address`, `BigIntString`, `DateTime`, `Timestamp`) are
//! aliased to `String` at the cynic boundary; parsing into newtypes / `U256`
//! / `chrono::DateTime` is a per-call-site concern.
//!
//! API surface: stateful [`GraphQLClient`] holding a shared
//! [`HttpClient`](reqwest::Client) + endpoint URL. JWT is intentionally NOT
//! forwarded — the GraphQL endpoint is unauthenticated, and the shared
//! `x-api-key` default header on the HTTP client is silently ignored by
//! the server.
//!
//! Methods are typed end-to-end: [`query_markets`](GraphQLClient::query_markets)
//! returns a single [`MarketConnection`]; [`query_all_markets`](GraphQLClient::query_all_markets)
//! paginates the entire connection and returns `Vec<Market>`. There is no
//! raw JSON path — snapshot persistence (`snapshot.rs`) round-trips through
//! the typed model.

use crate::error::{PredictError, PredictResult};

use cynic::http::CynicReqwestError;
use cynic::{Operation, QueryBuilder};
use reqwest::Client as HttpClient;
use url::Url;

mod enums;
mod inputs;
pub mod queries;
mod scalars;
pub(crate) mod schema;

pub use enums::{MarketSortInput, MarketStatus, MarketVariant, OutcomeStatus};
pub use inputs::{ForwardPaginationInput, MarketFilterInput};
pub use queries::{
    Category, Market, MarketConnection, MarketEdge, MarketStatistics, MarketsQuery, MarketsQueryArguments, Outcome, OutcomeConnection, OutcomeEdge, PageInfo,
};
pub use scalars::{Address, BigIntString, DateTime, Timestamp};

/// Public GraphQL endpoint.
pub const GRAPHQL_ENDPOINT: &str = "https://graphql.predict.fun/graphql";

/// Stateful GraphQL client wrapping a shared [`HttpClient`] + endpoint URL.
#[derive(Clone)]
pub struct GraphQLClient {
    http: HttpClient,
    endpoint: Url,
}

impl GraphQLClient {
    /// Construct with the public default endpoint ([`GRAPHQL_ENDPOINT`]).
    pub fn with_default_endpoint(http: HttpClient) -> PredictResult<Self> {
        Self::new(http, GRAPHQL_ENDPOINT)
    }

    /// Construct with a custom endpoint URL.
    pub fn new(http: HttpClient, endpoint: &str) -> PredictResult<Self> {
        let endpoint = Url::parse(endpoint).map_err(|e| PredictError::GraphQl(format!("invalid GraphQL endpoint `{}`: {}", endpoint, e)))?;
        Ok(Self { http, endpoint })
    }

    /// Endpoint URL.
    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    /// Send a single typed cynic [`Operation`] and return the typed response
    /// data, mapping errors / GraphQL-level errors into [`PredictError::GraphQl`].
    pub async fn execute<ResponseData, Vars>(&self, operation: Operation<ResponseData, Vars>) -> PredictResult<ResponseData>
    where
        Vars: serde::Serialize,
        ResponseData: serde::de::DeserializeOwned + 'static,
    {
        use cynic::http::ReqwestExt;

        let resp = self.http.post(self.endpoint.clone()).run_graphql(operation).await.map_err(|e| match e {
            CynicReqwestError::ReqwestError(re) => PredictError::Network(re),
            CynicReqwestError::ErrorResponse(status, body) => PredictError::GraphQl(format!("HTTP {}: {}", status, body)),
        })?;

        if let Some(errs) = resp.errors {
            if !errs.is_empty() {
                return Err(PredictError::GraphQl(format!("{:?}", errs)));
            }
        }
        resp.data.ok_or_else(|| PredictError::GraphQl("response missing `data`".to_string()))
    }

    /// Execute the typed `MarketsQuery` query — one page of the
    /// `Query.markets(filter, sort, pagination)` connection.
    ///
    /// Arguments are passed verbatim to GraphQL (1:1 with the schema). Pass
    /// `None` for any dimension you don't want to filter / sort / paginate
    /// on.
    pub async fn query_markets(
        &self,
        filter: Option<MarketFilterInput>,
        sort: Option<MarketSortInput>,
        pagination: Option<ForwardPaginationInput>,
    ) -> PredictResult<MarketConnection> {
        let op = MarketsQuery::build(MarketsQueryArguments { filter, sort, pagination });
        let data = self.execute(op).await?;
        Ok(data.markets)
    }

    /// Paginate the entire `markets` connection and collect typed [`Market`]
    /// nodes.
    ///
    /// `filter` is passed through verbatim; pass `None` for the unfiltered
    /// universe. `sort` is intentionally not exposed — pagination cursors
    /// are sort-order-dependent and we have no caller that needs a custom
    /// sort during full traversal.
    ///
    /// Page size is hard-coded to the server cap of 100. `progress` is
    /// invoked after each page with `(page_number, page_count,
    /// cumulative_total)`.
    pub async fn query_all_markets(&self, filter: Option<MarketFilterInput>, mut progress: impl FnMut(usize, usize, usize)) -> PredictResult<Vec<Market>> {
        let mut all: Vec<Market> = Vec::new();
        let mut after: Option<String> = None;
        let mut page = 0usize;

        loop {
            page += 1;
            let pagination = ForwardPaginationInput {
                first: Some(100),
                after: after.clone(),
            };
            let conn = self.query_markets(filter.clone(), None, Some(pagination)).await?;
            let has_next_page = conn.page_info.has_next_page;
            let end_cursor = conn.page_info.end_cursor.clone();
            let nodes = conn.into_nodes();
            let n = nodes.len();
            all.extend(nodes);
            progress(page, n, all.len());
            if !has_next_page {
                break;
            }
            match end_cursor {
                Some(c) if !c.is_empty() => after = Some(c),
                _ => break,
            }
        }

        Ok(all)
    }
}
