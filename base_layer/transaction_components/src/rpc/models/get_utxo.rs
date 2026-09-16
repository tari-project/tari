// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{rpc::models::transaction_output_schema, transaction_components::TransactionOutput};

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct GetUtxoRequest {
    pub output_hash: Vec<u8>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct GetUtxoResponse {
    #[schema(schema_with = transaction_output_schema)]
    pub output: Option<TransactionOutput>,
}
