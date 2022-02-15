// Copyright 2020. The Tari Project
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

use std::convert::{TryFrom, TryInto};

use tari_common_types::types::{Commitment, PublicKey};
use tari_core::{
    covenants::Covenant,
    transactions::transaction_components::{TransactionInput, TransactionInputVersion},
};
use tari_crypto::{
    script::{ExecutionStack, TariScript},
    tari_utilities::ByteArray,
};

use crate::tari_rpc as grpc;

impl TryFrom<grpc::TransactionInput> for TransactionInput {
    type Error = String;

    fn try_from(input: grpc::TransactionInput) -> Result<Self, Self::Error> {
        let script_signature = input
            .script_signature
            .ok_or_else(|| "script_signature not provided".to_string())?
            .try_into()
            .map_err(|_| "script_signature could not be converted".to_string())?;

        // Check if the received Transaction input is in compact form or not
        if !input.commitment.is_empty() {
            let commitment = Commitment::from_bytes(&input.commitment).map_err(|e| e.to_string())?;
            let features = input
                .features
                .map(TryInto::try_into)
                .ok_or_else(|| "transaction output features not provided".to_string())??;

            let sender_offset_public_key =
                PublicKey::from_bytes(input.sender_offset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;
            let covenant = Covenant::from_bytes(&input.covenant).map_err(|err| err.to_string())?;

            Ok(TransactionInput::new_with_output_data(
                TransactionInputVersion::try_from(
                    u8::try_from(input.version).map_err(|_| "Invalid version: overflowed u8")?,
                )?,
                features,
                commitment,
                TariScript::from_bytes(input.script.as_slice()).map_err(|err| format!("{:?}", err))?,
                ExecutionStack::from_bytes(input.input_data.as_slice()).map_err(|err| format!("{:?}", err))?,
                script_signature,
                sender_offset_public_key,
                covenant,
            ))
        } else {
            if input.output_hash.is_empty() {
                return Err("Compact Transaction Input does not contain `output_hash`".to_string());
            }
            Ok(TransactionInput::new_with_output_hash(
                input.output_hash,
                ExecutionStack::from_bytes(input.input_data.as_slice()).map_err(|err| format!("{:?}", err))?,
                script_signature,
            ))
        }
    }
}

impl TryFrom<TransactionInput> for grpc::TransactionInput {
    type Error = String;

    fn try_from(input: TransactionInput) -> Result<Self, Self::Error> {
        let script_signature = Some(grpc::ComSignature {
            public_nonce_commitment: Vec::from(input.script_signature.public_nonce().as_bytes()),
            signature_u: Vec::from(input.script_signature.u().as_bytes()),
            signature_v: Vec::from(input.script_signature.v().as_bytes()),
        });
        if input.is_compact() {
            let output_hash = input.output_hash();
            Ok(Self {
                script_signature,
                output_hash,
                ..Default::default()
            })
        } else {
            let features = input
                .features()
                .map_err(|_| "Non-compact Transaction input should contain features".to_string())?;

            Ok(Self {
                features: Some(features.clone().into()),
                commitment: input
                    .commitment()
                    .map_err(|_| "Non-compact Transaction input should contain commitment".to_string())?
                    .as_bytes()
                    .to_vec(),
                hash: input
                    .canonical_hash()
                    .map_err(|_| "Non-compact Transaction input should be able to be hashed".to_string())?,

                script: input
                    .script()
                    .map_err(|_| "Non-compact Transaction input should contain script".to_string())?
                    .as_bytes(),
                input_data: input.input_data.as_bytes(),
                script_signature,
                sender_offset_public_key: input
                    .sender_offset_public_key()
                    .map_err(|_| "Non-compact Transaction input should contain sender_offset_public_key".to_string())?
                    .as_bytes()
                    .to_vec(),
                output_hash: Vec::new(),
                covenant: input
                    .covenant()
                    .map_err(|_| "Non-compact Transaction input should contain covenant".to_string())?
                    .to_bytes(),
                version: input.version as u32,
            })
        }
    }
}
