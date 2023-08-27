//  Copyright 2021, The Taiji Project
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

use lmdb_zero::error;
use log::*;
use serde::{de::DeserializeOwned, Serialize};

use crate::chain_storage::ChainStorageError;

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb";

pub fn serialize<T>(data: &T) -> Result<Vec<u8>, ChainStorageError>
where T: Serialize {
    let size = bincode::serialized_size(&data).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    #[allow(clippy::cast_possible_truncation)]
    let mut buf = Vec::with_capacity(size as usize);
    bincode::serialize_into(&mut buf, data).map_err(|e| {
        error!(target: LOG_TARGET, "Could not serialize lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;
    Ok(buf)
}

pub fn deserialize<T>(buf_bytes: &[u8]) -> Result<T, error::Error>
where T: DeserializeOwned {
    bincode::deserialize(buf_bytes)
        .map_err(|e| {
            error!(target: LOG_TARGET, "Could not deserialize lmdb: {:?}", e);
            e
        })
        .map_err(|e| error::Error::ValRejected(e.to_string()))
}
