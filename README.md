# predict-client-rs

Rust client library for [predict.fun](https://predict.fun).

`predict-client-rs` provides typed REST and GraphQL clients, predict.fun domain types, Kernel smart-wallet authentication, and signed order construction for predict.fun markets.

## Status

This crate is in an early development stage. Public APIs may change without backward compatibility guarantees. Use it at your own risk.

WebSocket support is currently being implemented and is not part of the stable public API yet.

## Features

- Typed OpenAPI REST wrappers for markets, order books, orders, matches, positions, and auth.
- Typed GraphQL market queries powered by `cynic`.
- Domain newtypes for market IDs, token IDs, condition IDs, order IDs, and order hashes.
- Kernel smart-wallet login flow.
- Limit and market order builders compatible with predict.fun's 18-decimal order math.
- EIP-712 order signing with Kernel-wrapped signatures.

## Installation

```toml
[dependencies]
predict-client-rs = { git = "https://github.com/influenzETH/predict-client-rs" }
alloy = { version = "*", features = ["signer-local"], default-features = false }
rust_decimal = "*"
tokio = { version = "*", features = ["macros", "rt-multi-thread"] }
```

## Quick Start

```rust
use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use predict_client::{Client, PredictBnbContractConfig};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = "YOUR_PREDICTFUN_API_KEY".to_string();
    let signer: PrivateKeySigner = "0xYOUR_EXPORTED_PRIVY_WALLET_PRIVATE_KEY".parse()?;
    let predict_account = Address::from_str("0xYOUR_KERNEL_PREDICT_ACCOUNT")?;

    let client = Client::new(
        api_key,
        signer,
        predict_account,
        PredictBnbContractConfig::mainnet(),
        "https://api.predict.fun/v1".to_string(),
    )?;

    client.login().await?;

    let markets = client.api.get_all_markets(Default::default(), |_, _, _| {}).await?;
    println!("loaded {} markets", markets.len());

    Ok(())
}
```

## Place a Limit Order

The example below logs in, builds a signed BUY limit order, and submits it through `POST /v1/orders`.

```rust
use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use predict_client::openapi::domain::enums::OrderStrategy;
use predict_client::openapi::orders::PostOrderOpts;
use predict_client::order_builder::{LimitOrderArgs, OrderExtras};
use predict_client::{Client, PredictBnbContractConfig, Side, TokenId};
use rust_decimal::Decimal;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = "YOUR_PREDICTFUN_API_KEY".to_string();
    let signer: PrivateKeySigner = "0xYOUR_EXPORTED_PRIVY_WALLET_PRIVATE_KEY".parse()?;
    let predict_account = Address::from_str("0xYOUR_KERNEL_PREDICT_ACCOUNT")?;

    let client = Client::new(
        api_key,
        signer,
        predict_account,
        PredictBnbContractConfig::mainnet(),
        "https://api.predict.fun/v1".to_string(),
    )?;

    client.login().await?;

    let args = LimitOrderArgs {
        token_id: TokenId::from_dec("71321045679252212594626385532706912750332728571942134274518208114919032993418")?,
        side: Side::Buy,
        price: Decimal::from_str("0.46")?,
        quantity: Decimal::from_str("10")?,
        expires_at: None,
        extras: OrderExtras {
            fee_rate_bps: Some(0),
            ..Default::default()
        },
    };

    let neg_risk = false;
    let yield_bearing = false;
    let bundle = client.order_builder().build_signed_limit(args, neg_risk, yield_bearing)?;

    let response = client
        .api
        .post_order(
            bundle.order,
            bundle.price_per_share_wei,
            OrderStrategy::Limit,
            PostOrderOpts::default(),
        )
        .await?;

    println!("created order: {:?}", response);
    Ok(())
}
```

`neg_risk` and `yield_bearing` select the correct CTFExchange verifying contract for the market. Use the values from predict.fun market metadata for the market you are trading.

## Authentication

predict.fun trading uses Kernel smart wallets. The `signer` should be created from the Privy wallet private key exported from the predict.fun website. The client uses it to sign the auth challenge and caches the returned JWT inside `OpenApiClient`.

```rust
client.login().await?;
```

After login, authenticated REST calls such as `post_order`, `get_my_positions`, `get_orders`, and `get_matches` automatically include the JWT.

## License

MIT
