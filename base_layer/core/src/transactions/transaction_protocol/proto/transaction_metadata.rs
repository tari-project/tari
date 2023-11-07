// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::convert::TryFrom;

use tari_common_types::types::Commitment;
use tari_utilities::ByteArray;

use super::protocol as proto;
use crate::transactions::transaction_protocol::{KernelFeatures, TransactionMetadata};

impl TryFrom<proto::TransactionMetadata> for TransactionMetadata {
    type Error = String;

    fn try_from(metadata: proto::TransactionMetadata) -> Result<Self, Self::Error> {
        let kernel_features =
            u8::try_from(metadata.kernel_features).map_err(|_| "kernel_features must be less than 256")?;
        let commitment = metadata
            .burned_commitment
            .map(|burned_commitment| {
                Commitment::from_canonical_bytes(&burned_commitment.data)
                    .map_err(|e| format!("burned_commitment.data: {}", e))
            })
            .transpose()?;
        Ok(Self {
            fee: metadata.fee.into(),
            lock_height: metadata.lock_height,
            kernel_features: KernelFeatures::from_bits(kernel_features)
                .ok_or_else(|| "Invalid or unrecognised kernel feature flag".to_string())?,
            burn_commitment: commitment,
        })
    }
}

impl From<TransactionMetadata> for proto::TransactionMetadata {
    fn from(metadata: TransactionMetadata) -> Self {
        let commitment = metadata.burn_commitment.map(|commitment| commitment.into());
        Self {
            // The absolute fee for the transaction
            fee: metadata.fee.into(),
            // The earliest block this transaction can be mined
            lock_height: metadata.lock_height,
            kernel_features: u32::from(metadata.kernel_features.bits()),
            // optional burn commitment if present
            burned_commitment: commitment,
        }
    }
}
