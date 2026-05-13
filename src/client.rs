//! High-level protocol client for predict.fun.

use crate::contracts::PredictBnbContractConfig;
use crate::error::{PredictError, PredictResult};
use crate::graphql::GraphQLClient;
use crate::openapi::OpenApiClient;
use crate::order_builder::OrderBuilder;

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client as HttpClient;
use std::sync::{Arc, Arc as StdArc};

/// High-level predict.fun protocol client.
pub struct Client {
    pub api: OpenApiClient,
    pub graphql: GraphQLClient,
    pub signer: PrivateKeySigner,
    pub predict_account: Address,
    pub funder: Address,
    pub contracts: PredictBnbContractConfig,
}

impl Client {
    /// Build a client with explicit parameters.
    pub fn new(
        api_key: String,
        signer: PrivateKeySigner,
        predict_account: Address,
        contracts: PredictBnbContractConfig,
        base_url: String,
    ) -> PredictResult<Arc<Self>> {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::CONTENT_TYPE, HeaderValue::from_static("application/json; charset=utf-8"));

        let http = HttpClient::builder().default_headers(headers).build().map_err(PredictError::from)?;
        Self::new_with_http(http, api_key, signer, predict_account, contracts, base_url)
    }

    /// Build a client with a caller-provided reqwest client.
    pub fn new_with_http(
        http: HttpClient,
        api_key: String,
        signer: PrivateKeySigner,
        predict_account: Address,
        contracts: PredictBnbContractConfig,
        base_url: String,
    ) -> PredictResult<Arc<Self>> {
        let api = OpenApiClient::new(http.clone(), base_url, api_key)?;
        let graphql = GraphQLClient::with_default_endpoint(http)?;

        Ok(Arc::new(Self {
            api,
            graphql,
            signer,
            predict_account,
            funder: predict_account,
            contracts,
        }))
    }

    pub fn current_jwt(&self) -> Option<StdArc<String>> {
        self.api.current_jwt()
    }

    pub fn order_builder(&self) -> OrderBuilder {
        OrderBuilder::new(self.signer.clone(), self.contracts.clone(), self.predict_account)
    }

    /// Fetch an auth challenge, sign it as the Kernel account, post it, and cache the JWT.
    pub async fn login(&self) -> PredictResult<String> {
        let challenge = self.api.get_auth_message().await?;
        let message = challenge.message;

        let signature_hex = crate::signing::sign_message_predict_account(
            &self.signer,
            &message,
            self.contracts.chain_id(),
            self.predict_account,
            self.contracts.ecdsa_validator,
        )
        .map_err(|e| PredictError::Auth(format!("sign challenge failed: {}", e)))?;

        let signer_checksummed = Address::to_checksum(&self.predict_account, None);
        let token = self.api.post_auth(signer_checksummed, signature_hex, message).await?;
        self.api.set_jwt(token.token.clone());
        Ok(token.token)
    }
}
