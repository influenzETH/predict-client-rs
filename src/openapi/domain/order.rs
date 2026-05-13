//! Order / Match / Fee domain types — spec-faithful re-export.
//!
//! Per Plan C (Stage 2): all id-shaped, numeric-string, and address fields
//! on `Fee` / `OrderFillData` / `MatchData` / `OrderData` /
//! `CreateOrderResponseData` are type-strengthened **inside the codegen** via
//! `progenitor::GenerationSettings::with_conversion` (see
//! `predictfun/build.rs`). The wire shape is preserved end-to-end via the
//! `FromStr` + `Display` impls of each substituted type:
//!
//! | codegen field                     | wire           | substituted type            |
//! |-----------------------------------|----------------|-----------------------------|
//! | `Fee.amount`                      | wei string     | `alloy::primitives::U256`   |
//! | `OrderFillData.amount`            | wei string     | `U256`                      |
//! | `OrderFillData.price`             | decimal string | `rust_decimal::Decimal`     |
//! | `OrderFillData.signer`            | `0x…` 20-byte  | `alloy::primitives::Address`|
//! | `MatchData.amount_filled`         | wei string     | `U256`                      |
//! | `MatchData.price_executed`        | decimal string | `Decimal`                   |
//! | `MatchData.transaction_hash`      | `0x…` 32-byte  | `alloy::primitives::B256`   |
//! | `OrderData.id`                    | opaque string  | `crate::types::OrderId`     |
//! | `OrderData.market_id`             | i64            | `crate::types::MarketId`    |
//! | `OrderData.amount{,_filled}`      | wei string     | `U256`                      |
//! | `CreateOrderResponseData.order_id`| opaque string  | `OrderId`                   |
//! | `CreateOrderResponseData.order_hash` | `0x…` hash  | `crate::types::OrderHash`   |
//!
//! `ContractOrder` (the EIP-712-signed payload) is intentionally NOT
//! strengthened — it is treated as an opaque black box. Callers that need
//! to read `signer` / `signature` / `nonce` etc. read raw codegen fields.
//!
//! No hand-written `TryFrom`/`From` mirror layer remains. Spec field names
//! (`amount`, `amount_filled`, `order`) are preserved verbatim — callers
//! that previously used `amount_wei` / `amount_filled_wei` / `contract`
//! must update to the spec names.

pub use crate::openapi::codegen::types::{ContractOrder, ContractOrderSignature, CreateOrderResponseData, Fee, MatchData, OrderData, OrderFillData};
