//! High-level order construction & signing for predict.fun.
//!
//! Wraps [`crate::order_math`] (integer-wei amount math) and
//! [`crate::signing::sign_order_eip712`] (EIP-712 V1 signing) into an
//! ergonomic `Decimal`-in / [`ContractOrder`]-out flow that callers can ship
//! straight to `POST /v1/orders`.
//!
//! # Routing
//!
//! `(neg_risk, yield_bearing)` selects one of four CTFExchange addresses via
//! [`PredictBnbContractConfig::exchange_for`]; the resulting address becomes
//! the EIP-712 `verifyingContract`. Everything else (domain name/version,
//! chain id) is identical across variants.
//!
//! # Conventions (mirroring sdk-python)
//!
//! - `expiration` for MARKET orders is forced to `now + 5min` and any
//!   caller-supplied value is silently ignored.
//! - LIMIT orders without an explicit `expires_at` default to `2100-01-01 UTC`
//!   (sentinel "no expiry").
//! - LIMIT `expires_at` MUST be strictly in the future or
//!   [`OrderBuilderError::ExpirationInPast`] is returned.
//! - `salt` is a random `u64 < MAX_SALT` unless caller supplies one (mirrors
//!   `random.randrange(MAX_SALT)`).
//! - `taker` defaults to the zero address (open order).
//! - `nonce` defaults to `0`, `fee_rate_bps` defaults to `0`.
//! - `signature_type` defaults to [`SigType::Eoa`] — the only path exercised
//!   in the MVP.

use crate::contracts::{PredictBnbContractConfig, MAX_SALT};
use crate::openapi::codegen::types::ContractOrder;
use crate::order_math::{
    self, decimal_to_wei, get_limit_order_amounts, market_amounts_by_quantity, market_amounts_by_value, Book, LimitInput, MarketByQuantityInput,
    MarketByValueInput, OrderAmounts, OrderMathError,
};
use crate::signing::{sign_order_predict_account, Order, Side, SigType};
use crate::types::TokenId;
use alloy::primitives::{Address, U256};
use alloy::signers::local::PrivateKeySigner;
use chrono::{DateTime, TimeZone, Utc};
use rand::Rng;
use rust_decimal::Decimal;

/// 5 minutes, matching SDK `FIVE_MINUTES_SECONDS`.
const FIVE_MINUTES_SECS: i64 = 5 * 60;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderBuilderError {
    /// Underlying integer-wei math failed (bad price, qty below `1e16`, etc.).
    Math(OrderMathError),
    /// LIMIT order with `expires_at <= now`.
    ExpirationInPast,
    /// EIP-712 signing returned an error (e.g. signer rejected).
    Signing(String),
}

impl std::fmt::Display for OrderBuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderBuilderError::Math(e) => write!(f, "order math: {}", e),
            OrderBuilderError::ExpirationInPast => write!(f, "LIMIT expires_at must be strictly in the future"),
            OrderBuilderError::Signing(s) => write!(f, "EIP-712 signing failed: {}", s),
        }
    }
}

impl std::error::Error for OrderBuilderError {}

impl From<OrderMathError> for OrderBuilderError {
    fn from(e: OrderMathError) -> Self {
        OrderBuilderError::Math(e)
    }
}

// ---------------------------------------------------------------------------
// Caller-facing inputs
// ---------------------------------------------------------------------------

/// Optional fields shared by LIMIT and MARKET orders. `None` = use defaults.
#[derive(Debug, Clone, Default)]
pub struct OrderExtras {
    /// Counterparty address. `None` => `0x0` (open order).
    pub taker: Option<Address>,
    /// Override salt. `None` => random `u64 < MAX_SALT`.
    pub salt: Option<u64>,
    /// On-chain nonce. `None` => `0`.
    pub nonce: Option<U256>,
    /// Fee rate in basis points. `None` => `0`. Use the value returned by
    /// `GET /v1/markets` (`feeRateBps`) when posting to the live exchange.
    pub fee_rate_bps: Option<u32>,
    /// Signature type. `None` => [`SigType::Eoa`].
    pub signature_type: Option<SigType>,
    /// Override the maker address (defaults to signer).
    pub maker: Option<Address>,
}

