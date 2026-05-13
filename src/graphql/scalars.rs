//! Scalar newtype wrappers for the predict.fun GraphQL endpoint.
//!
//! cynic requires every GraphQL custom scalar to be backed by a Rust type
//! that `impl Scalar<SchemaScalar>` — the workspace project rule (D3) keeps
//! the wire representation as `String` for all four custom scalars
//! (`Address`, `BigIntString`, `DateTime`, `Timestamp`), so each wrapper is
//! a thin `pub struct Foo(pub String)` deriving [`cynic::Scalar`].
//!
//! Callers `Deref` to `&str` or move out the inner `String`; parsing into
//! `U256` / `chrono::DateTime` / `Address` newtypes happens at the call
//! boundary and is intentionally NOT done here.
//!
//! When a future change wants strong typing on a particular field, swap
//! the field's Rust type from one of these wrappers to a newtype that
//! `impl Scalar<MatchingSchemaScalar>` — cynic's per-field type override
//! makes this a single-file change with a compile-time net.

use std::ops::Deref;

#[cynic::schema_for_derives(file = "graphql/schema.graphql", module = "crate::graphql::schema")]
mod cynic_scalars {
    /// `BigIntString` — large integer serialised as a decimal string
    /// (e.g. on-chain `marketId`, ERC-1155 token IDs, wei-denominated
    /// thresholds).
    #[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
    #[cynic(graphql_type = "BigIntString")]
    pub struct BigIntString(pub String);

    /// `DateTime` — ISO 8601 UTC timestamp (e.g. `2024-01-02T03:04:05.000Z`).
    #[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
    #[cynic(graphql_type = "DateTime")]
    pub struct DateTime(pub String);

    /// `Timestamp` — Unix-seconds string used by `MarketFilterInput.endsBefore`.
    #[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
    #[cynic(graphql_type = "Timestamp")]
    pub struct Timestamp(pub String);

    /// `Address` — Ethereum address (checksummed hex string).
    #[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
    #[cynic(graphql_type = "Address")]
    pub struct Address(pub String);
}

pub use cynic_scalars::{Address, BigIntString, DateTime, Timestamp};

// Convenience `Deref` impls so callers can `&*scalar` into `&str`.
macro_rules! impl_deref_string {
    ($($t:ty),*) => {
        $(
            impl Deref for $t {
                type Target = str;
                fn deref(&self) -> &str { &self.0 }
            }
            impl AsRef<str> for $t {
                fn as_ref(&self) -> &str { &self.0 }
            }
            impl From<$t> for String {
                fn from(v: $t) -> String { v.0 }
            }
        )*
    };
}

impl_deref_string!(BigIntString, DateTime, Timestamp, Address);
