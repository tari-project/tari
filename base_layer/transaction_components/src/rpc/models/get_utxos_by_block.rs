// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use utoipa::ToSchema;

use crate::{rpc::models::transaction_output_schema, transaction_components::TransactionOutput};
#[derive(Serialize, Deserialize, Validate)]
pub struct GetUtxosByBlockRequest {
    pub header_hash: Vec<u8>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct GetUtxosByBlockResponse {
    pub header_hash: Vec<u8>,
    pub height: u64,
    #[schema(schema_with = transaction_output_schema)]
    pub outputs: Vec<TransactionOutput>,
    pub mined_timestamp: u64,
}
