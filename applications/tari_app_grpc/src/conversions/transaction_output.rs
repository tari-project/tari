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

use borsh::{BorshDeserialize, BorshSerialize};
use tari_common_types::types::{BulletRangeProof, Commitment, PublicKey};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction_components::{EncryptedData, TransactionOutput, TransactionOutputVersion},
};
use tari_script::TariScript;
use tari_utilities::ByteArray;

use crate::tari_rpc as grpc;

impl TryFrom<grpc::TransactionOutput> for TransactionOutput {
    type Error = String;

    fn try_from(output: grpc::TransactionOutput) -> Result<Self, Self::Error> {
        let features = output
            .features
            .map(TryInto::try_into)
            .ok_or_else(|| "Transaction output features not provided".to_string())??;

        let commitment =
            Commitment::from_bytes(&output.commitment).map_err(|err| format!("Invalid output commitment: {}", err))?;
        let sender_offset_public_key = PublicKey::from_bytes(output.sender_offset_public_key.as_bytes())
            .map_err(|err| format!("Invalid sender_offset_public_key {:?}", err))?;

        let range_proof = if let Some(proof) = output.range_proof {
            Some(BulletRangeProof::from_bytes(&proof.proof_bytes).map_err(|err| err.to_string())?)
        } else {
            None
        };

        let script = TariScript::from_bytes(output.script.as_slice())
            .map_err(|err| format!("Script deserialization: {:?}", err))?;

        let metadata_signature = output
            .metadata_signature
            .ok_or_else(|| "Metadata signature not provided".to_string())?
            .try_into()
            .map_err(|_| "Metadata signature could not be converted".to_string())?;
        let mut covenant = output.covenant.as_bytes();
        let covenant = BorshDeserialize::deserialize(&mut covenant).map_err(|err| err.to_string())?;
        let encrypted_data = EncryptedData::from_bytes(&output.encrypted_data).map_err(|err| err.to_string())?;
        let minimum_value_promise = MicroTari::from(output.minimum_value_promise);
        Ok(Self::new(
            TransactionOutputVersion::try_from(
                u8::try_from(output.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            features,
            commitment,
            range_proof,
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
            encrypted_data,
            minimum_value_promise,
        ))
    }
}

impl TryFrom<TransactionOutput> for grpc::TransactionOutput {
    type Error = String;

    fn try_from(output: TransactionOutput) -> Result<Self, Self::Error> {
        let hash = output.hash().to_vec();
        let mut covenant = Vec::new();
        BorshSerialize::serialize(&output.covenant, &mut covenant).map_err(|err| err.to_string())?;
        let range_proof = output.proof.map(|proof| grpc::RangeProof {
            proof_bytes: proof.to_vec(),
        });
        Ok(grpc::TransactionOutput {
            hash,
            features: Some(output.features.into()),
            commitment: Vec::from(output.commitment.as_bytes()),
            range_proof,
            script: output.script.to_bytes(),
            sender_offset_public_key: output.sender_offset_public_key.as_bytes().to_vec(),
            metadata_signature: Some(grpc::ComAndPubSignature {
                ephemeral_commitment: Vec::from(output.metadata_signature.ephemeral_commitment().as_bytes()),
                ephemeral_pubkey: Vec::from(output.metadata_signature.ephemeral_pubkey().as_bytes()),
                u_a: Vec::from(output.metadata_signature.u_a().as_bytes()),
                u_x: Vec::from(output.metadata_signature.u_x().as_bytes()),
                u_y: Vec::from(output.metadata_signature.u_y().as_bytes()),
            }),
            covenant,
            version: output.version as u32,
            encrypted_data: output.encrypted_data.to_byte_vec(),
            minimum_value_promise: output.minimum_value_promise.into(),
        })
    }
}
