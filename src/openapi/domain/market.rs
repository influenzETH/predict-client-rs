//! Market sub-tree — verbatim codegen re-exports.
//!
//! Under Plan C, ID-shaped fields on `MarketWithStats` / `Market` /
//! `Outcome` are type-strengthened **inside the codegen itself** via
//! `progenitor::GenerationSettings::with_conversion` (see
//! `predictfun/build.rs`). That removes the need for a hand-derived
//! [`MarketWithStats`] mirror at the domain boundary — the codegen
//! struct already has `id: MarketId`, `condition_id: ConditionId`,
//! `oracle_question_id: OracleQuestionId`,
//! `polymarket_condition_ids: Vec<PolymarketConditionId>`, and
//! `Outcome.on_chain_id: TokenId`.
//!
//! The whole sub-tree (`Market`, `MarketWithStats`, `Outcome`,
//! `MarketRewards`, `MarketStatsData`, `Team`, `VariantData` and
//! friends) is therefore re-exported verbatim. Callers that previously
//! went through hand-rolled `TryFrom` boundaries can use the codegen
//! types directly; cross-source equality checks in
//! [`crate::market_index`] simply use `==` on the typed newtypes.

pub use crate::openapi::codegen::types::{
    CryptoUpDownVariantData, LastSaleData, Market, MarketRewards, MarketStatsData, MarketStatus, MarketTradingStatus, MarketVariant, MarketWithStats, Outcome,
    PriceFeedProvider, PriceLevel, RewardPeriod, Team, TweetCountVariantData, VariantData, VariantDataCryptoUpDownVariantData,
    VariantDataCryptoUpDownVariantDataType, VariantDataTweetCountVariantData, VariantDataTweetCountVariantDataType,
};
