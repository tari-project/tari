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

use tari_common_types::tari_address::TariAddress;
use tari_script::TariScript;

use super::{MicroMinotari, weight::TransactionWeight};
use crate::{
    aggregated_body::AggregateBody,
    helpers::borsh::SerializedSize,
    transaction_builder::TransactionBuilderError,
    transaction_components::{
        OutputFeatures,
        covenants::Covenant,
        memo_field::{MemoField, TxType},
    },
};

/// Builds the memo that a single recipient output carries in its encrypted data on the paths that attach an
/// address to the caller's payment id.
///
/// Those paths have to measure the memo before they know the fee, but the memo takes the fee as an argument. The
/// fee is packed into a fixed-width field, so its value cannot change the memo's size: measure a copy built with a
/// zero fee, then build the real memo with the final fee through this same constructor, so that the memo that was
/// measured and the memo that ends up in the output cannot drift apart.
pub fn addressed_output_memo(
    payment_id: MemoField,
    address: TariAddress,
    fee: MicroMinotari,
    tx_type: TxType,
) -> Result<MemoField, TransactionBuilderError> {
    payment_id
        .add_sender_address(address, true, fee, Some(tx_type))
        .map_err(TransactionBuilderError::InvalidMemo)
}

/// The rounded-up features-and-scripts byte size that the transaction builder will charge for a single output
/// carrying `memo`.
///
/// `TransactionOutput::get_features_and_scripts_size` counts the memo bytes held in the output's encrypted data
/// alongside the features, script and covenant, so a fee estimate that leaves the memo out is short. A transaction
/// that spends a whole input with no change output has nothing to absorb the difference and fails to build at all,
/// so every fee estimate should come from here rather than summing the terms by hand at the call site.
pub fn recipient_output_features_and_scripts_size(
    weighting: &TransactionWeight,
    features: &OutputFeatures,
    script: &TariScript,
    covenant: &Covenant,
    memo: &MemoField,
) -> Result<usize, TransactionBuilderError> {
    let to_err = |e: std::io::Error| TransactionBuilderError::InvalidSerializedSize(e.to_string());
    Ok(weighting.round_up_features_and_scripts_size(
        features
            .get_serialized_size()
            .map_err(to_err)?
            .saturating_add(script.get_serialized_size().map_err(to_err)?)
            .saturating_add(covenant.get_serialized_size().map_err(to_err)?)
            .saturating_add(memo.get_size()),
    ))
}

#[derive(Debug, Clone, Copy)]
pub struct Fee(TransactionWeight);

impl Fee {
    pub fn new(weight: TransactionWeight) -> Self {
        Self(weight)
    }

    /// Computes the absolute transaction fee given the fee-per-gram, and the size of the transaction
    /// NB: Each fee calculation should be done per transaction. No commutative, associative or distributive properties
    /// are guaranteed to hold between calculations. for e.g. fee(1,1,1,4) + fee(1,1,1,12) != fee(1,1,1,16)
    pub fn calculate(
        &self,
        fee_per_gram: MicroMinotari,
        num_kernels: usize,
        num_inputs: usize,
        num_outputs: usize,
        rounded_features_and_scripts_byte_size: usize,
    ) -> MicroMinotari {
        let weight = self.weighting().calculate(
            num_kernels,
            num_inputs,
            num_outputs,
            rounded_features_and_scripts_byte_size,
        );
        // Saturating multiplication is used here to prevent overflow only; invalid values will be caught with
        // validation
        MicroMinotari::from(weight.saturating_mul(fee_per_gram.0))
    }

    pub fn calculate_body(&self, fee_per_gram: MicroMinotari, body: &AggregateBody) -> std::io::Result<MicroMinotari> {
        let weight = self.weighting().calculate_body(body)?;
        Ok(MicroMinotari::from(weight.saturating_mul(fee_per_gram.as_u64())))
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

    use tari_common_types::types::ComAndPubSignature;
    use tari_crypto::ristretto::RistrettoComAndPubSig;
    use tari_script::ExecutionStack;

    use super::*;
    use crate::transaction_components::{SpentOutput, TransactionInput};

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
            ComAndPubSignature::new_from_capk_signature(RistrettoComAndPubSig::default()),
        );
        let aggregate_body = AggregateBody::new_unsorted(vec![input], vec![], vec![]);
        let fee = Fee::new(TransactionWeight::latest());
        assert_eq!(
            fee.calculate_body(100.into(), &aggregate_body)
                .unwrap_or_else(|e| panic!("Failed with error: {e}")),
            fee.calculate(100.into(), 0, 1, 0, 0)
        )
    }
}
