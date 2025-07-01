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

// CompressedPublicKey in BTreeSet results in a mutable key type, however there link is not applicable
#![allow(clippy::mutable_key_type)]

use std::{collections::BTreeSet, ops::Deref};

use lmdb_zero::{ConstTransaction, WriteTransaction};
use log::*;
use serde::de::DeserializeOwned;
use tari_common_types::{epoch::VnEpoch, types::CompressedPublicKey};
use tari_storage::lmdb_store::DatabaseRef;
use tari_utilities::ByteArray;

use crate::chain_storage::{
    lmdb_db::{
        composite_key::CompositeKey,
        cursors::{FromKeyBytes, LmdbReadCursor},
        lmdb::{lmdb_delete, lmdb_delete_key_value, lmdb_exists, lmdb_get, lmdb_insert, lmdb_insert_dup, lmdb_len},
    },
    ChainStorageError,
    ValidatorNodeEntry,
};

const LOG_TARGET: &str = "c::cs::lmdb_db::validator_node_store";

const U64_SIZE: usize = size_of::<u64>();
const PK_SIZE: usize = 32;

/// <sid, pk, epoch>
type ValidatorNodeStoreKey = CompositeKey<{ PK_SIZE + PK_SIZE + U64_SIZE }>;
/// <sid, epoch, pk>
type ExitQueueKey = CompositeKey<{ PK_SIZE + U64_SIZE + PK_SIZE }>;
const EXIT_QUEUE_KEY_SECTIONS: [usize; 3] = [PK_SIZE, U64_SIZE, PK_SIZE];
/// <sid, epoch> DUPSORT
type ActivationQueueKey = CompositeKey<{ PK_SIZE + U64_SIZE }>;
const ACTIVATION_QUEUE_KEY_SECTIONS: [usize; 2] = [PK_SIZE, U64_SIZE];

pub struct ValidatorNodeStore<'a, Txn> {
    txn: &'a Txn,
    db_validator_nodes: DatabaseRef,
    db_validator_activation_queue: DatabaseRef,
    db_validator_nodes_exit: DatabaseRef,
}

impl<'a, Txn: Deref<Target = ConstTransaction<'a>>> ValidatorNodeStore<'a, Txn> {
    pub fn new(
        txn: &'a Txn,
        db_validator_nodes: DatabaseRef,
        db_validator_activation_queue: DatabaseRef,
        db_validator_nodes_exit: DatabaseRef,
    ) -> Self {
        Self {
            txn,
            db_validator_nodes,
            db_validator_activation_queue,
            db_validator_nodes_exit,
        }
    }
}

impl ValidatorNodeStore<'_, WriteTransaction<'_>> {
    pub fn insert(&self, validator: &ValidatorNodeEntry) -> Result<(), ChainStorageError> {
        let key = create_activation_key(validator.sidechain_public_key.as_ref(), validator.activation_epoch);
        lmdb_insert_dup(
            self.txn,
            &self.db_validator_activation_queue,
            &key,
            &validator.public_key,
        )?;

        let key = create_vn_key(validator.sidechain_public_key.as_ref(), &validator.public_key);
        lmdb_insert(self.txn, &self.db_validator_nodes, &key, validator, "Validator node")?;

        Ok(())
    }

    pub fn delete(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        public_key: &CompressedPublicKey,
    ) -> Result<(), ChainStorageError> {
        let key = create_vn_key(sidechain_pk, public_key);
        let vn = lmdb_get::<_, ValidatorNodeEntry>(self.txn, &self.db_validator_nodes, &key)?.ok_or_else(|| {
            ChainStorageError::ValueNotFound {
                entity: "Validator node (delete)",
                field: "public key",
                value: public_key.to_string(),
            }
        })?;
        lmdb_delete(self.txn, &self.db_validator_nodes, &key, "validator_nodes")?;

        let key = create_activation_key(sidechain_pk, vn.activation_epoch);
        lmdb_delete_key_value(self.txn, &self.db_validator_activation_queue, &key, &vn.public_key)?;

        Ok(())
    }

    pub fn exit(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        exit_node: &CompressedPublicKey,
        exit_epoch: VnEpoch,
    ) -> Result<(), ChainStorageError> {
        let vn_key = create_vn_key(sidechain_pk, exit_node);
        if !lmdb_exists(self.txn, &self.db_validator_nodes, &vn_key)? {
            return Err(ChainStorageError::ValueNotFound {
                entity: "Validator node (exit)",
                field: "public key",
                value: exit_node.to_string(),
            });
        }
        let vn = lmdb_get::<_, ValidatorNodeEntry>(self.txn, &self.db_validator_nodes, &vn_key)?
            .expect("Found node key but not node in db_validator_nodes");

        lmdb_delete(self.txn, &self.db_validator_nodes, &vn_key, "validator_nodes")?;

        let key = create_exit_queue_key(sidechain_pk, exit_epoch, exit_node);
        lmdb_insert(
            self.txn,
            &self.db_validator_nodes_exit,
            &key,
            &vn,
            "validator_nodes_exit",
        )?;
        Ok(())
    }

