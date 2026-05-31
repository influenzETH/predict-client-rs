# OpenAPI Spec Source

`openapi.json` is the live REST API schema for predict.fun.

- **Source page**: <https://api.predict.fun/docs>
  (the API does not expose a standalone `openapi.json` / `swagger.json` /
  `docs/json` endpoint — the spec is embedded as a `let spec = {...};`
  JavaScript literal inside the openapi-explorer HTML.)
- **Refresh script**: [`scripts/fetch-openapi.sh`](../scripts/fetch-openapi.sh)

## Regenerate

```sh
scripts/fetch-openapi.sh
cargo check     # re-runs build.rs → progenitor codegen
```

## After regeneration

`build.rs::patch_spec` mutates the spec before handing it to progenitor.
Re-verify these patch points still apply if upstream restructures:

1. **`Resolution` enum** — `1M` → `1mo` rename (avoids case collision with `1m`).
2. **`VariantData` oneOf** — append empty schema `{}` as catch-all and strip
   `discriminator` so unknown `type` values deserialize into
   `serde_json::Value` instead of crashing.
3. **ID-shaped field tagging** — every `tag_field` / `tag_array_items` call
   under `for parent in ["Market", "MarketWithStats"]` and the
   `Position/Order/Match/Orderbook` blocks must keep finding its target
   JSON pointer; missing pointers `panic!` at build time.
