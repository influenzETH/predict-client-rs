//! Order amount math, ported verbatim from the predict.fun Python SDK
//! (`predict_sdk/order_builder.py` + `predict_sdk/_internal/utils.py`).
//!
//! Two reasons to keep this in lock-step with the SDK:
//!
//! 1. **Determinism vs. on-chain matcher.** The exchange contract enforces
//!    `makerAmount/takerAmount` ratios via integer math; any divergence in
//!    rounding or significant-digit truncation produces orders that the
//!    matcher silently mis-prices.
//! 2. **Reproducibility against the SDK fixtures.** The Python SDK ships
//!    golden vectors we can replay byte-for-byte if we keep the algorithm
//!    identical (same truncation order, same `precision` multiplier, same
//!    `1e16` floor).
//!
//! All quantities here are in **wei** (i.e. already multiplied by `10^18`).
//! Public entry points accept `Decimal` for ergonomics and convert internally
//! via [`decimal_to_wei`] (uses `ROUND_DOWN`, matching SDK `float_to_wei`).

use crate::contracts::PRECISION_WEI;
use crate::signing::Side;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

/// Minimum order quantity the exchange will accept: `0.01` shares = `1e16` wei.
pub const MIN_QUANTITY_WEI: u128 = 10_000_000_000_000_000u128;
/// Minimum order value (BUY-by-value path): `1` USDT = `1e18` wei.
pub const MIN_VALUE_WEI: u128 = PRECISION_WEI;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderMathError {
    /// `pricePerShareWei <= 0`, or quantity below `MIN_QUANTITY_WEI`, or value
    /// below `MIN_VALUE_WEI`. Mirrors SDK's `InvalidQuantityError`.
    InvalidQuantity,
    /// Negative or non-representable Decimal input.
    DecimalOutOfRange(Decimal),
}

impl std::fmt::Display for OrderMathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderMathError::InvalidQuantity => write!(f, "invalid quantity / price (must satisfy: price > 0, qty >= 1e16 wei, value >= 1e18 wei)"),
            OrderMathError::DecimalOutOfRange(v) => write!(f, "decimal value out of u128 range or negative: {}", v),
        }
    }
}

impl std::error::Error for OrderMathError {}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// Limit-order inputs. Mirrors SDK `LimitHelperInput`.
#[derive(Debug, Clone, Copy)]
pub struct LimitInput {
    pub side: Side,
    /// Price per share in wei (e.g. `0.46` → `460_000_000_000_000_000`).
    pub price_per_share_wei: u128,
    /// Share quantity in wei (e.g. `10` → `10_000_000_000_000_000_000`).
    pub quantity_wei: u128,
}

/// Market-order inputs (quantity-driven). Mirrors SDK `MarketHelperInput`.
#[derive(Debug, Clone, Copy)]
pub struct MarketByQuantityInput {
    pub side: Side,
    pub quantity_wei: u128,
    /// Slippage tolerance in basis points (1 bp = 0.01%). `0` disables.
    pub slippage_bps: u32,
    /// `is_min_amount_out` only affects the BUY path; see SDK comment block in
    /// `_get_market_order_amounts_by_quantity`.
    pub is_min_amount_out: bool,
}

/// Market-order inputs (value-driven, BUY only). Mirrors SDK `MarketHelperValueInput`.
#[derive(Debug, Clone, Copy)]
pub struct MarketByValueInput {
    /// Currency value to spend, in wei (USDT). Must be `>= MIN_VALUE_WEI`.
    pub value_wei: u128,
    pub slippage_bps: u32,
    pub is_min_amount_out: bool,
}

/// One book level (`(price, qty)`), matching SDK `DepthLevel = tuple[float, float]`.
#[derive(Debug, Clone, Copy)]
pub struct DepthLevel {
    pub price: Decimal,
    pub qty: Decimal,
}

/// Full orderbook for market-order routing. Mirrors SDK `Book`.
///
/// Both sides MUST be sorted in the order the matcher would walk them:
/// `asks` ascending by price (best/lowest first), `bids` descending by price
/// (best/highest first).
#[derive(Debug, Clone, Default)]
pub struct Book {
    pub asks: Vec<DepthLevel>,
    pub bids: Vec<DepthLevel>,
}

// ---------------------------------------------------------------------------
// Outputs
// ---------------------------------------------------------------------------

