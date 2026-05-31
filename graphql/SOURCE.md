# GraphQL Schema Source

`schema.graphql` is a Schema Definition Language (SDL) dump of the public
predict.fun GraphQL endpoint, produced via introspection.

- **Endpoint**: <https://graphql.predict.fun/graphql>
- **Tool**: [`cynic` CLI](https://cynic-rs.dev) (`cynic introspect`)

## Regenerate

```sh
cynic introspect https://graphql.predict.fun/graphql -o graphql/schema.graphql
```

After regenerating, audit `src/graphql/enums.rs` and `src/graphql/inputs.rs`
for new enum variants / input fields that the typed cynic mirrors must learn
about (cynic will deserialize-fail on unknown enum values).
