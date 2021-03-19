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

//! Impls for transaction proto

use crate::{
    proto,
    tari_utilities::convert::try_convert_all,
    transactions::{
        aggregated_body::AggregateBody,
        bullet_rangeproofs::BulletRangeProof,
        tari_amount::MicroTari,
        transaction::{
            KernelFeatures,
            OutputFeatures,
            OutputFlags,
            Transaction,
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
        },
        types::{BlindingFactor, Commitment, PublicKey},
    },
};
use std::convert::{TryFrom, TryInto};
use tari_crypto::{
    script::{ExecutionStack, TariScript},
    tari_utilities::{ByteArray, ByteArrayError},
};

//---------------------------------- TransactionKernel --------------------------------------------//

impl TryFrom<proto::types::TransactionKernel> for TransactionKernel {
    type Error = String;

    fn try_from(kernel: proto::types::TransactionKernel) -> Result<Self, Self::Error> {
        let excess = Commitment::from_bytes(
            &kernel
                .excess
                .ok_or_else(|| "Excess not provided in kernel".to_string())?
                .data,
        )
        .map_err(|err| err.to_string())?;

        let excess_sig = kernel
            .excess_sig
            .ok_or_else(|| "excess_sig not provided".to_string())?
            .try_into()
            .map_err(|err: ByteArrayError| err.to_string())?;

        Ok(Self {
            features: KernelFeatures::from_bits(kernel.features as u8)
                .ok_or_else(|| "Invalid or unrecognised kernel feature flag".to_string())?,
            excess,
            excess_sig,
            fee: MicroTari::from(kernel.fee),
            lock_height: kernel.lock_height,
        })
    }
}

impl From<TransactionKernel> for proto::types::TransactionKernel {
    fn from(kernel: TransactionKernel) -> Self {
        Self {
            features: kernel.features.bits() as u32,
            excess: Some(kernel.excess.into()),
            excess_sig: Some(kernel.excess_sig.into()),
            fee: kernel.fee.into(),
            lock_height: kernel.lock_height,
        }
    }
}

//---------------------------------- TransactionInput --------------------------------------------//

impl TryFrom<proto::types::TransactionInput> for TransactionInput {
    type Error = String;

    fn try_from(input: proto::types::TransactionInput) -> Result<Self, Self::Error> {
        let features = input
            .features
            .map(TryInto::try_into)
            .ok_or_else(|| "transaction output features not provided".to_string())??;

        let commitment = input
            .commitment
            .map(|commit| Commitment::from_bytes(&commit.data))
            .ok_or_else(|| "Transaction output commitment not provided".to_string())?
            .map_err(|err| err.to_string())?;

        let script_signature = input
            .script_signature
            .ok_or_else(|| "script_signature not provided".to_string())?
            .try_into()
            .map_err(|err: ByteArrayError| err.to_string())?;

        let script_offset_public_key =
            PublicKey::from_bytes(input.script_offset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        Ok(Self {
            features,
            commitment,
            script: TariScript::from_bytes(input.script.as_slice()).map_err(|err| format!("{:?}", err))?,
            input_data: ExecutionStack::from_bytes(input.input_data.as_slice()).map_err(|err| format!("{:?}", err))?,
            height: input.height,
            script_signature,
            script_offset_public_key,
        })
    }
}

impl From<TransactionInput> for proto::types::TransactionInput {
    fn from(input: TransactionInput) -> Self {
        Self {
            features: Some(input.features.into()),
            commitment: Some(input.commitment.into()),
            script: input.script.as_bytes(),
            input_data: input.input_data.as_bytes(),
            height: input.height,
            script_signature: Some(input.script_signature.into()),
            script_offset_public_key: input.script_offset_public_key.as_bytes().to_vec(),
        }
    }
}

//---------------------------------- TransactionOutput --------------------------------------------//

impl TryFrom<proto::types::TransactionOutput> for TransactionOutput {
    type Error = String;

    fn try_from(output: proto::types::TransactionOutput) -> Result<Self, Self::Error> {
        let features = output
            .features
            .map(TryInto::try_into)
            .ok_or_else(|| "transaction output features not provided".to_string())??;

        let commitment = output
            .commitment
            .map(|commit| Commitment::from_bytes(&commit.data))
            .ok_or_else(|| "Transaction output commitment not provided".to_string())?
            .map_err(|err| err.to_string())?;

        let script_offset_public_key =
            PublicKey::from_bytes(output.script_offset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        Ok(Self {
            features,
            commitment,
            proof: BulletRangeProof(output.range_proof),
            script_hash: output.script_hash,
            script_offset_public_key,
        })
    }
}

impl From<TransactionOutput> for proto::types::TransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        Self {
            features: Some(output.features.into()),
            commitment: Some(output.commitment.into()),
            range_proof: output.proof.to_vec(),
            script_hash: output.script_hash,
            script_offset_public_key: output.script_offset_public_key.as_bytes().to_vec(),
        }
    }
}

//---------------------------------- OutputFeatures --------------------------------------------//

impl TryFrom<proto::types::OutputFeatures> for OutputFeatures {
    type Error = String;

    fn try_from(features: proto::types::OutputFeatures) -> Result<Self, Self::Error> {
        Ok(Self {
            flags: OutputFlags::from_bits(features.flags as u8)
                .ok_or_else(|| "Invalid or unrecognised output flags".to_string())?,
            maturity: features.maturity,
        })
    }
}

impl From<OutputFeatures> for proto::types::OutputFeatures {
    fn from(features: OutputFeatures) -> Self {
        Self {
            flags: features.flags.bits() as u32,
            maturity: features.maturity,
        }
    }
}

//---------------------------------- AggregateBody --------------------------------------------//

impl TryFrom<proto::types::AggregateBody> for AggregateBody {
    type Error = String;

    fn try_from(body: proto::types::AggregateBody) -> Result<Self, Self::Error> {
        let inputs = try_convert_all(body.inputs)?;
        let outputs = try_convert_all(body.outputs)?;
        let kernels = try_convert_all(body.kernels)?;
        let mut body = AggregateBody::new(inputs, outputs, kernels);
        body.sort();
        Ok(body)
    }
}

impl From<AggregateBody> for proto::types::AggregateBody {
    fn from(body: AggregateBody) -> Self {
        let (i, o, k) = body.dissolve();
        Self {
            inputs: i.into_iter().map(Into::into).collect(),
            outputs: o.into_iter().map(Into::into).collect(),
            kernels: k.into_iter().map(Into::into).collect(),
        }
    }
}

//----------------------------------- Transaction ---------------------------------------------//

impl TryFrom<proto::types::Transaction> for Transaction {
    type Error = String;

    fn try_from(tx: proto::types::Transaction) -> Result<Self, Self::Error> {
        let offset = tx
            .offset
            .map(|offset| BlindingFactor::from_bytes(&offset.data))
            .ok_or_else(|| "Blinding factor offset not provided".to_string())?
            .map_err(|err| err.to_string())?;
        let body = tx
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Body not provided".to_string())??;
        let script_offset = tx
            .script_offset
            .map(|script_offset| BlindingFactor::from_bytes(&script_offset.data))
            .ok_or_else(|| "Script offset not provided".to_string())?
            .map_err(|err| err.to_string())?;

        Ok(Self {
            offset,
            body,
            script_offset,
        })
    }
}

impl From<Transaction> for proto::types::Transaction {
    fn from(tx: Transaction) -> Self {
        Self {
            offset: Some(tx.offset.into()),
            body: Some(tx.body.into()),
            script_offset: Some(tx.script_offset.into()),
        }
    }
}