/// Computed amounts for a single order. Mirrors SDK `OrderAmounts`.
///
/// All values are in wei. `last_price` / `price_per_share` are price-per-share
/// in wei (e.g. `0.46e18`). `maker_amount` / `taker_amount` go straight into
/// the EIP-712 `Order` payload. `amount` is the share quantity actually
/// matched against the book (== `taker_amount` for BUY, `maker_amount` for SELL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderAmounts {
    pub last_price: u128,
    pub price_per_share: u128,
    pub maker_amount: u128,
    pub taker_amount: u128,
    pub amount: u128,
    pub slippage_bps: u32,
    pub is_min_amount_out: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ProcessedBook {
    quantity_wei: u128,
    /// Accumulated `Σ price_wei * qty_wei` (no intermediate division — preserves precision).
    price_wei_sum: u128,
    last_price_wei: u128,
}

// ---------------------------------------------------------------------------
// Decimal → wei + sig-digit truncation
// ---------------------------------------------------------------------------

/// Convert a `Decimal` value to wei (`× 10^18`, `ROUND_DOWN`).
///
/// Mirrors SDK `float_to_wei(value, 10**18)` which uses `Decimal(str(v)) * 10^18`
/// + `ROUND_DOWN`. Returns `DecimalOutOfRange` for negative inputs or values
/// that don't fit in `u128`.
pub fn decimal_to_wei(value: Decimal) -> Result<u128, OrderMathError> {
    if value.is_sign_negative() {
        return Err(OrderMathError::DecimalOutOfRange(value));
    }
    let scaled = value * Decimal::from(PRECISION_WEI);
    let truncated = scaled.round_dp_with_strategy(0, RoundingStrategy::ToZero);
    truncated.to_u128().ok_or(OrderMathError::DecimalOutOfRange(value))
}

/// Retain `significant_digits` significant digits of `num`, truncating the rest
/// to zero (towards zero — sign is preserved).
///
/// Direct port of SDK `retain_significant_digits` from `_internal/utils.py`.
///
/// ```ignore
/// retain_significant_digits(123_456, 3) == 123_000
/// retain_significant_digits(7,        3) == 7
/// retain_significant_digits(0,        3) == 0
/// ```
pub fn retain_significant_digits(num: u128, significant_digits: u32) -> u128 {
    if num == 0 {
        return 0;
    }
    // u128::MAX has 39 digits; magnitude fits in u32 safely.
    let magnitude = num.ilog10() + 1;
    if magnitude <= significant_digits {
        return num;
    }
    let excess = magnitude - significant_digits;
    let divisor = 10u128.pow(excess);
    (num / divisor) * divisor
}

// ---------------------------------------------------------------------------
// Limit
// ---------------------------------------------------------------------------

/// Compute maker/taker amounts for a LIMIT order.
///
/// Direct port of SDK `OrderBuilder.get_limit_order_amounts`:
/// - validate `price > 0`, `qty >= 1e16`,
/// - truncate price to 3 sig digits, qty to 5 sig digits,
/// - BUY:  `maker = price*qty/precision`, `taker = qty`,
/// - SELL: `maker = qty`,                  `taker = price*qty/precision`.
pub fn get_limit_order_amounts(input: LimitInput) -> Result<OrderAmounts, OrderMathError> {
    if input.price_per_share_wei == 0 {
        return Err(OrderMathError::InvalidQuantity);
    }
    if input.quantity_wei < MIN_QUANTITY_WEI {
        return Err(OrderMathError::InvalidQuantity);
    }

    let price = retain_significant_digits(input.price_per_share_wei, 3);
    let qty = retain_significant_digits(input.quantity_wei, 5);

    let (maker_amount, taker_amount) = match input.side {
        Side::Buy => (mul_div(price, qty, PRECISION_WEI), qty),
        Side::Sell => (qty, mul_div(price, qty, PRECISION_WEI)),
    };

    Ok(OrderAmounts {
        last_price: price,
        price_per_share: price,
        maker_amount,
        taker_amount,
        amount: qty,
        slippage_bps: 0,
        is_min_amount_out: false,
    })
}

// ---------------------------------------------------------------------------
// Market: book walking
// ---------------------------------------------------------------------------

