//! Generated cynic schema bindings for the predict.fun GraphQL endpoint.
//!
//! `cynic::use_schema!` walks `predictfun/graphql/schema.graphql` (an SDL dump
//! produced by `cynic introspect`) and emits one marker type per GraphQL
//! type/scalar/enum into this module. These markers exist solely for the
//! `#[cynic(schema = "predictfun")]` derives in [`super::queries`] /
//! [`super::enums`] / [`super::inputs`] to point at — callers never touch
//! them directly.
//!
//! All custom scalars (`Address`, `BigIntString`, `DateTime`, `Timestamp`,
//! `JWT`, `Upload`, `Void`) are intentionally aliased to `String` at the
//! cynic boundary; parsing into newtypes / `U256` / `DateTime<Utc>` happens
//! at the call site so this layer stays free of foreign types.
//!
//! To regenerate the schema after upstream changes:
//! ```bash
//! cargo install cynic-cli --version 3.13.2
//! cynic introspect https://graphql.predict.fun/graphql \
//!   -o predictfun/graphql/schema.graphql
//! ```

cynic::use_schema!("graphql/schema.graphql");
