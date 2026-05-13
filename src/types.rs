//! Domain newtypes for predict.fun ids.
//!
//! predict.fun has a different id taxonomy from Polymarket:
//!
//! | Newtype             | Inner    | Default fmt | GraphQL scalar  | Notes                               |
//! |---------------------|----------|-------------|-----------------|-------------------------------------|
//! | [`MarketId`]        | `i64`    | integer     | —               | DB auto-increment id                |
//! | [`TokenId`]         | `U256`   | `"{}"`      | `BigIntString`  | ERC1155 token id (`onChainId`)      |
//! | [`ConditionId`]     | `B256`   | `"{:#x}"`   | `String`        | 32-byte hex                         |
//! | [`OracleQuestionId`]| `B256`   | `"{:#x}"`   | `String`        | 32-byte hex                         |
//! | [`NegRiskMarketId`] | `B256`   | `"{:#x}"`   | `BigIntString`  | category.onChainId                  |
//! | [`OrderHash`]       | `B256`   | `"{:#x}"`   | —               | EIP-712 order hash                  |
//! | [`OrderId`]         | `String` | opaque      | —               | server id; not parseable as bytes   |
//!
//! ## Macro: [`define_type!`]
//!
//! Single macro `define_type!(Name, Backing, "fmt")` modeled after
//! `polymarket/src/types.rs`. Three params:
//!   1. `Name` — the newtype name.
//!   2. `Backing` — `U256` or `B256` (drives Hash variant + dec/hex bridging).
//!   3. `"fmt"` — the `format!` literal used for the **default**
//!      `Display`/`Serialize` rendering.
//!
//! Each newtype produced by `define_type!` exposes 4 inherent helpers:
//!
//! ```ignore
//! fn from_hex(s: &str) -> Result<Self, …>;  // accepts "0x…" or bare hex
//! fn from_dec(s: &str) -> Result<Self, …>;  // decimal bigint
//! fn to_hex(&self) -> String;               // "{:#x}" form
//! fn to_dec(&self) -> String;               // decimal bigint form
//! ```
//!
//! `FromStr` is **lenient**: a leading `0x`/`0X` prefix routes to `from_hex`;
//! otherwise the default-format path is used. `Deserialize` calls `FromStr`,
//! so the boundary transparently accepts either wire shape.
//!
//! `MarketId` (transparent `i64`) and `OrderId` (transparent `String`) use
//! their own dedicated macros — they aren't hex/dec values.
//!
//! ## cynic integration
//!
//! Bottom of this file declares `IsScalar<Marker>` impls so cynic-generated
//! `QueryFragment` types can use these newtypes directly as response fields:
//!
//! - `ConditionId` / `OracleQuestionId` ⇄ schema scalar `String`
//! - `TokenId` / `NegRiskMarketId` ⇄ schema scalar `BigIntString`

use alloy::primitives::{B256, U256};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Unified macro for B256/U256 newtypes
// ---------------------------------------------------------------------------

