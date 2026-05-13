//! Stateful OpenAPI client wrapping the progenitor-generated `Client`.
//!
//! [`OpenApiClient`] owns the generated `Client` (which itself owns the
//! shared `JwtState` + `reqwest::Client`) and exposes ergonomic, domain-typed
//! wrappers on top of the raw progenitor builders. The wrappers live in the
//! sibling per-tag modules ([`auth`], [`markets`], [`orderbook`], [`orders`],
//! [`positions`]).
//!
//! JWT mutation flows through the inner [`JwtState`]; there is no auto
//! re-login, no 401 retry, no internal mutex.

pub(crate) mod codegen;
pub mod domain;
pub mod state;

mod auth;
pub mod markets;
pub mod orderbook;
pub mod orders;
pub mod positions;

use crate::error::{PredictError, PredictResult};
use codegen::Client as GenClient;
use state::JwtState;

use reqwest::Client as HttpClient;
use std::sync::Arc;

/// Stateful predict.fun OpenAPI client.
///
/// Cheap to share via `Arc`; the JWT slot inside [`JwtState`] is shared
/// across all clones (mutations are visible in-flight).
pub struct OpenApiClient {
    pub(crate) gen: GenClient,
    state: JwtState,
}

impl OpenApiClient {
    /// Build a client with explicit `base_url` (must include the version
    /// segment, e.g. `https://api.predict.fun/v1`) and a pre-configured
    /// `reqwest::Client`. The `x-api-key` header is injected per-request via
    /// the generated pre-hook — do NOT set it as a default header on `http`.
    pub fn new(http: HttpClient, base_url: String, api_key: String) -> PredictResult<Self> {
        let normalized = if base_url.ends_with('/') { base_url } else { format!("{}/", base_url) };
        // progenitor strips the trailing slash; keep the parsed validation here
        // so callers see a structured error early.
        let _ = url::Url::parse(&normalized).map_err(|e| PredictError::Other(format!("invalid base url: {}", e)))?;
        let state = JwtState::new(api_key);
        let gen = GenClient::new_with_client(normalized.trim_end_matches('/'), http, state.clone());
        Ok(Self { gen, state })
    }

    /// Replace the cached JWT (used by `Client::login`).
    pub fn set_jwt(&self, token: String) {
        self.state.set_jwt(token);
    }

    /// Drop the cached JWT.
    pub fn clear_jwt(&self) {
        self.state.clear_jwt();
    }

    /// Snapshot of the current JWT, or `None` if not logged in.
    pub fn current_jwt(&self) -> Option<Arc<String>> {
        self.state._jwt_load()
    }
}
