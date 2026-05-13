# predict-client-rs

[predict.fun](https://predict.fun) 的 Rust 客户端库。

`predict-client-rs` 提供类型化 REST 和 GraphQL 客户端、predict.fun 领域类型、Kernel 智能钱包认证，以及 predict.fun 市场的已签名订单构建能力。

## 状态

这个 crate 仍处于早期开发阶段。公开接口可能会变化，目前不提供向后兼容保证。请自行承担使用风险。

WebSocket 支持正在实现中，目前还不是稳定公开 API 的一部分。

## 功能

- 类型化 OpenAPI REST 封装，覆盖 markets、order books、orders、matches、positions 和 auth。
- 基于 `cynic` 的类型化 GraphQL 市场查询。
- 市场 ID、token ID、condition ID、order ID、order hash 等领域 newtype。
- Kernel 智能钱包登录流程。
- 与 predict.fun 18 位订单金额规则兼容的限价单和市价单构建器。
- EIP-712 订单签名和 Kernel-wrapped 签名编码。

## 安装

```toml
[dependencies]
predict-client-rs = { git = "https://github.com/influenzETH/predict-client-rs" }
alloy = { version = "*", features = ["signer-local"], default-features = false }
rust_decimal = "*"
tokio = { version = "*", features = ["macros", "rt-multi-thread"] }
```

## 快速开始

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

## 下限价单

下面的例子会登录、构建一个已签名的 BUY 限价单，并通过 `POST /v1/orders` 提交。

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

`neg_risk` 和 `yield_bearing` 用于为当前市场选择正确的 CTFExchange verifying contract。请使用 predict.fun 市场元数据里的对应值。

## 认证

predict.fun 交易使用 Kernel 智能钱包。`signer` 应该使用从 predict.fun 网站导出的 Privy 钱包私钥创建。客户端会用它签名认证 challenge，并把返回的 JWT 缓存在 `OpenApiClient` 中。

```rust
client.login().await?;
```

登录之后，`post_order`、`get_my_positions`、`get_orders`、`get_matches` 等需要认证的 REST 请求会自动携带 JWT。

## 许可证

MIT
