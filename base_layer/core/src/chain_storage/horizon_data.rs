use crate::transactions::types::Commitment;

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
use serde::{Deserialize, Serialize};
use tari_crypto::tari_utilities::ByteArray;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HorizonData {
    kernel_sum: Commitment,
    utxo_sum: Commitment,
}

impl HorizonData {
    pub fn new(kernel_sum: Commitment, utxo_sum: Commitment) -> Self {
        HorizonData { kernel_sum, utxo_sum }
    }

    pub fn zero() -> Self {
        HorizonData {
            kernel_sum: Commitment::from_bytes(&[0u8; 32]).expect("Could not create commitment"),
            utxo_sum: Commitment::from_bytes(&[0u8; 32]).expect("Could not create commitment"),
        }
    }

    pub fn kernel_sum(&self) -> &Commitment {
        &self.kernel_sum
    }

    pub fn utxo_sum(&self) -> &Commitment {
        &self.utxo_sum
    }
}
