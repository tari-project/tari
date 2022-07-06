//  Copyright 2022, The Tari Project
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

pub mod chain_db;
pub mod global_db;

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use tari_common_types::types::FixedHash;
use tari_dan_engine::state::{mocks::state_db::MockStateDbBackupAdapter, StateDb};

use crate::storage::{
    chain::{ChainDb, ChainDbMetadataKey, DbInstruction, DbNode, DbQc},
    global::GlobalDb,
    mocks::{chain_db::MockChainDbBackupAdapter, global_db::MockGlobalDbBackupAdapter},
    DbFactory,
    StorageError,
};

#[derive(Clone, Default)]
pub struct MockDbFactory {
    chain_db: Arc<RwLock<HashMap<FixedHash, MockChainDbBackupAdapter>>>,
    state_db: Arc<RwLock<HashMap<FixedHash, MockStateDbBackupAdapter>>>,
    _global_db: Arc<RwLock<MockGlobalDbBackupAdapter>>,
}

impl DbFactory for MockDbFactory {
    type ChainDbBackendAdapter = MockChainDbBackupAdapter;
    type GlobalDbBackendAdapter = MockGlobalDbBackupAdapter;
    type StateDbBackendAdapter = MockStateDbBackupAdapter;

    fn get_chain_db(
        &self,
        contract_id: &FixedHash,
    ) -> Result<Option<ChainDb<Self::ChainDbBackendAdapter>>, StorageError> {
        Ok(self
            .chain_db
            .read()
            .unwrap()
            .get(contract_id)
            .cloned()
            .map(ChainDb::new))
    }

    fn get_or_create_chain_db(
        &self,
        contract_id: &FixedHash,
    ) -> Result<ChainDb<Self::ChainDbBackendAdapter>, StorageError> {
        let entry = self.chain_db.write().unwrap().entry(*contract_id).or_default().clone();
        Ok(ChainDb::new(entry))
    }

    fn get_state_db(
        &self,
        contract_id: &FixedHash,
    ) -> Result<Option<StateDb<Self::StateDbBackendAdapter>>, StorageError> {
        Ok(self
            .state_db
            .read()
            .unwrap()
            .get(contract_id)
            .cloned()
            .map(|db| StateDb::new(*contract_id, db)))
    }

    fn get_or_create_state_db(
        &self,
        contract_id: &FixedHash,
    ) -> Result<StateDb<Self::StateDbBackendAdapter>, StorageError> {
        let entry = self.state_db.write().unwrap().entry(*contract_id).or_default().clone();
        Ok(StateDb::new(*contract_id, entry))
    }

    fn get_or_create_global_db(&self) -> Result<GlobalDb<Self::GlobalDbBackendAdapter>, StorageError> {
        // let entry = self.global_db.write().unwrap().clone();
        // Ok(GlobalDb::new(entry))
        todo!()
    }
}

#[derive(Debug, Default)]
pub(self) struct MemoryChainDb {
    pub nodes: MemoryDbTable<DbNode>,
    pub instructions: MemoryDbTable<DbInstruction>,
    pub prepare_qc: MemoryDbTable<DbQc>,
    pub locked_qc: MemoryDbTable<DbQc>,
    pub metadata: MemoryDbTable<(ChainDbMetadataKey, Vec<u8>)>,
}

#[derive(Debug)]
struct MemoryDbTable<V> {
    records: HashMap<usize, V>,
    id_counter: usize,
}

// We don't need/want the V: Default bound
impl<V> Default for MemoryDbTable<V> {
    fn default() -> Self {
        Self {
            records: Default::default(),
            id_counter: 0,
        }
    }
}

impl<V> MemoryDbTable<V> {
    pub fn next_id(&mut self) -> usize {
        let id = self.id_counter;
        self.id_counter = self.id_counter.wrapping_add(1);
        id
    }

    pub fn records(&self) -> impl Iterator<Item = (usize, &'_ V)> {
        self.records.iter().map(|(k, v)| (*k, v))
    }

    pub fn rows(&self) -> impl Iterator<Item = &'_ V> {
        self.records.values()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn get(&self, id: usize) -> Option<&V> {
        self.records.get(&id)
    }

    pub fn insert(&mut self, v: V) {
        let id = self.next_id();
        self.records.insert(id, v);
    }

    pub fn update(&mut self, id: usize, v: V) -> bool {
        match self.records.get_mut(&id) {
            Some(rec) => {
                *rec = v;
                true
            },
            None => false,
        }
    }
}
