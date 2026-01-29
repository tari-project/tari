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
        Self {
            available_utxos,
            selected_utxos: Vec::new(),
            current_value: MicroMinotari::from(0),
            waste: MicroMinotari::from(0),
            final_target: params.target_amount,
            params,
            allow_dust_waste,
        }
    }

    fn add_utxo_sorted(&mut self, index: usize) {
        let pos = self.selected_utxos.binary_search(&index).unwrap_or_else(|e| e);
        self.selected_utxos.insert(pos, index);
    }

    // this method starts and iterative search
    fn start_search(&mut self, max_iterations: usize, best_result: &mut Option<SelectionState<T>>) {
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
                if (self.waste + extra_waste) < best.waste {
                    let mut new_state = self.clone();
                    new_state.waste += extra_waste;
                    *best_result = Some(new_state);
                }
            },
            None => {
                let mut new_state = self.clone();
                new_state.waste += extra_waste;
                *best_result = Some(new_state);
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

#[allow(dead_code)]
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
}
