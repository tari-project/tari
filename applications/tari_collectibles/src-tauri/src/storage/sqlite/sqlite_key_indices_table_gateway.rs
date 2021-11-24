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

use crate::{
  schema,
  schema::key_indices,
  storage::{
    models::key_index_row::KeyIndexRow,
    sqlite::{models, sqlite_transaction::SqliteTransaction},
    KeyIndicesTableGateway, StorageError,
  },
};
use diesel::{prelude::*, Connection, SqliteConnection};
use uuid::Uuid;

pub struct SqliteKeyIndicesTableGateway {
  pub database_url: String,
}

impl KeyIndicesTableGateway<SqliteTransaction> for SqliteKeyIndicesTableGateway {
  fn list(&self, tx: Option<&SqliteTransaction>) -> Result<Vec<KeyIndexRow>, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let results: Vec<models::KeyIndex> = schema::key_indices::table.load(&conn)?;
    Ok(
      results
        .iter()
        .map(|k| KeyIndexRow {
          id: Uuid::from_slice(&k.id).unwrap(),
          branch_seed: k.branch_seed.clone(),
          last_index: k.last_index as u64,
        })
        .collect(),
    )
  }

  fn insert(&self, key_index: KeyIndexRow, tx: &SqliteTransaction) -> Result<(), StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let find_result: Option<models::KeyIndex> = schema::key_indices::table
      .filter(key_indices::branch_seed.eq(key_index.branch_seed.clone()))
      .first(&conn)
      .optional()?;

    let result = match find_result {
      Some(k) => {
        // update existing
        let id = k.id.clone();
        diesel::update(key_indices::table.filter(key_indices::id.eq(id)))
          .set(key_indices::last_index.eq(key_index.last_index as i64))
          .execute(&conn)?;
      }
      None => {
        let sql_model = models::KeyIndex {
          id: Vec::from(key_index.id.as_bytes().as_slice()),
          branch_seed: key_index.branch_seed.clone(),
          last_index: key_index.last_index as i64,
        };
        diesel::insert_into(key_indices::table)
          .values(sql_model)
          .execute(&conn)?;
      }
    };

    Ok(())
  }

  fn find(
    &self,
    branch_seed: String,
    tx: Option<&SqliteTransaction>,
  ) -> Result<Option<KeyIndexRow>, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let result: Option<models::KeyIndex> = schema::key_indices::table
      .filter(key_indices::branch_seed.eq(branch_seed))
      .first(&conn)
      .optional()?;

    Ok(result.map(|k| KeyIndexRow {
      id: Uuid::from_slice(&k.id).unwrap(),
      branch_seed: k.branch_seed.clone(),
      last_index: k.last_index as u64,
    }))
  }
}
