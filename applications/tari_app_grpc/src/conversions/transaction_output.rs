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

use tari_common_types::types::{BulletRangeProof, Commitment, PublicKey};
use tari_core::{
    covenants::Covenant,
    transactions::transaction_components::{TransactionOutput, TransactionOutputVersion},
};
use tari_crypto::script::TariScript;
use tari_utilities::{ByteArray, Hashable};

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

        let script = TariScript::from_bytes(output.script.as_slice())
            .map_err(|err| format!("Script deserialization: {:?}", err))?;

        let metadata_signature = output
            .metadata_signature
            .ok_or_else(|| "Metadata signature not provided".to_string())?
            .try_into()
            .map_err(|_| "Metadata signature could not be converted".to_string())?;
        let covenant = Covenant::from_bytes(&output.covenant).map_err(|err| err.to_string())?;
        Ok(Self::new(
            TransactionOutputVersion::try_from(
                u8::try_from(output.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            features,
            commitment,
            BulletRangeProof(output.range_proof),
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
        ))
    }
}

impl From<TransactionOutput> for grpc::TransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        let hash = output.hash();
        grpc::TransactionOutput {
            hash,
            features: Some(output.features.into()),
            commitment: Vec::from(output.commitment.as_bytes()),
            range_proof: Vec::from(output.proof.as_bytes()),
            script: output.script.as_bytes(),
            sender_offset_public_key: output.sender_offset_public_key.as_bytes().to_vec(),
            metadata_signature: Some(grpc::ComSignature {
                public_nonce_commitment: Vec::from(output.metadata_signature.public_nonce().as_bytes()),
                signature_u: Vec::from(output.metadata_signature.u().as_bytes()),
                signature_v: Vec::from(output.metadata_signature.v().as_bytes()),
            }),
            covenant: output.covenant.to_bytes(),
            version: output.version as u32,
        }
    }
}
