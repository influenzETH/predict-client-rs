//! Typed cynic GraphQL queries.
//!
//! Single entry point: the [`MarketsQuery`] query (1:1 mirror of
//! `Query.markets(filter, sort, pagination)`). Both the live GraphQL fetcher
//! and the snapshot persistence layer consume typed [`Market`] values
//! end-to-end — there is no longer a raw `serde_json::Value` escape hatch.
//!
//! ## Serde
//!
//! Every typed struct here carries `#[derive(Serialize)]` +
//! `#[serde(rename_all = "camelCase")]`. cynic auto-derives `Deserialize` for
//! `QueryFragment` types (response decoding), so we deliberately do **not**
//! add `#[derive(Deserialize)]` ourselves — that would collide with cynic's
//! impl (E0119). The combined effect:
//!
//!   * Snapshot **write**: serde Serialize → camelCase JSON, scalar newtypes
//!     transparently emit their inner `String` (cynic Scalar derive auto-impls
//!     Serialize).
//!   * Snapshot **read**: cynic's auto-derived Deserialize consumes that same
//!     camelCase JSON exactly as if it came off the GraphQL wire.
//!
//! Net: snapshot files are wire-shaped and round-trip through cynic without
//! a separate parsing layer.
//!
//! All custom scalars (`Address`, `BigIntString`, `DateTime`, `Timestamp`)
//! are kept as `String` newtypes per the project's D3 decision — newtype
//! upgrades happen field-by-field at the call boundary in a future change.

use super::enums::{MarketSortInput, MarketStatus, MarketVariant, OutcomeStatus};
use super::inputs::{ForwardPaginationInput, MarketFilterInput};
use super::scalars::{Address, BigIntString, DateTime};
use crate::types::{ConditionId, NegRiskMarketId, OracleQuestionId, TokenId};
use serde::Serialize;

/// Declarative macro for Relay-style `Connection` / `Edge` pairs.
///
/// cynic's `graphql_type` attribute requires a literal string per type, so a
/// generic `Connection<T>` is impossible — the only way to deduplicate the
/// `{ page_info, edges: Vec<Edge> }` + `{ node: T }` boilerplate is a
/// declarative macro. Expands to:
///   * `$conn { page_info: PageInfo, edges: Vec<$edge> }` (cynic QueryFragment)
///   * `$edge { node: $node }` (cynic QueryFragment)
///   * `impl $conn { nodes(&self), into_nodes(self) }` projection helpers
///
/// Each derive carries explicit `#[cynic(schema_path, schema_module)]`
/// attributes because the outer `#[cynic::schema_for_derives(...)]` attribute
/// macro only rewrites items already present at parse time — items produced
/// by `macro_rules!` expansion are invisible to it and must self-register.
///
/// `#[derive(Serialize)]` + `#[serde(rename_all = "camelCase")]` mirror the
/// outer struct convention so connections/edges round-trip through snapshot
/// files identically to the GraphQL wire shape.
///
/// Macro is intentionally limited to the bare `pageInfo + edges` shape;
/// selecting `totalCount` / `cursor` requires reverting to a hand-written
/// fragment.
macro_rules! relay_connection {
    ($conn:ident, $edge:ident => $node:ty) => {
        #[derive(cynic::QueryFragment, Serialize, Debug, Clone)]
        #[cynic(schema_path = "graphql/schema.graphql", schema_module = "crate::graphql::schema")]
        #[serde(rename_all = "camelCase")]
        pub struct $conn {
            pub page_info: PageInfo,
            pub edges: Vec<$edge>,
        }

        #[derive(cynic::QueryFragment, Serialize, Debug, Clone)]
        #[cynic(schema_path = "graphql/schema.graphql", schema_module = "crate::graphql::schema")]
        #[serde(rename_all = "camelCase")]
        pub struct $edge {
            pub node: $node,
        }

        impl $conn {
            /// Borrowing iterator over `edge.node` projections.
            pub fn nodes(&self) -> impl Iterator<Item = &$node> + '_ {
                self.edges.iter().map(|e| &e.node)
            }

            /// Owning projection — drops `Edge` wrappers, keeps `node`s.
            pub fn into_nodes(self) -> Vec<$node> {
                self.edges.into_iter().map(|e| e.node).collect()
            }
        }
    };
}

#[cynic::schema_for_derives(file = "graphql/schema.graphql", module = "crate::graphql::schema")]
mod cynic_queries {
    use super::*;

    /// `query MarketsQuery($filter, $sort, $pagination)` — one page of the
    /// top-level `markets` connection. Variables map 1:1 to the schema's
    /// `Query.markets(filter, sort, pagination)` arguments.
    #[derive(cynic::QueryFragment, Serialize, Debug, Clone)]
    #[cynic(graphql_type = "Query", variables = "MarketsQueryArguments")]
    #[serde(rename_all = "camelCase")]
    pub struct MarketsQuery {
        #[arguments(filter: $filter, sort: $sort, pagination: $pagination)]
        pub markets: MarketConnection,
    }

    impl MarketsQuery {
        /// Forward to `markets.page_info`.
        pub fn page_info(&self) -> &PageInfo {
            &self.markets.page_info
        }

        /// Borrowing iterator over flattened `Market` nodes.
        pub fn nodes(&self) -> impl Iterator<Item = &Market> + '_ {
            self.markets.nodes()
        }

