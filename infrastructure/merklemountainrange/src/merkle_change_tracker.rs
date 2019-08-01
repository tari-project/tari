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

use crate::{error::*, merkle_storage::*, merklenode::*};
use croaring::Treemap;
use serde::{de::DeserializeOwned, ser::Serialize};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use tari_utilities::hex::*;

/// This struct keeps track of the changes on the MMR
#[derive(Serialize, Deserialize)]
pub(crate) struct MerkleChangeTracker {
    pub enabled: bool,
    objects_to_save: Vec<ObjectHash>,
    objects_to_del: Vec<ObjectHash>,
    tree_saved: usize,           // how much of the mmr have saved to date in all CP
    pub pruning_horizon: usize,  // how many CP's do we keep before compressing and deleting
    current_head_horizon: usize, // how many cp's have you had to date, saved on disc
    pub current_horizon: usize,
    mmr_key: String,
    object_key: String,
    init_key: String,
    unsaved_checkpoints: Vec<MerkleCheckPoint>,
    uncleaned_checkpoints: Vec<CheckpointCleanup>,
    #[serde(with = "crate::treemap_ser::treemap_serialize")]
    unpruned_indices: Treemap,
    #[serde(with = "crate::treemap_ser::treemap_serialize")]
    pruned_indices: Treemap,
}

/// This struct is used as a temporary data struct summarizing all changes in a checkpoint.
/// It saves all the changes made to the MMR as a diff before dumping it to disc. This is done so we can iterate over
/// changes made to the MMR.
#[derive(Serialize, Deserialize)]
pub(crate) struct MerkleCheckPoint {
    objects_to_add: Vec<ObjectHash>,
    pub objects_to_del: Vec<ObjectHash>,
    mmr_to_add: Vec<MerkleNode>,
    pub tree_height_at_checkpoint: Vec<usize>,
    #[serde(with = "crate::treemap_ser::treemap_serialize")]
    unpruned_indices: Treemap,
    #[serde(with = "crate::treemap_ser::treemap_serialize")]
    pruned_indices: Treemap,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct CheckpointCleanup {
    pub objects_to_del: Vec<ObjectHash>,
    pub id: usize,
}

impl MerkleCheckPoint {
    pub(crate) fn add(&mut self, rhs: &mut MerkleCheckPoint) {
        self.objects_to_add.extend(rhs.objects_to_add.drain(..));
        self.objects_to_del = Vec::new();
        self.mmr_to_add.extend(rhs.mmr_to_add.drain(..));
        self.tree_height_at_checkpoint
            .extend(rhs.tree_height_at_checkpoint.drain(..));
        self.unpruned_indices.or_inplace(&rhs.unpruned_indices);
        self.pruned_indices.or_inplace(&rhs.pruned_indices);

        // The objects to be deleted will now be deleted without record of them as they are older than pruning horizon
        let mut i = rhs.objects_to_del.len();
        'outer: while i >= 1 {
            let mut k = self.objects_to_add.len();
            while k >= 1 {
                if self.objects_to_add[k - 1] == rhs.objects_to_del[i - 1] {
                    self.objects_to_add.remove(k - 1);
                    i -= 1;
                    continue 'outer; // found object, lets go look for next one
                }
                k -= 1;
            }
        }
    }

    pub(crate) fn create_cleanup(self, id: usize) -> CheckpointCleanup {
        CheckpointCleanup {
            objects_to_del: self.objects_to_add,
            id,
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
            current_head_horizon: 0,
            mmr_key: "".to_string(),
            object_key: "".to_string(),
            init_key: "".to_string(),
            unsaved_checkpoints: Vec::new(),
            uncleaned_checkpoints: Vec::new(),
            unpruned_indices: Treemap::create(),
            pruned_indices: Treemap::create(),
        }
    }

    /// initialise the change tracker
    pub fn init(&mut self, store_prefix: &str, pruning_horizon: usize) {
        self.enabled = true;
        self.mmr_key = (store_prefix.to_owned() + &"_mmr_checkpoints".to_string()).to_string();
        self.object_key = (store_prefix.to_owned() + &"_mmr_objects".to_string()).to_string();
        self.init_key = (store_prefix.to_owned() + &"_init".to_string()).to_string();
        self.pruning_horizon = pruning_horizon;
    }

    /// This function adds a ref to a object to be saved
    pub fn add_new_data(&mut self, hash: ObjectHash, leaf_index: usize) {
        if !self.enabled {
            return;
        }
        self.unpruned_indices.add(leaf_index as u64);
        self.objects_to_save.push(hash);
    }

