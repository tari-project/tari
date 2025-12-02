// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use tari_utilities::hex::Hex;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Validate, Debug)]
pub struct SyncUtxosByBlockRequest {
    pub start_header_hash: Vec<u8>,
    #[validate(minimum = 1)]
    #[validate(maximum = 2000)]
    pub limit: u64,
    #[validate(minimum = 0)]
    pub page: u64,
    #[serde(default)]
    pub exclude_spent: bool,
    #[serde(default)]
    pub version: u8,
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct SyncUtxosByBlockResponseV0 {
    pub blocks: Vec<BlockUtxoInfo>,
    pub has_next_page: bool,
    pub next_header_to_scan: Vec<u8>,
}

impl From<SyncUtxosByBlockResponseV1> for SyncUtxosByBlockResponseV0 {
    fn from(value: SyncUtxosByBlockResponseV1) -> Self {
        use base64::{prelude::BASE64_STANDARD, Engine};
        let blocks = value
            .blocks
            .into_iter()
            .map(|block| BlockUtxoInfo {
                header_hash: BASE64_STANDARD.decode(block.header_hash).unwrap_or_default(),
                height: block.height,
                outputs: block
                    .outputs
                    .into_iter()
                    .map(|utxo| MinimalUtxoSyncInfo {
                        output_hash: BASE64_STANDARD.decode(utxo.output_hash).unwrap_or_default(),
                        commitment: BASE64_STANDARD.decode(utxo.commitment).unwrap_or_default(),
                        encrypted_data: BASE64_STANDARD.decode(utxo.encrypted_data).unwrap_or_default(),
                        sender_offset_public_key: BASE64_STANDARD
                            .decode(utxo.sender_offset_public_key)
                            .unwrap_or_default(),
                    })
                    .collect(),
                inputs: block
                    .inputs
                    .into_iter()
                    .map(|input| BASE64_STANDARD.decode(input).unwrap_or_default())
                    .collect(),
                mined_timestamp: block.mined_timestamp,
            })
            .collect();

        Self {
            blocks,
            has_next_page: value.has_next_page,
            next_header_to_scan: Vec::<u8>::from_hex(&value.next_header_to_scan).unwrap_or_default(),
        }
    }
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct SyncUtxosByBlockResponseV1 {
    pub blocks: Vec<BlockUtxoInfoBase64>,
    pub has_next_page: bool,
    pub next_header_to_scan: String,
}

impl From<SyncUtxosByBlockResponseV0> for SyncUtxosByBlockResponseV1 {
    fn from(value: SyncUtxosByBlockResponseV0) -> Self {
        use base64::{prelude::BASE64_STANDARD, Engine};
        let blocks = value
            .blocks
            .into_iter()
            .map(|block| BlockUtxoInfoBase64 {
                header_hash: BASE64_STANDARD.encode(block.header_hash),
                height: block.height,
                outputs: block
                    .outputs
                    .into_iter()
                    .map(|utxo| MinimalUtxoSyncInfoBase64 {
                        output_hash: BASE64_STANDARD.encode(utxo.output_hash),
                        commitment: BASE64_STANDARD.encode(utxo.commitment),
                        encrypted_data: BASE64_STANDARD.encode(utxo.encrypted_data),
                        sender_offset_public_key: BASE64_STANDARD.encode(utxo.sender_offset_public_key),
                    })
                    .collect(),
                inputs: block
                    .inputs
                    .into_iter()
                    .map(|input| BASE64_STANDARD.encode(input))
                    .collect(),
                mined_timestamp: block.mined_timestamp,
            })
            .collect();

        Self {
            blocks,
            has_next_page: value.has_next_page,
            next_header_to_scan: value.next_header_to_scan.to_hex(),
        }
    }
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
pub struct BlockUtxoInfoBase64 {
    pub header_hash: String,
    pub height: u64,
    pub outputs: Vec<MinimalUtxoSyncInfoBase64>,
    pub inputs: Vec<String>,
    pub mined_timestamp: u64,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct MinimalUtxoSyncInfoBase64 {
    pub output_hash: String,
    pub commitment: String,
    pub encrypted_data: String,
    pub sender_offset_public_key: String,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct MinimalUtxoSyncInfo {
    pub output_hash: Vec<u8>,
    pub commitment: Vec<u8>,
    pub encrypted_data: Vec<u8>,
    pub sender_offset_public_key: Vec<u8>,
}
