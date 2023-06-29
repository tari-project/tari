// Copyright 2019. The Tari Project
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

use std::cmp::max;

use super::{tari_amount::MicroTari, weight::TransactionWeight};
use crate::transactions::aggregated_body::AggregateBody;

#[derive(Debug, Clone, Copy)]
pub struct Fee(TransactionWeight);

impl Fee {
    pub(crate) const MINIMUM_TRANSACTION_FEE: MicroTari = MicroTari(101);

    pub fn new(weight: TransactionWeight) -> Self {
        Self(weight)
    }

    /// Computes the absolute transaction fee given the fee-per-gram, and the size of the transaction
    /// NB: Each fee calculation should be done per transaction. No commutative, associative or distributive properties
    /// are guaranteed to hold between calculations. for e.g. fee(1,1,1,4) + fee(1,1,1,12) != fee(1,1,1,16)
    pub fn calculate(
        &self,
        fee_per_gram: MicroTari,
        num_kernels: usize,
        num_inputs: usize,
        num_outputs: usize,
        rounded_features_and_scripts_byte_size: usize,
    ) -> MicroTari {
        let weight = self.weighting().calculate(
            num_kernels,
            num_inputs,
            num_outputs,
            rounded_features_and_scripts_byte_size,
        );
        MicroTari::from(weight) * fee_per_gram
    }

    pub fn calculate_body(&self, fee_per_gram: MicroTari, body: &AggregateBody) -> std::io::Result<MicroTari> {
        let weight = self.weighting().calculate_body(body)?;
        Ok(MicroTari::from(weight) * fee_per_gram)
    }

    /// Normalizes the given fee returning a fee that is equal to or above the minimum fee
    pub fn normalize(fee: MicroTari) -> MicroTari {
        max(Self::MINIMUM_TRANSACTION_FEE, fee)
    }

    pub fn weighting(&self) -> &TransactionWeight {
        &self.0
    }
}

impl From<TransactionWeight> for Fee {
    fn from(weight: TransactionWeight) -> Self {
        Self(weight)
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;

    use tari_crypto::ristretto::RistrettoComAndPubSig;
    use tari_script::ExecutionStack;

    use super::*;
    use crate::transactions::transaction_components::{SpentOutput, TransactionInput};

    #[test]
    pub fn test_derive_clone() {
        let f0 = Fee::new(TransactionWeight::latest());
        let f1 = f0;
        assert_eq!(
            f0.weighting().params().kernel_weight,
            f1.weighting().params().kernel_weight
        );
        assert_eq!(
            f0.weighting().params().input_weight,
            f1.weighting().params().input_weight
        );
        assert_eq!(
            f0.weighting().params().output_weight,
            f1.weighting().params().output_weight
        );
        assert_eq!(
            f0.weighting().params().features_and_scripts_bytes_per_gram,
            f1.weighting().params().features_and_scripts_bytes_per_gram
        );
    }

    #[test]
    fn test_calculate_body() {
        let hash = vec![0u8; 32].try_into().unwrap();
        let spent_output = SpentOutput::OutputHash(hash);
        let input = TransactionInput::new_current_version(
            spent_output,
            ExecutionStack::new(vec![]),
            RistrettoComAndPubSig::default(),
        );
        let aggregate_body = AggregateBody::new(vec![input], vec![], vec![]);
        let fee = Fee::new(TransactionWeight::latest());
        assert_eq!(
            fee.calculate_body(100.into(), &aggregate_body)
                .unwrap_or_else(|e| panic!("Failed with error: {}", e)),
            fee.calculate(100.into(), 0, 1, 0, 0)
        )
    }
}
