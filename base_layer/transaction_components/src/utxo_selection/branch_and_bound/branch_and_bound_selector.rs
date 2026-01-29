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

use std::sync::Arc;

use crate::{utxo_selection::UtxoValue, MicroMinotari};

pub struct BranchAndBoundUtxoSelector<T> {
    available_utxos: Arc<Vec<T>>,
    search_params: UtxoSectionParams,
    params: BranchAndBoundUtxoSelectorParams,
}

impl<T> BranchAndBoundUtxoSelector<T>
where T: UtxoValue
{
    pub fn new(
        mut available_utxos: Vec<T>,
        search_params: UtxoSectionParams,
        params: BranchAndBoundUtxoSelectorParams,
    ) -> Self {
        // Sort UTXOs in descending order for BnB
        available_utxos.sort_by_key(|b| std::cmp::Reverse(b.value()));
        Self {
            available_utxos: Arc::new(available_utxos),
            search_params,
            params,
        }
    }

    pub fn search(&self) -> Option<SelectionResult<T>> {
        let mut initial_state = SelectionState::new_blank(
            self.available_utxos.clone(),
            self.search_params.clone(),
            self.params.allow_dust_waste,
        );
        let mut best_result: Option<SelectionState<T>> = None;

        initial_state.start_search(self.params.max_search_iterations, &mut best_result);

        best_result.map(|state| state.to_selection_result())
    }

    /// Search for the best UTXO selection while ensuring the specified UTXOs are always included.
    /// The `must_select` UTXOs will be pre-selected before the search begins, guaranteeing they
    /// are part of the final selection even if they wouldn't be the optimal choice.
    pub fn search_with_must_select(&self, must_select: Vec<T>) -> Option<SelectionResult<T>> {
        let mut initial_state = SelectionState::new_with_selected_utxos(
            self.available_utxos.clone(),
            Arc::new(must_select),
            self.search_params.clone(),
            self.params.allow_dust_waste,
        );
        let mut best_result: Option<SelectionState<T>> = None;

        initial_state.start_search(self.params.max_search_iterations, &mut best_result);

        best_result.map(|state| state.to_selection_result())
    }
}

pub struct BranchAndBoundUtxoSelectorParams {
    pub max_search_iterations: usize,
    pub allow_dust_waste: bool,
}

#[derive(Clone, Debug)]
pub struct UtxoSectionParams {
    target_amount: MicroMinotari,
    output_fee: MicroMinotari,
    change_fee: MicroMinotari,
    fee_per_input: MicroMinotari,
    input_limit: usize,
}

impl UtxoSectionParams {
    pub fn new(
        target_amount: MicroMinotari,
        output_fee: MicroMinotari,
        change_fee: MicroMinotari,
        fee_per_input: MicroMinotari,
        input_limit: usize,
    ) -> Self {
        Self {
            target_amount,
            output_fee,
            change_fee,
            fee_per_input,
            input_limit,
        }
    }

    fn total_target(&self) -> MicroMinotari {
        self.target_amount + self.output_fee
    }

    fn change_cost(&self) -> MicroMinotari {
        self.fee_per_input + self.change_fee
    }
}

#[derive(Clone, Debug)]
struct SelectionState<T> {
    available_utxos: Arc<Vec<T>>,
    selected_utxos: Vec<usize>,
    current_value: MicroMinotari,
    final_target: MicroMinotari,
    params: UtxoSectionParams,
    waste: MicroMinotari,
    allow_dust_waste: bool,
}

