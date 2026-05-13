//! predict.fun BSC contract config.
//!
//! 4 CTFExchange variants are routed by `(neg_risk, yield_bearing)`. Addresses
//! sourced from the official Python SDK (`predict_sdk/constants.py::ADDRESSES_BY_CHAIN_ID`).
//!
//! Order amount math no longer goes through a `ContractConfigProvider` trait:
//! predict.fun uses a different algorithm (integer wei + 3/5 sig-digit
//! truncation + book-aware market orders) than `polymarket`'s
//! Decimal+RoundConfig path, so we own the full builder locally
//! (see [`crate::order_math`] / [`crate::order_builder`]).

use alloy::primitives::{address, Address};

/// EIP-712 domain name used by predict.fun's CTFExchange contracts (all variants).
pub const PROTOCOL_NAME: &str = "predict.fun CTF Exchange";
/// EIP-712 domain version. All current exchange contracts use `"1"`.
pub const PROTOCOL_VERSION: &str = "1";

/// USDT decimals on BSC (note: 18, *not* 6 like USDC on Polygon).
pub const USDT_DECIMALS: u32 = 18;

/// Wei multiplier for amount math: `10^18`.
pub const PRECISION_WEI: u128 = 1_000_000_000_000_000_000u128;

/// Maximum salt value used by the SDK (`random.randrange(MAX_SALT)`).
pub const MAX_SALT: u64 = 2_147_483_648;

/// BNB Smart Chain network selector.
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum Network {
    BnbMainnet,
    BnbTestnet,
}

impl Network {
    /// EIP-155 chain id.
    pub fn chain_id(self) -> u64 {
        match self {
            Network::BnbMainnet => 56,
            Network::BnbTestnet => 97,
        }
    }
}

/// All on-chain addresses needed to trade & settle on predict.fun.
#[derive(Clone, Debug)]
pub struct PredictBnbContractConfig {
    pub network: Network,
    /// (neg_risk=false, yield_bearing=false)
    pub ctf_exchange: Address,
    /// (neg_risk=true,  yield_bearing=false)
    pub neg_risk_ctf_exchange: Address,
    /// (neg_risk=false, yield_bearing=true)
    pub yield_bearing_ctf_exchange: Address,
    /// (neg_risk=true,  yield_bearing=true)
    pub yield_bearing_neg_risk_ctf_exchange: Address,
    pub neg_risk_adapter: Address,
    pub yield_bearing_neg_risk_adapter: Address,
    pub conditional_tokens: Address,
    /// Mainnet: same address as `conditional_tokens` (per SDK).
    pub neg_risk_conditional_tokens: Address,
    pub yield_bearing_conditional_tokens: Address,
    pub yield_bearing_neg_risk_conditional_tokens: Address,
    pub usdt: Address,
    pub kernel: Address,
    pub ecdsa_validator: Address,
}

impl PredictBnbContractConfig {
    pub fn mainnet() -> Self {
        Self {
            network: Network::BnbMainnet,
            ctf_exchange: address!("8BC070BEdAB741406F4B1Eb65A72bee27894B689"),
            neg_risk_ctf_exchange: address!("365fb81bd4A24D6303cd2F19c349dE6894D8d58A"),
            yield_bearing_ctf_exchange: address!("6bEb5a40C032AFc305961162d8204CDA16DECFa5"),
            yield_bearing_neg_risk_ctf_exchange: address!("8A289d458f5a134bA40015085A8F50Ffb681B41d"),
            neg_risk_adapter: address!("c3Cf7c252f65E0d8D88537dF96569AE94a7F1A6E"),
            yield_bearing_neg_risk_adapter: address!("41dCe1A4B8FB5e6327701750aF6231B7CD0B2A40"),
            conditional_tokens: address!("22DA1810B194ca018378464a58f6Ac2B10C9d244"),
            neg_risk_conditional_tokens: address!("22DA1810B194ca018378464a58f6Ac2B10C9d244"),
            yield_bearing_conditional_tokens: address!("9400F8Ad57e9e0F352345935d6D3175975eb1d9F"),
            yield_bearing_neg_risk_conditional_tokens: address!("F64b0b318AAf83BD9071110af24D24445719A07F"),
            usdt: address!("55d398326f99059fF775485246999027B3197955"),
            kernel: address!("BAC849bB641841b44E965fB01A4Bf5F074f84b4D"),
            ecdsa_validator: address!("845ADb2C711129d4f3966735eD98a9F09fC4cE57"),
        }
    }

    pub fn testnet() -> Self {
        Self {
            network: Network::BnbTestnet,
            ctf_exchange: address!("2A6413639BD3d73a20ed8C95F634Ce198ABbd2d7"),
            neg_risk_ctf_exchange: address!("d690b2bd441bE36431F6F6639D7Ad351e7B29680"),
            yield_bearing_ctf_exchange: address!("8a6B4Fa700A1e310b106E7a48bAFa29111f66e89"),
            yield_bearing_neg_risk_ctf_exchange: address!("95D5113bc50eD201e319101bbca3e0E250662fCC"),
            neg_risk_adapter: address!("285c1B939380B130D7EBd09467b93faD4BA623Ed"),
            yield_bearing_neg_risk_adapter: address!("b74aea04bdeBE912Aa425bC9173F9668e6f11F99"),
            conditional_tokens: address!("2827AAef52D71910E8FBad2FfeBC1B6C2DA37743"),
            neg_risk_conditional_tokens: address!("2827AAef52D71910E8FBad2FfeBC1B6C2DA37743"),
            yield_bearing_conditional_tokens: address!("38BF1cbD66d174bb5F3037d7068E708861D68D7f"),
            yield_bearing_neg_risk_conditional_tokens: address!("26e865CbaAe99b62fbF9D18B55c25B5E079A93D5"),
            usdt: address!("B32171ecD878607FFc4F8FC0bCcE6852BB3149E0"),
            kernel: address!("BAC849bB641841b44E965fB01A4Bf5F074f84b4D"),
            ecdsa_validator: address!("845ADb2C711129d4f3966735eD98a9F09fC4cE57"),
        }
    }

    /// EIP-155 chain id (56 mainnet / 97 testnet).
    pub fn chain_id(&self) -> u64 {
        self.network.chain_id()
    }

    /// Pick the correct CTFExchange (EIP-712 verifying contract) for a market.
    pub fn exchange_for(&self, neg_risk: bool, yield_bearing: bool) -> Address {
        match (neg_risk, yield_bearing) {
            (false, false) => self.ctf_exchange,
            (true, false) => self.neg_risk_ctf_exchange,
            (false, true) => self.yield_bearing_ctf_exchange,
            (true, true) => self.yield_bearing_neg_risk_ctf_exchange,
        }
    }

    /// Pick the correct ConditionalTokens contract for a market.
    pub fn conditional_tokens_for(&self, neg_risk: bool, yield_bearing: bool) -> Address {
        match (neg_risk, yield_bearing) {
            (false, false) => self.conditional_tokens,
            (true, false) => self.neg_risk_conditional_tokens,
            (false, true) => self.yield_bearing_conditional_tokens,
            (true, true) => self.yield_bearing_neg_risk_conditional_tokens,
        }
    }

    /// Pick the correct NegRiskAdapter (yield-bearing or not). Only relevant
    /// for `neg_risk = true` markets.
    pub fn neg_risk_adapter_for(&self, yield_bearing: bool) -> Address {
        if yield_bearing {
            self.yield_bearing_neg_risk_adapter
        } else {
            self.neg_risk_adapter
        }
    }
}
