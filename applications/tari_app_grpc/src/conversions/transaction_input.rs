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

use crate::tari_rpc as grpc;
use std::convert::{TryFrom, TryInto};
use tari_core::transactions::{
    transaction::TransactionInput,
    types::{Commitment, PublicKey},
};
use tari_crypto::{
    script::{ExecutionStack, TariScript},
    tari_utilities::{ByteArray, Hashable},
};

impl TryFrom<grpc::TransactionInput> for TransactionInput {
    type Error = String;

    fn try_from(input: grpc::TransactionInput) -> Result<Self, Self::Error> {
        let features = input
            .features
            .map(TryInto::try_into)
            .ok_or_else(|| "transaction output features not provided".to_string())??;

        let commitment = Commitment::from_bytes(&input.commitment)
            .map_err(|err| format!("Could not convert input commitment:{}", err))?;

        let script_signature = input
            .script_signature
            .ok_or_else(|| "script_signature not provided".to_string())?
            .try_into()
            .map_err(|_| "script_signature could not be converted".to_string())?;

        let script_offset_public_key =
            PublicKey::from_bytes(input.script_offset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;
        let script = TariScript::from_bytes(input.script.as_slice()).map_err(|err| format!("{:?}", err))?;
        let input_data = ExecutionStack::from_bytes(input.input_data.as_slice()).map_err(|err| format!("{:?}", err))?;

        Ok(Self {
            features,
            commitment,
            script,
            input_data,
            height: input.height,
            script_signature,
            script_offset_public_key,
        })
    }
}

impl From<TransactionInput> for grpc::TransactionInput {
    fn from(input: TransactionInput) -> Self {
        let hash = input.hash();
        Self {
            features: Some(grpc::OutputFeatures {
                flags: input.features.flags.bits() as u32,
                maturity: input.features.maturity,
            }),
            commitment: Vec::from(input.commitment.as_bytes()),
            hash,
            script: input.script.as_bytes(),
            input_data: input.input_data.as_bytes(),
            height: input.height,
            script_signature: Some(grpc::Signature {
                public_nonce: Vec::from(input.script_signature.get_public_nonce().as_bytes()),
                signature: Vec::from(input.script_signature.get_signature().as_bytes()),
            }),
            script_offset_public_key: input.script_offset_public_key.as_bytes().to_vec(),
        }
    }
}
