//  Copyright 2021. The Tari Project
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

use crate::dan_layer::{
    models::SidechainMetadata,
    storage::{DbFactory, LmdbDbFactory, StorageError},
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

// One per asset, per network
#[async_trait]
pub trait ChainStorageService {
    async fn get_metadata(&self) -> Result<SidechainMetadata, StorageError>;
}

pub struct LmdbChainStorageService {
    db_factory: LmdbDbFactory,
}

impl LmdbChainStorageService {
    pub fn new(db_factory: LmdbDbFactory) -> Self {
        Self { db_factory }
    }
}

#[async_trait]
impl ChainStorageService for LmdbChainStorageService {
    async fn get_metadata(&self) -> Result<SidechainMetadata, StorageError> {
        let db = self.db_factory.create();
        let sidechain_data = db.metadata.read();
        Ok(sidechain_data)
    }
}

#[derive(Clone)]
pub struct ChainStorageServiceHandle {
    service: Arc<RwLock<LmdbChainStorageService>>,
}

impl ChainStorageServiceHandle {
    pub fn new() -> Self {
        todo!()
        // Self {

        // TODO: fix this ordering
        // service: Arc::new(RwLock::new(LmdbChainStorageService {})),
        // }
    }
}

#[async_trait]
impl ChainStorageService for ChainStorageServiceHandle {
    async fn get_metadata(&self) -> Result<SidechainMetadata, StorageError> {
        self.service.read().await.get_metadata().await
    }
}
