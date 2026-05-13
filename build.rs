//! Build script: generates the OpenAPI client from `openapi/openapi.json`.
//!
//! - Patches the spec in-memory: injects derived `operationId` for every operation
//!   missing one, and renames `Resolution` enum variant `1M` → `1mo` (to avoid a
//!   typify variant-name collision with `1m`).
//! - Runs `progenitor 0.14` with `Builder` interface, `Separate` tag style,
//!   `inner_type = JwtState`, and an async `pre_hook` that injects the
//!   `x-api-key` header and (when present) `Authorization: Bearer <jwt>`.
//! - Writes the formatted Rust source to `$OUT_DIR/codegen.rs`, which is
//!   `include!`d from `src/openapi/codegen.rs`.

use progenitor::{GenerationSettings, Generator, InterfaceStyle, TagStyle, TypeImpl};
use schemars::schema::{InstanceType, SchemaObject, SingleOrVec};
use serde_json::Value;
use std::path::PathBuf;
use std::{env, fs};

const SPEC_PATH: &str = "openapi/openapi.json";

fn derive_operation_id(method: &str, path: &str) -> String {
    let mut parts: Vec<String> = vec![method.to_lowercase()];
    for seg in path.trim_start_matches('/').split('/') {
        if seg.starts_with('{') && seg.ends_with('}') {
            parts.push(format!("by_{}", &seg[1..seg.len() - 1]));
        } else {
            parts.push(seg.replace('-', "_"));
        }
    }
    parts.join("_")
}