    pub fn undo_exit(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        min_epoch: VnEpoch,
        exit_node: &CompressedPublicKey,
    ) -> Result<(), ChainStorageError> {
        let mut epoch = min_epoch;
        let sidechain_pk_bytes = sid_as_slice(sidechain_pk);
        // Search through the epochs, from the min until we have no more records
        let (exit_key, vn) = loop {
            {
                // This is to check if there are possibly more records for the next epoch - if not we exit early with an
                // error. If we didnt do this, the loop would be endless if min_epoch/exit_node do not
                // exist.
                let mut cursor = self.exit_queue_read_cursor()?;
                let epoch_prefix = create_exit_queue_prefix_key(sidechain_pk, epoch);
                cursor.seek_range(&epoch_prefix)?;

                let key = cursor.next_key()?.ok_or_else(||
                    // Not in this epoch, and nothing in subsequent recs
                    ChainStorageError::ValueNotFound {
                        entity: "Validator node (undo exit)",
                        field: "public key (undo exit)",
                        value: exit_node.to_string(),
                    })?;
                let mut sections = key.section_iter(EXIT_QUEUE_KEY_SECTIONS);
                let sidechain = sections
                    .next()
                    .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                        function: "ValidatorNodeStore::undo_exit",
                        details: "Malformed exit queue key".to_string(),
                    })?;
                if sidechain != sidechain_pk_bytes {
                    return Err(ChainStorageError::ValueNotFound {
                        entity: "Validator node (undo exit)",
                        field: "public key (undo exit)",
                        value: exit_node.to_string(),
                    });
                }
            }

            let exit_key = create_exit_queue_key(sidechain_pk, epoch, exit_node);
            let vn = lmdb_get::<_, ValidatorNodeEntry>(self.txn, &self.db_validator_nodes_exit, &exit_key)?;
            if let Some(vn) = vn {
                break (exit_key, vn);
            }
            epoch += VnEpoch(1);
        };

        lmdb_delete(
            self.txn,
            &self.db_validator_nodes_exit,
            &exit_key,
            "validator_nodes_exit",
        )?;

        // Re-insert. Since we know the node was already activated in the past, we do not need to re-insert into the
        // historical activation queue.
        let key = create_vn_key(sidechain_pk, &vn.public_key);
        lmdb_insert(self.txn, &self.db_validator_nodes, &key, &vn, "Validator node")?;

        Ok(())
    }
}

