// Copyright 2021. The Taiji Project
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
use taiji_common_types::types::Commitment;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct HorizonData {
    kernel_sum: Commitment,
    utxo_sum: Commitment,
}

impl HorizonData {
    pub fn new(kernel_sum: Commitment, utxo_sum: Commitment) -> Self {
        HorizonData { kernel_sum, utxo_sum }
    }

    pub fn zero() -> Self {
        Default::default()
    }

    pub fn kernel_sum(&self) -> &Commitment {
        &self.kernel_sum
    }

    pub fn utxo_sum(&self) -> &Commitment {
        &self.utxo_sum
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn coverage_horizon_data() {
        let obj = HorizonData::zero();
        obj.kernel_sum();
        obj.utxo_sum();
        drop(obj.clone());
        format!("{:?}", obj);
    }
}
