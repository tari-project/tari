// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use tari_common_types::types;
use tari_crypto::{compressed_key::CompressedKey, ristretto::RistrettoSecretKey};
use tari_utilities::{ByteArray, ByteArrayError};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TxLocation {
    None = 0,
    NotStored = 1,
    InMempool = 2,
    Mined = 3,
}

#[derive(Serialize, Deserialize)]
pub struct TxQueryResponse {
    pub location: TxLocation,
    pub mined_height: Option<u64>,
    pub mined_header_hash: Option<Vec<u8>>,
    pub mined_timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct Signature {
    pub public_nonce: Vec<u8>,
    pub signature: Vec<u8>,
}

impl TryFrom<Signature> for types::Signature {
    type Error = ByteArrayError;

    fn try_from(signature: Signature) -> Result<Self, Self::Error> {
        Ok(types::Signature::new(
            CompressedKey::new(&signature.public_nonce),
            RistrettoSecretKey::from_canonical_bytes(&signature.signature)?,
        ))
    }
}