impl<'a, Txn: Deref<Target = ConstTransaction<'a>>> ValidatorNodeStore<'a, Txn> {
    fn validator_store_cursor(
        &self,
    ) -> Result<LmdbReadCursor<'a, ValidatorNodeStoreKey, ValidatorNodeEntry>, ChainStorageError> {
        self.new_read_cursor(self.db_validator_nodes.clone())
    }

    fn activation_queue_read_cursor(
        &self,
    ) -> Result<LmdbReadCursor<'a, ActivationQueueKey, CompressedPublicKey>, ChainStorageError> {
        self.new_read_cursor(self.db_validator_activation_queue.clone())
    }

    fn exit_queue_read_cursor(
        &self,
    ) -> Result<LmdbReadCursor<'a, ExitQueueKey, ValidatorNodeEntry>, ChainStorageError> {
        self.new_read_cursor(self.db_validator_nodes_exit.clone())
    }

    fn new_read_cursor<K: FromKeyBytes, V: DeserializeOwned>(
        &self,
        db: DatabaseRef,
    ) -> Result<LmdbReadCursor<'a, K, V>, ChainStorageError> {
        let cursor = self.txn.cursor(db)?;
        let access = self.txn.access();
        let cursor = LmdbReadCursor::new(cursor, access);
        Ok(cursor)
    }

    /// Checks if the given validator node (by its public key and side chain ID)
    /// exists until a given `end_epoch`.
    pub fn vn_exists(
        &self,
        sidechain_id: Option<&CompressedPublicKey>,
        public_key: &CompressedPublicKey,
        end_epoch: VnEpoch,
    ) -> Result<bool, ChainStorageError> {
        let key = create_vn_key(sidechain_id, public_key);
        let Some(vn) = lmdb_get::<_, ValidatorNodeEntry>(self.txn, &self.db_validator_nodes, &key)? else {
            return Ok(false);
        };

        Ok(vn.registration_epoch <= end_epoch)
    }

    /// Checks if the given validator node (by its public key and side chain ID)
    /// exists until a given `end_epoch`.
    pub fn is_vn_active(
        &self,
        sidechain_id: Option<&CompressedPublicKey>,
        public_key: &CompressedPublicKey,
        end_epoch: VnEpoch,
    ) -> Result<bool, ChainStorageError> {
        let key = create_vn_key(sidechain_id, public_key);
        match lmdb_get::<_, ValidatorNodeEntry>(self.txn, &self.db_validator_nodes, &key)? {
            Some(vn) => {
                debug!(target: LOG_TARGET, "Found validator node in store: {} (activated: {}, end: {})", public_key, vn.activation_epoch, end_epoch);
                Ok(vn.activation_epoch <= end_epoch)
            },
            None => {
                debug!(target: LOG_TARGET, "Validator node not found in store: {}, checking exit queue", public_key);
                let key = create_exit_queue_prefix_key(sidechain_id, end_epoch);
                let mut cursor = self.exit_queue_read_cursor()?;
                cursor.seek_range(&key)?;
                let sidechain_bytes = sid_as_slice(sidechain_id);
                // TODO: This is O(n) where n is the number of validators in the exit queue for the sid at or after
                // end_epoch.
                while let Some(key) = cursor.next_key()? {
                    let mut sections = key.section_iter(EXIT_QUEUE_KEY_SECTIONS);
                    let sidechain_pk = sections
                        .next()
                        .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                            function: "ValidatorNodeStore::is_vn_active",
                            details: "Malformed exit queue key".to_string(),
                        })?;
                    if sidechain_pk != sidechain_bytes {
                        break;
                    }

                    let key_epoch =
                        sections
                            .next_be_u64()
                            .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                                function: "ValidatorNodeStore::is_vn_active",
                                details: "Malformed exit queue key".to_string(),
                            })?;

                    let pk = sections
                        .next()
                        .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                            function: "ValidatorNodeStore::is_vn_active",
                            details: "Malformed exit queue key".to_string(),
                        })?;
                    if pk != public_key.as_bytes() {
                        continue;
                    }
                    if key_epoch <= end_epoch.as_u64() {
                        debug!(target: LOG_TARGET, "Found validator node in exit queue (exit applied): {} (exit: {}, end: {})", public_key, key_epoch, end_epoch);
                        return Ok(false);
                    }

                    let (_, v) = cursor
                        .current()?
                        .expect("Cursor is not at a valid position in is_vn_active");

                    debug!(target: LOG_TARGET, "Found validator node in exit queue: {} (exit: {}, end: {})", public_key, key_epoch, end_epoch);
                    return Ok(v.activation_epoch <= end_epoch);
                }

                debug!(target: LOG_TARGET, "Validator node not found in exit queue: {}", public_key);
                Ok(false)
            },
        }
    }

    pub fn get_next_activation_epoch(
        &self,
        sidechain_id: Option<&CompressedPublicKey>,
        current_epoch: VnEpoch,
        initial_validators: usize,
        validators_per_epoch: usize,
    ) -> Result<VnEpoch, ChainStorageError> {
        // Node activates earliest in the next epoch
        let mut activation_epoch = current_epoch + VnEpoch(1);
        // If there are less than the initial validators, we activate all new validators in the next epoch
        let len = lmdb_len(self.txn, &self.db_validator_nodes)?;
        if len < initial_validators {
            return Ok(activation_epoch);
        }

        if validators_per_epoch == 0 {
            return Err(ChainStorageError::InvalidQuery(
                "get_next_activation_epoch: validators_per_epoch is zero, an active epoch cannot be assigned to the \
                 validator"
                    .to_string(),
            ));
        }

        let mut cursor = self.activation_queue_read_cursor()?;
        loop {
            let key = create_activation_key(sidechain_id, activation_epoch);
            if !cursor.seek_range(&key)? {
                break;
            }

            // No activations in this epoch
            if key.to_be_u64(PK_SIZE)? != activation_epoch.as_u64() {
                break;
            }

            // If there are less than the required number of validators in the queue for the epoch, we'll activate the
            // next validator in this epoch
            let num_queued = cursor.count_dups()?;
            if num_queued < validators_per_epoch {
                break;
            }
            activation_epoch += VnEpoch(1);
        }

        Ok(activation_epoch)
    }

    pub fn count_active_validators(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        end_epoch: VnEpoch,
    ) -> Result<usize, ChainStorageError> {
        let mut count = 0;

        {
            let mut cursor = self.validator_store_cursor()?;
            let sidechain_bytes = sid_as_slice(sidechain_pk);
            if !cursor.seek_range(sidechain_bytes)? {
                return Ok(0);
            }

            while let Some((key, vn)) = cursor.next()? {
                if key[..PK_SIZE] != *sidechain_bytes {
                    // No further entries for this sidechain
                    break;
                }

                if vn.activation_epoch > end_epoch {
                    break;
                }

                count += 1;
            }
        }

        // We also need to search the exit queue, if the exit epoch is greater than the end epoch, the validator is
        // still active
        let mut cursor = self.exit_queue_read_cursor()?;
        let prefix = create_exit_queue_prefix_key(sidechain_pk, end_epoch);
        if !cursor.seek_range(&prefix)? {
            return Ok(count);
        }
        let sidechain_bytes = sid_as_slice(sidechain_pk);
        while let Some((key, _)) = cursor.next()? {
            let mut sections = key.section_iter(EXIT_QUEUE_KEY_SECTIONS);
            let sid = sections
                .next()
                .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                    function: "ValidatorNodeStore::count_active_validators",
                    details: "Malformed exit queue key".to_string(),
                })?;

            if sid != sidechain_bytes {
                // No further entries for this sidechain
                break;
            }

            let rec_epoch = sections
                .next_be_u64()
                .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                    function: "ValidatorNodeStore::count_active_validators",
                    details: "Malformed exit queue key".to_string(),
                })?;
            if rec_epoch <= end_epoch.as_u64() {
                // No further entries for this epoch
                continue;
            }
            count += 1;
        }

        Ok(count)
    }

    /// Returns a set of <public key, shard id> tuples ordered by epoch of registration.
    /// This set contains no duplicates. If a duplicate registration is found, the last registration is included.
    pub fn get_entire_vn_set(&self, end_epoch: VnEpoch) -> Result<BTreeSet<ValidatorNodeEntry>, ChainStorageError> {
        // Nodes in db_validator_nodes are not ordered by epoch, meaning retreival until an end epoch will have to
        // search the entire database incl > end_epoch. Instead, we first gather all public keys from the
        // activation queue which is ordered by epoch.
        let selected_nodes = {
            let mut cursor = self.activation_queue_read_cursor()?;
            cursor.seek_first()?;

            let mut selected_nodes = Vec::new();
            while let Some(key) = cursor.next_key()? {
                let mut sections = key.section_iter(ACTIVATION_QUEUE_KEY_SECTIONS);
                let sidechain_pk = sections
                    .next()
                    .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                        function: "ValidatorNodeStore::get_entire_vn_set",
                        details: "Malformed activation queue key".to_string(),
                    })?;

                let activation_epoch =
                    sections
                        .next_be_u64()
                        .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                            function: "ValidatorNodeStore::get_entire_vn_set",
                            details: "Malformed activation queue key".to_string(),
                        })?;

                if activation_epoch > end_epoch.as_u64() {
                    // Continue because we want to find the next sidechain ID - ideally we'd seek to it but that would
                    // require an ordered sidechain index
                    continue;
                }
                let (_, pk) = cursor
                    .current()?
                    .expect("Cursor is not at a valid position in get_entire_vn_set");
                let count = cursor.count_dups()?;
                let mut pks = Vec::with_capacity(count);
                // Collect all the public keys for the <sid, epoch> pair
                pks.push(pk);
                while let Some((_, pk)) = cursor.next_dup()? {
                    pks.push(pk)
                }

                let mut sid = [0u8; 32];
                sid.copy_from_slice(sidechain_pk);
                selected_nodes.push((sid, pks));
            }
            selected_nodes
        };

        let mut nodes = BTreeSet::new();

        for (sid, pks) in selected_nodes {
            for pk in pks {
                let key = create_vn_key_raw(&sid, pk.as_bytes());
                let maybe_vn = lmdb_get::<_, ValidatorNodeEntry>(self.txn, &self.db_validator_nodes, &key)?;
                match maybe_vn {
                    Some(vn) => {
                        nodes.insert(vn);
                    },
                    None => {
                        // Validator is queued for exit. Now we need to determine if the exit is before the end_epoch
                        // i.e an active validator at end_epoch
                        let mut cursor = self.exit_queue_read_cursor()?;
                        let prefix = create_exit_queue_prefix_key(Some(&sid), end_epoch);
                        if !cursor.seek_range(&prefix)? {
                            continue;
                        }

                        while let Some(key) = cursor.next_key()? {
                            let mut sections = key.section_iter(EXIT_QUEUE_KEY_SECTIONS);
                            let key_sid =
                                sections
                                    .next()
                                    .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                                        function: "ValidatorNodeStore::get_entire_vn_set",
                                        details: "Malformed exit queue key".to_string(),
                                    })?;

                            if key_sid != sid {
                                // No further entries for this sidechain
                                break;
                            }

                            let key_epoch =
                                sections
                                    .next_be_u64()
                                    .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                                        function: "ValidatorNodeStore::get_entire_vn_set",
                                        details: "Malformed exit queue key".to_string(),
                                    })?;

                            let key_pk =
                                sections
                                    .next()
                                    .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                                        function: "ValidatorNodeStore::get_entire_vn_set",
                                        details: "Malformed exit queue key".to_string(),
                                    })?;

                            if pk.as_bytes() != key_pk {
                                continue;
                            }

                            if key_epoch > end_epoch.as_u64() {
                                // We've found the exit record for the validator. It is active because the exit happens
                                // after end_epoch PANIC: the current key exists because
                                // we are iterating over the keys in the exit queue
                                let (_, v) = cursor
                                    .current()?
                                    .expect("Cursor is not at a valid position in get_entire_vn_set");
                                nodes.insert(v);
                            }
                            break;
                        }
                    },
                };
            }
        }

        Ok(nodes)
    }

    /// Returns a set of <public key, shard id> tuples ordered by epoch of registration.
    /// This set contains no duplicates. If a duplicate registration is found, the last registration is included.
    pub fn get_vn_set(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        start_epoch: VnEpoch,
        end_epoch: VnEpoch,
        limit: usize,
    ) -> Result<BTreeSet<ValidatorNodeEntry>, ChainStorageError> {
        if end_epoch < start_epoch {
            return Err(ChainStorageError::InvalidQuery(format!(
                "get_vn_set: End epoch is less than start epoch: {} < {}",
                end_epoch, start_epoch
            )));
        }

        if limit == 0 {
            return Ok(BTreeSet::new());
        }

        let mut cursor = self.validator_store_cursor()?;

        let prefix = create_vn_store_prefix_key(sidechain_pk, start_epoch);
        if !cursor.seek_range(&prefix)? {
            return Ok(BTreeSet::new());
        }

        let sidechain_bytes = sid_as_slice(sidechain_pk);
        let mut nodes = BTreeSet::new();
        while let Some((key, vn)) = cursor.next()? {
            if key[..PK_SIZE] != *sidechain_bytes {
                // No further entries for this sidechain
                break;
            }

            if vn.activation_epoch > end_epoch {
                break;
            }

            nodes.insert(vn);
            if nodes.len() == limit {
                break;
            }
        }

        Ok(nodes)
    }

    pub fn get_activating_in_epoch(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        epoch: VnEpoch,
    ) -> Result<BTreeSet<ValidatorNodeEntry>, ChainStorageError> {
        let keys = {
            let mut cursor = self.activation_queue_read_cursor()?;
            let key = create_activation_key(sidechain_pk, epoch);
            if !cursor.seek_range(&key)? {
                return Ok(BTreeSet::new());
            }

            let num_keys = cursor.count_dups()?;
            let mut keys = Vec::with_capacity(num_keys);
            let sidechain_bytes = sid_as_slice(sidechain_pk);
            while let Some((key, pk)) = cursor.next_dup()? {
                if key[..PK_SIZE] != *sidechain_bytes {
                    // No further entries for this sidechain
                    break;
                }
                if key.to_be_u64(PK_SIZE)? > epoch.as_u64() {
                    break;
                }

                keys.push(create_vn_key(sidechain_pk, &pk));
            }
            keys
        };

        debug!(target: LOG_TARGET, "Found {} activating validators in epoch {}", keys.len(), epoch);

        let mut validators = BTreeSet::new();
        for key in keys {
            let vn = lmdb_get::<_, ValidatorNodeEntry>(self.txn, &self.db_validator_nodes, &key)?.ok_or_else(|| {
                ChainStorageError::DataInconsistencyDetected {
                    function: "ValidatorNodeStore::get_activating_in_epoch",
                    details: format!(
                        "Validator node in db_validator_activation_queue but not found in store for public key {}",
                        key
                    ),
                }
            })?;

            validators.insert(vn);
        }

        Ok(validators)
    }

    pub fn get_exiting_in_epoch(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        epoch: VnEpoch,
    ) -> Result<BTreeSet<ValidatorNodeEntry>, ChainStorageError> {
        let mut cursor = self.exit_queue_read_cursor()?;
        let prefix = create_exit_queue_prefix_key(sidechain_pk, epoch);
        if !cursor.seek_range(&prefix)? {
            return Ok(BTreeSet::new());
        }

        let sidechain_bytes = sid_as_slice(sidechain_pk);
        let mut validators = BTreeSet::new();
        while let Some(key) = cursor.next_key()? {
            debug!(target: LOG_TARGET, "exit queue key: {}", key);
            let mut sections = key.section_iter(EXIT_QUEUE_KEY_SECTIONS);
            let sid = sections
                .next()
                .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                    function: "ValidatorNodeStore::get_exiting_in_epoch",
                    details: "Malformed exit queue key".to_string(),
                })?;

            if sid != sidechain_bytes {
                // No further entries for this sidechain
                break;
            }

            let rec_epoch = sections
                .next_be_u64()
                .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                    function: "ValidatorNodeStore::get_exiting_in_epoch",
                    details: "Malformed exit queue key".to_string(),
                })?;
            if rec_epoch != epoch.as_u64() {
                debug!(target: LOG_TARGET, "No further entries for this epoch {}, last rec epoch {}", epoch, rec_epoch);
                // No further entries for this epoch
                break;
            }

            let (_, vn) = cursor
                .current()?
                .expect("Cursor is not at a valid position in get_exiting_in_epoch");
            debug!(target: LOG_TARGET, "Found exiting validator in epoch {}: {}", rec_epoch, vn.public_key);
            validators.insert(vn);
        }

        Ok(validators)
    }

    pub fn get_next_exit_epoch(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        epoch: VnEpoch,
        max_exits: usize,
    ) -> Result<VnEpoch, ChainStorageError> {
        if max_exits == 0 {
            return Err(ChainStorageError::InvalidQuery(
                "get_next_exit_epoch: max_exits is zero, an exit epoch cannot be assigned to the validator".to_string(),
            ));
        }

        let mut cursor = self.exit_queue_read_cursor()?;
        let prefix = create_exit_queue_prefix_key(sidechain_pk, epoch);
        if !cursor.seek_range(&prefix)? {
            return Ok(epoch);
        }

        let sidechain_bytes = sid_as_slice(sidechain_pk);
        let mut exit_count = 0;
        let mut exit_epoch = epoch;
        while let Some(key) = cursor.next_key()? {
            trace!(target: LOG_TARGET, "exit queue key: {}", key);
            let mut sections = key.section_iter(EXIT_QUEUE_KEY_SECTIONS);
            let sid = sections
                .next()
                .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                    function: "ValidatorNodeStore::get_exiting_in_epoch",
                    details: "Malformed exit queue key".to_string(),
                })?;

            if sid != sidechain_bytes {
                // No further entries for this sidechain
                break;
            }

            let rec_epoch = sections
                .next_be_u64()
                .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                    function: "ValidatorNodeStore::get_exiting_in_epoch",
                    details: "Malformed exit queue key".to_string(),
                })?;

            if rec_epoch == exit_epoch.as_u64() {
                exit_count += 1;
                if exit_count >= max_exits {
                    // Scan to the next epoch
                    exit_epoch += VnEpoch(1);
                    cursor.seek_range(&create_exit_queue_prefix_key(sidechain_pk, exit_epoch))?;
                    exit_count = 0;
                }
            } else {
                // Epoch has changed - first check if the previous epoch was below the max_exits
                if exit_count < max_exits {
                    break;
                }
                exit_epoch = VnEpoch(rec_epoch);
                exit_count = 1;
            }
        }

        Ok(exit_epoch)
    }

    pub fn get(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        public_key: &CompressedPublicKey,
    ) -> Result<Option<ValidatorNodeEntry>, ChainStorageError> {
        let key = create_vn_key(sidechain_pk, public_key);
        let vn = lmdb_get::<_, ValidatorNodeEntry>(self.txn, &self.db_validator_nodes, &key)?;
        Ok(vn)
    }
}

