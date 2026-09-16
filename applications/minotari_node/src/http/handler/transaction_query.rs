// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt::Display, sync::Arc};

use axum::{
    Extension,
    Json,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use log::debug;
use serde::Deserialize;
use tari_common_types::types::HashOutput;
use tari_core::{
    base_node::rpc::{BaseNodeWalletQueryService, query_service},
    chain_storage::BlockchainBackend,
};
use tari_transaction_components::rpc::models;
use tari_utilities::hex::Hex;
use tonic::service::AxumBody;

use crate::{
    HttpCacheConfig,
    http::{
        cache_config::{RouteKey, apply_cache_control},
        handler::{ErrorResponse, error_handler_with_message, util::from_hex},
    },
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::transaction_query";

#[derive(Deserialize, Debug)]
pub struct TransactionQueryQueryParams {
    #[serde(deserialize_with = "from_hex")]
    pub excess_sig_nonce: Vec<u8>,
    #[serde(deserialize_with = "from_hex")]
    pub excess_sig_sig: Vec<u8>,
}

impl From<TransactionQueryQueryParams> for models::Signature {
    fn from(params: TransactionQueryQueryParams) -> Self {
        Self {
            public_nonce: params.excess_sig_nonce,
            signature: params.excess_sig_sig,
        }
    }
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<TransactionQueryQueryParams>,
    Extension(cache_cfg): Extension<Arc<HttpCacheConfig>>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received transaction_query request: {params}");
    let request = params.into();

    let response = query_service
        .transaction_query(request)
        .await
        .map_err(error_handler_with_message)?;

    let body = Json(response);
    let mut response = body.into_response();
    apply_cache_control(response.headers_mut(), &cache_cfg, RouteKey::TransactionQuery, 0, 0);
    Ok(response)
}

impl Display for TransactionQueryQueryParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TransactionQueryQueryParams {{ excess_sig_sig: {}, excess_sig_nonce: {} }}",
            HashOutput::try_from(self.excess_sig_sig.as_slice())
                .unwrap_or_default()
                .to_hex(),
            HashOutput::try_from(self.excess_sig_nonce.as_slice())
                .unwrap_or_default()
                .to_hex(),
        )
    }
}
