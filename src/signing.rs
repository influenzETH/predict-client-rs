//! Order signing for predict.fun's V1 CTFExchange contracts (Kernel smart wallet only).
//!
//! predict.fun trading is **only** supported via Kernel smart wallets ("Predict
//! accounts"); plain EOA actors are not valid on the exchange. Every order is
//! ECDSA-signed by the EOA owner over a Kernel-wrapped EIP-712 digest, then
//! length-prefixed with the validator address so the on-chain ERC-1271 check
//! dispatches to the ECDSA validator module.
//!
//! The exchange EIP-712 domain (`name = "predict.fun CTF Exchange"`,
//! `version = "1"`) is hard-coded since all four exchange variants
//! (`(neg_risk, yield_bearing)`) share it — only the `verifyingContract` differs.
//!
//! A cross-check against the official Python SDK fixture (raw EIP-712 hash)
//! lives in the test module below.

use alloy::primitives::{keccak256, Address, B256, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::{Signature, SignerSync};
use alloy::sol;
use alloy::sol_types::SolValue;
use anyhow::{Context, Result};

use crate::contracts::{PROTOCOL_NAME, PROTOCOL_VERSION};

// ---------------------------------------------------------------------------
// Order primitives (Side / SigType)
// ---------------------------------------------------------------------------

/// Trading side. On-the-wire representation: `0 = BUY`, `1 = SELL`.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
#[repr(u8)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

/// EIP-712 signature type.
///
/// predict.fun's on-chain `Order.signatureType` field is always `EOA = 0`
/// — the smart-wallet flow is invisible to the exchange contract; the Kernel
/// wrapping happens entirely in the signature bytes (see [`encode_kernel_signature`]).
/// Other variants are kept for SDK enum parity but are never set.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
#[repr(u8)]
pub enum SigType {
    /// ECDSA signature (Kernel-wrapped). Default & only path used.
    Eoa = 0,
    /// Reserved (predict.fun proxy wallet path).
    PolyProxy = 1,
    /// Reserved (Gnosis Safe path; predict.fun does not use Safe).
    PolyGnosisSafe = 2,
    /// Reserved (EIP-1271 contract signature).
    Poly1271 = 3,
}

// ---------------------------------------------------------------------------
// EIP-712 Order struct (12-field V1 layout, identical to predict.fun SDK)
// ---------------------------------------------------------------------------

sol! {
    /// EIP-712 typed `Order` struct as defined by the predict.fun CTFExchange.
    ///
    /// Field order and types are load-bearing: the SDK and the on-chain
    /// contract both depend on this exact layout.
    struct Order {
        uint256 salt;
        address maker;
        address signer;
        address taker;
        uint256 tokenId;
        uint256 makerAmount;
        uint256 takerAmount;
        uint256 expiration;
        uint256 nonce;
        uint256 feeRateBps;
        uint8 side;
        uint8 signatureType;
    }
}

// ---------------------------------------------------------------------------
// Predict-account (Kernel smart wallet) signing
// ---------------------------------------------------------------------------

/// Kernel EIP-712 domain (BNB chain). Constants mirror SDK
/// `KERNEL_DOMAIN_BY_CHAIN_ID` (`name="Kernel"`, `version="0.3.1"`).
const KERNEL_DOMAIN_NAME: &str = "Kernel";
const KERNEL_DOMAIN_VERSION: &str = "0.3.1";

/// Wrap an order EIP-712 hash for a Kernel smart wallet ("Predict account").
///
/// Algorithm (mirrors `predict_sdk._internal.utils.eip712_wrap_hash` +
/// `hash_kernel_message`):
///
/// 1. `kernel_msg = keccak256(keccak256("Kernel(bytes32 hash)") || order_hash)`
/// 2. `domain_separator = keccak256(EIP712Domain(name="Kernel", version="0.3.1",
///    chainId, verifyingContract=predict_account))`
/// 3. `digest = keccak256(0x1901 || domain_separator || kernel_msg)`
///
/// The returned digest is the 32-byte hash that must be ECDSA-signed by the
/// EOA owner; do NOT EIP-191 prefix again.
pub fn kernel_wrap_order_hash(order_hash: B256, predict_account: Address, chain_id: u64) -> B256 {
    // Step 1: hash_kernel_message
    let kernel_type_hash = keccak256(b"Kernel(bytes32 hash)");
    let inner_encoded = <(B256, B256)>::abi_encode(&(kernel_type_hash, order_hash));
    let kernel_msg = keccak256(inner_encoded);

    // Step 2: EIP-712 domain separator for Kernel
    let domain_type_hash = keccak256(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
    let name_hash = keccak256(KERNEL_DOMAIN_NAME.as_bytes());
    let version_hash = keccak256(KERNEL_DOMAIN_VERSION.as_bytes());
    let domain_encoded = <(B256, B256, B256, U256, Address)>::abi_encode(&(domain_type_hash, name_hash, version_hash, U256::from(chain_id), predict_account));
    let domain_separator = keccak256(domain_encoded);

    // Step 3: 0x1901 || domainSeparator || kernel_msg
    let mut buf = Vec::with_capacity(2 + 32 + 32);
    buf.extend_from_slice(&[0x19, 0x01]);
    buf.extend_from_slice(domain_separator.as_slice());
    buf.extend_from_slice(kernel_msg.as_slice());
    keccak256(buf)
}

/// Encode a Kernel signature for posting to predict.fun:
/// `0x01 || ECDSA_VALIDATOR_ADDRESS(20) || sig(65)` = 86 bytes / 174 hex chars.
///
/// The leading `0x01` byte selects the Kernel "validator" mode; the validator
/// address tells Kernel which ISigner module to dispatch to (ECDSA validator
/// for an EOA-owned predict.fun account).
pub fn encode_kernel_signature(ecdsa_validator: Address, sig: &Signature) -> String {
    let mut hex = String::with_capacity(2 + 2 + 40 + 130);
    hex.push_str("0x01");
    // strip leading "0x" from the validator address — `:x` lowercases without it
    hex.push_str(&format!("{:x}", ecdsa_validator));
    hex.push_str(&const_hex::encode(sig.as_bytes()));
    hex
}

/// Sign an [`Order`] as a Predict account (Kernel smart wallet).
///
/// Uses the EOA `signer` to sign the wrapped Kernel digest, then encodes the
/// result with the Kernel validator prefix expected by predict.fun's API.
pub fn sign_order_predict_account(
    signer: &PrivateKeySigner,
    order: &Order,
    verifying_contract: Address,
    chain_id: u64,
    predict_account: Address,
    ecdsa_validator: Address,
) -> Result<String> {
    use alloy::sol_types::SolStruct;

    let domain = alloy::sol_types::Eip712Domain {
        name: Some(std::borrow::Cow::Borrowed(PROTOCOL_NAME)),
        version: Some(std::borrow::Cow::Borrowed(PROTOCOL_VERSION)),
        chain_id: Some(U256::from(chain_id)),
        verifying_contract: Some(verifying_contract),
        salt: None,
    };

    let order_hash = order.eip712_signing_hash(&domain);
    let digest = kernel_wrap_order_hash(order_hash, predict_account, chain_id);

    // Sign the 32-byte digest with EIP-191 personal_sign prefix
    // (matches SDK `encode_defunct(primitive=message_bytes)` + `sign_message`).
    let sig = signer
        .sign_message_sync(digest.as_slice())
        .context("Error signing Kernel-wrapped order digest")?;

    Ok(encode_kernel_signature(ecdsa_validator, &sig))
}

/// Sign an arbitrary message (e.g. SIWE auth challenge) as a Predict account.
///
/// Mirrors SDK `sign_predict_account_message(text)`:
/// 1. `H_msg = keccak256("\x19Ethereum Signed Message:\n{len}" || text)` (EIP-191)
/// 2. wrap with Kernel domain → digest
/// 3. EIP-191-sign digest, prefix with `0x01 || validator`
pub fn sign_message_predict_account(
    signer: &PrivateKeySigner,
    message: &str,
    chain_id: u64,
    predict_account: Address,
    ecdsa_validator: Address,
) -> Result<String> {
    // Step 1: EIP-191 hash of the original message text.
    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());
    let mut buf = Vec::with_capacity(prefix.len() + message.len());
    buf.extend_from_slice(prefix.as_bytes());
    buf.extend_from_slice(message.as_bytes());
    let inner_hash = keccak256(buf);

    // Step 2 + 3: Kernel-wrap and sign.
    let digest = kernel_wrap_order_hash(inner_hash, predict_account, chain_id);
    let sig = signer
        .sign_message_sync(digest.as_slice())
        .context("Error signing Kernel-wrapped message digest")?;

    Ok(encode_kernel_signature(ecdsa_validator, &sig))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{address, b256, Address};
    use alloy::sol_types::SolStruct;

    /// Cross-check EIP-712 order hash against the predict.fun Python SDK fixture.
    ///
    /// Source: `predict-sdk-python/tests/test_order_builder.py::test_build_typed_data_hash_matches_ts_sdk`
    ///
    /// Domain: { name: "predict.fun CTF Exchange", version: "1", chainId: 56,
    ///           verifyingContract: 0x8BC070BEdAB741406F4B1Eb65A72bee27894B689 }
    /// Expected hash: 0x814000c89efa61ae42a2bcc4c98e06e90c11480b95a12edea00e3411ec76821d
    #[test]
    fn order_eip712_hash_matches_predictfun_sdk_vector() {
        let order = Order {
            salt: U256::from(123_456_789u64),
            maker: address!("1234567890123456789012345678901234567890"),
            signer: address!("1234567890123456789012345678901234567890"),
            taker: Address::ZERO,
            tokenId: U256::from(12345u64),
            makerAmount: U256::from(1_000_000_000_000_000_000u128),
            takerAmount: U256::from(2_000_000_000_000_000_000u128),
            expiration: U256::from(4_102_444_800u64),
            nonce: U256::ZERO,
            feeRateBps: U256::from(100u64),
            side: 0,
            signatureType: 0,
        };

        let domain = alloy::sol_types::Eip712Domain {
            name: Some(std::borrow::Cow::Borrowed(PROTOCOL_NAME)),
            version: Some(std::borrow::Cow::Borrowed(PROTOCOL_VERSION)),
            chain_id: Some(U256::from(56u64)),
            verifying_contract: Some(address!("8BC070BEdAB741406F4B1Eb65A72bee27894B689")),
            salt: None,
        };

        let hash = order.eip712_signing_hash(&domain);
        let expected = b256!("814000c89efa61ae42a2bcc4c98e06e90c11480b95a12edea00e3411ec76821d");
        assert_eq!(hash, expected, "EIP-712 hash mismatch vs predict.fun SDK vector");
    }
}
