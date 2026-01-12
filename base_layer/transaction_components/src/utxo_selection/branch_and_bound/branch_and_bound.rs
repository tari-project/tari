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

use std::sync::{Arc, RwLock};

use rayon::iter::{IntoParallelIterator, ParallelIterator};

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
        available_utxos.sort_by(|a, b| b.value().cmp(&a.value()));
        Self {
            available_utxos: Arc::new(available_utxos),
            search_params,
            params,
        }
    }

    pub fn search(&self) -> Result<Vec<T>, String> {
        let mut initial_state = SelectionState::new_blank(self.available_utxos.clone(), self.search_params.clone(), self.params.allow_dust_waste);
        let mut best_result: Option<SelectionState<T>> = None;

        initial_state.start_search(self.params.max_search_iterations, &mut best_result);

        let selected_utxos: Vec<T> = match best_result {
            Some(best_result) => best_result
                .selected_utxos
                .iter()
                .map(|&i| self.available_utxos.get(i).expect("utxo_index out of bounds").clone())
                .collect(),
            None => Vec::new(),
        };
        Ok(selected_utxos)
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
struct SelectionState<T: Sized> {
    available_utxos: Arc<Vec<T>>,
    selected_utxos: Vec<usize>,
    current_value: MicroMinotari,
    final_target: MicroMinotari,
    parms: UtxoSectionParams,
    waste: MicroMinotari,
    allow_dust_waste: bool,
}

impl<T> SelectionState<T>
where T: UtxoValue
{
    fn new_blank(available_utxos: Arc<Vec<T>>, parms: UtxoSectionParams,  allow_dust_waste: bool) -> Self {
        Self {
            available_utxos,
            selected_utxos: Vec::new(),
            current_value: MicroMinotari::from(0),
            waste: MicroMinotari::from(0),
            final_target: parms.target_amount,
            parms,
            allow_dust_waste
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
            if self.selected_utxos.contains(&i) {
                continue;
            }
            let mut new_state = self.clone();
            iterations = new_state.search_and_add_index(i, iterations, max_iterations, best_result);
            if iterations >= max_iterations {
                break;
            }
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
        self.waste += self.parms.fee_per_input;

        let done = self.check_current_state(best_result);
        let mut iterations = current_iterations + 1;
        if done {
            // we found a solution, so stop here
            return iterations;
        }

        if self.selected_utxos.len() >= self.parms.input_limit {
            // tx is now at the limit, dont search further
            return current_iterations;
        }
        if let Some(best) = best_result {
            // we are already worse off than the best here, stop right here
            if self.waste + self.parms.fee_per_input >= best.waste {
                // no need to continue searching this branch
                return current_iterations;
            }
        }

        for i in 0..self.available_utxos.len() {
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
        let target = self.parms.total_target();
        let current_value = self.current_value;
        if current_value >= target {
            if current_value == target {
                // perfect match, no better branch to search
                self.compare_to_best(best_result, 0.into());
                return true;
            }
            // not perfect, lets handle change
            let change_waste = self.parms.change_cost();
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
                if extra_waste < self.parms.fee_per_input {
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
                } else {
                }
            },
            None => {
                let mut new_state = self.clone();
                new_state.waste += extra_waste;
                *best_result = Some(new_state);
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::*;

    fn default_params() -> BranchAndBoundUtxoSelectorParams {
        BranchAndBoundUtxoSelectorParams {
            threads: 2,
            max_search_iterations: 1000,
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
        let sum: u64 = result.iter().map(|u| u.value().as_u64()).sum();
        assert_eq!(sum, 100);
    }

    #[test]
    fn test_overfunded_with_change() {
        let utxos = vec![MicroMinotari(60), MicroMinotari(50)];
        let params = section_params(100, 0, 5, 2, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        let sum: u64 = result.iter().map(|u| u.value().as_u64()).sum();
        assert_eq!(sum, 110);
    }

    #[test]
    fn test_underfunded() {
        let utxos = vec![MicroMinotari(10), MicroMinotari(20)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_multiple_solutions_choose_least_waste() {
        let utxos = vec![MicroMinotari(70), MicroMinotari(40), MicroMinotari(50)];
        let params = section_params(100, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        let sum: u64 = result.iter().map(|u| u.value().as_u64()).sum();
        assert_eq!(sum, 110);
        // Should not use all three if two suffice
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_input_limit() {
        let utxos = vec![MicroMinotari(10); 20];
        let params = section_params(100, 0, 0, 0, 5); // input limit 5
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_change_less_than_fee() {
        let utxos = vec![MicroMinotari(51), MicroMinotari(50)];
        let params = section_params(100, 0, 10, 5, 10); // change fee 10, fee per input 5
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        let sum: u64 = result.iter().map(|u| u.value().as_u64()).sum();
        assert_eq!(sum, 101);
    }

    #[test]
    fn test_no_utxos() {
        let utxos: Vec<MicroMinotari> = Vec::new();
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_single_utxo_match() {
        let utxos = vec![MicroMinotari(100)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value().as_u64(), 100);
    }

    #[test]
    fn test_all_utxos_too_small() {
        let utxos = vec![MicroMinotari(10), MicroMinotari(20), MicroMinotari(30)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search();
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_large_number_of_utxos_iteration_limit() {
        let utxos = (1..=30).map(MicroMinotari).collect::<Vec<_>>();
        let mut params = default_params();
        params.max_search_iterations = 10; // force early stop
        let section = section_params(1000, 0, 0, 0, 30);
        let selector = BranchAndBoundUtxoSelector::new(utxos, section, params);
        let result = selector.search().unwrap();
        assert!(result.is_empty())
    }

    #[test]
    fn test_duplicate_utxo_values() {
        let utxos = vec![MicroMinotari(50), MicroMinotari(50), MicroMinotari(50)];
        let params = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        let sum: u64 = result.iter().map(|u| u.value().as_u64()).sum();
        assert_eq!(sum, 100);
    }

    #[test]
    fn test_thread_count_variation() {
        let utxos = vec![MicroMinotari(60), MicroMinotari(40), MicroMinotari(50)];
        let mut params = default_params();
        params.threads = 1;
        let section = section_params(100, 0, 0, 0, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, section, params);
        let result = selector.search().unwrap();
        let sum: u64 = result.iter().map(|u| u.value().as_u64()).sum();
        assert_eq!(sum, 110);
    }

    #[test]
    fn large_input_set() {
        let mut utxos = Vec::new();
        for _ in 0..1000 {
            let value: u64 = rand::thread_rng().gen_range(500..1500);
            utxos.push(MicroMinotari(value));
        }
        let input_sum: u64 = utxos.iter().map(|u| u.value().as_u64()).sum();
        let params = section_params(500000, 0, 0, 0, 500);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        let sum: u64 = result.iter().map(|u| u.value().as_u64()).sum();
        assert!(sum >= 500000);
        assert!(result.len() < 500);
    }

    #[ignore]
    #[test]
    fn speed_bench() {
        use std::time::Instant;
        let mut total_duration = 0;
        for i in 0..1000 {
            let start = Instant::now();
            large_input_set();
            let duration = start.elapsed().as_millis();
            total_duration += duration;
        }
        dbg!("Average duration over 1000 runs: {} ms", total_duration / 1000);
    }
}