fn patch_spec(spec: &mut Value) {
    // 1. Inject operationId where missing.
    if let Some(paths) = spec.get_mut("paths").and_then(|v| v.as_object_mut()) {
        let path_keys: Vec<String> = paths.keys().cloned().collect();
        for path in path_keys {
            let item = paths.get_mut(&path).unwrap();
            let item_obj = item.as_object_mut().expect("path item is object");
            for method in ["get", "post", "put", "delete", "patch"] {
                if let Some(op) = item_obj.get_mut(method) {
                    let op_obj = op.as_object_mut().expect("operation is object");
                    if !op_obj.contains_key("operationId") {
                        op_obj.insert("operationId".to_string(), Value::String(derive_operation_id(method, &path)));
                    }
                }
            }
        }
    }

    // 2. Rename Resolution enum variant `1M` -> `1mo` to avoid case collision with `1m`.
    if let Some(en) = spec.pointer_mut("/components/schemas/Resolution/enum").and_then(|v| v.as_array_mut()) {
        for v in en.iter_mut() {
            if v.as_str() == Some("1M") {
                *v = Value::String("1mo".to_string());
            }
        }
    }

    // 3. Make `VariantData` forward-compatible with new upstream variants.
    //
    // Upstream spec only enumerates two `oneOf` branches (CRYPTO_UP_DOWN,
    // TWEET_COUNT) but the parent `MarketVariant` enum already lists 5 values
    // (DEFAULT, SPORTS_MATCH, CRYPTO_UP_DOWN, TWEET_COUNT, SPORTS_TEAM_MATCH).
    // When the API returns a non-null `variantData` whose `type` is one of the
    // un-modelled values (e.g. on page 13 of /markets, observed in production),
    // progenitor's generated `#[serde(untagged)]` enum fails to deserialize
    // with: `data did not match any variant of untagged enum VariantData`.
    //
    // The spec docstring even claims unknown variants are handled gracefully
    // by a `deserialize_variant_data` helper, but progenitor never generates
    // that helper — it just emits a strict untagged enum.
    //
    // Fix (Plan B from CLAUDE chat): inject an extra `oneOf` branch holding
    // an empty schema `{}`, which OpenAPI semantically means "any JSON value".
    // typify renders this as `serde_json::Value`, and as the LAST untagged
    // branch it acts as a permissive catch-all: known typed branches still
    // win on first match, unknown payloads land in the Value fallback instead
    // of crashing the entire page deserialize.
    //
    // The `discriminator` block is removed at the same time; progenitor 0.14
    // already ignores it (the codegen is `#[serde(untagged)]` regardless),
    // and leaving a discriminator that doesn't list the catch-all would be
    // misleading documentation.
    if let Some(vd) = spec.pointer_mut("/components/schemas/VariantData").and_then(|v| v.as_object_mut()) {
        if let Some(one_of) = vd.get_mut("oneOf").and_then(|v| v.as_array_mut()) {
            // Empty schema = "any JSON" → typify emits `serde_json::Value`.
            one_of.push(Value::Object(serde_json::Map::new()));
        }
        // Strip the discriminator so it doesn't falsely promise typed dispatch
        // for unknown `type` values.
        vd.remove("discriminator");
    }

    // 4. Tag ID-shaped inline schemas with custom `format` strings so typify's
    //    `SchemaCache` (see `with_conversion` in `main`) substitutes our domain
    //    newtypes wholesale.
    //
    // typify's lookup is exact `SchemaObject == SchemaObject` equality with
    // `metadata` stripped, so the tagged schema must contain ONLY {type, format}
    // (any extra keyword like `description` is in `metadata` and is dropped on
    // both sides before the compare). We strip `description` defensively.
    //
    // Targets — all inline (no $ref) string/integer fields that join across
    // GraphQL ↔ OpenAPI in `MarketIndex`:
    //
    //   {Market,MarketWithStats}.id                    → MarketId      (integer)
    //   {Market,MarketWithStats}.conditionId           → ConditionId   (string)
    //   {Market,MarketWithStats}.oracleQuestionId      → OracleQuestionId
    //   {Market,MarketWithStats}.polymarketConditionIds[] → PolymarketConditionId
    //   Outcome.onChainId                              → TokenId       (string)
    //
    // Outcome is a single named schema referenced from many sites
    // (MarketWithStats.outcomes, Position.outcome, OrderFill.outcome, …) — the
    // tag therefore propagates `TokenId` to every consumer, which is intentional
    // and benign per Q1 in the design doc.
    fn tag_field(spec: &mut Value, schema_path: &str, field: &str, instance_type: &str, format_tag: &str) {
        let ptr = format!("/components/schemas/{schema_path}/properties/{field}");
        let Some(schema) = spec.pointer_mut(&ptr) else {
            panic!("build.rs: spec pointer {ptr} not found — has the upstream schema moved?");
        };
        let obj = schema.as_object_mut().unwrap_or_else(|| panic!("build.rs: {ptr} is not an object"));
        obj.clear();
        obj.insert("type".into(), Value::String(instance_type.into()));
        obj.insert("format".into(), Value::String(format_tag.into()));
    }

    fn tag_array_items(spec: &mut Value, schema_path: &str, field: &str, instance_type: &str, format_tag: &str) {
        let ptr = format!("/components/schemas/{schema_path}/properties/{field}/items");
        let Some(schema) = spec.pointer_mut(&ptr) else {
            panic!("build.rs: spec pointer {ptr} not found");
        };
        let obj = schema.as_object_mut().unwrap_or_else(|| panic!("build.rs: {ptr} is not an object"));
        obj.clear();
        obj.insert("type".into(), Value::String(instance_type.into()));
        obj.insert("format".into(), Value::String(format_tag.into()));
    }

    for parent in ["Market", "MarketWithStats"] {
        tag_field(spec, parent, "id", "integer", "predictfun-market-id");
        tag_field(spec, parent, "conditionId", "string", "predictfun-condition-id");
        tag_field(spec, parent, "oracleQuestionId", "string", "predictfun-oracle-question-id");
        tag_array_items(spec, parent, "polymarketConditionIds", "string", "predictfun-polymarket-condition-id");
    }
    tag_field(spec, "Outcome", "onChainId", "string", "predictfun-token-id");

    // PositionData numeric-string fields → external newtypes:
    //   amount               → alloy U256          (CTF share-wei, 18 decimals, decimal bigint string)
    //   valueUsd / pnlUsd /
    //   averageBuyPriceUsd   → rust_decimal::Decimal (USD, decimal string)
    //
    // Both U256 and Decimal implement FromStr + Display, which is all
    // typify needs for the substitution. Wire shape is unchanged: U256
    // Display emits a decimal bigint, Decimal Display emits "12.34".
    tag_field(spec, "PositionData", "amount", "string", "predictfun-u256-decimal-string");
    for f in ["valueUsd", "averageBuyPriceUsd", "pnlUsd"] {
        tag_field(spec, "PositionData", f, "string", "predictfun-usd-decimal");
    }

    // Order / Match / Orderbook id-shaped + numeric-string fields.
    //
    // Wire stays identical (all FromStr+Display targets):
    //   OrderId   — transparent String newtype (opaque server id)
    //   OrderHash — B256, "{:#x}" lower-hex
    //   B256      — alloy primitive, "{:#x}" lower-hex (BSC tx hash)
    //   U256      — wei decimal-bigint string (existing tag)
    //   Decimal   — rust_decimal, plain decimal string (existing tag)
    //   MarketId  — transparent i64
    //
    // Fee.amount: "fee amount in wei" → U256 (USDT wei or share wei).
    tag_field(spec, "Fee", "amount", "string", "predictfun-u256-decimal-string");

    // OrderData: id (server order id), market_id, amount/amount_filled (share wei).
    tag_field(spec, "OrderData", "id", "string", "predictfun-order-id");
    tag_field(spec, "OrderData", "marketId", "integer", "predictfun-market-id");
    tag_field(spec, "OrderData", "amount", "string", "predictfun-u256-decimal-string");
    tag_field(spec, "OrderData", "amountFilled", "string", "predictfun-u256-decimal-string");

    // MatchData: amount_filled (wei), price_executed (decimal), transaction_hash (B256).
    tag_field(spec, "MatchData", "amountFilled", "string", "predictfun-u256-decimal-string");
    tag_field(spec, "MatchData", "priceExecuted", "string", "predictfun-usd-decimal");
    tag_field(spec, "MatchData", "transactionHash", "string", "predictfun-tx-hash");

    // CreateOrderResponseData: order_id, order_hash.
    tag_field(spec, "CreateOrderResponseData", "orderId", "string", "predictfun-order-id");
    tag_field(spec, "CreateOrderResponseData", "orderHash", "string", "predictfun-order-hash");

    // RemoveOrdersResponse: removed/noop are arrays of order IDs (per
    // RemoveOrdersData.ids — removal is by-ID not by-hash). Items must be
    // bare {type:string,format:tag} after stripping the array-level
    // description (handled by `tag_array_items`'s obj.clear()).
    tag_array_items(spec, "RemoveOrdersResponse", "removed", "string", "predictfun-order-id");
    tag_array_items(spec, "RemoveOrdersResponse", "noop", "string", "predictfun-order-id");

    // LastOrderSettled: id (order id), market_id.
    tag_field(spec, "LastOrderSettled", "id", "string", "predictfun-order-id");
    tag_field(spec, "LastOrderSettled", "marketId", "integer", "predictfun-market-id");

    // OrderFillData: maker/taker EOA → Address.
    tag_field(spec, "OrderFillData", "signer", "string", "predictfun-eoa-address");

    // OrderbookData.market_id.
    tag_field(spec, "OrderbookData", "marketId", "integer", "predictfun-market-id");
}