fn create_vn_key(
    sidechain_pk: Option<&CompressedPublicKey>,
    public_key: &CompressedPublicKey,
) -> ValidatorNodeStoreKey {
    create_vn_key_raw(sid_as_slice(sidechain_pk), public_key.as_bytes())
}
fn create_vn_key_raw(sidechain_pk: &[u8], public_key: &[u8]) -> ValidatorNodeStoreKey {
    ValidatorNodeStoreKey::try_from_parts(&[sidechain_pk, public_key])
        .expect("create_key: Composite key length is incorrect")
}

fn create_exit_queue_key(
    sidechain_pk: Option<&CompressedPublicKey>,
    epoch: VnEpoch,
    public_key: &CompressedPublicKey,
) -> ExitQueueKey {
    ExitQueueKey::try_from_parts(&[
        sid_as_slice(sidechain_pk),
        epoch.to_be_bytes().as_slice(),
        public_key.as_bytes(),
    ])
    .expect("create_key: Composite key length is incorrect")
}

fn create_exit_queue_prefix_key<B: ByteArray>(sidechain_pk: Option<&B>, epoch: VnEpoch) -> [u8; PK_SIZE + U64_SIZE] {
    let mut buf = [0u8; PK_SIZE + U64_SIZE];
    if let Some(pk) = sidechain_pk {
        buf[..PK_SIZE].copy_from_slice(pk.as_bytes());
    }
    buf[PK_SIZE..].copy_from_slice(&epoch.to_be_bytes());
    buf
}

