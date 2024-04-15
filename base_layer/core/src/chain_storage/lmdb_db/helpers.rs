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

use std::time::Instant;

use lmdb_zero::error;
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use tari_storage::lmdb_store::BYTES_PER_MB;

use crate::chain_storage::ChainStorageError;

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb";

/// Serialize the given data into a byte vector
/// Note:
///   `size_hint` is given as an option as checking what the serialized would be is expensive
///   for large data structures at ~30% overhead
pub fn serialize<T>(data: &T, size_hint: Option<usize>) -> Result<Vec<u8>, ChainStorageError>
where T: Serialize {
    let start = Instant::now();
    let mut buf = if let Some(size) = size_hint {
        Vec::with_capacity(size)
    } else {
        let size = bincode::serialized_size(&data).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        #[allow(clippy::cast_possible_truncation)]
        Vec::with_capacity(size as usize)
    };
    let check_time = start.elapsed();
    bincode::serialize_into(&mut buf, data).map_err(|e| {
        error!(target: LOG_TARGET, "Could not serialize lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;
    if buf.len() >= BYTES_PER_MB {
        let serialize_time = start.elapsed() - check_time;
        trace!(
            "lmdb_replace - {} MB, serialize check in {:.2?}, serialize in {:.2?}",
            buf.len() / BYTES_PER_MB,
            check_time,
            serialize_time
        );
    }
    if let Some(size) = size_hint {
        if buf.len() > size {
            warn!(
                target: LOG_TARGET,
                "lmdb_replace - Serialized size hint was too small. Expected {}, got {}", size, buf.len()
            );
        }
    }
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