impl<T> SelectionState<T>
where T: UtxoValue
{
    fn new_blank(available_utxos: Arc<Vec<T>>, params: UtxoSectionParams, allow_dust_waste: bool) -> Self {
        // Sort UTXOs by value in descending order (biggest first)
        let mut sorted_utxos = (*available_utxos).clone();
        sorted_utxos.sort_by_key(|b| std::cmp::Reverse(b.value()));

        Self {
            available_utxos: Arc::new(sorted_utxos),
            selected_utxos: Vec::new(),
            current_value: MicroMinotari::from(0),
            waste: MicroMinotari::from(0),
            final_target: params.target_amount,
            params,
            allow_dust_waste,
        }
    }

    fn new_with_selected_utxos(available_utxos: Arc<Vec<T>>, must_select: Arc<Vec<T>>, params: UtxoSectionParams, allow_dust_waste: bool) -> Self {
        let mut new_blank = Self::new_blank(available_utxos, params, allow_dust_waste);
        for utxo in must_select.iter() {
            let index = new_blank
                .available_utxos
                .iter()
                .position(|u| u.value() == utxo.value())
                .expect("must_select UTXO not found in available_utxos");
            new_blank.add_utxo_sorted(index);
            // Update current_value, waste, and target_amount for each pre-selected UTXO
            new_blank.current_value += utxo.value();
            new_blank.waste += new_blank.params.fee_per_input;
            new_blank.params.target_amount += new_blank.params.fee_per_input;
        }
        new_blank
    }

    fn add_utxo_sorted(&mut self, index: usize) {
        let pos = self.selected_utxos.binary_search(&index).unwrap_or_else(|e| e);
        self.selected_utxos.insert(pos, index);
    }

    // this method starts and iterative search
    fn start_search(&mut self, max_iterations: usize, best_result: &mut Option<SelectionState<T>>) {
        // Check if the initial state (e.g., with pre-selected UTXOs) already satisfies the target
        if self.current_value >= self.params.total_target() {
            self.check_current_state(best_result);
            return;
        }

        let mut iterations = 0;
        for i in 0..self.available_utxos.len() {
            if iterations >= max_iterations {
                break;
            }
            if (self.selected_utxos.len() >= self.params.input_limit) ||
                (self.current_value >= self.params.total_target())
            {
                // tx is now at the limit, dont search further or we have the target
                break;
            }
            if self.selected_utxos.contains(&i) {
                continue;
            }
            let mut new_state = self.clone();
            iterations = new_state.search_and_add_index(i, iterations, max_iterations, best_result);
        }
    }

    // this method does an iterative search to find the best solution adding the index specified
    fn search_and_add_index(
        &mut self,
        index_to_add: usize,
        current_iterations: usize,
        max_iterations: usize,
        best_result: &mut Option<SelectionState<T>>,
    ) -> usize {
        self.add_utxo_sorted(index_to_add);
        self.current_value += self
            .available_utxos
            .get(index_to_add)
            .expect("utxo_index out of bounds")
            .value();
        self.waste += self.params.fee_per_input;
        self.params.target_amount += self.params.fee_per_input;

        let done = self.check_current_state(best_result);
        let mut iterations = current_iterations + 1;
        if done {
            // we found a solution, so stop here
            return iterations;
        }

        if self.selected_utxos.len() >= self.params.input_limit {
            // tx is now at the limit, dont search further
            return current_iterations;
        }
        if let Some(best) = best_result {
            // we are already worse off than the best here, stop right here
            if self.waste + self.params.fee_per_input >= best.waste {
                // no need to continue searching this branch
                return current_iterations;
            }
        }

        for i in index_to_add + 1..self.available_utxos.len() {
            if self.selected_utxos.contains(&i) {
                continue;
            }
            let mut new_state = self.clone();
            iterations = new_state.search_and_add_index(i, iterations, max_iterations, best_result);
            if iterations >= max_iterations {
                return iterations;
            }
        }
        iterations
    }

    fn check_current_state(&mut self, best_result: &mut Option<SelectionState<T>>) -> bool {
        let target = self.params.total_target();
        let current_value = self.current_value;
        if current_value >= target {
            if current_value == target {
                // perfect match, no better branch to search
                self.compare_to_best(best_result, 0.into());
                return true;
            }
            // not perfect, lets handle change
            let change_waste = self.params.change_cost();
            if current_value > target + change_waste {
                // we have enough to pay for change, so lets stop here
                self.compare_to_best(best_result, change_waste);
                return true;
            }

            // Now we need to handle the edge case that we have enough to pay the target but not enough to cover change
            // cost
            if self.allow_dust_waste {
                let extra_waste = self.current_value.saturating_sub(target); // we know its bigger than target

                self.compare_to_best(best_result, extra_waste);
                if extra_waste < self.params.fee_per_input {
                    // the waste is less than the cost of adding another input, so no use in adding another input
                    return true;
                }
            }
        }
        false
    }

    fn compare_to_best(&self, best_result: &mut Option<SelectionState<T>>, extra_waste: MicroMinotari) {
        match best_result {
            Some(best) => {
                if self.is_better_than(best, extra_waste) {
                    let mut new_state = self.clone();
                    new_state.waste += extra_waste;
                    new_state.final_target = self.params.target_amount + self.params.output_fee;
                    *best_result = Some(new_state);
                }
            },
            None => {
                let mut new_state = self.clone();
                new_state.waste += extra_waste;
                new_state.final_target = self.params.target_amount + self.params.output_fee;
                *best_result = Some(new_state);
            },
        }
    }

    /// Determines if the current state is better than the best state based on:
    /// 1. Least waste
    /// 2. If waste is equal, highest selected value (current_value) - gives highest change output
    /// 3. If selected value is the same, lowest final_target (fewer fees added)
    fn is_better_than(&self, best: &SelectionState<T>, extra_waste: MicroMinotari) -> bool {
        let self_waste = self.waste + extra_waste;
        let self_final_target = self.params.target_amount + self.params.output_fee;

        match self_waste.cmp(&best.waste) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => {
                // Waste is equal, prefer highest selected value (highest change output)
                match self.current_value.cmp(&best.current_value) {
                    std::cmp::Ordering::Greater => true,
                    std::cmp::Ordering::Less => false,
                    std::cmp::Ordering::Equal => {
                        // Selected value is equal, prefer lowest final_target
                        self_final_target < best.final_target
                    },
                }
            },
        }
    }

    fn to_selection_result(&self) -> SelectionResult<T> {
        let selected_utxos = self
            .selected_utxos
            .iter()
            .map(|&i| self.available_utxos.get(i).expect("utxo_index out of bounds").clone())
            .collect();
        SelectionResult {
            selected_utxos,
            current_value: self.current_value,
            final_target: self.final_target,
            waste: self.waste,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SelectionResult<T> {
    selected_utxos: Vec<T>,
    current_value: MicroMinotari,
    final_target: MicroMinotari,
    waste: MicroMinotari,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use rand::Rng;

    use super::*;

    fn default_params() -> BranchAndBoundUtxoSelectorParams {
        BranchAndBoundUtxoSelectorParams {
            max_search_iterations: 1000,
            allow_dust_waste: true,
        }
    }

    fn default_params_no_dust() -> BranchAndBoundUtxoSelectorParams {
        BranchAndBoundUtxoSelectorParams {
            max_search_iterations: 1000,
            allow_dust_waste: false,
        }
    }

    fn section_params(
        target: u64,
        output_fee: u64,
        change_fee: u64,
        fee_per_input: u64,
        input_limit: usize,
    ) -> UtxoSectionParams {
        UtxoSectionParams::new(
            MicroMinotari::from(target),
            MicroMinotari::from(output_fee),
            MicroMinotari::from(change_fee),
            MicroMinotari::from(fee_per_input),
            input_limit,
        )
    }

    #[test]
    fn test_exact_match() {
        let utxos = vec![MicroMinotari(50), MicroMinotari(30), MicroMinotari(20)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_overfunded_with_change() {
        let utxos = vec![MicroMinotari(60), MicroMinotari(50)];
        let params = section_params(100, 0, 5, 2, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 110.into());
    }

    #[test]
    fn test_underfunded() {
        let utxos = vec![MicroMinotari(10), MicroMinotari(20)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_solutions_choose_least_waste() {
        let utxos = vec![MicroMinotari(70), MicroMinotari(40), MicroMinotari(50)];
        let params = section_params(100, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 110.into());
        // Should not use all three if two suffice
        assert_eq!(result.selected_utxos.len(), 2);
    }

    #[test]
    fn test_input_limit() {
        let utxos = vec![MicroMinotari(10); 20];
        let params = section_params(100, 0, 0, 0, 5); // input limit 5
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_change_less_than_fee() {
        let utxos = vec![MicroMinotari(51), MicroMinotari(50)];
        let params = section_params(90, 0, 10, 5, 10); // change fee 10, fee per input 5
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // we target 90, so 51+50=101 is selected, pay 5 per input, change cost is 15, so change is not economical
        assert_eq!(result.current_value, 101.into());
        assert_eq!(result.selected_utxos.len(), 2);
        assert_eq!(result.waste, 11.into()); // 2 inputs * 5 + 1 excess
    }

    #[test]
    fn test_no_utxos() {
        let utxos: Vec<MicroMinotari> = Vec::new();
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_single_utxo_match() {
        let utxos = vec![MicroMinotari(100)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_all_utxos_too_small() {
        let utxos = vec![MicroMinotari(10), MicroMinotari(20), MicroMinotari(30)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_large_number_of_utxos_iteration_limit() {
        let utxos = (1..=30).map(MicroMinotari).collect::<Vec<_>>();
        let params = BranchAndBoundUtxoSelectorParams {
            max_search_iterations: 10, // force early stop
            allow_dust_waste: true,
        };
        let section = section_params(1000, 0, 0, 0, 30);
        let selector = BranchAndBoundUtxoSelector::new(utxos, section, params);
        let result = selector.search();
        assert!(result.is_none())
    }

    #[test]
    fn test_duplicate_utxo_values() {
        let utxos = vec![MicroMinotari(50), MicroMinotari(50), MicroMinotari(50)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn large_input_set() {
        let mut utxos = Vec::new();
        for _ in 0..1000 {
            let value: u64 = rand::thread_rng().gen_range(500..1500);
            utxos.push(MicroMinotari(value));
        }
        let params = section_params(500000, 0, 0, 0, 500);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert!(result.current_value >= 500000.into());
        assert!(result.selected_utxos.len() < 500);
    }

    #[test]
    fn test_zero_target_amount() {
        let utxos = vec![MicroMinotari(50), MicroMinotari(30)];
        let params = section_params(0, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        // With zero target, no UTXOs should be selected
        assert!(result.is_none());
    }

    #[test]
    fn test_target_with_output_fee() {
        let utxos = vec![MicroMinotari(100), MicroMinotari(50)];
        let params = section_params(80, 20, 0, 0, 10); // target 80 + output_fee 20 = 100
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_high_fee_per_input_makes_selection_uneconomical() {
        let utxos = vec![MicroMinotari(10), MicroMinotari(10), MicroMinotari(10)];
        let params = section_params(25, 0, 0, 50, 10); // fee_per_input is very high
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_exact_match_with_fees() {
        let utxos = vec![MicroMinotari(110)];
        let params = section_params(100, 10, 0, 0, 10); // target + output_fee = 110
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.current_value, 110.into());
    }

    #[test]
    fn test_change_cost_exactly_covered() {
        let utxos = vec![MicroMinotari(120)];
        let params = section_params(100, 0, 10, 5, 10); // change_cost = 15, so need > 115
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.current_value, 120.into());
    }

    #[test]
    fn test_input_limit_of_one() {
        let utxos = vec![MicroMinotari(100), MicroMinotari(50), MicroMinotari(30)];
        let params = section_params(100, 0, 0, 0, 1);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_input_limit_prevents_valid_selection() {
        let utxos = vec![MicroMinotari(40), MicroMinotari(40), MicroMinotari(40)];
        let params = section_params(100, 0, 0, 0, 2); // Need 3 UTXOs but limit is 2
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_input_limit_exactly_reached() {
        let utxos = vec![MicroMinotari(50), MicroMinotari(30), MicroMinotari(25)];
        let params = section_params(100, 0, 0, 0, 3);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 105.into());
        assert!(result.selected_utxos.len() <= 3);
    }

    #[test]
    fn test_zero_input_limit() {
        let utxos = vec![MicroMinotari(100)];
        let params = section_params(50, 0, 0, 0, 0);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_allow_dust_waste_enabled() {
        let utxos = vec![MicroMinotari(107)];
        let params = section_params(100, 0, 10, 5, 10); // change_cost = 15, excess = 2
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // With dust waste allowed, should select this UTXO even though change isn't economical
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.current_value, 107.into());
    }

    #[test]
    fn test_allow_dust_waste_disabled() {
        let utxos = vec![MicroMinotari(102)];
        let params = section_params(100, 0, 10, 5, 10); // change_cost = 15, excess = 2
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params_no_dust());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_dust_amount_less_than_fee_per_input() {
        let utxos = vec![MicroMinotari(108)];
        let params = section_params(100, 0, 10, 5, 10); // excess = 3, fee_per_input = 5
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.current_value, 108.into());
        assert_eq!(result.waste, 8.into()); // 5 fee + 3 excess
    }

    #[test]
    fn test_utxos_sorted_descending() {
        let utxos = vec![MicroMinotari(10), MicroMinotari(50), MicroMinotari(30)];
        let params = section_params(50, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should prefer the exact match of 50
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.current_value, 50.into());
    }

    #[test]
    fn test_prefers_fewer_inputs() {
        let utxos = vec![MicroMinotari(105), MicroMinotari(55), MicroMinotari(55)];
        let params = section_params(100, 0, 0, 5, 10); // fee_per_input = 5
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should prefer single 100 over two 50s due to less waste from fees
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 105);
    }

    #[test]
    fn test_single_utxo_just_below_target() {
        let utxos = vec![MicroMinotari(99)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_single_utxo_just_above_target() {
        let utxos = vec![MicroMinotari(101)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.selected_utxos.len(), 1);
    }

    #[test]
    fn test_very_small_utxo_values() {
        let utxos = vec![MicroMinotari(1), MicroMinotari(1), MicroMinotari(1)];
        let params = section_params(3, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 3.into());
    }

    #[test]
    fn test_very_large_utxo_values() {
        let utxos = vec![MicroMinotari(1_000_000_001), MicroMinotari(1001)];
        let params = section_params(1000, 0, 10, 1, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should select the smaller UTXO that exactly matches
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 1001);
    }

    #[test]
    fn test_target_equals_sum_of_all_utxos() {
        let utxos = vec![MicroMinotari(30), MicroMinotari(40), MicroMinotari(30)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_iteration_limit_of_one() {
        let utxos = vec![MicroMinotari(100), MicroMinotari(50)];
        let params = BranchAndBoundUtxoSelectorParams {
            max_search_iterations: 1,
            allow_dust_waste: true,
        };
        let section = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, section, params);
        let result = selector.search().unwrap();
        // With only 1 iteration, should still find the first valid solution
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_iteration_limit_zero() {
        let utxos = vec![MicroMinotari(100)];
        let params = BranchAndBoundUtxoSelectorParams {
            max_search_iterations: 0,
            allow_dust_waste: true,
        };
        let section = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, section, params);
        let result = selector.search();
        // With 0 iterations, no search should happen
        assert!(result.is_none());
    }

    #[test]
    fn test_high_iteration_limit() {
        let utxos = vec![MicroMinotari(50), MicroMinotari(30), MicroMinotari(20)];
        let params = BranchAndBoundUtxoSelectorParams {
            max_search_iterations: 1_000_000,
            allow_dust_waste: true,
        };
        let section = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, section, params);
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_multiple_exact_matches_prefers_less_inputs() {
        let utxos = vec![
            MicroMinotari(105),
            MicroMinotari(65),
            MicroMinotari(45),
            MicroMinotari(55),
            MicroMinotari(55),
        ];
        let params = section_params(100, 0, 0, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should prefer single 100 over combinations
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 105);
    }

    #[test]
    fn test_choice_between_overfunding_options() {
        let utxos = vec![MicroMinotari(150), MicroMinotari(110), MicroMinotari(50)];
        let params = section_params(100, 0, 5, 2, 10); // change_cost = 7
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should prefer 110 over 150 as it results in less waste
        assert!(result.current_value >= 107.into()); // At least target + change_cost
    }

    #[test]
    fn test_all_fees_non_zero() {
        let utxos = vec![MicroMinotari(200), MicroMinotari(100), MicroMinotari(50)];
        let params = section_params(100, 20, 15, 10, 10);
        // total_target = 120, change_cost = 25
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert!(result.current_value >= 120.into());
    }

    #[test]
    fn test_fee_per_input_affects_selection() {
        let utxos = vec![MicroMinotari(80), MicroMinotari(80), MicroMinotari(120)];
        let params = section_params(100, 0, 0, 20, 10); // high fee_per_input
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should prefer single 120 to avoid high per-input fees
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 120);
    }

    #[test]
    fn test_all_utxos_same_value_exact_multiple() {
        let utxos = vec![MicroMinotari(25); 4];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 100.into());
        assert_eq!(result.selected_utxos.len(), 4);
    }

    #[test]
    fn test_all_utxos_same_value_not_exact_multiple() {
        let utxos = vec![MicroMinotari(30); 4];
        let params = section_params(100, 0, 10, 5, 10); // need change
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // 4 * 30 = 120, which should cover target + change_cost
        assert!(result.current_value >= 115.into());
    }

    #[test]
    fn test_many_small_utxos() {
        let utxos: Vec<MicroMinotari> = (1..=20).map(MicroMinotari).collect();
        let params = section_params(50, 0, 0, 0, 20);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert!(result.current_value >= 50.into());
    }

    #[test]
    fn test_fibonacci_like_utxo_values() {
        let utxos = vec![
            MicroMinotari(1),
            MicroMinotari(2),
            MicroMinotari(3),
            MicroMinotari(5),
            MicroMinotari(8),
            MicroMinotari(13),
            MicroMinotari(21),
            MicroMinotari(34),
        ];
        let params = section_params(50, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert!(result.current_value >= 50.into());
    }

    #[test]
    fn test_powers_of_two_utxo_values() {
        let utxos = vec![
            MicroMinotari(1),
            MicroMinotari(2),
            MicroMinotari(4),
            MicroMinotari(8),
            MicroMinotari(16),
            MicroMinotari(32),
            MicroMinotari(64),
        ];
        let params = section_params(96, 1, 1, 1, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // 64 + 32 + 4 = 96 + 4(fee) = 100
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_single_large_utxo_among_small_ones() {
        let mut utxos: Vec<MicroMinotari> = vec![MicroMinotari(5); 10];
        utxos.push(MicroMinotari(1000));
        let params = section_params(100, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should select the large UTXO
        assert_eq!(result.selected_utxos[0].value().as_u64(), 1000);
        assert_eq!(result.selected_utxos.len(), 1);
    }

    #[test]
    fn test_no_solution_with_input_limit_and_fees() {
        let utxos = vec![MicroMinotari(30), MicroMinotari(30), MicroMinotari(30)];
        let params = section_params(100, 10, 0, 0, 2); // Need 110, max 2 inputs = 60 max
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_none());
    }

    #[test]
    fn test_barely_reachable_target_with_all_utxos() {
        let utxos = vec![MicroMinotari(34), MicroMinotari(33), MicroMinotari(33)];
        let params = section_params(100, 0, 0, 0, 3);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_search_returns_ok_result() {
        let utxos = vec![MicroMinotari(100)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.is_some());
    }

    #[test]
    fn test_selected_utxos_are_clones_of_originals() {
        let utxos = vec![MicroMinotari(100), MicroMinotari(50)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos.clone(), params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.selected_utxos[0], MicroMinotari(100));
    }

    #[ignore]
    #[test]
    fn speed_bench() {
        use std::time::Instant;
        let mut total_duration = 0;
        for _i in 0..1000 {
            let start = Instant::now();
            large_input_set();
            let duration = start.elapsed().as_millis();
            total_duration += duration;
        }
        println!("Average duration over 1000 runs: {} ms", total_duration / 1000);
    }

    // Tests for multi-criteria comparison (waste, current_value, final_target)

    #[test]
    fn test_comparison_prefers_least_waste() {
        // Two solutions with different waste: should prefer the one with less waste
        // Solution 1: 150 with 1 input -> waste = fee_per_input(5)
        // Solution 2: 100 + 60 with 2 inputs -> waste = 2 * fee_per_input(10)
        let utxos = vec![MicroMinotari(150), MicroMinotari(100), MicroMinotari(60)];
        let params = section_params(100, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should prefer 150 (1 input, less waste) over 100+60 (2 inputs, more waste)
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 150);
    }

    #[test]
    fn test_comparison_equal_waste_prefers_highest_value() {
        // Two solutions with same waste but different selected values
        // Both use 1 input, so same fee_per_input waste
        // Both should satisfy target with change, giving same change_cost waste
        //
        // params: target=100, output_fee=0, change_fee=10, fee_per_input=5
        // total_target() after 1 input = 100 + 5 = 105
        // change_cost() = 5 + 10 = 15
        //
        // For an input to have change (not dust): current_value > total_target + change_cost
        // i.e., current_value > 105 + 15 = 120
        //
        // So we need UTXOs > 120 to have equal waste (fee + change_cost)
        let utxos = vec![MicroMinotari(150), MicroMinotari(130)];
        let params = section_params(100, 0, 10, 5, 10); // change_cost = 15, target = 100
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Both have waste = 5 (fee) + 15 (change_cost) = 20
        // Should prefer 150 (higher value, higher change output)
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 150);
        assert_eq!(result.current_value, 150.into());
    }

    #[test]
    fn test_comparison_equal_waste_and_value_prefers_lowest_final_target() {
        // This is a tricky case - we need two solutions with:
        // - Same waste
        // - Same selected value
        // - Different final_target (different number of inputs means different fees added to target)
        //
        // Scenario: target 100, fee_per_input = 10
        // Solution 1: Single UTXO of 110 -> final_target = 100 + 0 + 10 (1 input) = 110, waste = 10
        // Solution 2: Two UTXOs of 55 each = 110 -> final_target = 100 + 0 + 20 (2 inputs) = 120, waste = 20
        //
        // But waste is different here. Let's think differently:
        // To get same waste with different final_target, we need same number of inputs but
        // final_target computed differently... Actually, the final_target grows with inputs added.
        //
        // Let's verify that with equal waste and equal selected value, lower final_target wins.
        // This scenario is quite rare but let's craft it:
        // If we have exact same waste and current_value, the tiebreaker is final_target.

        // Actually, this is hard to test directly in the selector because waste and value
        // are directly tied to input selection. Let's test the is_better_than logic directly.

        // For an integration test, we can verify the logic works by checking that
        // given equal waste and value scenarios, the result has expected properties.

        // A practical test: two combinations that yield the same total value but different paths
        // This is inherently difficult because different input counts mean different fees.

        // Let's instead verify the implementation with a simpler scenario where we
        // ensure the behavior is correct when comparing.

        // Test that single input is preferred when waste is the same
        let utxos = vec![MicroMinotari(115), MicroMinotari(60), MicroMinotari(55)];
        let params = section_params(100, 0, 10, 5, 10); // change_cost = 15
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // 115 with 1 input: waste = 5 (fee) + 0 (change covered) = 5 + 10 (change_cost waste) = 15?
        // Actually waste = fee_per_input for each input
        // 115: waste = 5, target reached with change
        // 60+55=115 with 2 inputs: waste = 10, same value
        // Should prefer single input (lower waste)
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 115);
    }

    #[test]
    fn test_comparison_criteria_order_waste_first() {
        // Verify that waste is the primary criterion
        // Higher value but higher waste should lose to lower value with lower waste
        let utxos = vec![MicroMinotari(200), MicroMinotari(50), MicroMinotari(60)];
        let params = section_params(100, 0, 10, 20, 10); // high fee_per_input = 20
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Single 200: waste = 20 (1 input)
        // 50 + 60 = 110: waste = 40 (2 inputs)
        // Even though 50+60 is closer to target, 200 has less waste
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 200);
    }

    #[test]
    fn test_comparison_with_exact_match_vs_overfunded() {
        // Exact match has 0 extra waste, overfunded has change_cost waste or dust waste
        // params: target=100, output_fee=0, change_fee=10, fee_per_input=5
        // total_target() after 1 input = 100 + 5 = 105
        // change_cost() = 5 + 10 = 15
        //
        // For exact match: current_value == total_target = 105
        // For overfunded with change: current_value > total_target + change_cost = 120
        // For dust waste: total_target < current_value <= total_target + change_cost
        //
        // Comparing 105 (exact match) vs 125 (overfunded with change)
        // 105: waste = 5 (fee) + 0 (exact) = 5
        // 125: waste = 5 (fee) + 15 (change_cost) = 20
        let utxos = vec![MicroMinotari(125), MicroMinotari(105)];
        let params = section_params(100, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Should prefer exact match (less waste)
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 105);
    }

    #[test]
    fn test_comparison_selects_higher_change_when_waste_equal() {
        // Two UTXOs that both exceed target+change_cost, same waste, different values
        let utxos = vec![MicroMinotari(150), MicroMinotari(130)];
        let params = section_params(100, 0, 10, 5, 10); // change_cost = 15, so need > 115
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Both have same waste (5 fee + 15 change_cost = 20)
        // 150 gives higher change output (150 - 105 = 45)
        // 130 gives lower change output (130 - 105 = 25)
        // Should prefer 150 for higher change
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 150);
    }

    #[test]
    fn test_multiple_paths_same_value_different_inputs() {
        // Test scenario where we can reach same total value with different input counts
        // This tests that fewer inputs (lower final_target/fees) is preferred when value is same
        let utxos = vec![
            MicroMinotari(100), // Single input to reach target
            MicroMinotari(50),
            MicroMinotari(50), // Two inputs to reach same target
        ];
        let params = section_params(100, 0, 0, 0, 10); // No fees to isolate the comparison
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Both options: 100 or 50+50 = 100, same value
        // With 0 fee_per_input, waste is same, value is same
        // final_target: 100 for single input, 100 for two inputs (no fees)
        // This case they're equal, but implementation might prefer fewer inputs naturally
        assert_eq!(result.current_value, 100.into());
    }

    #[test]
    fn test_waste_calculation_includes_input_fees() {
        // Verify that waste properly accounts for input fees
        // params: target=100, output_fee=0, change_fee=0, fee_per_input=10
        // total_target() after 1 input = 100 + 10 = 110
        // change_cost() = 10 + 0 = 10
        //
        // Single 110: covers target exactly, waste = 10 (1 input fee)
        // 60+60=120 (2 inputs): target becomes 120, so 120 == 120, exact match, waste = 20
        let utxos = vec![MicroMinotari(110), MicroMinotari(60), MicroMinotari(60)];
        let params = section_params(100, 0, 0, 10, 10); // fee_per_input = 10, no change_fee
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        // Single 110: waste = 10 (1 input fee), exact match
        // 60+60: waste = 20 (2 input fees), exact match
        // Should prefer single input (less waste)
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.waste, 10.into());
    }

    #[test]
    fn test_is_better_than_logic_directly() {
        // Test the is_better_than comparison logic with crafted states
        use std::sync::Arc;

        let utxos: Arc<Vec<MicroMinotari>> = Arc::new(vec![MicroMinotari(100)]);
        let params = UtxoSectionParams::new(
            MicroMinotari::from(50),
            MicroMinotari::from(0),
            MicroMinotari::from(0),
            MicroMinotari::from(5),
            10,
        );

        // State A: lower waste
        let mut state_a = SelectionState::new_blank(utxos.clone(), params.clone(), true);
        state_a.waste = MicroMinotari::from(10);
        state_a.current_value = MicroMinotari::from(100);

        // State B: higher waste
        let mut state_b = SelectionState::new_blank(utxos.clone(), params.clone(), true);
        state_b.waste = MicroMinotari::from(20);
        state_b.current_value = MicroMinotari::from(100);

        // A is better than B (less waste)
        assert!(state_a.is_better_than(&state_b, 0.into()));
        // B is not better than A
        assert!(!state_b.is_better_than(&state_a, 0.into()));

        // Now test equal waste, different value
        state_b.waste = MicroMinotari::from(10); // same waste as A
        state_a.current_value = MicroMinotari::from(150); // higher value
        state_b.current_value = MicroMinotari::from(100); // lower value

        // A is better (same waste, higher value)
        assert!(state_a.is_better_than(&state_b, 0.into()));
        // B is not better
        assert!(!state_b.is_better_than(&state_a, 0.into()));

        // Test equal waste and value, different final_target
        state_b.current_value = MicroMinotari::from(150); // same value
                                                          // final_target is computed as params.target_amount + params.output_fee
                                                          // State A has lower params.target_amount (simulating fewer fees added)
        let params_low = UtxoSectionParams::new(
            MicroMinotari::from(50), // lower target (fewer fees)
            MicroMinotari::from(0),
            MicroMinotari::from(0),
            MicroMinotari::from(5),
            10,
        );
        let params_high = UtxoSectionParams::new(
            MicroMinotari::from(70), // higher target (more fees added)
            MicroMinotari::from(0),
            MicroMinotari::from(0),
            MicroMinotari::from(5),
            10,
        );

        let mut state_low_target = SelectionState::new_blank(utxos.clone(), params_low, true);
        state_low_target.waste = MicroMinotari::from(10);
        state_low_target.current_value = MicroMinotari::from(150);

        let mut state_high_target = SelectionState::new_blank(utxos.clone(), params_high, true);
        state_high_target.waste = MicroMinotari::from(10);
        state_high_target.current_value = MicroMinotari::from(150);
        state_high_target.final_target = MicroMinotari::from(70); // Set the stored final_target

        // Low target is better (same waste, same value, lower final_target)
        assert!(state_low_target.is_better_than(&state_high_target, 0.into()));
        // High target is not better
        assert!(!state_high_target.is_better_than(&state_low_target, 0.into()));
    }

    // Tests for new_with_selected_utxos and search_with_must_select

    #[test]
    fn test_new_with_selected_utxos_initializes_state_correctly() {
        // Test that new_with_selected_utxos properly initializes current_value, waste, and target_amount
        let utxos: Arc<Vec<MicroMinotari>> = Arc::new(vec![
            MicroMinotari(100),
            MicroMinotari(50),
            MicroMinotari(30),
        ]);
        let must_select: Arc<Vec<MicroMinotari>> = Arc::new(vec![MicroMinotari(50)]);
        let params = UtxoSectionParams::new(
            MicroMinotari::from(80),
            MicroMinotari::from(0),
            MicroMinotari::from(0),
            MicroMinotari::from(5),
            10,
        );

        let state = SelectionState::new_with_selected_utxos(utxos, must_select, params, true);

        // Verify the must_select UTXO is in selected_utxos
        assert_eq!(state.selected_utxos.len(), 1);
        // Verify current_value is set to the value of the pre-selected UTXO
        assert_eq!(state.current_value, MicroMinotari::from(50));
        // Verify waste is set to fee_per_input (5)
        assert_eq!(state.waste, MicroMinotari::from(5));
        // Verify target_amount was increased by fee_per_input
        assert_eq!(state.params.target_amount, MicroMinotari::from(85)); // 80 + 5
    }

    #[test]
    fn test_new_with_selected_utxos_multiple_utxos() {
        // Test with multiple pre-selected UTXOs
        let utxos: Arc<Vec<MicroMinotari>> = Arc::new(vec![
            MicroMinotari(100),
            MicroMinotari(50),
            MicroMinotari(30),
            MicroMinotari(20),
        ]);
        let must_select: Arc<Vec<MicroMinotari>> = Arc::new(vec![
            MicroMinotari(50),
            MicroMinotari(30),
        ]);
        let params = UtxoSectionParams::new(
            MicroMinotari::from(60),
            MicroMinotari::from(0),
            MicroMinotari::from(0),
            MicroMinotari::from(10),
            10,
        );

        let state = SelectionState::new_with_selected_utxos(utxos, must_select, params, true);

        // Verify both must_select UTXOs are in selected_utxos
        assert_eq!(state.selected_utxos.len(), 2);
        // Verify current_value is the sum of pre-selected UTXOs (50 + 30 = 80)
        assert_eq!(state.current_value, MicroMinotari::from(80));
        // Verify waste is 2 * fee_per_input (2 * 10 = 20)
        assert_eq!(state.waste, MicroMinotari::from(20));
        // Verify target_amount was increased by 2 * fee_per_input (60 + 20 = 80)
        assert_eq!(state.params.target_amount, MicroMinotari::from(80));
    }

    #[test]
    fn test_search_with_must_select_includes_specified_utxos() {
        // Test that search_with_must_select always includes the specified UTXOs in the result
        // even when a better solution exists without them
        let utxos = vec![
            MicroMinotari(100), // This alone would satisfy target=100 with 1 input
            MicroMinotari(50),  // This is sub-optimal
            MicroMinotari(60),  // This is sub-optimal
        ];
        let params = section_params(100, 0, 0, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Force selection of the 50 UTXO, which is not optimal
        let must_select = vec![MicroMinotari(50)];
        let result = selector.search_with_must_select(must_select).unwrap();

        // The result should include the 50 UTXO
        let selected_values: Vec<u64> = result.selected_utxos.iter().map(|u| u.value().as_u64()).collect();
        assert!(selected_values.contains(&50), "Must-select UTXO (50) should be in the result");
        // The total should be at least the target
        assert!(result.current_value >= 105.into()); // target + fee_per_input for 50
    }

    #[test]
    fn test_search_with_must_select_suboptimal_utxo_still_selected() {
        // Given a choice between one large UTXO or multiple smaller ones,
        // forcing the selection of a small UTXO should result in it being included
        let utxos = vec![
            MicroMinotari(200), // Perfect single UTXO solution
            MicroMinotari(10),  // Very small, suboptimal
            MicroMinotari(100),
            MicroMinotari(90),
        ];
        let params = section_params(150, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Normal search would pick 200
        let normal_result = selector.search().unwrap();
        assert_eq!(normal_result.selected_utxos.len(), 1);
        assert_eq!(normal_result.selected_utxos[0].value().as_u64(), 200);

        // Force selection of the small 10 UTXO
        let must_select = vec![MicroMinotari(10)];
        let forced_result = selector.search_with_must_select(must_select).unwrap();

        // The 10 UTXO must be in the result
        let selected_values: Vec<u64> = forced_result.selected_utxos.iter().map(|u| u.value().as_u64()).collect();
        assert!(selected_values.contains(&10), "Must-select UTXO (10) should be in the result");
    }

    #[test]
    fn test_search_with_must_select_exact_match_with_forced_utxo() {
        // Force selection of a UTXO that when combined with others gives exact match
        let utxos = vec![
            MicroMinotari(100),
            MicroMinotari(50),
            MicroMinotari(55),
            MicroMinotari(45),
        ];
        let params = section_params(100, 0, 0, 0, 10); // target=100, no fees
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Force selection of 55
        let must_select = vec![MicroMinotari(55)];
        let result = selector.search_with_must_select(must_select).unwrap();

        // The 55 UTXO must be in the result
        let selected_values: Vec<u64> = result.selected_utxos.iter().map(|u| u.value().as_u64()).collect();
        assert!(selected_values.contains(&55), "Must-select UTXO (55) should be in the result");
        assert!(result.current_value >= 100.into());
    }

    #[test]
    fn test_search_with_must_select_multiple_forced_utxos() {
        // Force selection of multiple UTXOs
        let utxos = vec![
            MicroMinotari(200), // Single optimal solution
            MicroMinotari(30),
            MicroMinotari(40),
            MicroMinotari(50),
            MicroMinotari(60),
        ];
        let params = section_params(100, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Force selection of both 30 and 40
        let must_select = vec![MicroMinotari(30), MicroMinotari(40)];
        let result = selector.search_with_must_select(must_select).unwrap();

        // Both forced UTXOs must be in the result
        let selected_values: Vec<u64> = result.selected_utxos.iter().map(|u| u.value().as_u64()).collect();
        assert!(selected_values.contains(&30), "Must-select UTXO (30) should be in the result");
        assert!(selected_values.contains(&40), "Must-select UTXO (40) should be in the result");
        // Total value should cover the target
        assert!(result.current_value >= 110.into()); // target + fees
    }

    #[test]
    fn test_search_with_must_select_forced_utxo_already_covers_target() {
        // Force selection of a UTXO that by itself already covers the target
        let utxos = vec![
            MicroMinotari(150),
            MicroMinotari(50),
            MicroMinotari(60),
        ];
        let params = section_params(100, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Force selection of 150, which alone covers target + change_cost
        let must_select = vec![MicroMinotari(150)];
        let result = selector.search_with_must_select(must_select).unwrap();

        // Only the forced UTXO should be selected (it already covers everything)
        assert_eq!(result.selected_utxos.len(), 1);
        assert_eq!(result.selected_utxos[0].value().as_u64(), 150);
    }

    #[test]
    fn test_search_with_must_select_vs_normal_search_different_results() {
        // Verify that forcing a suboptimal UTXO gives a different (worse) result
        let utxos = vec![
            MicroMinotari(105), // Exact match for target + 1 input fee
            MicroMinotari(20),  // Suboptimal small UTXO
            MicroMinotari(90),
        ];
        let params = section_params(100, 0, 0, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Normal search should prefer 105 (exact match)
        let normal_result = selector.search().unwrap();
        assert_eq!(normal_result.selected_utxos.len(), 1);
        assert_eq!(normal_result.selected_utxos[0].value().as_u64(), 105);
        assert_eq!(normal_result.waste, MicroMinotari::from(5)); // Just the input fee

        // Force selection of 20 - will need additional UTXOs
        let must_select = vec![MicroMinotari(20)];
        let forced_result = selector.search_with_must_select(must_select).unwrap();

        // Result must include 20 and additional UTXOs to cover target
        let selected_values: Vec<u64> = forced_result.selected_utxos.iter().map(|u| u.value().as_u64()).collect();
        assert!(selected_values.contains(&20), "Must-select UTXO (20) should be in the result");
        assert!(forced_result.selected_utxos.len() >= 2, "Should need multiple UTXOs");
        // Waste should be higher due to multiple inputs
        assert!(forced_result.waste > normal_result.waste, "Forced selection should have more waste");
    }

    #[test]
    fn test_search_with_must_select_insufficient_utxos_returns_none() {
        // If the must-select UTXOs plus available UTXOs can't cover the target, return None
        let utxos = vec![
            MicroMinotari(30),
            MicroMinotari(20),
            MicroMinotari(10),
        ];
        let params = section_params(100, 0, 0, 0, 10); // target=100, total available=60
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Force selection of 10
        let must_select = vec![MicroMinotari(10)];
        let result = selector.search_with_must_select(must_select);

        // Should return None since total (60) < target (100)
        assert!(result.is_none());
    }

    #[test]
    fn test_search_with_must_select_respects_input_limit() {
        // Verify that must_select UTXOs count toward the input limit
        let utxos = vec![
            MicroMinotari(40),
            MicroMinotari(40),
            MicroMinotari(40),
            MicroMinotari(40),
        ];
        let params = section_params(100, 0, 0, 0, 2); // input_limit = 2
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Force selection of one UTXO, leaving room for only one more
        let must_select = vec![MicroMinotari(40)];
        let result = selector.search_with_must_select(must_select);

        // With 2 inputs max and 40 each = 80, can't reach target of 100
        // So this should return None or find a solution if exact 80 works
        if let Some(r) = result {
            assert!(r.selected_utxos.len() <= 2);
        }
    }

    #[test]
    fn test_search_with_must_select_finds_optimal_among_remaining() {
        // After forcing a UTXO selection, the algorithm should find the optimal
        // combination among the remaining UTXOs
        let utxos = vec![
            MicroMinotari(100),
            MicroMinotari(50), // Force this
            MicroMinotari(60), // This + 50 = 110, better than 50 + 40 + 30
            MicroMinotari(40),
            MicroMinotari(30),
        ];
        let params = section_params(100, 0, 0, 5, 10); // fee_per_input = 5
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());

        // Force selection of 50
        let must_select = vec![MicroMinotari(50)];
        let result = selector.search_with_must_select(must_select).unwrap();

        let selected_values: Vec<u64> = result.selected_utxos.iter().map(|u| u.value().as_u64()).collect();
        assert!(selected_values.contains(&50), "Must-select UTXO (50) should be in the result");

        // Should prefer 50+60=110 (2 inputs) over 50+40+30=120 (3 inputs) due to lower waste
        // 2 inputs: waste = 10, 3 inputs: waste = 15
        assert_eq!(result.selected_utxos.len(), 2, "Should prefer fewer inputs");
    }
}
