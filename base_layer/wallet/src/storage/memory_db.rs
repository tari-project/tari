// Copyright 2019. The Tari Project
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
    error::WalletStorageError,
    storage::database::{DbKey, DbKeyValuePair, DbValue, WalletBackend, WriteOperation},
};
use aes_gcm::Aes256Gcm;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    multiaddr::Multiaddr,
    tor::TorIdentity,
    types::{CommsPublicKey, CommsSecretKey},
    NodeIdentity,
};
use tari_crypto::keys::PublicKey;

#[derive(Default)]
pub struct InnerDatabase {
    comms_private_key: Option<CommsSecretKey>,
    client_key_values: HashMap<String, String>,
    comms_address: Option<Multiaddr>,
    features: u64,
    identity: Option<NodeIdentity>,
    tor_id: Option<TorIdentity>,
    chain_metadata: Option<ChainMetadata>,
}

impl InnerDatabase {
    pub fn new() -> Self {
        Self {
            comms_private_key: None,
            client_key_values: HashMap::new(),
            comms_address: None,
            features: 0,
            identity: None,
            tor_id: None,
            chain_metadata: None,
        }
    }
}

#[derive(Clone)]
pub struct WalletMemoryDatabase {
    db: Arc<RwLock<InnerDatabase>>,
}

impl WalletMemoryDatabase {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(InnerDatabase::new())),
        }
    }
}

impl Default for WalletMemoryDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletBackend for WalletMemoryDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError> {
        let db = acquire_read_lock!(self.db);
        let result = match key {
            DbKey::CommsSecretKey => db.comms_private_key.clone().map(DbValue::CommsSecretKey),
            DbKey::CommsPublicKey => db
                .comms_private_key
                .clone()
                .map(|sk| DbValue::CommsPublicKey(CommsPublicKey::from_secret_key(&sk))),
            DbKey::ClientKey(k) => db.client_key_values.get(k).map(|v| DbValue::ClientValue(v.clone())),
            DbKey::CommsAddress => db.comms_address.clone().map(DbValue::CommsAddress),
            DbKey::CommsFeatures => Some(DbValue::CommsFeatures(db.features)),
            DbKey::Identity => db.identity.clone().map(DbValue::Identity),
            DbKey::TorId => db.tor_id.clone().map(DbValue::TorId),
            DbKey::BaseNodeChainMetadata => db.chain_metadata.clone().map(DbValue::BaseNodeChainMetadata),
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError> {
        let mut db = acquire_write_lock!(self.db);
        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::CommsSecretKey(secret) => {
                    db.comms_private_key = Some(secret);
                },
                DbKeyValuePair::ClientKeyValue(k, v) => {
                    db.client_key_values.insert(k, v);
                },
                DbKeyValuePair::Identity(v) => {
                    db.identity = Some(*v);
                },
                DbKeyValuePair::TorId(v) => {
                    db.tor_id = Some(v);
                },
                DbKeyValuePair::BaseNodeChainMetadata(metadata) => {
                    db.chain_metadata = Some(metadata);
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::CommsSecretKey => {
                    db.comms_private_key = None;
                },
                DbKey::CommsPublicKey => {
                    return Err(WalletStorageError::OperationNotSupported);
                },
                DbKey::ClientKey(k) => {
                    if db.client_key_values.remove(&k).is_some() {
                        return Ok(Some(DbValue::ValueCleared));
                    } else {
                        return Ok(None);
                    }
                },
                DbKey::CommsAddress => {
                    return Err(WalletStorageError::OperationNotSupported);
                },
                DbKey::CommsFeatures => {
                    return Err(WalletStorageError::OperationNotSupported);
                },
                DbKey::Identity => {
                    return Err(WalletStorageError::OperationNotSupported);
                },
                DbKey::BaseNodeChainMetadata => {
                    return Err(WalletStorageError::OperationNotSupported);
                },
                DbKey::TorId => {
                    db.tor_id = None;
                },
            },
        }

        Ok(None)
    }

    fn apply_encryption(&self, _: Aes256Gcm) -> Result<(), WalletStorageError> {
        Ok(())
    }

    fn remove_encryption(&self) -> Result<(), WalletStorageError> {
        Ok(())
    }
}
