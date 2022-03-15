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

use tari_common_types::types::Commitment;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction_components::{KernelFeatures, TransactionKernel, TransactionKernelVersion},
};
use tari_crypto::tari_utilities::{ByteArray, Hashable};

use crate::tari_rpc as grpc;

impl TryFrom<grpc::TransactionKernel> for TransactionKernel {
    type Error = String;

    fn try_from(kernel: grpc::TransactionKernel) -> Result<Self, Self::Error> {
        let excess =
            Commitment::from_bytes(&kernel.excess).map_err(|err| format!("Excess could not be converted:{}", err))?;

        let excess_sig = kernel
            .excess_sig
            .ok_or_else(|| "excess_sig not provided".to_string())?
            .try_into()
            .map_err(|_| "excess_sig could not be converted".to_string())?;

        Ok(Self::new(
            TransactionKernelVersion::try_from(
                u8::try_from(kernel.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            KernelFeatures::from_bits(kernel.features as u8)
                .ok_or_else(|| "Invalid or unrecognised kernel feature flag".to_string())?,
            MicroTari::from(kernel.fee),
            kernel.lock_height,
            excess,
            excess_sig,
        ))
    }
}

impl From<TransactionKernel> for grpc::TransactionKernel {
    fn from(kernel: TransactionKernel) -> Self {
        let hash = kernel.hash();

        grpc::TransactionKernel {
            features: kernel.features.bits() as u32,
            fee: kernel.fee.0,
            lock_height: kernel.lock_height,
            excess: Vec::from(kernel.excess.as_bytes()),
            excess_sig: Some(grpc::Signature {
                public_nonce: Vec::from(kernel.excess_sig.get_public_nonce().as_bytes()),
                signature: Vec::from(kernel.excess_sig.get_signature().as_bytes()),
            }),
            hash,
            version: kernel.version as u32,
        }
    }
}
