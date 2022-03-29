use std::convert::TryFrom;

use tari_dan_core::models::TreeNodeHash;
//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that
// the  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES,  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL,  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY,  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.
use tari_dan_core::storage::state::DbStateOpLogEntry;

use crate::{error::SqliteStorageError, schema::*};

#[derive(Debug, Clone, Identifiable, Queryable)]
#[table_name = "state_op_log"]
pub struct StateOpLogEntry {
    pub id: i32,
    pub height: i64,
    pub merkle_root: Option<Vec<u8>>,
    pub operation: String,
    pub schema: String,
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Insertable)]
#[table_name = "state_op_log"]
pub struct NewStateOpLogEntry {
    pub height: i64,
    pub merkle_root: Option<Vec<u8>>,
    pub operation: String,
    pub schema: String,
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
}

impl From<DbStateOpLogEntry> for NewStateOpLogEntry {
    fn from(entry: DbStateOpLogEntry) -> Self {
        Self {
            height: entry.height as i64,
            merkle_root: entry.merkle_root.map(|r| r.as_bytes().to_vec()),
            operation: entry.operation.as_op_str().to_string(),
            schema: entry.schema,
            key: entry.key,
            value: entry.value,
        }
    }
}

impl TryFrom<StateOpLogEntry> for DbStateOpLogEntry {
    type Error = SqliteStorageError;

    fn try_from(entry: StateOpLogEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            height: entry.height as u64,
            merkle_root: entry
                .merkle_root
                .map(TreeNodeHash::try_from)
                .transpose()
                .map_err(|_| SqliteStorageError::MalformedHashData)?,
            operation: entry
                .operation
                .parse()
                .map_err(|_| SqliteStorageError::MalformedDbData("Invalid OpLog operation".to_string()))?,
            schema: entry.schema,
            key: entry.key,
            value: entry.value,
        })
    }
}
