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

use tari_common_types::types::{PrivateKey, PublicKey};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{TransactionOutputVersion, UnblindedOutput},
    },
};
use tari_crypto::script::{ExecutionStack, TariScript};
use tari_utilities::ByteArray;

use crate::tari_rpc as grpc;

impl From<UnblindedOutput> for grpc::UnblindedOutput {
    fn from(output: UnblindedOutput) -> Self {
        grpc::UnblindedOutput {
            value: u64::from(output.value),
            spending_key: output.spending_key.as_bytes().to_vec(),
            features: Some(output.features.into()),
            script: output.script.as_bytes(),
            input_data: output.input_data.as_bytes(),
            script_private_key: output.script_private_key.as_bytes().to_vec(),
            sender_offset_public_key: output.sender_offset_public_key.as_bytes().to_vec(),
            metadata_signature: Some(grpc::ComSignature {
                public_nonce_commitment: Vec::from(output.metadata_signature.public_nonce().as_bytes()),
                signature_u: Vec::from(output.metadata_signature.u().as_bytes()),
                signature_v: Vec::from(output.metadata_signature.v().as_bytes()),
            }),
            script_lock_height: output.script_lock_height,
            covenant: output.covenant.to_bytes(),
        }
    }
}

impl TryFrom<grpc::UnblindedOutput> for UnblindedOutput {
    type Error = String;

    fn try_from(output: grpc::UnblindedOutput) -> Result<Self, Self::Error> {
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

        let covenant = Covenant::from_bytes(&output.covenant).map_err(|err| err.to_string())?;

        Ok(Self::new(
            TransactionOutputVersion::try_from(0u8)?,
            MicroTari::from(output.value),
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            output.script_lock_height,
            covenant,
        ))
    }
}
