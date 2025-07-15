// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Validate)]
pub struct GetUtxosMinedInfoRequest {
    pub hashes: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct GetUtxosMinedInfoResponse {
    pub utxos: Vec<MinedUtxoInfo>,
    pub best_block_hash: Vec<u8>,
    pub best_block_height: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct MinedUtxoInfo {
    pub utxo_hash: Vec<u8>,
    pub mined_in_hash: Vec<u8>,
    pub mined_in_height: u64,
    pub mined_in_timestamp: u64,
}
