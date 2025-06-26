// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::debug;
use serde::Deserialize;
use tari_core::{
    base_node::rpc::{models, models::TxQueryResponse, query_service, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};

use crate::http::handler::{error_handler_with_message, util::from_hex, ErrorResponse};

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
) -> Result<Json<TxQueryResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received transaction_query request: {params:?}");
    let request = params.into();

    let response = query_service
        .transaction_query(request)
        .await
        .map_err(error_handler_with_message)?;

    Ok(Json(response))
}
