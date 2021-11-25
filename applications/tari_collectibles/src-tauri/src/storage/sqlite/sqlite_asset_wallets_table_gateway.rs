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
  schema::asset_wallets,
  storage::{
    models::asset_wallet_row::AssetWalletRow,
    sqlite::{models, sqlite_transaction::SqliteTransaction},
    AssetWalletsTableGateway, StorageError,
  },
};
use diesel::{prelude::*, RunQueryDsl};
use uuid::Uuid;

pub struct SqliteAssetWalletsTableGateway {}

impl AssetWalletsTableGateway<SqliteTransaction> for SqliteAssetWalletsTableGateway {
  fn insert(&self, row: &AssetWalletRow, tx: &SqliteTransaction) -> Result<(), StorageError> {
    diesel::insert_into(asset_wallets::table)
      .values((
        asset_wallets::id.eq(Vec::from(row.id.as_bytes().as_slice())),
        asset_wallets::wallet_id.eq(Vec::from(row.wallet_id.as_bytes().as_slice())),
        asset_wallets::asset_id.eq(Vec::from(row.asset_id.as_bytes().as_slice())),
      ))
      .execute(tx.connection())?;
    Ok(())
  }

  fn find_by_wallet_id(
    &self,
    wallet_id: Uuid,
    tx: &SqliteTransaction,
  ) -> Result<Vec<AssetWalletRow>, StorageError> {
    let asset_wallets: Vec<models::AssetWallet> = asset_wallets::table
      .filter(asset_wallets::wallet_id.eq(Vec::from(wallet_id.as_bytes().as_slice())))
      .get_results(tx.connection())?;
    let mut result = vec![];
    for aw in asset_wallets {
      result.push(AssetWalletRow {
        id: Uuid::from_slice(aw.id.as_slice())?,
        asset_id: Uuid::from_slice(aw.asset_id.as_slice())?,
        wallet_id: Uuid::from_slice(aw.wallet_id.as_slice())?,
      });
    }
    Ok(result)
  }
}
