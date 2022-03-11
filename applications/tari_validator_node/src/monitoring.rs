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

use tari_common_types::types::PublicKey;
use tari_dan_core::models::AssetDefinition;

use crate::asset::Asset;

#[derive(Debug)]
pub struct Monitoring {
    committee_management_confirmation_time: u64,
    assets: HashMap<PublicKey, Asset>,
}

impl Monitoring {
    pub fn new(committee_management_confirmation_time: u64) -> Self {
        Self {
            committee_management_confirmation_time,
            assets: HashMap::new(),
        }
    }

    pub fn add_if_unmonitored(&mut self, asset: AssetDefinition) {
        if !self.assets.contains_key(&asset.public_key) {
            self.assets.insert(asset.public_key.clone(), Asset::new(asset.clone()));
        }
    }

    pub fn add_state(&mut self, asset_public_key: PublicKey, height: u64, involment: bool) {
        // Add committee_management_confirmation_time to the mined height = effective height
        self.assets
            .get_mut(&asset_public_key)
            .unwrap()
            .add_state(height + self.committee_management_confirmation_time, involment);
    }

    pub fn update_height<Fstart: Clone>(&mut self, height: u64, start: Fstart)
    where Fstart: Fn(AssetDefinition) -> Arc<Mutex<bool>> {
        for (_, proc) in &mut self.assets {
            proc.update_height(height, start.clone());
        }
        self.assets.retain(|_, proc| proc.should_monitor())
    }

    // Get active public keys, so we can check if we are still part of the committee
    pub fn get_active_public_keys(&self) -> Vec<&PublicKey> {
        self.assets
            .keys()
            .filter(|&a| self.assets.get(a).unwrap().get_current_state())
            .collect()
    }
}
