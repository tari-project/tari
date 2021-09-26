//  Copyright 2021, The Tari Project
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

use crate::{
    dan_layer::{
        models::TokenId,
        storage::{error::PersistenceError, lmdb::LmdbAssetBackend},
    },
    digital_assets_error::DigitalAssetError,
};
use bytecodec::{
    bincode_codec::{BincodeDecoder, BincodeEncoder},
    DecodeExt,
    EncodeExt,
};
use lmdb_zero::{ConstAccessor, ConstTransaction};
use patricia_tree::{
    node::{NodeDecoder, NodeEncoder},
    PatriciaMap,
};
use serde_json as json;
use std::str;

const PATRICIA_MAP_KEY: u64 = 1u64;

pub trait AssetStore {
    fn get_metadata(&mut self, token_id: &TokenId) -> Result<Option<Vec<u8>>, DigitalAssetError>;

    fn replace_metadata(&mut self, token_id: &TokenId, value: &[u8]) -> Result<(), DigitalAssetError>;
}

pub struct AssetDataStore<TBackend> {
    backend: TBackend,
}

impl<TBackend> AssetDataStore<TBackend>
where TBackend: AssetStore
{
    pub fn new(backend: TBackend) -> Self {
        Self { backend }
    }
}

impl<TBackend> AssetStore for AssetDataStore<TBackend>
where TBackend: AssetStore
{
    fn get_metadata(&mut self, token_id: &TokenId) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        let val = self.backend.get_metadata(token_id)?;
        Ok(val)
    }

    fn replace_metadata(&mut self, token_id: &TokenId, value: &[u8]) -> Result<(), DigitalAssetError> {
        self.backend.replace_metadata(token_id, value)?;
        Ok(())
    }
}
