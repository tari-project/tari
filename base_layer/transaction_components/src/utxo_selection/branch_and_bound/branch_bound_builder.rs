// Copyright 2024. The Tari Project
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

use crate::{
    transaction_components::MAX_TRANSACTION_INPUTS,
    utxo_selection::{
        branch_and_bound::branch_and_bound_selector::{
            BranchAndBoundUtxoSelector,
            BranchAndBoundUtxoSelectorParams,
            UtxoSectionParams,
        },
        UtxoValue,
    },
    MicroMinotari,
};

const MAX_SEARCH_ITERATIONS: usize = 100_000;

pub struct BranchAndBoundUtxoSelectionBuilder<T> {
    available_utxos: Vec<T>,
    max_search_iterations: usize,
    target_amount: Option<MicroMinotari>,
    output_fee: Option<MicroMinotari>,
    change_fee: Option<MicroMinotari>,
    fee_per_input: Option<MicroMinotari>,
    recipient_output_fee: Option<MicroMinotari>,
    input_limit: usize,
    allow_dust_waste: bool,
}

impl<T> BranchAndBoundUtxoSelectionBuilder<T>
where T: UtxoValue
{
    pub fn new(available_utxos: Vec<T>) -> BranchAndBoundUtxoSelectionBuilder<T> {
        BranchAndBoundUtxoSelectionBuilder {
            available_utxos,
            max_search_iterations: MAX_SEARCH_ITERATIONS,
            target_amount: None,
            output_fee: None,
            change_fee: None,
            fee_per_input: None,
            recipient_output_fee: None,
            input_limit: MAX_TRANSACTION_INPUTS,
            allow_dust_waste: true,
        }
    }

    pub fn allow_dust_waste(mut self, allow: bool) -> Self {
        self.allow_dust_waste = allow;
        self
    }

    pub fn with_max_search_iterations(mut self, max_search_iterations: usize) -> Self {
        self.max_search_iterations = max_search_iterations;
        self
    }

    pub fn with_input_limit(mut self, input_limit: usize) -> Self {
        self.input_limit = input_limit;
        self
    }

    pub fn with_target_amount(mut self, target_amount: MicroMinotari) -> Self {
        self.target_amount = Some(target_amount);
        self
    }

    pub fn with_fee_per_output(mut self, output_fee: MicroMinotari) -> Self {
        self.output_fee = Some(output_fee);
        self
    }

    pub fn with_change_fee(mut self, change_fee: MicroMinotari) -> Self {
        self.change_fee = Some(change_fee);
        self
    }

    pub fn with_fee_per_input(mut self, fee_per_input: MicroMinotari) -> Self {
        self.fee_per_input = Some(fee_per_input);
        self
    }

    pub fn with_recipient_output_fee(mut self, recipient_output_fee: MicroMinotari) -> Self {
        self.recipient_output_fee = Some(recipient_output_fee);
        self
    }

    pub fn build(self) -> Result<BranchAndBoundUtxoSelector<T>, String> {
        let target_amount = self.target_amount.ok_or("target_amount is required")?;
        let output_fee = self.output_fee.ok_or("output_fee is required")?;
        let change_fee = self.change_fee.ok_or("change_fee is required")?;
        let fee_per_input = self.fee_per_input.ok_or("fee_per_input is required")?;
        let recipient_output_fee = self
            .recipient_output_fee.ok_or("recipient_output_fee is required")?;

        let selection_params =
            UtxoSectionParams::new(target_amount, output_fee, change_fee, fee_per_input, recipient_output_fee, self.input_limit);

        let params = BranchAndBoundUtxoSelectorParams {
            max_search_iterations: self.max_search_iterations,
            allow_dust_waste: self.allow_dust_waste,
        };

        Ok(BranchAndBoundUtxoSelector::new(
            self.available_utxos,
            selection_params,
            params,
        ))
    }
}
