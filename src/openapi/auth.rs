//! `GET /v1/auth/message` and `POST /v1/auth`.
//!
//! Both endpoints require only `x-api-key` (injected by the pre-hook).
//! `post_auth` does NOT cache the JWT — `Client::login` is the orchestrator
//! that calls `set_jwt`.

use crate::error::{PredictError, PredictResult};
use crate::openapi::codegen::{types as gen, Client as GenClient, ClientAuthExt};
use crate::openapi::domain::{AuthMessageData, AuthTokenData};
use crate::openapi::OpenApiClient;

impl OpenApiClient {
    /// `GET /auth/message` — fetch the dynamic challenge to sign.
    pub async fn get_auth_message(&self) -> PredictResult<AuthMessageData> {
        let resp = self.gen.get_auth_message().send().await?;
        Ok(resp.into_inner().data)
    }

    /// `POST /auth` — exchange a signed challenge for a JWT.
    ///
    /// The returned token is *not* automatically cached — call
    /// [`OpenApiClient::set_jwt`] (or use `Client::login`).
    pub async fn post_auth(&self, signer: String, signature: String, message: String) -> PredictResult<AuthTokenData> {
        let body = gen::PostAuthRequest {
            signer,
            signature: gen::PostAuthRequestSignature::try_from(signature).map_err(|e| PredictError::Other(format!("invalid signature: {}", e)))?,
            message: gen::PostAuthRequestMessage::try_from(message).map_err(|e| PredictError::Other(format!("invalid message: {}", e)))?,
        };
        let resp = self.gen.post_auth().body(body).send().await?;
        Ok(resp.into_inner().data)
    }
}

// Silence unused-import warnings until we wire more wrappers.
const _: fn() = || {
    let _: PredictError;
    let _: &dyn ClientAuthExt;
    let _: &GenClient;
};
