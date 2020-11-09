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
use tari_core::transactions::{bullet_rangeproofs::BulletRangeProof, types::Commitment, TransactionOutput};
use tari_crypto::tari_utilities::ByteArray;

impl TryFrom<grpc::TransactionOutput> for TransactionOutput {
    type Error = String;

    fn try_from(output: grpc::TransactionOutput) -> Result<Self, Self::Error> {
        let features = output
            .features
            .map(TryInto::try_into)
            .ok_or_else(|| "transaction output features not provided".to_string())??;

        let commitment = Commitment::from_bytes(&output.commitment).map_err(|err| err.to_string())?;
        Ok(TransactionOutput::new(
            features,
            commitment,
            BulletRangeProof(output.range_proof),
            &output.script_hash,
        ))
    }
}

impl From<TransactionOutput> for grpc::TransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        grpc::TransactionOutput {
            features: Some(grpc::OutputFeatures {
                flags: output.features().flags.bits() as u32,
                maturity: output.features().maturity,
            }),
            commitment: Vec::from(output.commitment().as_bytes()),
            range_proof: Vec::from(output.proof().as_bytes()),
            script_hash: output.script_hash().to_vec(),
        }
    }
}
