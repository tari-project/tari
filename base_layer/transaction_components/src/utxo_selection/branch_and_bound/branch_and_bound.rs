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

        }

        done_results.sort_by(|a, b| a.waste.cmp(&b.waste));
        let best_result = done_results.first().expect("done_results should not be empty");
        // collect the selected utxos
        let selected_utxos: Vec<T> = best_result.selected_utxos.iter().map(|&i| {
            self.available_utxos.get(i).expect("utxo_index out of bounds").clone()
        }).collect();
        Ok(selected_utxos)
        // Ok(Vec::new())
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
            if new_state.current_value > target {
                // we have a solution
                new_state.done = true;
                if new_state.current_value == target {
                    // perfect match, no better branch to search
                    done_results.push(new_state.clone());
                    continue;
                }
                let change_waste = self.parms.change_cost();
                if new_state.current_value <= target + change_waste{
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
                    new_state.waste += change_waste;
                }
                done_results.push(new_state.clone());
            };
        }
        (done_results, not_done_results)
    }
}
