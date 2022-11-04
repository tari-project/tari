// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::cmp::max;

use super::{tari_amount::MicroTari, weight::TransactionWeight};
use crate::aggregated_body::AggregateBody;

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
        rounded_metadata_byte_size: usize,
    ) -> MicroTari {
        let weight = self
            .weighting()
            .calculate(num_kernels, num_inputs, num_outputs, rounded_metadata_byte_size);
        MicroTari::from(weight) * fee_per_gram
    }

    pub fn calculate_body(&self, fee_per_gram: MicroTari, body: &AggregateBody) -> MicroTari {
        let weight = self.weighting().calculate_body(body);
        MicroTari::from(weight) * fee_per_gram
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

    use tari_crypto::ristretto::RistrettoComSig;
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
            f0.weighting().params().metadata_bytes_per_gram,
            f1.weighting().params().metadata_bytes_per_gram
        );
    }

    #[test]
    fn test_calculate_body() {
        let hash = vec![0u8; 32].try_into().unwrap();
        let spent_output = SpentOutput::OutputHash(hash);
        let input = TransactionInput::new_current_version(
            spent_output,
            ExecutionStack::new(vec![]),
            RistrettoComSig::default(),
        );
        let aggregate_body = AggregateBody::new(vec![input], vec![], vec![]);
        let fee = Fee::new(TransactionWeight::latest());
        assert_eq!(
            fee.calculate_body(100.into(), &aggregate_body),
            fee.calculate(100.into(), 0, 1, 0, 0)
        )
    }
}
