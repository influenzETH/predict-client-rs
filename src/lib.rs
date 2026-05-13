//! predict.fun protocol and order-building SDK.

pub mod client;
pub mod contracts;
pub mod error;
pub mod graphql;
pub mod openapi;
pub mod order_builder;
pub mod order_math;
pub mod signing;
pub mod types;

pub use client::Client;
pub use contracts::{Network, PredictBnbContractConfig};
pub use error::{PredictError, PredictResult};
pub use graphql::GraphQLClient;
pub use openapi::OpenApiClient;
pub use order_builder::OrderBuilder;
pub use signing::{Side, SigType};
pub use types::{ConditionId, MarketId, NegRiskMarketId, OracleQuestionId, OrderHash, OrderId, PolymarketConditionId, TokenId};
