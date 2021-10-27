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

use lmdb_zero::ConstAccessor;
use patricia_tree::{node::Node, PatriciaMap};
use std::path::Path;

use tari_storage::lmdb_store::LMDBConfig;

use crate::{
    dan_layer::{
        models::TokenId,
        storage::{
            lmdb::{lmdb_asset_backend::LmdbAssetBackend, PATRICIA_MAP_KEY},
            AssetStore,
            StorageError,
        },
    },
    digital_assets_error::DigitalAssetError,
};
use bytecodec::{
    bincode_codec::{BincodeDecoder, BincodeEncoder},
    DecodeExt,
    EncodeExt,
};
use patricia_tree::node::{NodeDecoder, NodeEncoder};

pub struct LmdbAssetStore {
    db: LmdbAssetBackend,
    cached: Option<PatriciaMap<Vec<u8>>>,
}

impl LmdbAssetStore {
    pub fn initialize<P: AsRef<Path>>(path: P, config: LMDBConfig) -> Result<Self, StorageError> {
        Ok(Self {
            db: LmdbAssetBackend::initialize(path, config)?,
            cached: None,
        })
    }

    /// Returns the full persisted ParticiaMap of the metadata state.
    fn load_map(&self, access: &ConstAccessor<'_>) -> Result<PatriciaMap<Vec<u8>>, StorageError> {
        let map = self
            .db
            .get_metadata(access, PATRICIA_MAP_KEY)?
            .map(decode_patricia_nodes)
            .transpose()?
            .unwrap_or_else(Node::root);
        Ok(map.into())
    }
}

impl AssetStore for LmdbAssetStore {
    fn get_metadata(&mut self, token_id: &TokenId) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        match &self.cached {
            Some(cached) => {
                let val = cached.get(token_id);
                Ok(val.cloned())
            },
            None => {
                let txn = self.db.read_transaction()?;
                let map = self.load_map(&txn.access())?;
                let val = map.get(token_id).cloned();
                self.cached = Some(map);
                Ok(val)
            },
        }
    }

    fn replace_metadata(&mut self, token_id: &TokenId, value: &[u8]) -> Result<(), DigitalAssetError> {
        let mut cached = self.cached.take();
        let txn = self.db.write_transaction()?;
        {
            let mut access = txn.access();
            if cached.is_none() {
                cached = Some(self.load_map(&*access)?);
            };
            let cached_ref = cached.as_mut().unwrap();
            cached_ref.insert(token_id, value.to_vec());
            let encoded = encode_patricia_map(cached_ref.clone())
                .map_err(|_| DigitalAssetError::MalformedMetadata("Failed to encode Patricia map".to_string()))?;
            self.db.replace_metadata(&mut access, PATRICIA_MAP_KEY, &encoded)?;
        }
        txn.commit()?;
        self.cached = cached;

        Ok(())
    }
}

impl Clone for LmdbAssetStore {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            cached: None,
        }
    }
}
fn decode_patricia_nodes<T>(bytes: &[u8]) -> Result<Node<T>, bytecodec::Error>
where for<'de> T: serde::Deserialize<'de> {
    let mut decoder = NodeDecoder::new(BincodeDecoder::new());
    let nodes = decoder.decode_from_bytes(bytes)?;
    Ok(nodes)
}

fn encode_patricia_map<T>(map: PatriciaMap<T>) -> Result<Vec<u8>, bytecodec::Error>
where T: serde::Serialize {
    let mut encoder = NodeEncoder::new(BincodeEncoder::new());
    let encoded = encoder.encode_into_bytes(map.into())?;
    Ok(encoded)
}
