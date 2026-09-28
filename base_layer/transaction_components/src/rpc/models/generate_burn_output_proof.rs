// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};
use tari_common_types::burn_proof::BurnOutputProof;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenerateBurnOutputProofResponse {
    /// Proves that the burn output was mined in a block, against the block's `block_output_mr`
    #[schema(value_type = Object)]
    pub proof: BurnOutputProof,
}
