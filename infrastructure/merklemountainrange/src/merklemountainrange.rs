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

use crate::{
    error::MerkleMountainRangeError,
    merkle_change_tracker::MerkleChangeTracker,
    merkle_storage::*,
    merklenode::*,
    merkleproof::MerkleProof,
};
use digest::Digest;
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::HashMap, marker::PhantomData};
use tari_utilities::Hashable;
pub struct MerkleMountainRange<T, D> {
    // todo convert these to a bitmap
    mmr: Vec<MerkleNode>,
    data: HashMap<ObjectHash, MerkleObject<T>>,
    hasher: PhantomData<D>,
    current_peak_height: (usize, usize), // we store a tuple of peak height,index
    change_tracker: MerkleChangeTracker,
}

impl<T, D> MerkleMountainRange<T, D>
where
    T: Hashable + Serialize + DeserializeOwned,
    D: Digest,
{
    /// This function creates a new empty Merkle Mountain Range
    pub fn new() -> MerkleMountainRange<T, D> {
        MerkleMountainRange {
            mmr: Vec::new(),
            data: HashMap::new(),
            hasher: PhantomData,
            current_peak_height: (0, 0),
            change_tracker: MerkleChangeTracker::new(),
        }
    }

    /// This allows the DB to store its data on a persistent medium using the tari::keyvalue_store trait
    /// store_prefix is the db file name prefix used for this mmr.
    /// pruning horizon is how far back changes are kept so that it can rewind.
    pub fn init_persistance_store(&mut self, store_prefix: &str, pruning_horizon: usize) {
        self.change_tracker.init(store_prefix, pruning_horizon)
    }

    pub(crate) fn get_data_object(&self, hash: ObjectHash) -> Option<&MerkleObject<T>> {
        self.data.get(&hash)
    }

    /// This function returns a reference to the data stored in the mmr
    /// It will return none if the hash does not exist
    pub fn get_object(&self, hash: &ObjectHash) -> Option<&T> {
        let object = self.data.get(hash);
        if object.is_none() {
            return None;
        };
        Some(&object.unwrap().object)
    }

    /// This function returns a reference to the data stored in the mmr
    /// It will return an error if the index is out of bounds or the index is not a leaf
    pub fn get_object_by_object_index(&self, object_index: usize) -> Result<&T, MerkleMountainRangeError> {
        if object_index > self.data.len() {
            return Err(MerkleMountainRangeError::IndexOutOfBounds);
        }
        let index = get_object_index(object_index);
        let hash = &self.mmr[index].hash;
        let data = self.get_object(hash);
        match data {
            Some(value) => Ok(value),
            None => Err(MerkleMountainRangeError::ObjectNotFound),
        }
    }

    /// This function returns the length of objects stored in the MMR
    /// It does not return the total number of nodes
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// This function returns the hash of the node index provided, this counts from 0
    pub fn get_node_hash(&self, node_index: usize) -> Option<ObjectHash> {
        if node_index > self.get_last_added_index() {
            return None;
        };
        Some(self.mmr[node_index].hash.clone())
    }

    /// This function returns the hash of the leaf index provided, this counts from 0
    pub fn get_object_hash(&self, object_index: usize) -> Option<ObjectHash> {
        self.get_node_hash(get_object_index(object_index))
    }

    /// This function returns a MerkleProof of the provided index
    pub fn get_object_index_proof(&self, index: usize) -> MerkleProof {
        let mmr_index = get_object_index(index);
        if mmr_index >= self.mmr.len() {
            return MerkleProof::new();
        }
        self.get_proof(mmr_index)
    }

    /// This function returns a MerkleProof of the provided index
    pub fn get_hash_proof(&self, hash: &ObjectHash) -> MerkleProof {
        let mut i = self.mmr.len();
        for counter in 0..self.mmr.len() {
            if self.mmr[counter].hash == *hash {
                i = counter;
                break;
            }
        }
        if i == self.mmr.len() {
            return MerkleProof::new();
        };
        self.get_proof(i)
    }

    // This is the internal function given the correct mmr index
    fn get_proof(&self, i: usize) -> MerkleProof {
        let mut result = MerkleProof::new();
        self.get_ordered_hash_proof(i, true, &mut result);

        if self.current_peak_height.1 == self.get_last_added_index() {
            // we know there is no bagging as the mmr is a balanced binary tree
            return result;
        }

        let mut peaks = self.bag_mmr();
        let mut i = peaks.len();
        let mut was_on_correct_peak = false;

        let mut hasher = D::new();
        let _cur_proof_len = result.len();
        while i > 1 {
            // was_on_correct_height is used to track should we add from this point onwards both left and right
            // siblings. This loop tracks from bottom of the peaks, so we keep going up until we hit a known
            // point, we then add the missing sibling from that point
            let cur_proof_len = result.len();
            if was_on_correct_peak {
                result.push(Some(peaks[i - 2].clone()));
                result.push(None);
            } else if peaks[i - 1] == result[cur_proof_len - 1].clone().unwrap() {
                result.insert(result.len() - 1, Some(peaks[i - 2].clone()));
                if cur_proof_len > 2 {
                    result[cur_proof_len - 1] = None; // this is a calculated result, so we can remove this, we only remove if there was more than 2
                                                      // values
                }
                was_on_correct_peak = true;
            } else if peaks[i - 2] == result[cur_proof_len - 1].clone().unwrap() {
                if cur_proof_len > 2 {
                    result[cur_proof_len - 1] = None; // this is a calculated result, so we can remove this, we only remove if there was more than 2
                                                      // values
                }
                result.push(Some(peaks[i - 1].clone()));
                was_on_correct_peak = true;
            }

            hasher.input(&peaks[i - 2]);
            hasher.input(&peaks[i - 1]);
            peaks[i - 2] = hasher.result_reset().to_vec();
            i -= 1;
        }
        // lets calculate the final new peak
        hasher.input(&self.mmr[self.current_peak_height.1].hash);
        hasher.input(&peaks[0]);
        if was_on_correct_peak {
            // we where not in the main peak, so add main peak
            result.push(Some(self.mmr[self.current_peak_height.1].hash.clone()));
            result.push(None);
        } else {
            if result[result.len() - 1].clone().unwrap() == peaks[0] {
                let cur_proof_len = result.len();
                result[cur_proof_len - 1] = Some(self.mmr[self.current_peak_height.1].hash.clone());
                result.push(None);
            } else {
                let cur_proof_len = result.len();
                result[cur_proof_len - 1] = None; // this is a calculated result, so we can remove this, we have come from the main peak
                result.push(Some(peaks[0].clone()));
            }
        }
        result.push(Some(hasher.result_reset().to_vec()));

        result
    }

    // This function is an iterative function. It will add the left node first then the right node to the provided array
    // on the index. It will return when it reaches a single highest point.
    // this function will return the index of the local peak, negating the need to search for it again.
    fn get_ordered_hash_proof(&self, index: usize, is_first_run: bool, results: &mut MerkleProof) {
        let sibling = sibling_index(index);
        let mut next_index = index + 1;
        if sibling >= self.mmr.len() {
            // we are at a peak
            results.push(Some(self.mmr[index].hash.clone()));
            return;
        }
        // we check first run, as we need to store both children, after that we only need to store one child (the one
        // not a parent)
        if sibling < index {
            results.push(Some(self.mmr[sibling].hash.clone()));
            if !is_first_run {
                results.push(None) // index can be calculated
            } else {
                results.push(Some(self.mmr[index].hash.clone()));
            }
        } else {
            if !is_first_run {
                results.push(None) // index can be calculated
            } else {
                results.push(Some(self.mmr[index].hash.clone()));
            }
            results.push(Some(self.mmr[sibling].hash.clone()));
            next_index = sibling + 1;
        }
        self.get_ordered_hash_proof(next_index, false, results);
    }

    /// This function will verify the provided proof. Internally it uses the get_hash_proof function to construct a
    /// similar proof. This function will return true if the proof is valid
    /// If the order does not match Lchild-Rchild-parent(Lchild)-Rchild-parent-.. the validation will fail
    /// This function will only succeed if the given hash is of height 0
    pub fn verify_proof(&self, proof: &MerkleProof) -> bool {
        if proof.len() == 0 {
            return false;
        }
        if proof[0].is_none() {
            return false;
        }
        let mut our_proof = self.get_hash_proof(&proof[0].clone().unwrap());
        our_proof.compare::<D>(&proof)
    }

    // This function calculates the peak height of the mmr
    fn calc_peak_height(&self) -> (usize, usize) {
        let mut height_counter = 0;
        let mmr_len = self.get_last_added_index();
        let mut index: usize = (1 << height_counter + 2) - 2;
        let mut actual_height_index = 0;
        while mmr_len >= index {
            // find the height of the tree by finding if we can subtract the  height +1
            height_counter += 1;
            actual_height_index = index;
            index = (1 << height_counter + 2) - 2;
        }
        (height_counter, actual_height_index)
    }

    /// This function returns the peak height of the mmr
    pub fn get_peak_height(&self) -> usize {
        self.current_peak_height.0
    }

    /// This function will return the single merkle root of the MMR.
    pub fn get_merkle_root(&self) -> ObjectHash {
        let mut peaks = self.bag_mmr();
        let mut i = peaks.len();
        while i > 1 {
            // lets bag all the other peaks
            let mut hasher = D::new();
            hasher.input(&peaks[i - 2]);
            hasher.input(&peaks[i - 1]);
            peaks[i - 2] = hasher.result().to_vec();
            i -= 1;
        }
        if peaks.len() > 0 {
            // if there was other peaks, lets bag them with the highest peak
            let mut hasher = D::new();
            hasher.input(&self.mmr[self.current_peak_height.1].hash);
            hasher.input(&peaks[0]);
            return hasher.result().to_vec();
        }
        // there was no other peaks, return the highest peak
        return self.mmr[self.current_peak_height.1].hash.clone();
    }

    /// This function adds a vec of leaf nodes to the mmr.
    pub fn append(&mut self, objects: Vec<T>) {
        for object in objects {
            self.push(object);
        }
    }

    /// This function applies all changes to disc
    pub fn apply_checkpoint<S: MerkleStorage>(&mut self, store: &mut S) -> Result<(), MerkleStorageError> {
        self.change_tracker.save(&mut self.data, &mut self.mmr, store)
    }

    /// This function applies all changes to disc
    pub fn load_from_store<S: MerkleStorage>(&mut self, store: &mut S) -> Result<(), MerkleStorageError> {
        self.change_tracker.load(&mut self.data, &mut self.mmr, store)?;
        self.current_peak_height = self.calc_peak_height(); // calculate cached height after loading in data
        Ok(())
    }

    /// This function adds a new leaf node to the mmr.
    pub fn push(&mut self, object: T) {
        let node_hash = object.hash();
        let node = MerkleObject::new(object, self.mmr.len());
        self.data.insert(node_hash.clone(), node);
        self.change_tracker.add_new_data(node_hash.clone());
        self.mmr.push(MerkleNode::new(node_hash));
        if is_node_right(self.get_last_added_index()) {
            self.add_single_no_leaf(self.get_last_added_index())
        }
    }

    // This function adds non leaf nodes, eg nodes that are not directly a hash of data
    // This is iterative and will continue to up and till it hits the top, will be a future left child
    fn add_single_no_leaf(&mut self, index: usize) {
        let mut hasher = D::new();
        hasher.input(&self.mmr[sibling_index(index)].hash);
        hasher.input(&self.mmr[index].hash);
        let new_hash = hasher.result().to_vec();
        self.mmr.push(MerkleNode::new(new_hash));
        if is_node_right(self.get_last_added_index()) {
            self.add_single_no_leaf(self.get_last_added_index())
        } else {
            self.current_peak_height = self.calc_peak_height(); // because we have now stopped adding right nodes, we need to update the height of the mmr
        }
    }

    // This function is just a private function to return the index of the last added node
    fn get_last_added_index(&self) -> usize {
        self.mmr.len() - 1
    }

    // This function does not include the current largest peak
    fn bag_mmr(&self) -> Vec<ObjectHash> {
        // lets find all peaks of the mmr
        let mut peaks = Vec::new();
        self.find_bagging_indexes(
            self.current_peak_height.0 as i64,
            self.current_peak_height.1,
            &mut peaks,
        );
        peaks
    }

    fn find_bagging_indexes(&self, mut height: i64, index: usize, peaks: &mut Vec<ObjectHash>) {
        let mut new_index = index + (1 << height + 1) - 1; // go the potential right sibling
        while (new_index > self.get_last_added_index()) && (height > 0) {
            // lets go down left child till we hit a valid node or we reach height 0
            new_index = new_index - (1 << height);
            height -= 1;
        }
        if (new_index <= self.get_last_added_index()) && (height >= 0) {
            // is this a valid peak which needs to be bagged
            peaks.push(self.mmr[new_index].hash.clone());
            self.find_bagging_indexes(height, new_index, peaks); // lets go look for more peaks
        }
    }

    /// Mark an object as pruned, if the MMR can remove this safely it will
    pub fn prune_object_hash(&mut self, hash: &ObjectHash) -> Result<(), MerkleMountainRangeError> {
        let object = self.data.get_mut(hash);
        if object.is_none() {
            return Err(MerkleMountainRangeError::ObjectNotFound);
        };
        let object = object.unwrap();
        self.mmr[object.vec_index].pruned = true;

        self.data.remove(hash);
        if self.change_tracker.enabled {
            self.change_tracker.remove_data(hash.clone());
        };
        Ok(())
    }

    /// Mark an object as pruned, if the MMR can remove this safely it will
    pub fn prune_index(&mut self, node_index: usize) -> Result<(), MerkleMountainRangeError> {
        if node_index > self.data.len() {
            return Err(MerkleMountainRangeError::IndexOutOfBounds);
        }
        let index = get_object_index(node_index);
        let hash = self.mmr[index].hash.clone();
        self.prune_object_hash(&hash)
    }
}
/// This function takes in the index and calculates the index of the sibling.
pub fn sibling_index(node_index: usize) -> usize {
    let height = get_node_height(node_index);
    let index_count = (1 << height + 1) - 1;
    if is_node_right(node_index) {
        node_index - index_count
    } else {
        node_index + index_count
    }
}

