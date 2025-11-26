// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt::Display;

use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use tari_common_types::types::HashOutput;
use tari_utilities::hex::Hex;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Validate)]
pub struct GetUtxosMinedInfoRequest {
    pub hashes: Vec<Vec<u8>>,
}

impl Display for GetUtxosMinedInfoRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetUtxosMinedInfoRequest {{ hashes: {:?} }}",
            self.hashes
                .iter()
                .map(|h| HashOutput::try_from(h.as_slice()).unwrap_or_default().to_hex())
                .collect::<Vec<_>>()
        )
    }
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct GetUtxosMinedInfoResponse {
    pub utxos: Vec<MinedUtxoInfo>,
    pub best_block_hash: Vec<u8>,
    pub best_block_height: u64,
}

impl Display for GetUtxosMinedInfoResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let utxos = self
            .utxos
            .iter()
            .map(|u| format!("{}", u))
            .collect::<Vec<String>>()
            .join(", ")
            .to_string();
        write!(
            f,
            "GetUtxosMinedInfoResponse {{ utxos: {utxos}, best_block_hash: {}, best_block_height: {} }}",
            HashOutput::try_from(self.best_block_hash.as_slice())
                .unwrap_or_default()
                .to_hex(),
            self.best_block_height
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct MinedUtxoInfo {
    pub utxo_hash: Vec<u8>,
    pub mined_in_hash: Vec<u8>,
    pub mined_in_height: u64,
    pub mined_in_timestamp: u64,
}

impl Display for MinedUtxoInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ utxo: {}, block: {}, height: {}, timestamp: {} }}",
            HashOutput::try_from(self.utxo_hash.as_slice())
                .unwrap_or_default()
                .to_hex(),
            HashOutput::try_from(self.mined_in_hash.as_slice())
                .unwrap_or_default()
                .to_hex(),
            self.mined_in_height,
            self.mined_in_timestamp
        )
    }
}