macro_rules! define_type {
    // ---- Hash impl per backing -----------------------------------------
    (@hash_impl $name:ident, U256) => {
        impl Hash for $name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                state.write_u64(self.0.as_limbs()[0]);
            }
        }
        impl nohash_hasher::IsEnabled for $name {}
    };
    (@hash_impl $name:ident, B256) => {
        impl Hash for $name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                let bytes: &[u8; 32] = self.0.as_ref();
                state.write_u64(u64::from_ne_bytes(bytes[24..32].try_into().unwrap()));
            }
        }
        impl nohash_hasher::IsEnabled for $name {}
    };

    // ---- Hex/dec helper bodies per backing -----------------------------
    //
    // U256 path uses ruint's native radix parsers and Display impls.
    // B256 path bridges through U256 for the decimal direction (B256
    // doesn't natively render as decimal; bytes32 ↔ U256 via big-endian).
    (@from_hex_body U256, $s:ident) => {{
        let stripped = $s.strip_prefix("0x").or_else(|| $s.strip_prefix("0X")).unwrap_or($s);
        U256::from_str_radix(stripped, 16).map(Self).map_err(|e| e.to_string())
    }};
    (@from_hex_body B256, $s:ident) => {{
        // alloy's B256 FromStr accepts both with/without `0x` prefix.
        B256::from_str($s).map(Self).map_err(|e| e.to_string())
    }};
    (@from_dec_body U256, $s:ident) => {{
        U256::from_str_radix($s, 10).map(Self).map_err(|e| e.to_string())
    }};
    (@from_dec_body B256, $s:ident) => {{
        // Decimal bigint → U256 → big-endian bytes32.
        match U256::from_str_radix($s, 10) {
            Ok(u) => Ok(Self(B256::from(u.to_be_bytes::<32>()))),
            Err(e) => Err(e.to_string()),
        }
    }};
    (@to_hex_body U256, $self:ident) => { format!("{:#x}", $self.0) };
    (@to_hex_body B256, $self:ident) => { format!("{:#x}", $self.0) };
    (@to_dec_body U256, $self:ident) => { format!("{}", $self.0) };
    (@to_dec_body B256, $self:ident) => {{
        // bytes32 → U256 → decimal.
        let u = U256::from_be_bytes::<32>($self.0.0);
        format!("{}", u)
    }};

    // ---- FromStr Err type per backing ----------------------------------
    // Both backings expose Err = String to keep the macro's API uniform
    // (B256 FromStr is `hex::FromHexError`, U256 is `ruint::ParseError`).
    (@fromstr_err U256) => { String };
    (@fromstr_err B256) => { String };

    // ---- Main entry ----------------------------------------------------
    ($name:ident, $backing:ident, $fmt:expr) => {
        #[derive(Eq, PartialEq, Clone, Copy)]
        pub struct $name(pub $backing);

        define_type!(@hash_impl $name, $backing);

        impl Deref for $name {
            type Target = $backing;
            fn deref(&self) -> &$backing { &self.0 }
        }
        impl DerefMut for $name {
            fn deref_mut(&mut self) -> &mut $backing { &mut self.0 }
        }

        impl From<$backing> for $name {
            fn from(v: $backing) -> Self { $name(v) }
        }
        impl From<$name> for $backing {
            fn from(v: $name) -> Self { v.0 }
        }

        impl $name {
            /// Parse from hex (with or without `0x` prefix).
            pub fn from_hex(s: &str) -> Result<Self, define_type!(@fromstr_err $backing)> {
                define_type!(@from_hex_body $backing, s)
            }
            /// Parse from a decimal bigint string.
            pub fn from_dec(s: &str) -> Result<Self, define_type!(@fromstr_err $backing)> {
                define_type!(@from_dec_body $backing, s)
            }
            /// Render as hex `0x…` (independent of the type's default Display).
            pub fn to_hex(&self) -> String {
                define_type!(@to_hex_body $backing, self)
            }
            /// Render as decimal bigint (independent of the type's default Display).
            pub fn to_dec(&self) -> String {
                define_type!(@to_dec_body $backing, self)
            }
        }

        /// Lenient: a leading `0x`/`0X` always routes to `from_hex`; otherwise
        /// the default-format path (per macro's third arg) is used.
        impl FromStr for $name {
            type Err = define_type!(@fromstr_err $backing);
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if s.starts_with("0x") || s.starts_with("0X") {
                    Self::from_hex(s)
                } else {
                    // Choose decimal vs hex by looking at the default fmt
                    // string. We can't pattern-match $fmt at expansion time
                    // here, so we just try decimal first (which is what
                    // U256-default types want) and fall back to raw-hex via
                    // from_hex for B256-default types whose `to_hex()` would
                    // round-trip without `0x`.
                    //
                    // In practice all production callers pass either bare-hex
                    // (B256) or decimal (U256) — never the other backing's
                    // bare form — so a single attempt suffices. Decimal-first
                    // matches `polymarket`'s and predict.fun's wire reality.
                    if let Ok(v) = Self::from_dec(s) {
                        return Ok(v);
                    }
                    Self::from_hex(s)
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, $fmt, self.0)
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:#x})", stringify!($name), self.0)
            }
        }
        impl fmt::LowerHex for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:x}", self.0)
            }
        }
        impl fmt::UpperHex for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:X}", self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&format!($fmt, self.0))
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s: std::borrow::Cow<'de, str> = std::borrow::Cow::deserialize(d)?;
                <$name as FromStr>::from_str(&s).map_err(serde::de::Error::custom)
            }
        }
    };
}

