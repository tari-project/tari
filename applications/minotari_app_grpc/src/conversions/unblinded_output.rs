// Copyright 2021. The Tari Project
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
use tari_common_types::types::{PrivateKey, PublicKey};
use tari_core::transactions::{
    tari_amount::MicroMinotari,
    transaction_components::{EncryptedData, TransactionOutputVersion, UnblindedOutput},
};
use tari_script::{ExecutionStack, TariScript};
use tari_utilities::ByteArray;
use zeroize::Zeroize;

use crate::tari_rpc as grpc;

impl TryFrom<UnblindedOutput> for grpc::UnblindedOutput {
    type Error = String;

    fn try_from(output: UnblindedOutput) -> Result<Self, Self::Error> {
        let mut covenant = Vec::new();
        BorshSerialize::serialize(&output.covenant, &mut covenant).map_err(|err| err.to_string())?;
        Ok(grpc::UnblindedOutput {
            value: u64::from(output.value),
            spending_key: output.spending_key.as_bytes().to_vec(),
            features: Some(output.features.into()),
            script: output.script.to_bytes(),
            input_data: output.input_data.to_bytes(),
            script_private_key: output.script_private_key.as_bytes().to_vec(),
            sender_offset_public_key: output.sender_offset_public_key.as_bytes().to_vec(),
            metadata_signature: Some(grpc::ComAndPubSignature {
                ephemeral_commitment: Vec::from(output.metadata_signature.ephemeral_commitment().as_bytes()),
                ephemeral_pubkey: Vec::from(output.metadata_signature.ephemeral_pubkey().as_bytes()),
                u_a: Vec::from(output.metadata_signature.u_a().as_bytes()),
                u_x: Vec::from(output.metadata_signature.u_x().as_bytes()),
                u_y: Vec::from(output.metadata_signature.u_y().as_bytes()),
            }),
            script_lock_height: output.script_lock_height,
            covenant,
            encrypted_data: output.encrypted_data.to_byte_vec(),
            minimum_value_promise: output.minimum_value_promise.into(),
        })
    }
}

impl TryFrom<grpc::UnblindedOutput> for UnblindedOutput {
    type Error = String;

    fn try_from(mut output: grpc::UnblindedOutput) -> Result<Self, Self::Error> {
        let spending_key =
            PrivateKey::from_bytes(output.spending_key.as_bytes()).map_err(|e| format!("spending_key: {:?}", e))?;

        let features = output
            .features
            .map(TryInto::try_into)
            .ok_or_else(|| "output features not provided".to_string())??;

        let script = TariScript::from_bytes(output.script.as_bytes()).map_err(|e| format!("script: {:?}", e))?;

        let input_data =
            ExecutionStack::from_bytes(output.input_data.as_bytes()).map_err(|e| format!("input_data: {:?}", e))?;

        let script_private_key = PrivateKey::from_bytes(output.script_private_key.as_bytes())
            .map_err(|e| format!("script_private_key: {:?}", e))?;

        let sender_offset_public_key = PublicKey::from_bytes(output.sender_offset_public_key.as_bytes())
            .map_err(|err| format!("sender_offset_public_key {:?}", err))?;

        let metadata_signature = output
            .metadata_signature
            .ok_or_else(|| "Metadata signature not provided".to_string())?
            .try_into()
            .map_err(|_| "Metadata signature could not be converted".to_string())?;

        let mut buffer = output.covenant.as_bytes();
        let covenant = BorshDeserialize::deserialize(&mut buffer).map_err(|err| err.to_string())?;

        let encrypted_data = EncryptedData::from_bytes(&output.encrypted_data).map_err(|err| err.to_string())?;

        let minimum_value_promise = MicroMinotari::from(output.minimum_value_promise);

        // zeroize output sensitive data
        output.spending_key.zeroize();
        output.script_private_key.zeroize();

        Ok(Self::new(
            TransactionOutputVersion::try_from(0u8)?,
            MicroMinotari::from(output.value),
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            output.script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
        ))
    }
}
