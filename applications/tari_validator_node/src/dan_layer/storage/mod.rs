// Copyright 2021. The Tari Project
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

use crate::dan_layer::models::{ChainHeight, SidechainMetadata, TreeNodeHash};
pub use chain_storage_service::{ChainStorageService, ChainStorageServiceHandle, LmdbChainStorageService};
pub use error::StorageError;
pub use lmdb::{LmdbAssetBackend, LmdbAssetStore};
use std::sync::Arc;
pub use store::{AssetDataStore, AssetStore};
use tari_common::GlobalConfig;

mod chain_storage_service;
mod error;
pub mod lmdb;
mod store;

pub trait DbFactory {
    fn create(&self) -> ChainDb;
    fn create_state_db(&self) -> StateDb;
}

#[derive(Clone)]
pub struct LmdbDbFactory {}

impl LmdbDbFactory {
    pub fn new(config: &GlobalConfig) -> Self {
        Self {}
    }
}

impl DbFactory for LmdbDbFactory {
    fn create(&self) -> ChainDb {
        ChainDb {
            metadata: MetadataTable {},
        }
    }

    fn create_state_db(&self) -> StateDb {
        StateDb { unit_of_work: None }
    }
}

pub struct ChainDb {
    pub metadata: MetadataTable,
}

impl ChainDb {
    pub fn commit(&mut self) -> Result<(), ()> {
        todo!()
    }
}

pub struct MetadataTable {}

impl MetadataTable {
    pub fn read(&self) -> SidechainMetadata {
        SidechainMetadata::new(Default::default(), 0.into(), TreeNodeHash(vec![0u8; 32]))
    }
}

pub struct StateDb {
    unit_of_work: Option<StateDbUnitOfWork>,
}

impl StateDb {
    pub fn new_unit_of_work(&mut self) -> &mut StateDbUnitOfWork {
        unimplemented!()
        // let mut unit_of_work = self.current_unit_of_work_mut();
        // if unit_of_work.is_none() {
        //     self.unit_of_work = Some(StateDbUnitOfWork {});
        //     unit_of_work = self.unit_of_work
        // };
        // unit_of_work.as_mut().unwrap()
    }

    fn current_unit_of_work_mut(&mut self) -> Option<&mut StateDbUnitOfWork> {
        unimplemented!()
        // let mut result = self.unit_of_work.as_mut();
        // let mut child = result;
        // while let Some(c) = child {
        //     result = child;
        //     child = c.child.as_mut();
        // }
        //
        // return result;
    }
}

pub struct StateDbUnitOfWork {
    child: Option<Arc<StateDbUnitOfWork>>,
}

impl StateDbUnitOfWork {
    pub fn new_unit_of_work(&mut self) -> &mut StateDbUnitOfWork {
        unimplemented!()
    }

    pub fn commit(&mut self) -> Result<(), StorageError> {
        unimplemented!()
    }
}
