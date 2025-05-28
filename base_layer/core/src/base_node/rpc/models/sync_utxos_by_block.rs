/*
 * Copyright 2025 The Tari Project
 * SPDX-License-Identifier: BSD-3-Clause
 */
use crate::transactions::transaction_components::TransactionOutput;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Serialize, Deserialize, Validate)]
pub struct SyncUtxosByBlockRequest {
    pub start_header_hash: Vec<u8>,
    pub end_header_hash: Vec<u8>,
    #[validate(minimum = 1)]
    #[validate(maximum = 5)]
    pub limit: u64,
    pub page: u64,
}

#[derive(Serialize, Deserialize)]
pub struct SyncUtxosByBlockResponse {
    pub outputs: Vec<TransactionOutput>,
    pub height: u64,
    pub header_hash: Vec<u8>,
    pub mined_timestamp: u64,
    pub next_page: Option<u64>,
}