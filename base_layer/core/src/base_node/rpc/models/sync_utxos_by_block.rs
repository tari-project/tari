// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

use crate::transactions::transaction_components::TransactionOutput;

#[derive(Serialize, Deserialize, Validate)]
pub struct SyncUtxosByBlockRequest {
    pub start_header_hash: Vec<u8>,
    pub end_header_hash: Vec<u8>,
    #[validate(minimum = 1)]
    #[validate(maximum = 5)]
    pub limit: u64,
    #[validate(minimum = 0)]
    pub page: u64,
}

#[derive(Serialize, Deserialize)]
pub struct SyncUtxosByBlockResponse {
    pub utxos: Vec<SyncUtxoBlockResponse>,
    pub has_next_page: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SyncUtxoBlockResponse {
    pub header_hash: Vec<u8>,
    pub height: u64,
    pub outputs: Vec<TransactionOutput>,
    pub mined_timestamp: u64,
}
