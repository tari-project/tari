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

#[cfg(test)]
mod tests {
    use tari_common::configuration::Network;
    use tari_core::{
        base_node::{StateMachineHandle, state_machine_service::states::StatusInfo},
        chain_storage::async_db::AsyncBlockchainDb,
        mempool::test_utils::mock::create_mempool_service_mock,
        test_helpers::blockchain::create_new_blockchain_with_network,
    };
    use tari_shutdown::Shutdown;
    use tokio::sync::{broadcast, watch};

    use super::*;

    #[tokio::test]
    async fn transaction_query_returns_bad_request_on_invalid_signature_bytes() {
        let shutdown = Shutdown::new();
        let (mempool, _mempool_state) = create_mempool_service_mock();
        let db = AsyncBlockchainDb::from(create_new_blockchain_with_network(Network::LocalNet));
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        let query_service = Arc::new(query_service::Service::new(db, state_machine, mempool));

        let params = TransactionQueryQueryParams {
            excess_sig_nonce: vec![1u8; 3],
            excess_sig_sig: vec![2u8; 3],
        };
        let (status, _) = handle(
            Extension(query_service),
            Query(params),
            Extension(Arc::new(HttpCacheConfig::default())),
        )
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
