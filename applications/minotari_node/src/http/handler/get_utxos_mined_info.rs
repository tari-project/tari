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
use tari_transaction_components::rpc::models::{GetUtxosMinedInfoRequest, GetUtxosMinedInfoResponse};
use tari_utilities::hex::Hex;
use tonic::service::AxumBody;

use crate::{
    HttpCacheConfig,
    http::{
        cache_config::{RouteKey, apply_cache_control},
        handler::{ErrorResponse, error_handler_with_message, util::from_hex_comma_separated},
    },
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_utxos_mined_info";

#[derive(Deserialize, Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetUtxosMinedInfoParams {
    #[serde(deserialize_with = "from_hex_comma_separated")]
    pub hashes: Vec<Vec<u8>>,
}

impl From<GetUtxosMinedInfoParams> for GetUtxosMinedInfoRequest {
    fn from(params: GetUtxosMinedInfoParams) -> Self {
        Self { hashes: params.hashes }
    }
}

#[utoipa::path(
    get,
    operation_id = "get_utxos_mined_info",
    params(GetUtxosMinedInfoParams),
    path = "/get_utxos_mined_info",
    responses(
        (status = 200, description = "UTXOs Mined Info", body = GetUtxosMinedInfoResponse),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetUtxosMinedInfoParams>,
    Extension(cache_cfg): Extension<Arc<HttpCacheConfig>>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_utxos_mined_info request: {params}");
    let request = params.into();

    let response = query_service
        .get_utxos_mined_info(request)
        .await
        .map_err(error_handler_with_message)?;
    let body = Json(response);
    let mut response = body.into_response();
    apply_cache_control(response.headers_mut(), &cache_cfg, RouteKey::GetUtxosMinedInfo, 0, 0);
    Ok(response)
}

impl Display for GetUtxosMinedInfoParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetUtxosMinedInfoParams {{ hashes: {:?} }}",
            self.hashes
                .iter()
                .map(|h| HashOutput::try_from(h.as_slice()).unwrap_or_default().to_hex())
                .collect::<Vec<_>>()
        )
    }
}
