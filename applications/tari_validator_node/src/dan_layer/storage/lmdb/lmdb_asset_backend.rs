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

use lmdb_zero as lmdb;
use lmdb_zero::{put, ConstAccessor, LmdbResultExt, ReadTransaction, WriteAccessor, WriteTransaction};
use std::{fs, fs::File, path::Path, sync::Arc};

use tari_common::file_lock;
use tari_storage::lmdb_store::{DatabaseRef, LMDBConfig};

use crate::dan_layer::storage::{lmdb::helpers::create_lmdb_store, StorageError};

#[derive(Clone)]
pub struct LmdbAssetBackend {
    _file_lock: Arc<File>,
    env: Arc<lmdb::Environment>,
    metadata_db: DatabaseRef,
}

impl LmdbAssetBackend {
    pub(crate) fn initialize<P: AsRef<Path>>(path: P, config: LMDBConfig) -> Result<Self, StorageError> {
        fs::create_dir_all(&path)?;
        let file_lock = file_lock::try_lock_exclusive(path.as_ref())?;
        let store = create_lmdb_store(path, config)?;

        Ok(Self {
            _file_lock: Arc::new(file_lock),
            env: store.env(),
            metadata_db: store.get_handle("metadata").unwrap().db(),
        })
    }

    pub fn read_transaction(&self) -> Result<ReadTransaction<'_>, StorageError> {
        Ok(ReadTransaction::new(&*self.env)?)
    }

    pub fn write_transaction(&self) -> Result<WriteTransaction<'_>, StorageError> {
        Ok(WriteTransaction::new(&*self.env)?)
    }

    pub fn get_metadata<'a>(&self, access: &'a ConstAccessor<'_>, key: u64) -> Result<Option<&'a [u8]>, StorageError> {
        let val = access.get::<_, [u8]>(&*self.metadata_db, &key).to_opt()?;
        Ok(val)
    }

    pub fn replace_metadata(
        &self,
        access: &mut WriteAccessor<'_>,
        key: u64,
        metadata: &[u8],
    ) -> Result<(), StorageError> {
        access.put(&self.metadata_db, &key, metadata, put::Flags::empty())?;
        Ok(())
    }
}
