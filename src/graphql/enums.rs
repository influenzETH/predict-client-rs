//! GraphQL-side enums used by typed queries.
//!
//! These are **distinct from REST enums** in `crate::openapi::domain::enums`:
//! the GraphQL `MarketStatus` carries the full 10-variant on-chain lifecycle
//! (including pre-registration states), while the REST `MarketStatus` only
//! exposes the user-facing `OPEN/RESOLVED/...` subset.
//!
//! `MarketVariant` is duplicated here (rather than reusing the OpenAPI one)
//! to keep this layer free of cross-module type leakage; conversion happens
//! at the call boundary in `market_index.rs`.

#[cynic::schema_for_derives(file = "graphql/schema.graphql", module = "crate::graphql::schema")]
mod cynic_enums {
    /// `MarketStatus` as defined by the GraphQL schema (10 variants — the full
    /// on-chain lifecycle, superset of the REST surface).
    #[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MarketStatus {
        Initializing,
        Initialized,
        Creating,
        Created,
        Registered,
        PriceProposed,
        PriceDisputed,
        Paused,
        Unpaused,
        Resolved,
    }

    /// `MarketVariant` — controls UI template & trading mechanics.
    #[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MarketVariant {
        Default,
        SportsMatch,
        CryptoUpDown,
        TweetCount,
        SportsTeamMatch,
    }

    /// `OutcomeStatus` — resolution status (`null` while unresolved).
    #[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OutcomeStatus {
        Won,
        Lost,
    }

    /// `MarketSortInput` — sort modes for the top-level `markets` query.
    #[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MarketSortInput {
        #[cynic(rename = "VOLUME_24H_DESC")]
        Volume24hDesc,
        #[cynic(rename = "VOLUME_24H_ASC")]
        Volume24hAsc,
        VolumeTotalDesc,
        VolumeTotalAsc,
        PriorityAsc,
        PriorityDesc,
        ChanceAsc,
        ChanceDesc,
        #[cynic(rename = "VOLUME_24H_CHANGE_ASC")]
        Volume24hChangeAsc,
        #[cynic(rename = "VOLUME_24H_CHANGE_DESC")]
        Volume24hChangeDesc,
        #[cynic(rename = "CHANCE_24H_CHANGE_ASC")]
        Chance24hChangeAsc,
        #[cynic(rename = "CHANCE_24H_CHANGE_DESC")]
        Chance24hChangeDesc,
    }
}

pub use cynic_enums::{MarketSortInput, MarketStatus, MarketVariant, OutcomeStatus};
