pub type PredictResult<T> = Result<T, PredictError>;

#[derive(Debug)]
pub enum PredictError {
    /// HTTP/network transport error
    Network(reqwest::Error),
    /// HTTP error (non-2xx)
    Http { status: u16, body: String },
    /// JSON (de)serialization error
    Json(serde_json::Error),
    /// Authentication failure
    Auth(String),
    /// Order signing / EIP-712 error
    Signing(anyhow::Error),
    /// On-chain contract call failure
    Contract(String),
    /// GraphQL endpoint returned `errors` (or no `data`)
    GraphQl(String),
    /// Catch-all
    Other(String),
}

impl std::fmt::Display for PredictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PredictError::Network(e) => write!(f, "network error: {}", e),
            PredictError::Http { status, body } => write!(f, "HTTP {}: {}", status, body),
            PredictError::Json(e) => write!(f, "json error: {}", e),
            PredictError::Auth(s) => write!(f, "auth error: {}", s),
            PredictError::Signing(e) => write!(f, "signing error: {}", e),
            PredictError::Contract(s) => write!(f, "contract error: {}", s),
            PredictError::GraphQl(s) => write!(f, "graphql error: {}", s),
            PredictError::Other(s) => write!(f, "other: {}", s),
        }
    }
}

impl std::error::Error for PredictError {}

impl From<reqwest::Error> for PredictError {
    fn from(e: reqwest::Error) -> Self {
        PredictError::Network(e)
    }
}
impl From<serde_json::Error> for PredictError {
    fn from(e: serde_json::Error) -> Self {
        PredictError::Json(e)
    }
}
impl From<anyhow::Error> for PredictError {
    fn from(e: anyhow::Error) -> Self {
        PredictError::Signing(e)
    }
}

/// Map progenitor's transport-level [`Error`] (parameterised over the
/// generated [`ErrorResponse`] body) into [`PredictError`].
///
/// - `ErrorResponse(401)` → [`PredictError::Auth`] (caller decides whether to
///   re-login; we never retry implicitly).
/// - Any other `ErrorResponse` → [`PredictError::Http`] with the formatted
///   server-side message + trace.
/// - `CommunicationError` / `ResponseBodyError` / `InvalidUpgrade` →
///   [`PredictError::Network`].
/// - `InvalidResponsePayload` → [`PredictError::Json`].
/// - `UnexpectedResponse` → [`PredictError::Http`] with the bare status.
/// - `InvalidRequest` / `Custom` (pre-/post-hook failures) →
///   [`PredictError::Other`].
impl From<progenitor_client::Error<crate::openapi::codegen::types::ErrorResponse>> for PredictError {
    fn from(e: progenitor_client::Error<crate::openapi::codegen::types::ErrorResponse>) -> Self {
        use progenitor_client::Error as PE;
        match e {
            PE::InvalidRequest(s) => PredictError::Other(format!("invalid request: {}", s)),
            PE::CommunicationError(re) => PredictError::Network(re),
            PE::InvalidUpgrade(re) => PredictError::Network(re),
            PE::ResponseBodyError(re) => PredictError::Network(re),
            PE::InvalidResponsePayload(_, je) => PredictError::Json(je),
            PE::UnexpectedResponse(resp) => PredictError::Http {
                status: resp.status().as_u16(),
                body: format!("unexpected response: {}", resp.status()),
            },
            PE::ErrorResponse(rv) => {
                let status = rv.status().as_u16();
                let body = rv.into_inner();
                let detail = match &body.trace {
                    Some(t) => format!("{:?}: {} (trace={})", body.error, body.message, t),
                    None => format!("{:?}: {}", body.error, body.message),
                };
                if status == 401 {
                    PredictError::Auth(detail)
                } else {
                    PredictError::Http { status, body: detail }
                }
            }
            PE::Custom(s) => PredictError::Other(format!("hook error: {}", s)),
        }
    }
}

/// Companion `From` for endpoints whose OpenAPI spec doesn't declare a typed
/// error body — progenitor parameterises those over `()`. We collapse the
/// `ErrorResponse` variant to a bare `PredictError::Http { status, … }` since
/// there's no typed body to forward.
impl From<progenitor_client::Error<()>> for PredictError {
    fn from(e: progenitor_client::Error<()>) -> Self {
        use progenitor_client::Error as PE;
        match e {
            PE::InvalidRequest(s) => PredictError::Other(format!("invalid request: {}", s)),
            PE::CommunicationError(re) => PredictError::Network(re),
            PE::InvalidUpgrade(re) => PredictError::Network(re),
            PE::ResponseBodyError(re) => PredictError::Network(re),
            PE::InvalidResponsePayload(_, je) => PredictError::Json(je),
            PE::UnexpectedResponse(resp) => PredictError::Http {
                status: resp.status().as_u16(),
                body: format!("unexpected response: {}", resp.status()),
            },
            PE::ErrorResponse(rv) => {
                let status = rv.status().as_u16();
                let detail = format!("error response (no typed body): {}", rv.status());
                if status == 401 {
                    PredictError::Auth(detail)
                } else {
                    PredictError::Http { status, body: detail }
                }
            }
            PE::Custom(s) => PredictError::Other(format!("hook error: {}", s)),
        }
    }
}