fn create_activation_key(sidechain_pk: Option<&CompressedPublicKey>, epoch: VnEpoch) -> ActivationQueueKey {
    ActivationQueueKey::try_from_parts(&[sid_as_slice(sidechain_pk), &epoch.to_be_bytes()])
        .expect("create_activation_key: Composite key length is incorrect")
}

fn create_vn_store_prefix_key(sidechain_pk: Option<&CompressedPublicKey>, epoch: VnEpoch) -> [u8; PK_SIZE + U64_SIZE] {
    let mut buf = [0u8; PK_SIZE + U64_SIZE];
    if let Some(pk) = sidechain_pk {
        buf[..PK_SIZE].copy_from_slice(pk.as_bytes());
    }
    buf[PK_SIZE..].copy_from_slice(&epoch.to_be_bytes());
    buf
}

fn sid_as_slice(sidechain_pk: Option<&CompressedPublicKey>) -> &[u8] {
    sidechain_pk.map_or([0u8; 32].as_slice(), |pk| pk.as_bytes())
}

#[cfg(test)]
mod tests {
    use lmdb_zero::db;
    use tari_common_types::types::CompressedCommitment;
    use tari_test_utils::unpack_enum;

    use super::*;
    use crate::{
        chain_storage::tests::temp_db::TempLmdbDatabase,
        test_helpers::{make_hash, new_public_key},
    };

