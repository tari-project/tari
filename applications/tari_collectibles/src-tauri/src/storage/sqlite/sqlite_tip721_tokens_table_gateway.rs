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
  schema::*,
  storage::{
    models::tip721_token_row::Tip721TokenRow,
    sqlite::{models, SqliteTransaction},
    StorageError, Tip721TokensTableGateway,
  },
};
use diesel::prelude::*;
use uuid::Uuid;

pub struct SqliteTip721TokensTableGateway {}

impl Tip721TokensTableGateway<SqliteTransaction> for SqliteTip721TokensTableGateway {
  fn insert(&self, row: &Tip721TokenRow, tx: &SqliteTransaction) -> Result<(), StorageError> {
    let existing: Option<models::Tip721Token> = tip721_tokens::table
      .filter(tip721_tokens::address_id.eq(Vec::from(row.address_id.as_bytes().as_slice())))
      .filter(tip721_tokens::token_id.eq(&row.token_id))
      .first(tx.connection())
      .optional()?;
    match existing {
      Some(existing) => {
        diesel::update(&existing)
          .set(tip721_tokens::is_deleted.eq(false))
          .execute(tx.connection())?;
      }
      None => {
        diesel::insert_into(tip721_tokens::table)
          .values(models::Tip721Token {
            id: Vec::from(row.id.as_bytes().as_slice()),
            address_id: Vec::from(row.address_id.as_bytes().as_slice()),
            token_id: row.token_id.clone(),
            is_deleted: false,
            token: row.token.clone(),
          })
          .execute(tx.connection())?;
      }
    }
    Ok(())
  }

  fn delete_all_for_address(
    &self,
    address_id: Uuid,
    tx: &SqliteTransaction,
  ) -> Result<(), StorageError> {
    diesel::update(
      tip721_tokens::table
        .filter(tip721_tokens::address_id.eq(Vec::from(address_id.as_bytes().as_slice()))),
    )
    .set(tip721_tokens::is_deleted.eq(true))
    .execute(tx.connection())?;
    Ok(())
  }
}
