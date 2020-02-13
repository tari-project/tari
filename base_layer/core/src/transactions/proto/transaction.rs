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

use super::types as proto;
use crate::transactions::{
    aggregated_body::AggregateBody,
    bullet_rangeproofs::BulletRangeProof,
    proto::utils::try_convert_all,
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
    types::{BlindingFactor, Commitment},
};
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::{ByteArray, ByteArrayError};

//---------------------------------- TransactionKernel --------------------------------------------//

impl TryFrom<proto::TransactionKernel> for TransactionKernel {
    type Error = String;

    fn try_from(kernel: proto::TransactionKernel) -> Result<Self, Self::Error> {
        let excess = Commitment::from_bytes(&kernel.excess.ok_or("Excess not provided in kernel".to_string())?.data)
            .map_err(|err| err.to_string())?;

        let excess_sig = kernel
            .excess_sig
            .ok_or("excess_sig not provided".to_string())?
            .try_into()
            .map_err(|err: ByteArrayError| err.to_string())?;

        Ok(Self {
            features: KernelFeatures::from_bits(kernel.features as u8)
                .ok_or("Invalid or unrecognised kernel feature flag".to_string())?,
            excess,
            excess_sig,
            fee: MicroTari::from(kernel.fee),
            linked_kernel: kernel.linked_kernel.map(Into::into),
            lock_height: kernel.lock_height,
            meta_info: kernel.meta_info.map(Into::into),
        })
    }
}

impl From<TransactionKernel> for proto::TransactionKernel {
    fn from(kernel: TransactionKernel) -> Self {
        Self {
            features: kernel.features.bits() as u32,
            excess: Some(kernel.excess.into()),
            excess_sig: Some(kernel.excess_sig.into()),
            fee: kernel.fee.into(),
            linked_kernel: kernel.linked_kernel.map(Into::into),
            lock_height: kernel.lock_height,
            meta_info: kernel.meta_info.map(Into::into),
        }
    }
}

//---------------------------------- TransactionInput --------------------------------------------//

impl TryFrom<proto::TransactionInput> for TransactionInput {
    type Error = String;

    fn try_from(input: proto::TransactionInput) -> Result<Self, Self::Error> {
        let features = input
            .features
            .map(TryInto::try_into)
            .ok_or("transaction output features not provided".to_string())??;

        let commitment = input
            .commitment
            .map(|commit| Commitment::from_bytes(&commit.data))
            .ok_or("Transaction output commitment not provided".to_string())?
            .map_err(|err| err.to_string())?;

        Ok(Self { features, commitment })
    }
}

impl From<TransactionInput> for proto::TransactionInput {
    fn from(output: TransactionInput) -> Self {
        Self {
            features: Some(output.features.into()),
            commitment: Some(output.commitment.into()),
        }
    }
}

//---------------------------------- TransactionOutput --------------------------------------------//

impl TryFrom<proto::TransactionOutput> for TransactionOutput {
    type Error = String;

    fn try_from(output: proto::TransactionOutput) -> Result<Self, Self::Error> {
        let features = output
            .features
            .map(TryInto::try_into)
            .ok_or("transaction output features not provided".to_string())??;

        let commitment = output
            .commitment
            .map(|commit| Commitment::from_bytes(&commit.data))
            .ok_or("Transaction output commitment not provided".to_string())?
            .map_err(|err| err.to_string())?;

        Ok(Self {
            features,
            commitment,
            proof: BulletRangeProof(output.range_proof),
        })
    }
}

impl From<TransactionOutput> for proto::TransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        Self {
            features: Some(output.features.into()),
            commitment: Some(output.commitment.into()),
            range_proof: output.proof.to_vec(),
        }
    }
}

//---------------------------------- OutputFeatures --------------------------------------------//

impl TryFrom<proto::OutputFeatures> for OutputFeatures {
    type Error = String;

    fn try_from(features: proto::OutputFeatures) -> Result<Self, Self::Error> {
        Ok(Self {
            flags: OutputFlags::from_bits(features.flags as u8)
                .ok_or("Invalid or unrecognised output flags".to_string())?,
            maturity: features.maturity,
        })
    }
}

impl From<OutputFeatures> for proto::OutputFeatures {
    fn from(features: OutputFeatures) -> Self {
        Self {
            flags: features.flags.bits() as u32,
            maturity: features.maturity,
        }
    }
}

//---------------------------------- AggregateBody --------------------------------------------//

impl TryFrom<proto::AggregateBody> for AggregateBody {
    type Error = String;

    fn try_from(body: proto::AggregateBody) -> Result<Self, Self::Error> {
        let inputs = try_convert_all(body.inputs)?;
        let outputs = try_convert_all(body.outputs)?;
        let kernels = try_convert_all(body.kernels)?;
        let mut body = AggregateBody::new(inputs, outputs, kernels);
        body.sort();
        Ok(body)
    }
}

impl From<AggregateBody> for proto::AggregateBody {
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

impl TryFrom<proto::Transaction> for Transaction {
    type Error = String;

    fn try_from(tx: proto::Transaction) -> Result<Self, Self::Error> {
        let offset = tx
            .offset
            .map(|offset| BlindingFactor::from_bytes(&offset.data))
            .ok_or("Blinding factor offset not provided".to_string())?
            .map_err(|err| err.to_string())?;
        let body = tx
            .body
            .map(TryInto::try_into)
            .ok_or("Body not provided".to_string())??;

        Ok(Self { offset, body })
    }
}

impl From<Transaction> for proto::Transaction {
    fn from(tx: Transaction) -> Self {
        Self {
            offset: Some(tx.offset.into()),
            body: Some(tx.body.into()),
        }
    }
}
