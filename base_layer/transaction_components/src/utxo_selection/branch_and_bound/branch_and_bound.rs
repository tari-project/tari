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
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use crate::{utxo_selection::UtxoValue, MicroMinotari};
use std::sync::RwLock;

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

    pub fn search(&self) -> Result<Vec<T>, String>{
        let initial_state = SelectionState::new_blank(self.available_utxos.clone(), self.search_params.clone());

        let mut to_search = vec![initial_state];
        let mut done_results = Vec::new();

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.params.threads)
            .build()
            .map_err(|e| format!("Failed to build thread pool: {}", e))?;
        let mut iterations = 0;
        loop {
            let thread_done = RwLock::new(Vec::new());
            let thread_not_done = RwLock::new(Vec::new());
            iterations += to_search.len();
            pool.install(|| {
                to_search.into_par_iter().for_each(|state| {
                    let (mut done, not_done) = state.search();
                    thread_done
                        .write()
                        .expect("write lock should not be poisoned")
                        .append(&mut done);
                    thread_not_done
                        .write()
                        .expect("write lock should not be poisoned")
                        .extend(not_done);


                });
            });
            done_results.append(&mut thread_done
                .into_inner()
                .expect("into_inner should not fail"));
            to_search = thread_not_done
                .into_inner()
                .expect("into_inner should not fail");
            // todo remote duplicates from to_search to reduce search space
            if iterations >= self.params.max_search_iterations {
                break;
            }
            if to_search.is_empty() {
                break;
            }

        }

        done_results.sort_by(|a, b| a.waste.cmp(&b.waste));
        // collect the selected utxos
        let selected_utxos: Vec<T> = match done_results.first(){
            Some(best_result) => best_result.selected_utxos.iter().map(|&i| {
                                                           self.available_utxos.get(i).expect("utxo_index out of bounds").clone()
                                                           }).collect(),
            None => Vec::new()
        };

        Ok(selected_utxos)
    }
}

pub struct BranchAndBoundUtxoSelectorParams{
    pub threads: usize,
    pub max_search_iterations: usize,
}

#[derive(Clone)]
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

#[derive(Clone)]
struct SelectionState<T: Sized> {
    available_utxos: Arc<Vec<T>>,
    selected_utxos: Vec<usize>,
    current_value: MicroMinotari,
    final_target: MicroMinotari,
    parms: UtxoSectionParams,
    waste: MicroMinotari,
    done: bool,
}

impl<T> SelectionState<T>
where T: UtxoValue
{
    fn new_blank(available_utxos: Arc<Vec<T>>, parms: UtxoSectionParams) -> Self {
        Self {
            available_utxos,
            selected_utxos: Vec::new(),
            current_value: MicroMinotari::from(0),
            waste: MicroMinotari::from(0),
            final_target: parms.target_amount,
            parms,
            done: false,
        }
    }

    fn search(self) -> (Vec<SelectionState<T>>, Vec<SelectionState<T>>) {
        let mut done_results = Vec::new();
        let mut not_done_results = Vec::new();
        if self.selected_utxos.len() >= self.parms.input_limit {
            // tx is too large, dont continue searching this branch
            return (done_results, not_done_results);
        }
        for i in 0..self.available_utxos.len() {
            if self.selected_utxos.contains(&i) {
                continue;
            }
            let mut new_state = self.clone();
            new_state.selected_utxos.push(i);
            new_state.current_value += new_state
                .available_utxos
                .get(i)
                .expect("utxo_index out of bounds")
                .value();
            new_state.waste += new_state.parms.fee_per_input;
            let target = new_state.parms.total_target();
            let current_value = new_state.current_value;
            if current_value >= target {
                // we have a solution
                new_state.done = true;
                if current_value == target {
                    // perfect match, no better branch to search
                    done_results.push(new_state.clone());
                    continue;
                }
                let change_waste = self.parms.change_cost();
                if current_value <= target + change_waste{
                    // we have less change than the fee, so lets up the amount so we dont create a dust fee
                    // this is an edge case and should be avoided is possible. We should try and find a better solution
                    // here
                    let exstra_waste = new_state.current_value.saturating_sub(target); // we know its bigger than target
                    if exstra_waste >= new_state.parms.fee_per_input {
                        // it might be better to just add another input so lets try that
                        let mut not_done = new_state.clone();
                        not_done.done = false;
                        not_done_results.push(not_done);
                    }
                    // we update the send result to send more as its cheaper to send the extra bit then create change or include another input.
                    new_state.final_target = new_state.current_value;
                    // update the waste to include the extra we have to pay
                    new_state.waste += exstra_waste;
                } else {
                    // we have change, so lets count the change fee and future spend cost as waste
                    new_state.waste += new_state.parms.fee_per_input + new_state.parms.change_fee;
                }
                done_results.push(new_state.clone());
            };
        }
        (done_results, not_done_results)
    }
}

#[cfg(test)]
mod tests {
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
        assert!(sum == 110);
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
        let utxos = vec![MicroMinotari(60), MicroMinotari(40), MicroMinotari(50)];
        let params = section_params(100, 0, 10, 5, 10);
        let selector = BranchAndBoundUtxoSelector::new(utxos, params, default_params());
        let result = selector.search().unwrap();
        let sum: u64 = result.iter().map(|u| u.value().as_u64()).sum();
        assert!(sum == 110);
        // Should not use all three if two suffice
        assert!(result.len() == 2);
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
        assert!(sum == 101);
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
        let section = section_params(100, 0, 0, 0, 30);
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
        assert_eq!(sum,110);
    }
}