/// Inputs for a LIMIT order.
#[derive(Debug, Clone)]
pub struct LimitOrderArgs {
    pub token_id: TokenId,
    pub side: Side,
    /// Human-readable price per share (e.g. `0.46` USDT/share).
    pub price: Decimal,
    /// Human-readable share quantity (e.g. `10` shares).
    pub quantity: Decimal,
    /// Hard expiry (UTC). `None` => `2100-01-01` sentinel.
    pub expires_at: Option<DateTime<Utc>>,
    pub extras: OrderExtras,
}

/// Quantity- vs value-driven dispatch for MARKET orders.
#[derive(Debug, Clone)]
pub enum MarketOrderQty {
    /// Spend a fixed number of shares (BUY *or* SELL).
    ByQuantity { side: Side, quantity: Decimal },
    /// Spend a fixed amount of USDT (BUY-only; SDK enforces this).
    ByValue { value: Decimal },
}

/// Inputs for a MARKET order.
#[derive(Debug, Clone)]
pub struct MarketOrderArgs {
    pub token_id: TokenId,
    pub qty: MarketOrderQty,
    /// Slippage tolerance in basis points (1 bp = 0.01%). `0` disables.
    pub slippage_bps: u32,
    /// See `OrderAmounts::is_min_amount_out` doc / SDK comment block in
    /// `_get_market_order_amounts_by_quantity`.
    pub is_min_amount_out: bool,
    pub extras: OrderExtras,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Output of [`OrderBuilder::build_signed_limit`] /
/// [`OrderBuilder::build_signed_market`]. Carries everything that
/// `OpenApiClient::post_order` needs:
///
/// - [`order`](Self::order): the EIP-712-signed [`ContractOrder`] payload
///   (codegen type, ready for `POST /v1/orders`).
/// - [`price_per_share_wei`](Self::price_per_share_wei): probability as an
///   integer in 1e-18 units (`U256`). LIMIT: derived directly from the input
///   `args.price * 1e18`. MARKET: mirrors `OrderAmounts.price_per_share`
///   (sdk-python convention — average fill price, also wei-scaled).
///   Stringified for the `pricePerShare` JSON wire field, which the
///   predict.fun matcher rejects when sent as a human-readable decimal
///   (`CreateOrderInvalidNumericValue`).
/// - [`is_min_amount_out`](Self::is_min_amount_out): `false` for LIMIT; for
///   MARKET, mirrors `MarketOrderArgs.is_min_amount_out` so callers don't
///   have to thread it separately to `PostOrderOpts`.
#[derive(Clone, Debug)]
pub struct SignedOrderBundle {
    pub order: ContractOrder,
    pub price_per_share_wei: U256,
    pub is_min_amount_out: bool,
}

/// Stateless-ish wrapper around an EOA owner key + the contract config.
///
/// Owns no mutable state — safe to clone/share. `predict_account` is the
/// Kernel smart wallet whose maker/signer is forced on every order; the EOA
/// `signer` only attests via the Kernel-wrapped EIP-1271 signature path.
#[derive(Clone)]
pub struct OrderBuilder {
    signer: PrivateKeySigner,
    config: PredictBnbContractConfig,
    predict_account: Address,
}

impl OrderBuilder {
    pub fn new(signer: PrivateKeySigner, config: PredictBnbContractConfig, predict_account: Address) -> Self {
        Self {
            signer,
            config,
            predict_account,
        }
    }

    pub fn signer_address(&self) -> Address {
        self.signer.address()
    }

    pub fn predict_account(&self) -> Address {
        self.predict_account
    }

    pub fn config(&self) -> &PredictBnbContractConfig {
        &self.config
    }

    // -----------------------------------------------------------------------
    // Public build entry points
    // -----------------------------------------------------------------------

