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
use tari_transaction_components::rpc::models::{GetUtxosDeletedInfoRequest, GetUtxosDeletedInfoResponse};
use tari_utilities::hex::Hex;
use tonic::service::AxumBody;

use crate::{
    HttpCacheConfig,
    http::{
        cache_config::{RouteKey, apply_cache_control},
        handler::{
            ErrorResponse,
            error_handler_with_message,
            util::{from_hex, from_hex_comma_separated},
        },
    },
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_utxos_deleted_info";

#[derive(Deserialize, Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetUtxosDeletedInfoParams {
    #[serde(deserialize_with = "from_hex_comma_separated")]
    pub hashes: Vec<Vec<u8>>,
    #[serde(deserialize_with = "from_hex")]
    pub must_include_header: Vec<u8>,
}

impl From<GetUtxosDeletedInfoParams> for GetUtxosDeletedInfoRequest {
    fn from(params: GetUtxosDeletedInfoParams) -> Self {
        Self {
            hashes: params.hashes,
            must_include_header: params.must_include_header,
        }
    }
}

#[utoipa::path(
    get,
    operation_id = "get_utxos_deleted_info",
    params(GetUtxosDeletedInfoParams),
    path = "/get_utxos_deleted_info",
    responses(
        (status = 200, description = "UTXOs Deleted Info", body = GetUtxosDeletedInfoResponse),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetUtxosDeletedInfoParams>,
    Extension(cache_cfg): Extension<Arc<HttpCacheConfig>>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_utxos_deleted_info request: {params}");
    let request = params.into();

    let response = query_service
        .get_utxos_deleted_info(request)
        .await
        .map_err(error_handler_with_message)?;

    let body = Json(response);
    let mut response = body.into_response();
    apply_cache_control(response.headers_mut(), &cache_cfg, RouteKey::GetUtxosDeletedInfo, 0, 0);
    Ok(response)
}

impl Display for GetUtxosDeletedInfoParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetUtxosDeletedInfoParams {{ must_include_header: {}, hashes: {:?} }}",
            HashOutput::try_from(self.must_include_header.as_slice())
                .unwrap_or_default()
                .to_hex(),
            self.hashes
                .iter()
                .map(|h| HashOutput::try_from(h.as_slice()).unwrap_or_default().to_hex())
                .collect::<Vec<_>>()
        )
    }
}