    /// This function adds a ref to a object to be saved
    pub fn remove_data(&mut self, hash: ObjectHash, leaf_index: usize) {
        if !self.enabled {
            return;
        }
        self.pruned_indices.add(leaf_index as u64);
        self.objects_to_del.push(hash);
    }

    /// Function to save the current checkpoint
    /// The current checkpoint is only saved in memory until save is called to apply these changes
    pub fn checkpoint(&mut self, mmr: &[MerkleNode]) -> Result<(), MerkleStorageError> {
        if !self.enabled {
            return Ok(());
        }
        let mut checkpoint = MerkleCheckPoint {
            objects_to_add: Vec::new(),
            objects_to_del: Vec::new(),
            mmr_to_add: Vec::new(),
            tree_height_at_checkpoint: Vec::new(),
            pruned_indices: Treemap::create(),
            unpruned_indices: Treemap::create(),
        };

        checkpoint.objects_to_add.extend(self.objects_to_save.drain(..));
        checkpoint.objects_to_del.extend(self.objects_to_del.drain(..));
        checkpoint.tree_height_at_checkpoint.push(mmr.len());
        let mut counter = self.tree_saved;
        while counter < mmr.len() {
            checkpoint.mmr_to_add.push(mmr[counter].clone());
            counter += 1;
        }
        checkpoint.pruned_indices.or_inplace(&self.pruned_indices);
        checkpoint.unpruned_indices.or_inplace(&self.unpruned_indices);
        self.unsaved_checkpoints.push(checkpoint);

        self.tree_saved = counter;
        self.current_horizon += 1;
        self.pruned_indices = Treemap::create();
        self.unpruned_indices = Treemap::create();

        Ok(())
    }

    /// This function steps through all the checkpoints and looks at which len their mmr's ended.
    /// It will compare the given len and give age at which the len was exceeded.
    pub fn get_checkpoint_age_at_index<S: MerkleStorage>(&self, index: usize, store: &mut S) -> Result<usize, MerkleStorageError> {
        let mut age = 0;
        for i in (0..self.unsaved_checkpoints.len()).rev() {
            if len >= self.unsaved_checkpoints[i].tree_height_at_checkpoint[0]
            // these should only ever have 1
            {
                return Ok(age);
            }
            age += 1;
        }
        // its older than memory checkpoints or there is none, lets go to disc
        let amount_of_cps = store.load::<usize>(&("init").to_string(), &self.init_key)?;
        let first_cp = match amount_of_cps.checked_sub(self.pruning_horizon) {
            None => 1,
            Some(v) => v,
        };
        for i in (first_cp..=amount_of_cps).rev() {
            let cp = store.load::<MerkleCheckPoint>(&i.to_string(), &self.mmr_key)?;
            for k in (0..cp.tree_height_at_checkpoint.len()).rev() {
                if len >= cp.tree_height_at_checkpoint[k] {
                    return Ok(age);
                }
                age += 1;
            }
        }
        Ok(age)
    }