impl<T, D> From<Vec<T>> for MerkleMountainRange<T, D>
where
    T: Hashable + Serialize + DeserializeOwned,
    D: Digest,
{
    fn from(items: Vec<T>) -> Self {
        let mut mmr = MerkleMountainRange {
            mmr: Vec::new(),
            data: HashMap::new(),
            hasher: PhantomData,
            current_peak_height: (0, 0),
            change_tracker: MerkleChangeTracker::new(),
        };
        mmr.append(items);
        mmr
    }
}

/// This function takes in the index and calculates if the node is the right child node or not.
/// If the node is the tree root it will still give the answer as if it is a child of a node.
/// This function is an iterative function as we might have to subtract the largest left_most tree.
pub fn is_node_right(node_index: usize) -> bool {
    let mut height_counter = 0;
    while node_index >= ((1 << height_counter + 2) - 2) {
        // find the height of the tree by finding if we can subtract the  height +1
        height_counter += 1;
    }
    let height_index = (1 << height_counter + 1) - 2;
    if node_index == height_index {
        // If this is the first peak then subtracting the height of first peak will be 0
        return false;
    };
    if node_index == (height_index + ((1 << height_counter + 1) - 1)) {
        // we are looking if its the right sibling
        return true;
    };
    // if we are here means it was not a right node at height counter, we therefor search lower
    let new_index = node_index - height_index - 1;
    is_node_right(new_index)
}

