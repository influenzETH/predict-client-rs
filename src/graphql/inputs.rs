//! GraphQL `input` object types used as query variables.
//!
//! Currently only `MarketFilterInput` (8 optional fields) — exposed as a
//! plain Rust struct with `#[derive(Default)]` so callers idiomatically write
//! `MarketFilterInput { is_resolved: Some(false), ..Default::default() }`.
//!
//! Custom scalars (`Timestamp` etc.) are kept as `String` here; callers
//! format them into the right shape before populating the input.

use super::enums::MarketVariant;
use super::scalars::Timestamp;

#[cynic::schema_for_derives(file = "graphql/schema.graphql", module = "crate::graphql::schema")]
mod cynic_inputs {
    use super::*;

    /// Mirror of GraphQL `MarketFilterInput`. All fields are optional —
    /// `None` means "no filter on this dimension".
    #[derive(cynic::InputObject, Debug, Clone, Default)]
    #[cynic(graphql_type = "MarketFilterInput")]
    pub struct MarketFilterInput {
        /// Restrict to markets in a specific category (numeric ID as string).
        pub category_id: Option<cynic::Id>,
        /// `true` = resolved only, `false` = unresolved only, `None` = both.
        pub is_resolved: Option<bool>,
        /// `true` = only markets with an active fee multiplier promotion.
        pub is_boosted: Option<bool>,
        /// Restrict to markets whose parent category ends before this Unix
        /// timestamp (`Timestamp` scalar, formatted as a string by the caller).
        pub ends_before: Option<Timestamp>,
        /// Single tag ID.
        pub tag: Option<cynic::Id>,
        /// Multiple tag IDs (takes precedence over `tag` when both set).
        pub tags: Option<Vec<cynic::Id>>,
        /// Restrict to markets of a specific variant.
        pub market_variant: Option<MarketVariant>,
        /// Authenticated-only: bookmarked markets only. Silently ignored
        /// for unauthenticated requests (and we never authenticate, so
        /// always `None` in practice).
        pub bookmarked: Option<bool>,
    }

    /// Mirror of GraphQL `ForwardPaginationInput`.
    #[derive(cynic::InputObject, Debug, Clone, Default)]
    #[cynic(graphql_type = "ForwardPaginationInput")]
    pub struct ForwardPaginationInput {
        pub first: Option<i32>,
        pub after: Option<String>,
    }
}

pub use cynic_inputs::{ForwardPaginationInput, MarketFilterInput};