// ---------------------------------------------------------------------------
// macro: i64 newtype (MarketId)
// ---------------------------------------------------------------------------

macro_rules! define_int_type {
    ($name:ident) => {
        /// Newtype around `i64`; (de)serialized as a JSON integer.
        #[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl Deref for $name {
            type Target = i64;
            fn deref(&self) -> &i64 {
                &self.0
            }
        }
        impl DerefMut for $name {
            fn deref_mut(&mut self) -> &mut i64 {
                &mut self.0
            }
        }

        impl From<i64> for $name {
            fn from(v: i64) -> Self {
                $name(v)
            }
        }
        impl From<$name> for i64 {
            fn from(v: $name) -> Self {
                v.0
            }
        }

        impl FromStr for $name {
            type Err = std::num::ParseIntError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                i64::from_str(s).map($name)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

// ---------------------------------------------------------------------------
// macro: opaque String newtype (OrderId)
// ---------------------------------------------------------------------------

macro_rules! define_string_type {
    ($name:ident) => {
        /// Newtype around an opaque `String` server-issued id.
        #[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(v: String) -> Self {
                $name(v)
            }
        }
        impl From<&str> for $name {
            fn from(v: &str) -> Self {
                $name(v.to_owned())
            }
        }
        impl From<$name> for String {
            fn from(v: $name) -> Self {
                v.0
            }
        }

        impl FromStr for $name {
            type Err = std::convert::Infallible;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok($name(s.to_owned()))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), self.0)
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Concrete newtypes
// ---------------------------------------------------------------------------

define_int_type!(MarketId);
define_string_type!(OrderId);

define_type!(TokenId, U256, "{}");
define_type!(ConditionId, B256, "{:#x}");
define_type!(OracleQuestionId, B256, "{:#x}");
define_type!(NegRiskMarketId, B256, "{:#x}");
define_type!(OrderHash, B256, "{:#x}");

// Polymarket-side condition_id; carried in OpenAPI `polymarket_condition_ids`
// for cross-venue arbitrage joins. We keep this newtype local to predictfun
// (no dep on the polymarket crate) and convert at the scanner boundary.
define_type!(PolymarketConditionId, B256, "{:#x}");

// ---------------------------------------------------------------------------
// cynic schema bindings
// ---------------------------------------------------------------------------
//
// Each `IsScalar<Marker>` impl tells cynic that the newtype is a valid Rust
// representation of the GraphQL scalar `Marker`. cynic delegates the actual
// JSON decoding to the type's `serde::Deserialize` impl (defined above), so
// the boundary is fully serde-driven — no codegen-side conversion hooks.
//
// `impl_coercions!` plugs into cynic's argument-coercion network, allowing
// the newtype to satisfy `Vec<Marker>`/`Option<Marker>` argument positions.

impl cynic::schema::IsScalar<String> for ConditionId {
    type SchemaType = String;
}
cynic::impl_coercions!(ConditionId, String);

impl cynic::schema::IsScalar<String> for OracleQuestionId {
    type SchemaType = String;
}
cynic::impl_coercions!(OracleQuestionId, String);

impl cynic::schema::IsScalar<crate::graphql::schema::BigIntString> for TokenId {
    type SchemaType = crate::graphql::schema::BigIntString;
}
cynic::impl_coercions!(TokenId, crate::graphql::schema::BigIntString);

impl cynic::schema::IsScalar<crate::graphql::schema::BigIntString> for NegRiskMarketId {
    type SchemaType = crate::graphql::schema::BigIntString;
}
cynic::impl_coercions!(NegRiskMarketId, crate::graphql::schema::BigIntString);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, to_string};

    #[test]
    fn market_id_roundtrip_int() {
        let id: MarketId = from_str("12345").unwrap();
        assert_eq!(*id, 12345);
        assert_eq!(to_string(&id).unwrap(), "12345");
    }

    #[test]
    fn token_id_roundtrip_decimal_string() {
        // Realistic ERC1155 token id (256-bit decimal string).
        let s = "\"71321045679252212594626385532706912750332728571942134274518208114919032993418\"";
        let tid: TokenId = from_str(s).unwrap();
        assert_eq!(to_string(&tid).unwrap(), s);
    }

    #[test]
    fn token_id_accepts_hex_via_from_str() {
        let tid: TokenId = "0xff".parse().unwrap();
        assert_eq!(*tid, U256::from(255u64));
    }

    #[test]
    fn token_id_to_hex_to_dec() {
        let tid: TokenId = "255".parse().unwrap();
        assert_eq!(tid.to_hex(), "0xff");
        assert_eq!(tid.to_dec(), "255");
    }

    #[test]
    fn condition_id_roundtrip_hex() {
        let hex = "\"0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20\"";
        let cid: ConditionId = from_str(hex).unwrap();
        assert_eq!(to_string(&cid).unwrap(), hex);
    }

    #[test]
    fn condition_id_invalid_hex_errors() {
        let r: Result<ConditionId, _> = "not-hex".parse();
        assert!(r.is_err());
    }

    #[test]
    fn order_hash_display_is_zero_x_hex() {
        let h: OrderHash = "0x814000c89efa61ae42a2bcc4c98e06e90c11480b95a12edea00e3411ec76821d".parse().unwrap();
        assert_eq!(format!("{}", h), "0x814000c89efa61ae42a2bcc4c98e06e90c11480b95a12edea00e3411ec76821d");
    }

    #[test]
    fn neg_risk_market_id_from_hex() {
        // B256 LowerHex via `{:#x}` always renders the full 32-byte width
        // (matches `polymarket/src/types.rs` behavior).
        let full = "0x00000000000000000000000000000000000000000000000000000000deadbeef";
        let id = NegRiskMarketId::from_hex(full).unwrap();
        assert_eq!(id.to_hex(), full);
        assert_eq!(format!("{}", id), full);
    }

    #[test]
    fn neg_risk_market_id_from_dec() {
        // 0xdeadbeef = 3735928559
        let id = NegRiskMarketId::from_dec("3735928559").unwrap();
        assert_eq!(id.to_hex(), "0x00000000000000000000000000000000000000000000000000000000deadbeef");
        assert_eq!(id.to_dec(), "3735928559");
    }

    #[test]
    fn neg_risk_market_id_fromstr_dual() {
        // Lenient FromStr: both forms work. Bare-hex must be full 32-byte width
        // (alloy's B256::from_str rejects shorter strings — "invalid string length").
        let a: NegRiskMarketId = "0x00000000000000000000000000000000000000000000000000000000deadbeef".parse().unwrap();
        let b: NegRiskMarketId = "3735928559".parse().unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn order_id_is_opaque_string() {
        let oid: OrderId = from_str("\"abc-123\"").unwrap();
        assert_eq!(&*oid, "abc-123");
        assert_eq!(to_string(&oid).unwrap(), "\"abc-123\"");
    }

    #[test]
    fn b256_to_dec_via_u256_bridge() {
        let cid = ConditionId::from_hex("0x0000000000000000000000000000000000000000000000000000000000000064").unwrap();
        assert_eq!(cid.to_dec(), "100");
        // B256 LowerHex always full-width.
        assert_eq!(cid.to_hex(), "0x0000000000000000000000000000000000000000000000000000000000000064");
    }
}