/// Walk `depths` accumulating up to `quantity_wei` shares.
///
/// Direct port of SDK `OrderBuilder._process_book`. Accumulates
/// `Σ price_wei * qty_wei` without intermediate division so the average price
/// is computed at full precision in [`market_amounts_by_quantity`].
fn process_book(depths: &[DepthLevel], quantity_wei: u128) -> Result<ProcessedBook, OrderMathError> {
    let mut acc = ProcessedBook::default();
    for level in depths {
        let remaining = quantity_wei.saturating_sub(acc.quantity_wei);
        if remaining == 0 {
            break;
        }
        let price_wei = decimal_to_wei(level.price)?;
        let qty_wei = decimal_to_wei(level.qty)?;

        if remaining < qty_wei {
            acc.quantity_wei += remaining;
            acc.price_wei_sum += price_wei * remaining;
            acc.last_price_wei = price_wei;
            // SDK breaks implicitly on next iteration; explicit break is identical.
            break;
        } else {
            acc.quantity_wei += qty_wei;
            acc.price_wei_sum += price_wei * qty_wei;
            acc.last_price_wei = price_wei;
        }
    }
    Ok(acc)
}

/// Compute amounts for a quantity-driven MARKET order.
///
/// Direct port of SDK `OrderBuilder._get_market_order_amounts_by_quantity`.
/// BUY walks `book.asks`; SELL walks `book.bids`. See SDK doc-comment block
/// (lines 319-333 in `order_builder.py`) for the rationale behind
/// `is_min_amount_out`.
pub fn market_amounts_by_quantity(input: MarketByQuantityInput, book: &Book) -> Result<OrderAmounts, OrderMathError> {
    let qty = retain_significant_digits(input.quantity_wei, 5);
    if qty < MIN_QUANTITY_WEI {
        return Err(OrderMathError::InvalidQuantity);
    }
    let slippage_bps = input.slippage_bps as u128;

    match input.side {
        Side::Buy => {
            let processed = process_book(&book.asks, qty)?;
            let price_per_share = if processed.quantity_wei > 0 {
                processed.price_wei_sum / processed.quantity_wei
            } else {
                0
            };

            if input.is_min_amount_out {
                // makerAmount = expected cost (avg price * shares) — see SDK comment.
                let maker_amount = processed.price_wei_sum / PRECISION_WEI;
                let signed_shares = if processed.last_price_wei > 0 {
                    processed.price_wei_sum / processed.last_price_wei
                } else {
                    0
                };
                let taker_amount = if slippage_bps > 0 {
                    apply_slippage_down(signed_shares, slippage_bps)
                } else {
                    signed_shares
                };
                return Ok(OrderAmounts {
                    last_price: processed.last_price_wei,
                    price_per_share,
                    maker_amount,
                    taker_amount,
                    amount: processed.quantity_wei,
                    slippage_bps: input.slippage_bps,
                    is_min_amount_out: true,
                });
            }

            // Default BUY path: makerAmount = worstTierPrice * shares, inflated by slippage.
            let base_maker_amount = mul_div(processed.last_price_wei, processed.quantity_wei, PRECISION_WEI);
            let maker_amount = if slippage_bps > 0 {
                // Cap at processed.quantity_wei (matches SDK `min(...)` clause).
                apply_slippage_up(base_maker_amount, slippage_bps).min(processed.quantity_wei)
            } else {
                base_maker_amount
            };
            Ok(OrderAmounts {
                last_price: processed.last_price_wei,
                price_per_share,
                maker_amount,
                taker_amount: processed.quantity_wei,
                amount: processed.quantity_wei,
                slippage_bps: input.slippage_bps,
                is_min_amount_out: false,
            })
        }
        Side::Sell => {
            let processed = process_book(&book.bids, qty)?;
            let base_taker_amount = mul_div(processed.last_price_wei, processed.quantity_wei, PRECISION_WEI);
            let taker_amount = if slippage_bps > 0 {
                apply_slippage_down(base_taker_amount, slippage_bps)
            } else {
                base_taker_amount
            };
            let price_per_share = if processed.quantity_wei > 0 {
                processed.price_wei_sum / processed.quantity_wei
            } else {
                0
            };
            Ok(OrderAmounts {
                last_price: processed.last_price_wei,
                price_per_share,
                maker_amount: processed.quantity_wei,
                taker_amount,
                amount: processed.quantity_wei,
                slippage_bps: input.slippage_bps,
                is_min_amount_out: false,
            })
        }
    }
}