/// Build a `SchemaObject` matching `{ "type": <instance>, "format": <format_tag> }`
/// — the exact shape `tag_field` / `tag_array_items` write into the spec.
fn conversion_schema(instance: InstanceType, format_tag: &str) -> SchemaObject {
    SchemaObject {
        instance_type: Some(SingleOrVec::Single(Box::new(instance))),
        format: Some(format_tag.to_string()),
        ..Default::default()
    }
}

fn main() {
    println!("cargo:rerun-if-changed={}", SPEC_PATH);
    println!("cargo:rerun-if-changed=build.rs");

    let spec_text = fs::read_to_string(SPEC_PATH).expect("read openapi spec");
    let mut spec_json: Value = serde_json::from_str(&spec_text).expect("parse openapi spec as JSON");
    patch_spec(&mut spec_json);
    let patched = serde_json::to_string(&spec_json).expect("reserialize patched spec");
    let spec: openapiv3::OpenAPI = serde_json::from_str(&patched).expect("parse patched spec as OpenAPI");

    let mut settings = GenerationSettings::default();
    settings
        .with_interface(InterfaceStyle::Builder)
        .with_tag(TagStyle::Separate)
        .with_inner_type(syn::parse_str("crate::openapi::state::JwtState").unwrap())
        .with_pre_hook_async(syn::parse_str("crate::openapi::state::_inject_auth_headers").unwrap());

    // Register one conversion per (instance_type, format_tag) tag injected by
    // `patch_spec` step 4. The schema we hand to typify must be byte-identical
    // (after metadata stripping) to what's in the spec — see `conversion_schema`.
    //
    // `TypeImpl::{FromStr, Display}` are the only impls typify uses to reason
    // about substituted types; our newtypes already derive Clone/Debug/Eq/Hash
    // so containing structs derive everything they need.
    let id_impls = || [TypeImpl::FromStr, TypeImpl::Display].into_iter();

    settings.with_conversion(
        conversion_schema(InstanceType::Integer, "predictfun-market-id"),
        "crate::types::MarketId",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-condition-id"),
        "crate::types::ConditionId",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-oracle-question-id"),
        "crate::types::OracleQuestionId",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-polymarket-condition-id"),
        "crate::types::PolymarketConditionId",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-token-id"),
        "crate::types::TokenId",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-u256-decimal-string"),
        "::alloy::primitives::U256",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-usd-decimal"),
        "::rust_decimal::Decimal",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-order-id"),
        "crate::types::OrderId",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-order-hash"),
        "crate::types::OrderHash",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-tx-hash"),
        "::alloy::primitives::B256",
        id_impls(),
    );
    settings.with_conversion(
        conversion_schema(InstanceType::String, "predictfun-eoa-address"),
        "::alloy::primitives::Address",
        id_impls(),
    );

    let mut generator = Generator::new(&settings);
    let tokens = generator.generate_tokens(&spec).expect("progenitor generate_tokens");
    let ast: syn::File = syn::parse2(tokens).expect("parse generated tokens");
    let formatted = prettyplease::unparse(&ast);

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set"));
    let out_path = out_dir.join("codegen.rs");
    fs::write(&out_path, formatted).expect("write codegen.rs");
    println!("cargo:warning=progenitor codegen written to {}", out_path.display());
}
