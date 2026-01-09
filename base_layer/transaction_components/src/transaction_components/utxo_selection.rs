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

use crate::MicroMinotari;

pub struct BranchAndBoundUtxoSelector<T> {
    available_utxos: Vec<T>,
}

impl<T> BranchAndBoundUtxoSelector<T>
where T: Utxo + Clone
{
    pub fn new(mut available_utxos: Vec<T>) -> Self {
        // Sort UTXOs in descending order for BnB
        available_utxos.sort_by(|a, b| b.value().cmp(&a.value()));
        Self { available_utxos }
    }

    /// Selects UTXOs using the Branch and Bound algorithm.
    /// target_amount: The amount we want to spend.
    /// cost_of_change: The additional fee incurred if we add a change output.
    /// fee_per_utxo: The fee cost for each input UTXO.
    pub fn select(
        &self,
        target_amount: MicroMinotari,
        cost_of_change: MicroMinotari,
        fee_per_utxo: MicroMinotari,
    ) -> Option<Vec<T>> {
        let mut selected_utxos = Vec::new();
        let mut best_selection = None;
        let mut best_waste = MicroMinotari::from(u64::MAX);

        // We want to find a selection where total_value is in [target_amount + total_fees, target_amount + total_fees +
        // cost_of_change]
        self.search(
            0,
            MicroMinotari::from(0),
            MicroMinotari::from(0),
            target_amount,
            cost_of_change,
            fee_per_utxo,
            &mut selected_utxos,
            &mut best_selection,
            &mut best_waste,
            &mut 0,
        );

        best_selection
    }

    #[allow(clippy::too_many_arguments)]
    fn search(
        &self,
        index: usize,
        current_value: MicroMinotari,
        current_fees: MicroMinotari,
        target_amount: MicroMinotari,
        cost_of_change: MicroMinotari,
        fee_per_utxo: MicroMinotari,
        selected_utxos: &mut Vec<usize>,
        best_selection: &mut Option<Vec<T>>,
        best_waste: &mut MicroMinotari,
        iterations: &mut usize,
    ) {
        *iterations += 1;
        if *iterations > 100_000 {
            return;
        }

        let total_target = target_amount + current_fees;

        // If current value is within the range [total_target, total_target + cost_of_change]
        if current_value >= total_target {
            let waste = current_value - total_target;
            if waste <= cost_of_change && waste < *best_waste {
                *best_waste = waste;
                *best_selection = Some(selected_utxos.iter().map(|&i| self.available_utxos[i].clone()).collect());
            }
            return;
        }

        if index >= self.available_utxos.len() {
            return;
        }

        // Branch with current UTXO included
        let utxo_value = self.available_utxos[index].value();
        selected_utxos.push(index);
        self.search(
            index + 1,
            current_value + utxo_value,
            current_fees + fee_per_utxo,
            target_amount,
            cost_of_change,
            fee_per_utxo,
            selected_utxos,
            best_selection,
            best_waste,
            iterations,
        );
        selected_utxos.pop();

        // Branch with current UTXO excluded
        self.search(
            index + 1,
            current_value,
            current_fees,
            target_amount,
            cost_of_change,
            fee_per_utxo,
            selected_utxos,
            best_selection,
            best_waste,
            iterations,
        );
    }
}

pub trait Utxo {
    fn value(&self) -> MicroMinotari;
}

impl Utxo for MicroMinotari {
    fn value(&self) -> MicroMinotari {
        *self
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bnb_selection() {
        let utxos = vec![
            MicroMinotari::from(10),
            MicroMinotari::from(20),
            MicroMinotari::from(30),
            MicroMinotari::from(40),
            MicroMinotari::from(50),
        ];
        let selector = BranchAndBoundUtxoSelector::new(utxos);

        // Exact match
        let selected = selector.select(MicroMinotari::from(90), MicroMinotari::from(5), MicroMinotari::from(0));
        assert!(selected.is_some());
        let sum: MicroMinotari = selected.unwrap().into_iter().sum();
        assert_eq!(sum, MicroMinotari::from(90));

        // Match within cost of change
        let selected = selector.select(MicroMinotari::from(88), MicroMinotari::from(5), MicroMinotari::from(0));
        assert!(selected.is_some());
        let sum: MicroMinotari = selected.unwrap().into_iter().sum();
        assert_eq!(sum, MicroMinotari::from(90)); // 40 + 50 or 20 + 30 + 40 etc.

        // No match
        let selected = selector.select(MicroMinotari::from(200), MicroMinotari::from(5), MicroMinotari::from(0));
        assert!(selected.is_none());
    }

    #[test]
    fn test_bnb_with_fees() {
        let utxos = vec![
            MicroMinotari::from(100),
            MicroMinotari::from(200),
            MicroMinotari::from(300),
        ];
        let selector = BranchAndBoundUtxoSelector::new(utxos);

        // Target 290, fee 10 per UTXO.
        // If we pick 300: value=300, total_target = 290 + 10 = 300. Perfect match.
        let selected = selector.select(MicroMinotari::from(290), MicroMinotari::from(5), MicroMinotari::from(10));
        assert!(selected.is_some());
        let sel = selected.unwrap();
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0], MicroMinotari::from(300));

        // Target 280, fee 10 per UTXO.
        // If we pick 300: value=300, total_target = 280 + 10 = 290. Waste = 10.
        // Cost of change is 5, so waste 10 > 5. 300 is NOT a good match.
        // If we pick 200 + 100: value=300, total_target = 280 + 20 = 300. Perfect match.
        let selected = selector.select(MicroMinotari::from(280), MicroMinotari::from(5), MicroMinotari::from(10));
        assert!(selected.is_some());
        let sel = selected.unwrap();
        assert_eq!(sel.len(), 2);
    }
}