    const DBS: &[(&str, db::Flags)] = &[
        ("validator_node_store", db::CREATE),
        ("validator_node_activation_queue", db::DUPSORT),
        ("validator_node_exit_queue", db::CREATE),
    ];

    fn create_store<'a, Txn: Deref<Target = ConstTransaction<'a>>>(
        db: &TempLmdbDatabase,
        txn: &'a Txn,
    ) -> ValidatorNodeStore<'a, Txn> {
        let store_db = db.get_db(DBS[0].0).clone();
        let activation_queue = db.get_db(DBS[1].0).clone();
        let exit_db = db.get_db(DBS[2].0).clone();
        ValidatorNodeStore::new(txn, store_db, activation_queue, exit_db)
    }

    fn insert_n_vns(
        store: &ValidatorNodeStore<'_, WriteTransaction<'_>>,
        start_epoch: u64,
        epoch_increment: u64,
        n: usize,
        sidechain_id: Option<&CompressedPublicKey>,
    ) -> Vec<ValidatorNodeEntry> {
        let mut nodes = Vec::with_capacity(n);
        for i in 0..n {
            let public_key = new_public_key();
            let shard_key = make_hash(public_key.as_bytes());
            let start_epoch = VnEpoch(start_epoch + (i as u64 * epoch_increment));
            let entry = ValidatorNodeEntry {
                public_key: public_key.clone(),
                shard_key,
                commitment: CompressedCommitment::from_compressed_key(new_public_key()),
                activation_epoch: start_epoch,
                sidechain_public_key: sidechain_id.cloned(),
                ..Default::default()
            };
            store.insert(&entry).unwrap();
            nodes.push(entry);
        }
        nodes.sort_by(|a, b| {
            a.sidechain_public_key
                .cmp(&b.sidechain_public_key)
                .then(a.shard_key.cmp(&b.shard_key))
        });
        nodes
    }

    mod insert {
        use super::*;

        #[test]
        fn it_inserts_validator_nodes() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 0, 3, None);
            let set = store.get_vn_set(None, VnEpoch(1), VnEpoch(3), 4).unwrap();
            for (i, node) in set.iter().enumerate() {
                assert_eq!(*node, nodes[i]);
            }
            assert_eq!(set.len(), 3);
        }

        #[test]
        fn it_does_not_allow_duplicate_entries() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let p1 = new_public_key();
            let entry = ValidatorNodeEntry {
                shard_key: make_hash(p1.as_bytes()),
                public_key: p1,
                commitment: CompressedCommitment::from_compressed_key(new_public_key()),
                ..Default::default()
            };
            store.insert(&entry).unwrap();
            let err = store.insert(&entry).unwrap_err();
            unpack_enum!(ChainStorageError::KeyExists { .. } = err);
        }

        #[test]
        fn it_returns_key_exists_if_duplicate_inserted() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 0, 3, None);
            // Node 0 and 1 re-register at height 4

            let s0 = make_hash(nodes[0].shard_key);
            let err = store
                .insert(&ValidatorNodeEntry {
                    public_key: nodes[0].public_key.clone(),
                    shard_key: s0,
                    commitment: CompressedCommitment::from_compressed_key(new_public_key().clone()),
                    ..Default::default()
                })
                .unwrap_err();
            assert!(matches!(err, ChainStorageError::KeyExists { .. }));
        }
    }

    mod get {
        use super::*;

        #[test]
        fn it_returns_the_validator_node() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 0, 3, None);

            let s = store.get(None, &nodes[0].public_key).unwrap().unwrap();
            assert_eq!(s, nodes[0]);
        }
    }

    mod activating_in_epoch {
        use super::*;

        #[test]
        fn it_returns_vns_activating_in_epoch() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 0, 3, None);
            let nodes2 = insert_n_vns(&store, 10, 0, 4, None);
            let sid = new_public_key();
            let nodes3 = insert_n_vns(&store, 1, 0, 3, Some(&sid));

            let epoch = VnEpoch(1);
            let activating = store.get_activating_in_epoch(None, epoch).unwrap();
            assert_eq!(activating.len(), 3);
            for (i, node) in activating.iter().enumerate() {
                assert_eq!(*node, nodes[i]);
            }

            for epoch in 2..10 {
                let activating = store.get_activating_in_epoch(None, VnEpoch(epoch)).unwrap();
                assert_eq!(activating.len(), 0);
            }

            let epoch = VnEpoch(10);
            let activating = store.get_activating_in_epoch(None, epoch).unwrap();
            assert_eq!(activating.len(), 4);
            for (i, node) in activating.iter().enumerate() {
                assert_eq!(*node, nodes2[i]);
            }

            let epoch = VnEpoch(1);
            let activating = store.get_activating_in_epoch(Some(&sid), epoch).unwrap();
            assert_eq!(activating.len(), 3);
            for (i, node) in activating.iter().enumerate() {
                assert_eq!(*node, nodes3[i]);
            }
        }
    }

    mod get_exiting_in_epoch {
        use super::*;
        use crate::test_helpers::make_hash2;

        #[test]
        fn it_returns_vns_exiting() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 0, 3, None);
            let nodes2 = insert_n_vns(&store, 10, 0, 4, None);
            let sid = new_public_key();
            let nodes3 = insert_n_vns(&store, 1, 0, 3, Some(&sid));

            assert!(store.is_vn_active(None, &nodes[0].public_key, VnEpoch(1)).unwrap());
            assert!(!store.is_vn_active(None, &nodes2[0].public_key, VnEpoch(1)).unwrap());
            assert!(store.is_vn_active(None, &nodes2[0].public_key, VnEpoch(11)).unwrap());
            assert!(store
                .is_vn_active(Some(&sid), &nodes3[0].public_key, VnEpoch(3))
                .unwrap());

            // Exit some nodes
            store.exit(None, &nodes[0].public_key, VnEpoch(11)).unwrap();
            store.exit(None, &nodes[0].public_key, VnEpoch(11)).unwrap_err();

            store.exit(None, &nodes[1].public_key, VnEpoch(11)).unwrap();
            store.exit(None, &nodes2[0].public_key, VnEpoch(110)).unwrap();
            store.exit(None, &nodes2[1].public_key, VnEpoch(110)).unwrap();
            store.exit(Some(&sid), &nodes3[0].public_key, VnEpoch(11)).unwrap();
            store.exit(Some(&sid), &nodes3[1].public_key, VnEpoch(11)).unwrap();

            let next_exit_epoch = store.get_next_exit_epoch(None, VnEpoch(11), 2).unwrap();
            assert_eq!(next_exit_epoch, VnEpoch(12));

            assert!(store.is_vn_active(None, &nodes[0].public_key, VnEpoch(9)).unwrap());
            assert!(!store.is_vn_active(None, &nodes[0].public_key, VnEpoch(12)).unwrap());

            let count = store.count_active_validators(None, VnEpoch(10)).unwrap();
            assert_eq!(count, 7);
            let count = store.count_active_validators(None, VnEpoch(11)).unwrap();
            assert_eq!(count, 5);
            let count = store.count_active_validators(Some(&sid), VnEpoch(10)).unwrap();
            assert_eq!(count, 3);
            let count = store.count_active_validators(Some(&sid), VnEpoch(11)).unwrap();
            assert_eq!(count, 1);
            let count = store.count_active_validators(None, VnEpoch(109)).unwrap();
            assert_eq!(count, 5);
            let count = store.count_active_validators(None, VnEpoch(110)).unwrap();
            assert_eq!(count, 3);
            let count = store.count_active_validators(None, VnEpoch(111)).unwrap();
            assert_eq!(count, 3);

            let exiting = store.get_exiting_in_epoch(None, VnEpoch(11)).unwrap();
            assert_eq!(exiting.len(), 2);
            for (i, node) in exiting.iter().enumerate() {
                assert_eq!(*node, nodes[i]);
            }
            assert!(store.is_vn_active(None, &nodes[0].public_key, VnEpoch(10)).unwrap());
            assert!(!store.is_vn_active(None, &nodes[0].public_key, VnEpoch(11)).unwrap());

            let exiting = store.get_exiting_in_epoch(None, VnEpoch(110)).unwrap();
            assert_eq!(exiting.len(), 2);
            for (i, node) in exiting.iter().enumerate() {
                assert_eq!(*node, nodes2[i]);
            }
            assert!(store.is_vn_active(None, &nodes2[0].public_key, VnEpoch(109)).unwrap());
            assert!(!store.is_vn_active(None, &nodes2[0].public_key, VnEpoch(110)).unwrap());

            let exiting = store.get_exiting_in_epoch(Some(&sid), VnEpoch(10)).unwrap();
            assert_eq!(exiting.len(), 0);
            let exiting = store.get_exiting_in_epoch(Some(&sid), VnEpoch(11)).unwrap();
            assert_eq!(exiting.len(), 2);
            for (i, node) in exiting.iter().enumerate() {
                assert_eq!(*node, nodes3[i]);
            }
            assert!(store
                .is_vn_active(Some(&sid), &nodes3[0].public_key, VnEpoch(10))
                .unwrap());
            assert!(!store
                .is_vn_active(Some(&sid), &nodes3[0].public_key, VnEpoch(11))
                .unwrap());
        }

        #[test]
        fn it_returns_then_next_exit_epoch() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 0, 3, None);
            let nodes2 = insert_n_vns(&store, 10, 0, 4, None);
            let sid = new_public_key();
            let nodes3 = insert_n_vns(&store, 1, 0, 3, Some(&sid));
            let nodes4 = insert_n_vns(&store, 1, 0, 3, None);

            // Empty
            let next_exit_epoch = store.get_next_exit_epoch(None, VnEpoch(10), 2).unwrap();
            assert_eq!(next_exit_epoch, VnEpoch(10));

            // Exit some nodes
            store.exit(None, &nodes[0].public_key, VnEpoch(11)).unwrap();
            store.exit(None, &nodes[1].public_key, VnEpoch(11)).unwrap();
            store.exit(None, &nodes2[0].public_key, VnEpoch(12)).unwrap();
            store.exit(None, &nodes2[1].public_key, VnEpoch(12)).unwrap();
            store.exit(None, &nodes4[0].public_key, VnEpoch(13)).unwrap();
            store.exit(None, &nodes4[1].public_key, VnEpoch(13)).unwrap();
            store.exit(Some(&sid), &nodes3[0].public_key, VnEpoch(11)).unwrap();
            store.exit(Some(&sid), &nodes3[1].public_key, VnEpoch(11)).unwrap();

            let next_exit_epoch = store.get_next_exit_epoch(None, VnEpoch(11), 2).unwrap();
            assert_eq!(next_exit_epoch, VnEpoch(14));

            store.undo_exit(None, VnEpoch(11), &nodes4[0].public_key).unwrap();
            assert!(store
                .undo_exit(None, VnEpoch(11), &Default::default())
                .unwrap_err()
                .is_value_not_found());
            assert!(store
                .undo_exit(None, VnEpoch(110), &Default::default())
                .unwrap_err()
                .is_value_not_found());

            let next_exit_epoch = store.get_next_exit_epoch(None, VnEpoch(11), 2).unwrap();
            assert_eq!(next_exit_epoch, VnEpoch(13));
        }

        #[test]
        fn it_allows_register_exit_register() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 0, 3, None);
            // Exit some nodes
            store.exit(None, &nodes[0].public_key, VnEpoch(10)).unwrap();

            let public_key = nodes[0].public_key.clone();
            let shard_key = make_hash2(public_key.as_bytes(), [1u8]);
            let start_epoch = VnEpoch(15);
            let entry = ValidatorNodeEntry {
                public_key: public_key.clone(),
                shard_key,
                commitment: CompressedCommitment::from_compressed_key(new_public_key()),
                activation_epoch: start_epoch,
                sidechain_public_key: None,
                ..Default::default()
            };
            store.insert(&entry).unwrap();

            assert!(store.is_vn_active(None, &public_key, VnEpoch(15)).unwrap());

            let next_exit_epoch = store.get_next_exit_epoch(None, VnEpoch(15), 2).unwrap();
            assert_eq!(next_exit_epoch, VnEpoch(15));
        }
    }

    mod get_entire_vn_set {
        use super::*;

        #[test]
        fn it_returns_all_active_validators_at_given_epoch() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 0, 3, None);
            let nodes2 = insert_n_vns(&store, 10, 0, 4, None);
            let sid = new_public_key();
            let nodes3 = insert_n_vns(&store, 1, 0, 3, Some(&sid));

            // Exit some nodes
            store.exit(None, &nodes[0].public_key, VnEpoch(11)).unwrap();
            store.exit(None, &nodes[1].public_key, VnEpoch(11)).unwrap();
            store.exit(None, &nodes2[0].public_key, VnEpoch(110)).unwrap();
            store.exit(None, &nodes2[1].public_key, VnEpoch(110)).unwrap();
            store.exit(Some(&sid), &nodes3[0].public_key, VnEpoch(11)).unwrap();
            store.exit(Some(&sid), &nodes3[1].public_key, VnEpoch(11)).unwrap();

            let set = store.get_entire_vn_set(VnEpoch(10)).unwrap();
            assert_eq!(set.len(), 10);
            let set = store.get_entire_vn_set(VnEpoch(11)).unwrap();
            assert_eq!(set.len(), 6);
            let set = store.get_entire_vn_set(VnEpoch(110)).unwrap();
            assert_eq!(set.len(), 4);

            // re-register
            store
                .insert(&ValidatorNodeEntry {
                    shard_key: nodes2[0].shard_key,
                    activation_epoch: VnEpoch(210),
                    registration_epoch: VnEpoch(210),
                    public_key: nodes2[0].public_key.clone(),
                    commitment: CompressedCommitment::from_compressed_key(new_public_key()),
                    sidechain_public_key: None,
                    minimum_value_promise: Default::default(),
                })
                .unwrap();

            store
                .insert(&ValidatorNodeEntry {
                    shard_key: nodes2[1].shard_key,
                    activation_epoch: VnEpoch(210),
                    registration_epoch: VnEpoch(210),
                    public_key: nodes2[1].public_key.clone(),
                    commitment: CompressedCommitment::from_compressed_key(new_public_key()),
                    sidechain_public_key: None,
                    minimum_value_promise: Default::default(),
                })
                .unwrap();
            // TODO: re-register with the same public key is not supported properly
            // let set = store.get_entire_vn_set(VnEpoch(110)).unwrap();
            // assert_eq!(set.len(), 4);

            // let set = store.get_entire_vn_set(VnEpoch(210)).unwrap();
            // assert_eq!(set.len(), 6);
        }
    }
}
