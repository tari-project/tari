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
  schema::tip002_address::dsl::tip002_address,
  storage::{
    models::tip002_address_row::Tip002AddressRow,
    sqlite::{models::Tip002Address, sqlite_transaction::SqliteTransaction},
    StorageError, Tip002AddressesTableGateway,
  },
};
use diesel::RunQueryDsl;

pub struct SqliteTip002AddressesTableGateway {}

impl Tip002AddressesTableGateway<SqliteTransaction> for SqliteTip002AddressesTableGateway {
  fn insert(&self, row: &Tip002AddressRow, tx: &SqliteTransaction) -> Result<(), StorageError> {
    let db_row = Tip002Address {
      id: Vec::from(row.id.as_bytes().as_slice()),
      address_id: Vec::from(row.address_id.as_bytes().as_slice()),
      balance: 0,
      at_height: None,
    };
    diesel::insert_into(tip002_address)
      .values(db_row)
      .execute(tx.connection())?;
    Ok(())
  }
}
