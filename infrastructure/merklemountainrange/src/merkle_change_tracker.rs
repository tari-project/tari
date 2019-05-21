// Copyright 2019 The Tari Project
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

use crate::{merkle_storage::*, merklenode::*};
use serde::{de::DeserializeOwned, Serialize};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use tari_utilities::hex::*;

/// This struct keeps track of the changes on the MMR
pub(crate) struct MerkleChangeTracker {
    pub enabled: bool,
    objects_to_save: Vec<ObjectHash>,
    objects_to_del: Vec<ObjectHash>,
    tree_saved: usize,      // how much of the mmr have saved to date in all CP
    pruning_horizon: usize, // how many CP's do we keep before compressing and deleting
    current_horizon: usize, // how many cp's have you had to date
    mmr_key: String,
    object_key: String,
    init_key: String,
}

/// This struct is used as a data struct summarizing all changes in a checkpoint.
#[derive(Serialize, Deserialize)]
pub(crate) struct MerkleCheckPoint {
    objects_to_add: Vec<ObjectHash>,
    pub objects_to_del: Vec<ObjectHash>,
    mmr_to_add: Vec<MerkleNode>,
}
impl MerkleCheckPoint {
    pub(crate) fn add(&mut self, rhs: &mut MerkleCheckPoint) {
        self.objects_to_add.extend(rhs.objects_to_add.drain(..));
        self.objects_to_del = Vec::new();
        self.mmr_to_add.extend(rhs.mmr_to_add.drain(..));

        // The objects to be deleted will now be deleted without record of them as they are older than pruning horizon
        let mut i = rhs.objects_to_del.len();
        while i >= 1 {
            let k = self.objects_to_add.len();
            if self.objects_to_add[k - 1] == rhs.objects_to_del[i - 1] {
                self.objects_to_add.remove(i - 1);
                i -= 1;
                continue;
            }
        }
    }
}

impl MerkleChangeTracker {
    /// create a new change tracker
    pub fn new() -> MerkleChangeTracker {
        MerkleChangeTracker {
            enabled: false,
            objects_to_save: Vec::new(),
            objects_to_del: Vec::new(),
            tree_saved: 0,
            pruning_horizon: 0,
            current_horizon: 0,
            mmr_key: "".to_string(),
            object_key: "".to_string(),
            init_key: "".to_string(),
        }
    }

    /// initialise the change tracker
    pub fn init(&mut self, store_prefix: &str, pruning_horizon: usize) {
        self.enabled = true;
        self.mmr_key = (store_prefix.clone().to_owned() + &"_mmr_checkpoints".to_string()).to_string();
        self.object_key = (store_prefix.clone().to_owned() + &"_mmr_objects".to_string()).to_string();
        self.init_key = (store_prefix.clone().to_owned() + &"_init".to_string()).to_string();
        self.pruning_horizon = pruning_horizon;
    }

    /// This function adds a ref to a object to be saved
    pub fn add_new_data(&mut self, hash: &ObjectHash) {
        if !self.enabled {
            return;
        }
        self.objects_to_save.push(hash.clone());
    }

    /// This function adds a ref to a object to be saved
    pub fn remove_data(&mut self, hash: &ObjectHash) {
        if !self.enabled {
            return;
        }
        self.objects_to_del.push(hash.clone());
    }

    /// Function to save the current checkpoint
    pub fn save<T, S: MerkleStorage>(
        &mut self,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        mmr: &Vec<MerkleNode>,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: Serialize,
    {
        if !self.enabled {
            return Ok(());
        }
        self.current_horizon += 1;
        let inc_index = self.current_horizon - self.pruning_horizon;
        if inc_index > 0 {
            self.inc_pruning_hor(inc_index, hashmap, store)?
        }

        let mut checkpoint = MerkleCheckPoint {
            objects_to_add: Vec::new(),
            objects_to_del: Vec::new(),
            mmr_to_add: Vec::new(),
        };
        checkpoint.objects_to_add.extend(self.objects_to_save.drain(..));
        for hash in &checkpoint.objects_to_add {
            let object = hashmap.get(hash);
            if object.is_none() {
                return Err(MerkleStorageError::SyncError);
            }
            let object = object.unwrap();
            let _result = store.store(&to_hex(hash), &self.object_key, object)?;
        }
        checkpoint.objects_to_del.extend(self.objects_to_del.drain(..));
        let mut counter = self.tree_saved + 1;
        while counter < mmr.len() {
            checkpoint.mmr_to_add.push(mmr[counter].clone());
            counter += 1;
        }

        let _result = store.store(&(self.current_horizon).to_string(), &self.mmr_key, &checkpoint)?;
        let _result = store.store(
            &(self.current_horizon).to_string(),
            &self.init_key,
            &self.current_horizon,
        )?;
        self.tree_saved = counter - 1;

        Ok(())
    }

    /// Function to load the checkpoint on pruning horizon and move that up
    fn inc_pruning_hor<T, S: MerkleStorage>(
        &self,
        index: usize,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    {
        let mut cp = store.load::<MerkleCheckPoint>(&index.to_string(), &self.mmr_key)?;
        let mut cp2 = store.load::<MerkleCheckPoint>(&(index + 1).to_string(), &self.mmr_key)?;
        for hash in &cp2.objects_to_del {
            let result = hashmap.remove(hash);
            if result.is_none() {
                return Err(MerkleStorageError::SyncError);
            }
        }
        cp.add(&mut cp2);
        let _result = store.store(&(index + 1).to_string(), &self.mmr_key, &cp)?;
        let _result = store.delete(&(index).to_string(), &self.mmr_key)?;

        Ok(())
    }

    /// Function to load a checkpoint
    pub fn load<T, S: MerkleStorage>(
        &mut self,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        mmr: &mut Vec<MerkleNode>,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: DeserializeOwned,
    {
        if !self.enabled {
            return Ok(());
        }

        let amount_of_cps = store.load::<usize>(&(self.current_horizon).to_string(), &self.init_key)?;
        self.current_horizon = if (amount_of_cps as i64 - self.pruning_horizon as i64) >= 0 {
            amount_of_cps - self.pruning_horizon
        } else {
            0
        };

        while self.current_horizon <= amount_of_cps {
            let mut cp = store.load::<MerkleCheckPoint>(&self.current_horizon.to_string(), &self.mmr_key)?;
            self.apply_cp(&mut cp, hashmap, mmr, store)?
        }
        Ok(())
    }

    fn apply_cp<T, S: MerkleStorage>(
        &self,
        checkpoint: &mut MerkleCheckPoint,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        mmr: &mut Vec<MerkleNode>,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: DeserializeOwned,
    {
        mmr.extend(checkpoint.mmr_to_add.drain(..));
        for hash in &checkpoint.objects_to_add {
            let object = store.load::<MerkleObject<T>>(&to_hex(hash), &self.object_key)?;
            hashmap.insert(hash.clone(), object);
        }
        for hash in &checkpoint.objects_to_del {
            let result = hashmap.remove(hash);
            if result.is_none() {
                return Err(MerkleStorageError::SyncError);
            }
        }
        Ok(())
    }
}