    /// Build, sign, and return a ready-to-POST LIMIT order bundle.
    pub fn build_signed_limit(&self, args: LimitOrderArgs, neg_risk: bool, yield_bearing: bool) -> Result<SignedOrderBundle, OrderBuilderError> {
        let price_wei = decimal_to_wei(args.price)?;
        let quantity_wei = decimal_to_wei(args.quantity)?;
        let amounts = get_limit_order_amounts(LimitInput {
            side: args.side,
            price_per_share_wei: price_wei,
            quantity_wei,
        })?;

        let expiration = limit_expiration(args.expires_at)?;
        let order = self.assemble_signed(&args.token_id, args.side, amounts, expiration, args.extras, neg_risk, yield_bearing)?;
        Ok(SignedOrderBundle {
            order,
            price_per_share_wei: U256::from(price_wei),
            is_min_amount_out: false,
        })
    }

    /// Build, sign, and return a ready-to-POST MARKET order bundle.
    ///
    /// `book` must come from `GET /v1/orderbook/{marketId}` for the same
    /// `tokenId`; sides MUST be sorted as documented on [`Book`].
    pub fn build_signed_market(&self, args: MarketOrderArgs, book: &Book, neg_risk: bool, yield_bearing: bool) -> Result<SignedOrderBundle, OrderBuilderError> {
        let (side, amounts) = match &args.qty {
            MarketOrderQty::ByQuantity { side, quantity } => {
                let quantity_wei = decimal_to_wei(*quantity)?;
                let amounts = market_amounts_by_quantity(
                    MarketByQuantityInput {
                        side: *side,
                        quantity_wei,
                        slippage_bps: args.slippage_bps,
                        is_min_amount_out: args.is_min_amount_out,
                    },
                    book,
                )?;
                (*side, amounts)
            }
            MarketOrderQty::ByValue { value } => {
                let value_wei = decimal_to_wei(*value)?;
                let amounts = market_amounts_by_value(
                    MarketByValueInput {
                        value_wei,
                        slippage_bps: args.slippage_bps,
                        is_min_amount_out: args.is_min_amount_out,
                    },
                    book,
                )?;
                (Side::Buy, amounts)
            }
        };

        let price_per_share_wei = U256::from(amounts.price_per_share);
        let expiration = market_expiration_now();
        let order = self.assemble_signed(&args.token_id, side, amounts, expiration, args.extras, neg_risk, yield_bearing)?;
        Ok(SignedOrderBundle {
            order,
            price_per_share_wei,
            is_min_amount_out: args.is_min_amount_out,
        })
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    fn assemble_signed(
        &self,
        token_id: &TokenId,
        side: Side,
        amounts: OrderAmounts,
        expiration: i64,
        extras: OrderExtras,
        neg_risk: bool,
        yield_bearing: bool,
    ) -> Result<ContractOrder, OrderBuilderError> {
        let maker = self.predict_account;
        let signer_field = self.predict_account;
        // `extras.maker` is no longer respected — predict.fun pins maker to the
        // Kernel smart wallet. Field is retained on `OrderExtras` for ABI
        // stability but ignored here.
        let _ = extras.maker;
        let taker = extras.taker.unwrap_or(Address::ZERO);
        let salt = extras.salt.unwrap_or_else(generate_order_salt);
        let nonce = extras.nonce.unwrap_or(U256::ZERO);
        let fee_rate_bps = extras.fee_rate_bps.unwrap_or(0);
        let sig_type = extras.signature_type.unwrap_or(SigType::Eoa);

        let exchange = self.config.exchange_for(neg_risk, yield_bearing);
        let chain_id = self.config.chain_id();

        let order = Order {
            salt: U256::from(salt),
            maker,
            signer: signer_field,
            taker,
            tokenId: token_id.0,
            makerAmount: U256::from(amounts.maker_amount),
            takerAmount: U256::from(amounts.taker_amount),
            expiration: U256::from(expiration as u64),
            nonce,
            feeRateBps: U256::from(fee_rate_bps),
            side: side as u8,
            signatureType: sig_type as u8,
        };

        let sig_hex = sign_order_predict_account(&self.signer, &order, exchange, chain_id, self.predict_account, self.config.ecdsa_validator)
            .map_err(|e| OrderBuilderError::Signing(format!("{e:#}")))?;

        let signature = crate::openapi::codegen::types::ContractOrderSignature::try_from(sig_hex.clone())
            .map_err(|e| OrderBuilderError::Signing(format!("signature regex check: {e}")))?;

        Ok(ContractOrder {
            hash: None,
            salt: salt.to_string(),
            maker: format!("{maker:#x}"),
            signer: format!("{signer_field:#x}"),
            taker: format!("{taker:#x}"),
            token_id: token_id.to_string(),
            maker_amount: amounts.maker_amount.to_string(),
            taker_amount: amounts.taker_amount.to_string(),
            expiration,
            nonce: nonce.to_string(),
            fee_rate_bps: fee_rate_bps.to_string(),
            side: side as i32,
            signature_type: sig_type as i32,
            signature: Some(signature),
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Random salt in `[0, MAX_SALT)`. Mirrors SDK `generate_order_salt`.
pub fn generate_order_salt() -> u64 {
    rand::thread_rng().gen_range(0..MAX_SALT)
}

/// LIMIT expiration: caller value (must be strictly future), or year-2100 sentinel.
fn limit_expiration(expires_at: Option<DateTime<Utc>>) -> Result<i64, OrderBuilderError> {
    let resolved = expires_at.unwrap_or_else(|| Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap());
    let ts = resolved.timestamp();
    if expires_at.is_some() && ts <= Utc::now().timestamp() {
        return Err(OrderBuilderError::ExpirationInPast);
    }
    Ok(ts)
}

/// MARKET expiration: `now + 5min`, ignoring any caller-supplied value.
fn market_expiration_now() -> i64 {
    Utc::now().timestamp() + FIVE_MINUTES_SECS
}

// Silence the unused-import warning when the trait isn't transitively used.
#[allow(dead_code)]
fn _unused() {
    let _ = order_math::MIN_QUANTITY_WEI;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::signers::local::PrivateKeySigner;
    use rust_decimal_macros::dec;

    fn test_signer() -> PrivateKeySigner {
        // Deterministic key for stable test addresses (NOT a real key).
        "0x1111111111111111111111111111111111111111111111111111111111111111".parse().unwrap()
    }

    fn test_predict_account() -> Address {
        "0x000000000000000000000000000000000000bEEF".parse().unwrap()
    }

    fn test_builder() -> OrderBuilder {
        OrderBuilder::new(test_signer(), PredictBnbContractConfig::mainnet(), test_predict_account())
    }

    fn test_token() -> TokenId {
        // Arbitrary 256-bit token id.
        "1234567890".parse().unwrap()
    }

    #[test]
    fn limit_buy_signed_order_shape() {
        let builder = test_builder();
        let order = builder
            .build_signed_limit(
                LimitOrderArgs {
                    token_id: test_token(),
                    side: Side::Buy,
                    price: dec!(0.46),
                    quantity: dec!(10),
                    expires_at: None,
                    extras: OrderExtras::default(),
                },
                false,
                false,
            )
            .unwrap();
        let order = order.order;

        assert_eq!(order.side, 0);
        assert_eq!(order.signature_type, 0);
        // 10 shares @ 0.46 -> maker = 4.6 USDT (1e18 wei)
        assert_eq!(order.maker_amount, "4600000000000000000");
        assert_eq!(order.taker_amount, "10000000000000000000");
        assert_eq!(order.token_id, "1234567890");
        // 2100-01-01 UTC
        assert_eq!(order.expiration, 4_102_444_800);
        assert_eq!(order.fee_rate_bps, "0");
        assert_eq!(order.nonce, "0");
        // Kernel-wrapped sig: 0x01 + 20-byte validator + 65-byte ECDSA sig
        // = 0x + 2 + 40 + 130 = 174 chars total
        let sig = order.signature.as_ref().unwrap();
        assert!(sig.starts_with("0x01"));
        assert_eq!(sig.len(), 174);
        // maker == signer == predict_account on every order
        let expected = format!("{:#x}", test_predict_account());
        assert_eq!(order.maker, expected);
        assert_eq!(order.signer, expected);
        assert_eq!(order.taker, format!("{:#x}", Address::ZERO));
    }

    #[test]
    fn limit_sell_signed_order_amounts() {
        let builder = test_builder();
        let order = builder
            .build_signed_limit(
                LimitOrderArgs {
                    token_id: test_token(),
                    side: Side::Sell,
                    price: dec!(0.46),
                    quantity: dec!(10),
                    expires_at: None,
                    extras: OrderExtras::default(),
                },
                false,
                false,
            )
            .unwrap();
        let order = order.order;

        assert_eq!(order.side, 1);
        assert_eq!(order.maker_amount, "10000000000000000000");
        assert_eq!(order.taker_amount, "4600000000000000000");
    }

    #[test]
    fn limit_rejects_past_expiration() {
        let builder = test_builder();
        let err = builder
            .build_signed_limit(
                LimitOrderArgs {
                    token_id: test_token(),
                    side: Side::Buy,
                    price: dec!(0.5),
                    quantity: dec!(10),
                    expires_at: Some(Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap()),
                    extras: OrderExtras::default(),
                },
                false,
                false,
            )
            .unwrap_err();
        assert_eq!(err, OrderBuilderError::ExpirationInPast);
    }

    #[test]
    fn maker_extras_override_is_ignored() {
        // predict.fun pins maker == predict_account; OrderExtras.maker is a no-op.
        let other: Address = "0x0000000000000000000000000000000000001234".parse().unwrap();
        let builder = test_builder();
        let order = builder
            .build_signed_limit(
                LimitOrderArgs {
                    token_id: test_token(),
                    side: Side::Buy,
                    price: dec!(0.5),
                    quantity: dec!(10),
                    expires_at: None,
                    extras: OrderExtras {
                        maker: Some(other),
                        ..OrderExtras::default()
                    },
                },
                false,
                false,
            )
            .unwrap();
        let order = order.order;
        let expected = format!("{:#x}", test_predict_account());
        assert_eq!(order.maker, expected);
        assert_eq!(order.signer, expected);
    }

    #[test]
    fn market_buy_by_quantity_signed() {
        let book = Book {
            asks: vec![
                crate::order_math::DepthLevel {
                    price: dec!(0.50),
                    qty: dec!(100),
                },
                crate::order_math::DepthLevel {
                    price: dec!(0.55),
                    qty: dec!(200),
                },
            ],
            bids: vec![],
        };
        let builder = test_builder();
        let order = builder
            .build_signed_market(
                MarketOrderArgs {
                    token_id: test_token(),
                    qty: MarketOrderQty::ByQuantity {
                        side: Side::Buy,
                        quantity: dec!(50),
                    },
                    slippage_bps: 0,
                    is_min_amount_out: false,
                    extras: OrderExtras::default(),
                },
                &book,
                false,
                false,
            )
            .unwrap();
        let order = order.order;
        assert_eq!(order.side, 0);
        // 50 shares @ 0.50 -> maker = 25 USDT
        assert_eq!(order.maker_amount, "25000000000000000000");
        assert_eq!(order.taker_amount, "50000000000000000000");
        // expiration ~ now + 5 min
        let now = Utc::now().timestamp();
        assert!(order.expiration > now + 4 * 60 && order.expiration <= now + FIVE_MINUTES_SECS + 5);
    }

    #[test]
    fn extras_override_routing_and_fees() {
        let builder = test_builder();
        let order = builder
            .build_signed_limit(
                LimitOrderArgs {
                    token_id: test_token(),
                    side: Side::Buy,
                    price: dec!(0.5),
                    quantity: dec!(10),
                    expires_at: None,
                    extras: OrderExtras {
                        salt: Some(42),
                        nonce: Some(U256::from(7u64)),
                        fee_rate_bps: Some(50),
                        ..OrderExtras::default()
                    },
                },
                true, // neg_risk
                true, // yield_bearing
            )
            .unwrap();
        let order = order.order;
        assert_eq!(order.salt, "42");
        assert_eq!(order.nonce, "7");
        assert_eq!(order.fee_rate_bps, "50");
    }
}
