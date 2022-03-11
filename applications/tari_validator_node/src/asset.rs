//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tari_dan_core::models::AssetDefinition;

#[derive(Debug)]
pub struct Asset {
    definition: AssetDefinition,
    current_state: bool,
    // Changes in the committee for this asset.
    // Mined height of the change TXs, and the involvment in the committe (true = part of committee)
    next_states: HashMap<u64, bool>,
    kill_signal: Option<Arc<Mutex<bool>>>,
}

impl Asset {
    pub fn new(definition: AssetDefinition) -> Self {
        Self {
            definition,
            current_state: false,
            next_states: HashMap::new(),
            kill_signal: None,
        }
    }

    pub fn update_height<Fstart>(&mut self, height: u64, start: Fstart)
    where Fstart: Fn(AssetDefinition) -> Arc<Mutex<bool>> {
        if let Some((&height, &involment)) = self
            .next_states
            .iter()
            .find(|(&mined_height, _)| mined_height <= height)
        {
            // State change
            if self.current_state != involment {
                if involment {
                    self.kill_signal = Some(start(self.definition.clone()));
                } else {
                    // Switch on the kill signal for the asset to end processing
                    let stop = self.kill_signal.clone().unwrap();
                    *stop.as_ref().lock().unwrap() = true;
                    self.kill_signal = None;
                }
            }
            self.current_state = involment;
            // We have the current state set and we will keep only future updates
            self.next_states
                .retain(|&effective_height, _| effective_height > height);
            // Monitor this asset if we are part of committee or there is a next state
        }
    }

    // If we are part of committe or there is a next state then monitor this asset
    pub fn should_monitor(&self) -> bool {
        self.current_state || !self.next_states.is_empty()
    }

    pub fn add_state(&mut self, height: u64, involment: bool) {
        self.next_states.insert(height, involment);
    }

    pub fn get_current_state(&self) -> bool {
        self.current_state
    }
}