/// This function takes in the index and calculates the height of the node
/// This function is an iterative function as we might have to subtract the largest left_most tree.
pub fn get_node_height(node_index: usize) -> usize {
    let mut height_counter = 0;
    while node_index >= ((1 << height_counter + 2) - 2) {
        // find the height of the tree by finding if we can subtract the  height +1
        height_counter += 1;
    }
    let height_index = (1 << height_counter + 1) - 2;
    if node_index == height_index {
        // If this is the first peak then subtracting the height of first peak will be 0
        return height_counter;
    };
    if node_index == (height_index + ((1 << height_counter + 1) - 1)) {
        // we are looking if its the right sibling
        return height_counter;
    };
    // if we are here means it was not a right node at height counter, we therefor search lower
    let new_index = node_index - height_index - 1;
    get_node_height(new_index)
}

/// This function will convert the given index and get its location in the MMR, this only works for leaf nodes
pub fn get_object_index(node_index: usize) -> usize {
    let offset = calculate_leaf_index_offset(node_index, 0);
    (node_index + offset)
}

// This is the iterative companion function to get_leaf_index and this will search the tree for the correct height
fn calculate_leaf_index_offset(index: usize, offset: usize) -> usize {
    let mut height_counter = 0;
    while index * 2 > ((1 << height_counter + 2) - 2) {
        // find the height of the tree by finding if we can subtract the  height +1
        height_counter += 1;
    }
    let height_index = (1 << height_counter + 1) - 2;
    if index == 0 {
        // If this is the first peak then subtracting the height of first peak will be 0
        return offset;
    };
    if index == 1 {
        // we are looking if its the right sibling
        return offset;
    };
    // if we are here means it was not a right node at height counter, we therefor search lower
    let new_offset = offset + (height_index / 2);
    let new_index = index - (height_index / 2) - 1;
    calculate_leaf_index_offset(new_index, new_offset)
}