/// Compute amounts for a value-driven MARKET BUY (USDT-in, shares-out).
///
/// Direct port of SDK `OrderBuilder._get_market_order_amounts_by_value`.
/// Walks `book.asks` consuming USDT until `value_wei` is exhausted, then
/// hands off to [`market_amounts_by_quantity`] for the maker/taker reconciliation.
pub fn market_amounts_by_value(input: MarketByValueInput, book: &Book) -> Result<OrderAmounts, OrderMathError> {
    if input.value_wei < MIN_VALUE_WEI {
        return Err(OrderMathError::InvalidQuantity);
    }

    let currency_amount_wei = input.value_wei;
    let mut number_of_shares: u128 = 0;
    let mut total_price: u128 = 0;

    for level in &book.asks {
        let remaining_spend = currency_amount_wei.saturating_sub(total_price);
        if remaining_spend == 0 {
            break;
        }
        let price_wei = decimal_to_wei(level.price)?;
        let qty_wei = decimal_to_wei(level.qty)?;

        let tier_total_price = mul_div(price_wei, qty_wei, PRECISION_WEI);

        if tier_total_price <= remaining_spend {
            number_of_shares += qty_wei;
            total_price += tier_total_price;
        } else {
            // Consume as much as we can.
            let fractional = if price_wei > 0 {
                mul_div(remaining_spend, PRECISION_WEI, price_wei)
            } else {
                0
            };
            number_of_shares += fractional;
            total_price += mul_div(price_wei, fractional, PRECISION_WEI);
            // SDK does not break here; loop terminates naturally on next iteration
            // because `remaining_spend` becomes 0 (within rounding). We mirror that.
        }
    }

    let rounded_shares = retain_significant_digits(number_of_shares, 5);
    let amounts = market_amounts_by_quantity(
        MarketByQuantityInput {
            side: Side::Buy,
            quantity_wei: rounded_shares,
            slippage_bps: input.slippage_bps,
            is_min_amount_out: input.is_min_amount_out,
        },
        book,
    )?;

    // SDK overwrites `amount` with the *post-rounding* share count.
    Ok(OrderAmounts {
        amount: rounded_shares,
        ..amounts
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `(a * b) / d` with `u128` operands but `u256` intermediate to avoid overflow
/// at the `price_wei * quantity_wei` step (1e36 max).
fn mul_div(a: u128, b: u128, d: u128) -> u128 {
    use alloy::primitives::U256;
    let prod = U256::from(a) * U256::from(b);
    let div = prod / U256::from(d);
    div.try_into().expect("mul_div: result exceeds u128")
}

/// `x * (10_000 - bps) / 10_000`, floored at 0. Used on taker-side reductions.
fn apply_slippage_down(x: u128, bps: u128) -> u128 {
    if bps >= 10_000 {
        return 0;
    }
    mul_div(x, 10_000 - bps, 10_000)
}

/// `x * (10_000 + bps) / 10_000`. Used on maker-side inflations.
fn apply_slippage_up(x: u128, bps: u128) -> u128 {
    mul_div(x, 10_000 + bps, 10_000)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // -------- decimal_to_wei --------

    #[test]
    fn decimal_to_wei_matches_sdk_examples() {
        // SDK doctests: 0.46 -> 460_000_000_000_000_000, 0.421031 -> 421031e12.
        assert_eq!(decimal_to_wei(dec!(0.46)).unwrap(), 460_000_000_000_000_000u128);
        assert_eq!(decimal_to_wei(dec!(0.421031)).unwrap(), 421_031_000_000_000_000u128);
        assert_eq!(decimal_to_wei(dec!(1)).unwrap(), PRECISION_WEI);
        assert_eq!(decimal_to_wei(dec!(0)).unwrap(), 0);
    }

    #[test]
    fn decimal_to_wei_rejects_negative() {
        assert!(matches!(decimal_to_wei(dec!(-0.1)), Err(OrderMathError::DecimalOutOfRange(_))));
    }

    #[test]
    fn decimal_to_wei_rounds_down() {
        // 0.0000000000000000001 (= 1e-19) truncates to 0; matches Solidity / SDK.
        assert_eq!(decimal_to_wei(dec!(0.0000000000000000001)).unwrap(), 0);
    }

    // -------- retain_significant_digits --------

    #[test]
    fn retain_sig_digits_zero() {
        assert_eq!(retain_significant_digits(0, 3), 0);
    }

    #[test]
    fn retain_sig_digits_below_threshold_unchanged() {
        assert_eq!(retain_significant_digits(7, 3), 7);
        assert_eq!(retain_significant_digits(123, 3), 123);
        assert_eq!(retain_significant_digits(99, 3), 99);
    }

    #[test]
    fn retain_sig_digits_truncates() {
        assert_eq!(retain_significant_digits(123_456, 3), 123_000);
        assert_eq!(retain_significant_digits(987_654_321, 5), 987_650_000);
        // 0.421031e18 -> 3 sig digits -> 0.421e18
        assert_eq!(retain_significant_digits(421_031_000_000_000_000u128, 3), 421_000_000_000_000_000u128);
        // 10.5e18 -> 5 sig digits -> 10.500e18 (no change since exactly 5 sig digits in 1.05e19)
        assert_eq!(retain_significant_digits(10_500_000_000_000_000_000u128, 5), 10_500_000_000_000_000_000u128);
    }

    // -------- get_limit_order_amounts --------

    #[test]
    fn limit_buy_basic() {
        // BUY 10 shares @ 0.46  ->  maker = 4.6 USDT, taker = 10 shares.
        let amounts = get_limit_order_amounts(LimitInput {
            side: Side::Buy,
            price_per_share_wei: decimal_to_wei(dec!(0.46)).unwrap(),
            quantity_wei: decimal_to_wei(dec!(10)).unwrap(),
        })
        .unwrap();
        assert_eq!(amounts.price_per_share, 460_000_000_000_000_000u128);
        assert_eq!(amounts.maker_amount, 4_600_000_000_000_000_000u128);
        assert_eq!(amounts.taker_amount, 10_000_000_000_000_000_000u128);
        assert_eq!(amounts.amount, 10_000_000_000_000_000_000u128);
    }

    #[test]
    fn limit_sell_basic() {
        // SELL 10 shares @ 0.46 -> maker = 10 shares, taker = 4.6 USDT.
        let amounts = get_limit_order_amounts(LimitInput {
            side: Side::Sell,
            price_per_share_wei: decimal_to_wei(dec!(0.46)).unwrap(),
            quantity_wei: decimal_to_wei(dec!(10)).unwrap(),
        })
        .unwrap();
        assert_eq!(amounts.maker_amount, 10_000_000_000_000_000_000u128);
        assert_eq!(amounts.taker_amount, 4_600_000_000_000_000_000u128);
    }

    #[test]
    fn limit_truncates_price_to_3_sig_digits() {
        // 0.421031 -> 3 sig digits -> 0.421
        let amounts = get_limit_order_amounts(LimitInput {
            side: Side::Buy,
            price_per_share_wei: decimal_to_wei(dec!(0.421031)).unwrap(),
            quantity_wei: decimal_to_wei(dec!(10)).unwrap(),
        })
        .unwrap();
        assert_eq!(amounts.price_per_share, 421_000_000_000_000_000u128);
        assert_eq!(amounts.maker_amount, 4_210_000_000_000_000_000u128);
    }

    #[test]
    fn limit_rejects_zero_price() {
        let err = get_limit_order_amounts(LimitInput {
            side: Side::Buy,
            price_per_share_wei: 0,
            quantity_wei: decimal_to_wei(dec!(10)).unwrap(),
        });
        assert_eq!(err, Err(OrderMathError::InvalidQuantity));
    }

    #[test]
    fn limit_rejects_too_small_quantity() {
        // 0.001 shares = 1e15 wei < MIN_QUANTITY_WEI (1e16).
        let err = get_limit_order_amounts(LimitInput {
            side: Side::Buy,
            price_per_share_wei: decimal_to_wei(dec!(0.5)).unwrap(),
            quantity_wei: 1_000_000_000_000_000u128,
        });
        assert_eq!(err, Err(OrderMathError::InvalidQuantity));
    }

    // -------- market_amounts_by_quantity --------

    fn book_simple() -> Book {
        // Asks: 100 @ 0.50, 200 @ 0.55. Bids: 100 @ 0.48, 200 @ 0.45.
        Book {
            asks: vec![
                DepthLevel {
                    price: dec!(0.50),
                    qty: dec!(100),
                },
                DepthLevel {
                    price: dec!(0.55),
                    qty: dec!(200),
                },
            ],
            bids: vec![
                DepthLevel {
                    price: dec!(0.48),
                    qty: dec!(100),
                },
                DepthLevel {
                    price: dec!(0.45),
                    qty: dec!(200),
                },
            ],
        }
    }

    #[test]
    fn market_buy_consumes_first_tier_only() {
        let amounts = market_amounts_by_quantity(
            MarketByQuantityInput {
                side: Side::Buy,
                quantity_wei: decimal_to_wei(dec!(50)).unwrap(),
                slippage_bps: 0,
                is_min_amount_out: false,
            },
            &book_simple(),
        )
        .unwrap();
        assert_eq!(amounts.last_price, 500_000_000_000_000_000u128);
        // 50 shares @ 0.50 -> 25 USDT
        assert_eq!(amounts.maker_amount, 25_000_000_000_000_000_000u128);
        assert_eq!(amounts.taker_amount, decimal_to_wei(dec!(50)).unwrap());
    }

    #[test]
    fn market_buy_walks_two_tiers_avg_price() {
        // 200 shares: first 100 @ 0.50, next 100 @ 0.55. avg = 0.525.
        let amounts = market_amounts_by_quantity(
            MarketByQuantityInput {
                side: Side::Buy,
                quantity_wei: decimal_to_wei(dec!(200)).unwrap(),
                slippage_bps: 0,
                is_min_amount_out: false,
            },
            &book_simple(),
        )
        .unwrap();
        assert_eq!(amounts.last_price, 550_000_000_000_000_000u128);
        assert_eq!(amounts.price_per_share, 525_000_000_000_000_000u128);
        // makerAmount = worstTierPrice * shares = 0.55 * 200 = 110 USDT (no slippage).
        assert_eq!(amounts.maker_amount, 110_000_000_000_000_000_000u128);
    }

    #[test]
    fn market_sell_walks_bids() {
        // SELL 50 shares -> hits bid 0.48. taker = 0.48 * 50 = 24 USDT.
        let amounts = market_amounts_by_quantity(
            MarketByQuantityInput {
                side: Side::Sell,
                quantity_wei: decimal_to_wei(dec!(50)).unwrap(),
                slippage_bps: 0,
                is_min_amount_out: false,
            },
            &book_simple(),
        )
        .unwrap();
        assert_eq!(amounts.last_price, 480_000_000_000_000_000u128);
        assert_eq!(amounts.maker_amount, decimal_to_wei(dec!(50)).unwrap());
        assert_eq!(amounts.taker_amount, 24_000_000_000_000_000_000u128);
    }

    #[test]
    fn market_sell_with_slippage_reduces_taker() {
        // 100 bps = 1% slippage on taker side.
        let amounts = market_amounts_by_quantity(
            MarketByQuantityInput {
                side: Side::Sell,
                quantity_wei: decimal_to_wei(dec!(50)).unwrap(),
                slippage_bps: 100,
                is_min_amount_out: false,
            },
            &book_simple(),
        )
        .unwrap();
        // base = 24e18, * 9900/10000 = 23.76e18.
        assert_eq!(amounts.taker_amount, 23_760_000_000_000_000_000u128);
    }

    // -------- market_amounts_by_value --------

    #[test]
    fn market_by_value_buys_full_first_tier_then_fraction() {
        // Spend 60 USDT: 50 USDT consumes 100 shares @ 0.50, remaining 10 USDT
        // buys 10/0.55 ≈ 18.181818... shares. Total ≈ 118.181818.
        // After 5-sig-digit truncation: 118_180_000_000_000_000_000 wei.
        let amounts = market_amounts_by_value(
            MarketByValueInput {
                value_wei: decimal_to_wei(dec!(60)).unwrap(),
                slippage_bps: 0,
                is_min_amount_out: false,
            },
            &book_simple(),
        )
        .unwrap();
        let expected_shares_truncated = retain_significant_digits(118_181_818_181_818_181_818u128, 5);
        assert_eq!(amounts.amount, expected_shares_truncated);
    }

    #[test]
    fn market_by_value_rejects_below_minimum() {
        let err = market_amounts_by_value(
            MarketByValueInput {
                value_wei: 999_999_999_999_999_999u128, // 0.99999... USDT
                slippage_bps: 0,
                is_min_amount_out: false,
            },
            &book_simple(),
        );
        assert_eq!(err, Err(OrderMathError::InvalidQuantity));
    }
}
