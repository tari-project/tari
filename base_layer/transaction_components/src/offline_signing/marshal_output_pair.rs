// Copyright 2025. The Tari Project
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
use serde::{Deserialize, Serialize};

use crate::{transaction_builder::OutputPair, transaction_components::TransactionError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MarshalOutputPair {
    pub output_pair: OutputPair,
    pub encrypted_kernel_nonce: String,
    pub encrypted_sender_offset_key: Option<String>,
    pub encrypted_output_commitment_mask: String,
}

impl MarshalOutputPair {
    pub fn marshal(output_pair: OutputPair) -> Result<Self, TransactionError> {
        let encrypted_kernel_nonce = output_pair.kernel_nonce.to_string();
        let encrypted_sender_offset_key = output_pair.sender_offset_key_id.as_ref().map(|key| key.to_string());
        let encrypted_output_commitment_mask = output_pair.output.commitment_mask_key_id().to_string();

        Ok(MarshalOutputPair {
            output_pair,
            encrypted_kernel_nonce,
            encrypted_sender_offset_key,
            encrypted_output_commitment_mask,
        })
    }
}
