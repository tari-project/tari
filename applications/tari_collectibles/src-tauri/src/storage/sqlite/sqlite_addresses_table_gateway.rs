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
  schema::addresses,
  storage::{
    models::address_row::AddressRow,
    sqlite::{models, sqlite_transaction::SqliteTransaction},
    AddressesTableGateway, StorageError,
  },
};
use diesel::RunQueryDsl;
use tari_utilities::ByteArray;

pub struct SqliteAddressesTableGateway {}

impl AddressesTableGateway<SqliteTransaction> for SqliteAddressesTableGateway {
  fn insert(&self, row: &AddressRow, tx: &SqliteTransaction) -> Result<(), StorageError> {
    let model = models::Address {
      id: Vec::from(row.id.as_bytes().as_slice()),
      asset_wallet_id: Vec::from(row.asset_wallet_id.as_bytes().as_slice()),
      name: Some(row.name.clone()),
      public_key: Vec::from(row.public_key.as_bytes()),
      key_manager_path: row.key_manager_path.clone(),
    };
    diesel::insert_into(addresses::table)
      .values(model)
      .execute(tx.connection())?;
    Ok(())
  }
}