#[cfg(test)]
mod tests {

    use super::*;
    use blake2::Blake2b;
    use serde_derive::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    pub struct IWrapper(u32);

    impl Hashable for IWrapper {
        fn hash(&self) -> Vec<u8> {
            Blake2b::new().chain(self.0.to_le_bytes()).result().to_vec()
        }
    }

    fn create_mmr(leaves: u32) -> MerkleMountainRange<IWrapper, Blake2b> {
        let mut mmr: MerkleMountainRange<IWrapper, Blake2b> = MerkleMountainRange::new();
        for i in 1..leaves + 1 {
            let object: IWrapper = IWrapper(i);
            mmr.push(object);
        }
        mmr
    }

    #[test]
    fn test_inner_data_pruning_handling() {
        let mut mmr = create_mmr(2);
        assert_eq!(1, mmr.get_peak_height());
        let hash0 = mmr.get_node_hash(0).unwrap();
        let proof = mmr.get_hash_proof(&hash0);
        let mut our_proof = MerkleProof::new();
        for i in 0..3 {
            our_proof.push(mmr.get_node_hash(i));
        }
        // test pruning
        assert_eq!(mmr.get_object(&hash0).is_some(), true);
        assert_eq!(mmr.get_data_object(hash0.clone()).is_some(), true);
        assert_eq!(mmr.prune_object_hash(&hash0).is_ok(), true);
        assert_eq!(mmr.get_data_object(hash0.clone()).is_some(), false);
        assert_eq!(mmr.get_object(&hash0).is_some(), false);

        let hash1 = mmr.get_node_hash(1).unwrap();
        assert_eq!(mmr.get_object(&hash1).is_some(), true);
        assert_eq!(mmr.get_data_object(hash1.clone()).is_some(), true);
        assert_eq!(mmr.prune_object_hash(&hash1).is_ok(), true);
        assert_eq!(mmr.get_object(&hash1).is_some(), false);
        // both are now pruned, thus deleted
        assert_eq!(mmr.get_data_object(hash1).is_none(), true);
        assert_eq!(mmr.get_data_object(hash0).is_none(), true);
    }
}
