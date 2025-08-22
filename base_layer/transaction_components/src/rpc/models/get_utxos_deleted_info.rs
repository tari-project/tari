// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Validate)]
pub struct GetUtxosDeletedInfoRequest {
    pub hashes: Vec<Vec<u8>>,
    pub must_include_header: Vec<u8>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct GetUtxosDeletedInfoResponse {
    pub utxos: Vec<DeletedUtxoInfo>,
    pub best_block_hash: Vec<u8>,
    pub best_block_height: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct DeletedUtxoInfo {
    pub utxo_hash: Vec<u8>,
    pub found_in_header: Option<(u64, Vec<u8>)>,
    pub spent_in_header: Option<(u64, Vec<u8>)>,
}