        /// Owning projection — drops connection/edge wrappers.
        pub fn into_nodes(self) -> Vec<Market> {
            self.markets.into_nodes()
        }
    }

    #[derive(cynic::QueryVariables, Debug, Clone)]
    pub struct MarketsQueryArguments {
        pub filter: Option<MarketFilterInput>,
        pub sort: Option<MarketSortInput>,
        pub pagination: Option<ForwardPaginationInput>,
    }

    relay_connection!(MarketConnection, MarketEdge => Market);

    #[derive(cynic::QueryFragment, Serialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct PageInfo {
        pub has_next_page: bool,
        pub end_cursor: Option<String>,
    }

    /// `Market` — superset of fields persisted in the snapshot.
    ///
    /// Field selection mirrors the historic `markets_page.graphql` (32
    /// fields). Adding a field requires a one-line addition here; cynic's
    /// `cynic::QueryFragment` derive will reject schema mismatches.
    #[derive(cynic::QueryFragment, Serialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct Market {
        pub id: cynic::Id,
        pub title: String,
        pub title_translation_key: Option<String>,
        pub question: String,
        pub description: Option<String>,
        pub image_url: String,
        pub status: MarketStatus,
        pub question_index: Option<i32>,
        pub maker_fee_bps: i32,
        pub taker_fee_bps: i32,
        pub decimal_precision: i32,
        pub is_trading_enabled: bool,
        pub fee_multiplier: bool,
        /// `DateTime` scalar.
        pub fee_multiplier_start_time: Option<DateTime>,
        /// `DateTime` scalar.
        pub fee_multiplier_end_time: Option<DateTime>,
        pub point_cap_modifier: bool,
        /// `DateTime` scalar.
        pub created_at: DateTime,
        pub condition_id: ConditionId,
        pub oracle_question_id: OracleQuestionId,
        pub oracle_tx_hash: Option<String>,
        /// `Address` scalar.
        pub resolver_address: Address,
        /// `BigIntString` scalar.
        pub spread_threshold: BigIntString,
        /// `BigIntString` scalar.
        pub share_threshold: BigIntString,
        pub chance_percentage: Option<f64>,
        pub near_midpoint_liquidity_usd: Option<f64>,
        pub statistics: MarketStatistics,
        pub outcomes: OutcomeConnection,
        pub resolution: Option<Outcome>,
        pub category: Category,
    }

    #[derive(cynic::QueryFragment, Serialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct MarketStatistics {
        pub total_liquidity_usd: f64,
        pub volume_total_usd: f64,
        pub volume24h_usd: f64,
        pub volume24h_change_usd: Option<f64>,
        pub percentage_chance_change24h: Option<f64>,
    }

    relay_connection!(OutcomeConnection, OutcomeEdge => Outcome);

    #[derive(cynic::QueryFragment, Serialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct Outcome {
        pub id: cynic::Id,
        pub index: i32,
        pub name: String,
        pub image_url: Option<String>,
        pub status: Option<OutcomeStatus>,
        /// `BigIntString` scalar — decimal token ID, parsed at the cynic
        /// boundary into a typed [`TokenId`].
        pub on_chain_id: TokenId,
        pub chance_percentage: Option<f64>,
        pub bid_price_in_currency: Option<f64>,
        pub ask_price_in_currency: Option<f64>,
    }

    /// `Category` is a GraphQL **interface**; cynic models interface fields
    /// via `InlineFragments` when concrete-type fields are needed. We only
    /// query fields shared across all implementers, so a plain `QueryFragment`
    /// against the interface root is sufficient.
    #[derive(cynic::QueryFragment, Serialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct Category {
        pub id: cynic::Id,
        pub slug: String,
        pub title: String,
        pub description: Option<String>,
        pub image_url: String,
        pub status: super::CategoryStatusPlaceholder,
        pub market_variant: MarketVariant,
        pub resolution_provider: super::ResolutionProviderPlaceholder,
        pub is_neg_risk: bool,
        pub is_yield_bearing: bool,
        pub decimal_precision: i32,
        pub conversion_fee_bps: Option<i32>,
        /// `BigIntString` scalar — on-chain NegRisk `marketId`. Required by
        /// `convertPositions`; the canonical reason this query exists at all.
        /// Parsed at the cynic boundary into a typed [`NegRiskMarketId`].
        pub on_chain_id: Option<NegRiskMarketId>,
        /// `DateTime` scalar.
        pub starts_at: DateTime,
        /// `DateTime` scalar.
        pub ends_at: Option<DateTime>,
        /// `DateTime` scalar.
        pub created_at: DateTime,
    }
}

pub use cynic_queries::{
    Category, Market, MarketConnection, MarketEdge, MarketStatistics, MarketsQuery, MarketsQueryArguments, Outcome, OutcomeConnection, OutcomeEdge, PageInfo,
};

// ---------------------------------------------------------------------------
// Placeholder enum stubs for `Category` interface fields whose full enum
// definitions we don't currently consume. Keeping them as catch-all enums
// avoids forcing the caller to depend on schema enum identity for fields
// that flow straight into the snapshot as opaque strings.
// ---------------------------------------------------------------------------

#[cynic::schema_for_derives(file = "graphql/schema.graphql", module = "crate::graphql::schema")]
mod cynic_placeholders {
    /// Stub for `CategoryStatus` — only included so `Category.status`
    /// type-checks against the schema. Variants are kept minimal; downstream
    /// consumers should never branch on this directly.
    #[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
    #[cynic(graphql_type = "CategoryStatus")]
    pub enum CategoryStatusPlaceholder {
        Open,
        Resolved,
    }

    /// Stub for `ResolutionProvider`.
    #[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
    #[cynic(graphql_type = "ResolutionProvider")]
    pub enum ResolutionProviderPlaceholder {
        PredictDotFun,
        ThreePo,
        Chainlink,
    }
}

pub use cynic_placeholders::{CategoryStatusPlaceholder, ResolutionProviderPlaceholder};
