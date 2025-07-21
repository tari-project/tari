// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Validate)]
pub struct SyncUtxosByBlockRequest {
    pub start_header_hash: Vec<u8>,
    #[validate(minimum = 1)]
    #[validate(maximum = 2000)]
    pub limit: u64,
    #[validate(minimum = 0)]
    pub page: u64,
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct SyncUtxosByBlockResponse {
    pub blocks: Vec<BlockUtxoInfo>,
    pub has_next_page: bool,
    pub next_header_to_scan: Vec<u8>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct BlockUtxoInfo {
    pub header_hash: Vec<u8>,
    pub height: u64,
    pub outputs: Vec<MinimalUtxoSyncInfo>,
    pub inputs: Vec<Vec<u8>>,
    pub mined_timestamp: u64,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct MinimalUtxoSyncInfo {
    pub output_hash: Vec<u8>,
    pub commitment: Vec<u8>,
    pub encrypted_data: Vec<u8>,
    pub sender_offset_public_key: Vec<u8>,
}