    /// This function will reset the MMR back to its head reverting all unchanged states=
    pub fn reset_to_head<T, S: MerkleStorage>(
        &mut self,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        mmr: &mut Vec<MerkleNode>,
        unpruned_indices: &mut Treemap,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: Serialize + DeserializeOwned,
    {
        // Todo investigate doing this without IO, currently this is a duplicate of load which is not ideal
        if !self.enabled {
            return Ok(());
        }
        hashmap.drain();
        mmr.drain(..);
        self.unsaved_checkpoints = Vec::new();
        self.uncleaned_checkpoints = Vec::new();
        self.pruned_indices = Treemap::create();
        self.unpruned_indices = Treemap::create();
        let amount_of_cps = store.load::<usize>(&("init").to_string(), &self.init_key)?;
        self.current_head_horizon = match amount_of_cps.checked_sub(self.pruning_horizon) {
            None => 0,
            Some(v) => v,
        };

        while self.current_head_horizon < amount_of_cps {
            self.current_head_horizon += 1;
            let mut cp = store.load::<MerkleCheckPoint>(&self.current_head_horizon.to_string(), &self.mmr_key)?;
            self.apply_cp(&mut cp, hashmap, mmr, unpruned_indices, store)?;
        }
        self.current_horizon = self.current_head_horizon;
        Ok(())
    }

    /// Function to save all unsaved changed to disc
    pub fn save<T, S: MerkleStorage>(
        &mut self,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: Serialize + DeserializeOwned,
    {
        if !self.enabled {
            return Ok(());
        }

        for i in 0..self.unsaved_checkpoints.len() {
            self.current_head_horizon += 1;
            let inc_index = self.current_head_horizon as i64 - self.pruning_horizon as i64;
            if inc_index > 0 {
                self.increase_pruning_horizon::<T, S>(inc_index as usize, store)?
            }
            self.save_single(i, hashmap, store)?;
            store.store(&("init").to_string(), &self.init_key, &self.current_head_horizon)?;
        }
        for i in 0..self.uncleaned_checkpoints.len() {
            self.cleanup_rewind(&(self.uncleaned_checkpoints[i]), store)?
        }
        store.commit()?;
        self.unsaved_checkpoints = Vec::new(); // clear out all unsaved changes
        self.uncleaned_checkpoints = Vec::new();
        self.current_horizon = self.current_head_horizon;

        Ok(())
    }

    /// Function to save a single checkpoint to disc
    fn save_single<T, S: MerkleStorage>(
        &self,
        checkpoint: usize,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: Serialize + DeserializeOwned,
    {
        for hash in &self.unsaved_checkpoints[checkpoint].objects_to_add {
            let object = hashmap.get(hash);
            if object.is_none() {
                return Err(MerkleStorageError::SyncError);
            }
            let object = object.unwrap();
            store.store(&to_hex(hash), &self.object_key, object)?;
        }
        store.store(
            &(self.current_head_horizon).to_string(),
            &self.mmr_key,
            &self.unsaved_checkpoints[checkpoint],
        )?;

        Ok(())
    }

    /// Function to load the checkpoint on pruning horizon and move that up
    fn increase_pruning_horizon<T, S: MerkleStorage>(
        &self,
        index: usize,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    {
        let mut cp = store.load::<MerkleCheckPoint>(&index.to_string(), &self.mmr_key)?;
        let mut cp2 = store.load::<MerkleCheckPoint>(&(index + 1).to_string(), &self.mmr_key)?;
        for hash in &cp2.objects_to_del {
            store.delete(&to_hex(hash), &self.object_key)?;
        }
        cp.add(&mut cp2);
        store.store(&(index + 1).to_string(), &self.mmr_key, &cp)?;
        store.delete(&(index).to_string(), &self.mmr_key)?;
        Ok(())
    }

    /// Function to load an mmr
    pub fn load<T, S: MerkleStorage>(
        &mut self,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        mmr: &mut Vec<MerkleNode>,
        unpruned_indices: &mut Treemap,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: Serialize + DeserializeOwned,
    {
        if !self.enabled {
            return Ok(());
        }
        hashmap.drain();
        mmr.drain(..);
        self.unsaved_checkpoints = Vec::new();
        self.uncleaned_checkpoints = Vec::new();
        let amount_of_cps = store.load::<usize>(&("init").to_string(), &self.init_key)?;
        self.current_head_horizon = match amount_of_cps.checked_sub(self.pruning_horizon) {
            None => 0,
            Some(v) => v,
        };

        while self.current_head_horizon < amount_of_cps {
            self.current_head_horizon += 1;
            let mut cp = store.load::<MerkleCheckPoint>(&self.current_head_horizon.to_string(), &self.mmr_key)?;
            self.apply_cp(&mut cp, hashmap, mmr, unpruned_indices, store)?;
        }
        self.current_horizon = self.current_head_horizon;
        Ok(())
    }

    /// Function to load an mmr
    pub fn rewind<T, S: MerkleStorage>(
        &mut self,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        mmr: &mut Vec<MerkleNode>,
        unpruned_indices: &mut Treemap,
        rewind_amount: usize,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: Serialize + DeserializeOwned,
    {
        if !self.enabled {
            return Ok(());
        }

        for _i in 0..rewind_amount {
            let mut cp = store.load::<MerkleCheckPoint>(&(self.current_horizon).to_string(), &self.mmr_key)?;
            self.apply_cp_reverse(&mut cp, hashmap, mmr, unpruned_indices, store)?;
            self.uncleaned_checkpoints.push(cp.create_cleanup(self.current_horizon));
            self.current_horizon -= 1;
        }
        Ok(())
    }

    fn apply_cp<T, S: MerkleStorage>(
        &self,
        checkpoint: &mut MerkleCheckPoint,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        mmr: &mut Vec<MerkleNode>,
        unpruned_indices: &mut Treemap,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: Serialize + DeserializeOwned,
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
            mmr[result.unwrap().vec_index].pruned = true;
        }
        unpruned_indices.or_inplace(&checkpoint.unpruned_indices);
        if !&checkpoint.pruned_indices.is_empty() {
            unpruned_indices.andnot_inplace(&checkpoint.pruned_indices);
        }
        Ok(())
    }

    fn apply_cp_reverse<T, S: MerkleStorage>(
        &self,
        checkpoint: &mut MerkleCheckPoint,
        hashmap: &mut HashMap<ObjectHash, MerkleObject<T>>,
        mmr: &mut Vec<MerkleNode>,
        unpruned_indices: &mut Treemap,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    where
        T: Serialize + DeserializeOwned,
    {
        for hash in &checkpoint.objects_to_add {
            let result = hashmap.remove(hash);
            if result.is_none() {
                return Err(MerkleStorageError::SyncError);
            }
        }

        for hash in &checkpoint.objects_to_del {
            let object = store.load::<MerkleObject<T>>(&to_hex(hash), &self.object_key)?;
            mmr[object.vec_index].pruned = false;
            hashmap.insert(hash.clone(), object);
        }
        mmr.drain((mmr.len() - checkpoint.mmr_to_add.len())..);
        if !&checkpoint.unpruned_indices.is_empty() {
            unpruned_indices.andnot_inplace(&checkpoint.unpruned_indices);
        }
        unpruned_indices.or_inplace(&checkpoint.pruned_indices);
        Ok(())
    }

    fn cleanup_rewind<S: MerkleStorage>(
        &self,
        checkpoint: &CheckpointCleanup,
        store: &mut S,
    ) -> Result<(), MerkleStorageError>
    {
        for hash in &checkpoint.objects_to_del {
            store.delete(&to_hex(hash), &self.object_key)?;
        }
        store.delete(&(checkpoint.id).to_string(), &self.object_key)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::mmr::*;
    use blake2::Blake2b;
    use digest::Digest;
    use serde_derive::{Deserialize, Serialize};
    use std::fs;
    use tari_storage::lmdb::*;
    use tari_utilities::Hashable;

    #[derive(Serialize, Deserialize)]
    pub struct IWrapper(u32);

    impl Hashable for IWrapper {
        fn hash(&self) -> Vec<u8> {
            Blake2b::new().chain(self.0.to_le_bytes()).result().to_vec()
        }
    }

    fn create_mmr(leaves: u32) -> MerkleMountainRange<IWrapper, Blake2b> {
        let mut mmr: MerkleMountainRange<IWrapper, Blake2b> = MerkleMountainRange::new();
        mmr.init_persistance_store(&"mmr".to_string(), 5);
        for i in 1..leaves + 1 {
            let object: IWrapper = IWrapper(i);
            assert!(mmr.push(object).is_ok());
        }
        mmr
    }

    #[test]
    fn create_med_mmr() {
        let _res = fs::remove_dir_all("./tests/test_mmr_cm"); // we ensure that the test dir is empty
        let mut mmr = create_mmr(14);
        // create storage
        fs::create_dir("./tests/test_mmr_cm").unwrap();
        let builder = LMDBBuilder::new();
        let mut store = builder
            .set_mapsize(5)
            .set_path("./tests/test_mmr_cm/")
            .add_database(&"mmr_mmr_checkpoints".to_string())
            .add_database(&"mmr_mmr_objects".to_string())
            .add_database(&"mmr_init".to_string())
            .build()
            .unwrap();
        assert_eq!(mmr.checkpoint().is_ok(), true);
        assert_eq!(mmr.apply_state(&mut store).is_ok(), true);
        let mut mmr2: MerkleMountainRange<IWrapper, Blake2b> = MerkleMountainRange::new();
        mmr2.init_persistance_store(&"mmr".to_string(), 5);
        assert_eq!(mmr2.load_from_store(&mut store).is_ok(), true);
        assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
        assert_eq!(mmr.get_unpruned_hash(), mmr2.get_unpruned_hash());

        // add more leaves
        for i in 15..25 {
            let object: IWrapper = IWrapper(i);
            assert!(mmr.push(object).is_ok());
            assert_eq!(mmr.change_tracker.objects_to_save.len() > 0, true);
            assert_eq!(mmr.checkpoint().is_ok(), true);
            assert_eq!(mmr.apply_state(&mut store).is_ok(), true);
            assert_eq!(mmr.change_tracker.objects_to_save.len() == 0, true);
            let mut mmr2: MerkleMountainRange<IWrapper, Blake2b> = MerkleMountainRange::new();
            mmr2.init_persistance_store(&"mmr".to_string(), 5);
            assert_eq!(mmr2.load_from_store(&mut store).is_ok(), true);
            assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
            assert_eq!(mmr.get_unpruned_hash(), mmr2.get_unpruned_hash());
        }

        // add much more leafs
        for i in 26..50 {
            let object: IWrapper = IWrapper(i);
            assert!(mmr.push(object).is_ok());
            let object_delete = IWrapper(i - 25);
            assert_eq!(mmr.prune_object_hash(&object_delete.hash()).is_ok(), true);
            assert!(mmr.change_tracker.objects_to_save.len() > 0);
            assert_eq!(mmr.change_tracker.objects_to_del.len() > 0, true);
            assert_eq!(mmr.checkpoint().is_ok(), true);
            assert_eq!(mmr.apply_state(&mut store).is_ok(), true);
            assert_eq!(mmr.change_tracker.objects_to_save.len() == 0, true);
            assert_eq!(mmr.change_tracker.objects_to_del.len() == 0, true);
            let mut mmr2: MerkleMountainRange<IWrapper, Blake2b> = MerkleMountainRange::new();
            mmr2.init_persistance_store(&"mmr".to_string(), 5);
            assert_eq!(mmr2.load_from_store(&mut store).is_ok(), true);
            assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
            assert_eq!(mmr.get_unpruned_hash(), mmr2.get_unpruned_hash());
        }

        // try and find old deleted objects
        for i in 1..11 {
            let object: IWrapper = IWrapper(i);
            assert_eq!(
                store
                    .load::<MerkleObject<IWrapper>>(&to_hex(&object.hash()), &"mmr_mmr_objects".to_string())
                    .is_ok(),
                false
            );
        }
        assert!(fs::remove_dir_all("./tests/test_mmr_cm").is_ok()); // we ensure that the test dir is empty
    }

    #[test]
    fn mmr_age() {
        let _res = fs::remove_dir_all("./tests/test_mmr_age"); // we ensure that the test dir is empty
        let mut mmr = create_mmr(1);
        // create storage
        fs::create_dir("./tests/test_mmr_age").unwrap();
        let builder = LMDBBuilder::new();
        let mut store = builder
            .set_mapsize(5)
            .set_path("./tests/test_mmr_age/")
            .add_database(&"mmr_mmr_checkpoints".to_string())
            .add_database(&"mmr_mmr_objects".to_string())
            .add_database(&"mmr_init".to_string())
            .build()
            .unwrap();
        assert_eq!(mmr.checkpoint().is_ok(), true);
        assert_eq!(mmr.apply_state(&mut store).is_ok(), true);

        // add more leaves
        for i in 2..50 {
            let object: IWrapper = IWrapper(i);
            let hash = object.hash();
            assert!(mmr.push(object).is_ok());
            assert_eq!(mmr.checkpoint().is_ok(), true);
            assert_eq!(mmr.apply_state(&mut store).is_ok(), true);
            let result = mmr.get_object_checkpoint_age(&hash, &mut store);
            assert_eq!(result.unwrap(), 1);
        }
        // lets look at old objects
        let mut age = 1;
        for i in (2..50).rev() {
            let object: IWrapper = IWrapper(i);
            let hash = object.hash();
            let result = mmr.get_object_checkpoint_age(&hash, &mut store);
            assert_eq!(result.unwrap(), age);
            age += 1;
        }
        assert!(fs::remove_dir_all("./tests/test_mmr_age").is_ok()); // we ensure that the test dir is empty
    }

    #[test]
    fn mmr_age_mem_only() {
        let _res = fs::remove_dir_all("./tests/test_mmr_age_m"); // we ensure that the test dir is empty
        let mut mmr = create_mmr(1);
        // create storage
        fs::create_dir("./tests/test_mmr_age_m").unwrap();
        let builder = LMDBBuilder::new();
        let mut store = builder
            .set_mapsize(5)
            .set_path("./tests/test_mmr_age_m/")
            .add_database(&"mmr_mmr_checkpoints".to_string())
            .add_database(&"mmr_mmr_objects".to_string())
            .add_database(&"mmr_init".to_string())
            .build()
            .unwrap();
        assert_eq!(mmr.checkpoint().is_ok(), true);

        // add more leaves
        for i in 2..50 {
            let object: IWrapper = IWrapper(i);
            let hash = object.hash();
            assert!(mmr.push(object).is_ok());
            assert_eq!(mmr.checkpoint().is_ok(), true);
            let result = mmr.get_object_checkpoint_age(&hash, &mut store);
            assert_eq!(result.unwrap(), 1);
        }
        assert!(fs::remove_dir_all("./tests/test_mmr_age_m").is_ok()); // we ensure that the test dir is empty
    }
}
