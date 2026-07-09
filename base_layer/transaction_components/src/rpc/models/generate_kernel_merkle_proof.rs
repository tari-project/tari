// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};
use tari_common_types::{serializers, types::FixedHash};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GenerateKernelMerkleProofResponse {
    #[serde(with = "serializers::base64")]
    pub block_hash: FixedHash,
    #[serde(with = "serializers::base64")]
    pub encoded_merkle_proof: Vec<u8>,
    pub leaf_index: u64,
    /// The height of the block the kernel was mined in. `Option` (via `serde(default)`) so responses from
    /// older base nodes that predate this field deserialize as `None` rather than failing.
    #[serde(default)]
    pub block_height: Option<u64>,
}
