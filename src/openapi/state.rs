//! Inner state shared with every progenitor-generated request via the async
//! `pre_hook`.
//!
//! The hook (defined in `build.rs`) reads the `api_key` and current `jwt`
//! from this struct on each request and injects the `x-api-key` and
//! `Authorization: Bearer <jwt>` headers respectively.
//!
//! Mutating the JWT through [`set_jwt`] / [`clear_jwt`] is lock-free
//! (`ArcSwapOption`) and visible to in-flight requests as soon as the swap
//! completes.

use arc_swap::ArcSwapOption;
use std::sync::Arc;

/// Shared via `Arc` inside the generated `Client` so `Clone` is cheap and
/// JWT mutations through one handle are visible to all clones.
#[derive(Debug, Clone)]
pub struct JwtState {
    inner: Arc<JwtStateInner>,
}

#[derive(Debug)]
struct JwtStateInner {
    pub(super) jwt: ArcSwapOption<String>,
    pub(super) api_key: String,
}

impl JwtState {
    pub fn new(api_key: String) -> Self {
        Self {
            inner: Arc::new(JwtStateInner {
                jwt: ArcSwapOption::from(None),
                api_key,
            }),
        }
    }

    pub fn set_jwt(&self, jwt: String) {
        self.inner.jwt.store(Some(Arc::new(jwt)));
    }

    pub fn clear_jwt(&self) {
        self.inner.jwt.store(None);
    }

    pub fn current_jwt(&self) -> Option<String> {
        self.inner.jwt.load_full().map(|s| (*s).clone())
    }

    /// Field accessors used only by the generated `pre_hook` (in `build.rs`).
    /// Marked `#[doc(hidden)]` to discourage direct use.
    #[doc(hidden)]
    pub fn _api_key(&self) -> &str {
        &self.inner.api_key
    }

    #[doc(hidden)]
    pub fn _jwt_load(&self) -> Option<Arc<String>> {
        self.inner.jwt.load_full()
    }
}

/// Async pre-hook used by every generated request to inject `x-api-key` and
/// (when set) `Authorization: Bearer <jwt>` headers. Path referenced from
/// `build.rs`.
#[doc(hidden)]
pub fn _inject_auth_headers<'a>(
    state: &'a JwtState,
    req: &'a mut reqwest::Request,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
    let api_key = state._api_key().to_string();
    let jwt_arc = state._jwt_load();
    Box::pin(async move {
        let api_key_value = reqwest::header::HeaderValue::from_str(&api_key).map_err(|e| e.to_string())?;
        req.headers_mut().insert("x-api-key", api_key_value);
        if let Some(jwt) = jwt_arc {
            let bearer = format!("Bearer {}", jwt);
            let auth_value = reqwest::header::HeaderValue::from_str(&bearer).map_err(|e| e.to_string())?;
            req.headers_mut().insert(reqwest::header::AUTHORIZATION, auth_value);
        }
        Ok(())
    })
}
