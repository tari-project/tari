// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use tari_common_types::types::{Commitment, Signature};

use crate::transactions::{
    tari_amount::MicroTari,
    transaction_components::{KernelFeatures, TransactionError, TransactionKernel},
};

/// A version of Transaction kernel with optional fields. This struct is only used in constructing transaction kernels
pub struct KernelBuilder {
    features: KernelFeatures,
    fee: MicroTari,
    lock_height: u64,
    excess: Option<Commitment>,
    excess_sig: Option<Signature>,
}

/// Implementation of the transaction kernel
impl KernelBuilder {
    /// Creates an empty transaction kernel
    pub fn new() -> KernelBuilder {
        KernelBuilder::default()
    }

    /// Build a transaction kernel with the provided features
    pub fn with_features(mut self, features: KernelFeatures) -> KernelBuilder {
        self.features = features;
        self
    }

    /// Build a transaction kernel with the provided fee
    pub fn with_fee(mut self, fee: MicroTari) -> KernelBuilder {
        self.fee = fee;
        self
    }

    /// Build a transaction kernel with the provided lock height
    pub fn with_lock_height(mut self, lock_height: u64) -> KernelBuilder {
        self.lock_height = lock_height;
        self
    }

    /// Add the excess (sum of public spend keys minus the offset)
    pub fn with_excess(mut self, excess: &Commitment) -> KernelBuilder {
        self.excess = Some(excess.clone());
        self
    }

    /// Add the excess signature
    pub fn with_signature(mut self, signature: &Signature) -> KernelBuilder {
        self.excess_sig = Some(signature.clone());
        self
    }

    pub fn build(self) -> Result<TransactionKernel, TransactionError> {
        if self.excess.is_none() || self.excess_sig.is_none() {
            return Err(TransactionError::NoSignatureError);
        }
        Ok(TransactionKernel::new_current_version(
            self.features,
            self.fee,
            self.lock_height,
            self.excess.unwrap(),
            self.excess_sig.unwrap(),
        ))
    }
}

impl Default for KernelBuilder {
    fn default() -> Self {
        KernelBuilder {
            features: KernelFeatures::empty(),
            fee: MicroTari::from(0),
            lock_height: 0,
            excess: None,
            excess_sig: None,
        }
    }
}
