//  Copyright 2022. The Tari Project
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

use chrono::NaiveDateTime;
use diesel::{prelude::*, SqliteConnection};

use crate::{key_manager_service::error::KeyManagerStorageError, schema::key_manager_states_old};

// This is a temporary migration file to convert existing indexes to new ones.
// Todo remove at next testnet reset (currently on Dibbler) #testnet_reset
#[derive(Clone, Debug, Queryable, Identifiable)]
#[table_name = "key_manager_states_old"]
#[primary_key(id)]
pub struct KeyManagerStateSqlOld {
    pub id: i32,
    pub seed: Vec<u8>,
    pub branch_seed: String,
    pub primary_key_index: i64,
    pub timestamp: NaiveDateTime,
}

impl KeyManagerStateSqlOld {
    pub fn index(conn: &SqliteConnection) -> Result<Vec<KeyManagerStateSqlOld>, KeyManagerStorageError> {
        Ok(key_manager_states_old::table.load::<KeyManagerStateSqlOld>(conn)?)
    }

    pub fn delete(conn: &SqliteConnection) -> Result<(), KeyManagerStorageError> {
        diesel::delete(key_manager_states_old::dsl::key_manager_states_old).execute(conn)?;
        Ok(())
    }
}
