use std::fmt::Formatter;
// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::{debug, error};
use serde::{de::Visitor, Deserialize, Deserializer};
use tari_core::{
    base_node::rpc::{
        models,
        models::TxQueryResponse,
        query_service,
        query_service::Error,
        BaseNodeWalletQueryService,
    },
    chain_storage::BlockchainBackend,
};
use tari_utilities::hex;

use crate::http::handler::{query_service_error_to_status_code, util::from_hex};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::transaction_query";

#[derive(Deserialize, Debug)]
pub struct TransactionQueryQueryParams {
    #[serde(deserialize_with = "from_hex")]
    pub public_nonce: Vec<u8>,
    #[serde(deserialize_with = "from_hex")]
    pub signature: Vec<u8>,
}

impl From<TransactionQueryQueryParams> for models::Signature {
    fn from(params: TransactionQueryQueryParams) -> Self {
        Self {
            public_nonce: params.public_nonce,
            signature: params.signature,
        }
    }
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<TransactionQueryQueryParams>,
) -> Result<Json<TxQueryResponse>, StatusCode> {
    debug!(target: LOG_TARGET, "Received transaction_query request: {params:?}");
    let request = params.into();

    let response = query_service
        .transaction_query(request)
        .await
        .map_err(query_service_error_to_status_code)?;

    Ok(Json(response))
}
