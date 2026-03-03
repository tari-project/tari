// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::Query,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use log::debug;
use serde::Deserialize;
use tari_common_types::{
    serializers,
    types::{CompressedPublicKey, CompressedSignature, PrivateKey},
};
use tari_core::{
    base_node::rpc::{BaseNodeWalletQueryService, query_service},
    chain_storage::BlockchainBackend,
};
use tari_transaction_components::rpc::models::GenerateKernelMerkleProofResponse;
use tari_utilities::ByteArray;
use tonic::service::AxumBody;

use crate::http::handler::{ErrorResponse, error_handler_with_message};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::generate_kernel_merkle_proof";

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GenerateKernelMerkleProofParams {
    #[serde(deserialize_with = "serializers::hex::deserialize")]
    pub excess_sig_public_nonce: Vec<u8>,
    #[serde(deserialize_with = "serializers::hex::deserialize")]
    pub excess_sig_signature: Vec<u8>,
}

#[utoipa::path(
    get,
    operation_id = "generate_kernel_merkle_proof",
    params(GenerateKernelMerkleProofParams),
    path = "/generate_kernel_merkle_proof",
    responses(
        (status = 200, description = "Merkle proof generated successfully", body = GenerateKernelMerkleProofResponse),
        (status = NOT_FOUND, description = "Kernel not found", body = ErrorResponse, example = json!({"error": "Kernel not found"})),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GenerateKernelMerkleProofParams>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received generate_kernel_merkle_proof request");

    let GenerateKernelMerkleProofParams {
        excess_sig_public_nonce,
        excess_sig_signature,
    } = params;
    if excess_sig_public_nonce.len() != CompressedPublicKey::key_length() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(format!(
                "Invalid signature public nonce length {}",
                excess_sig_public_nonce.len()
            ))),
        ));
    }

    let public_nonce = CompressedPublicKey::new(&excess_sig_public_nonce);
    let signature = PrivateKey::from_canonical_bytes(&excess_sig_signature).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(format!("Invalid signature: {}", e))),
        )
    })?;
    let excess_sig = CompressedSignature::new(public_nonce, signature);

    let response = query_service
        .generate_kernel_merkle_proof(excess_sig)
        .await
        .map_err(error_handler_with_message)?;

    let body = Json(response);
    let mut response = body.into_response();
    response.headers_mut().insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=120, s-maxage=60, stale-while-revalidate=15"),
    );
    Ok(response)
}
