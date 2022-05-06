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
  schema::{self, *},
  storage::{
    models::asset_row::AssetRow,
    sqlite::{models, sqlite_transaction::SqliteTransaction},
    AssetsTableGateway, StorageError,
  },
};
use diesel::prelude::*;
use std::convert::TryFrom;
use tari_common_types::types::PublicKey;

use tari_utilities::ByteArray;
use uuid::Uuid;

pub struct SqliteAssetsTableGateway {
  pub database_url: String,
}

impl AssetsTableGateway<SqliteTransaction> for SqliteAssetsTableGateway {
  fn list(&self, tx: &SqliteTransaction) -> Result<Vec<AssetRow>, StorageError> {
    let results: Vec<models::Asset> = schema::assets::table
      .order_by(schema::assets::name.asc())
      .load(tx.connection())?;
    results
      .iter()
      .map(SqliteAssetsTableGateway::convert_asset)
      .collect::<Result<_, _>>()
  }

  #[allow(clippy::cast_possible_wrap)]
  #[allow(clippy::cast_possible_truncation)]
  fn insert(&self, asset: &AssetRow, tx: &SqliteTransaction) -> Result<(), StorageError> {
    let mut committee_pub_keys = vec![];
    if let Some(pub_keys) = asset.committee.as_ref() {
      for key in pub_keys {
        committee_pub_keys.extend_from_slice(key.as_bytes());
      }
    }
    // let committee_pub_keys = if committee_pub_keys.is_empty() { None} else {Some(committee_pub_keys)};

    let sql_model = models::Asset {
      id: Vec::from(asset.id.as_bytes().as_slice()),
      asset_public_key: Vec::from(asset.asset_public_key.as_bytes()),
      name: asset.name.clone(),
      description: asset.description.clone(),
      image: asset.image.clone(),
      committee_length: asset
        .committee
        .as_ref()
        .map(|s| s.len() as i32)
        .unwrap_or(0i32),
      committee_pub_keys,
    };

    diesel::insert_into(assets::table)
      .values(sql_model)
      .execute(tx.connection())?;

    Ok(())
  }

  fn find(&self, asset_id: Uuid, tx: &SqliteTransaction) -> Result<AssetRow, StorageError> {
    let db_account = schema::assets::table
      .find(Vec::from(asset_id.as_bytes().as_slice()))
      .get_result(tx.connection())?;

    SqliteAssetsTableGateway::convert_asset(&db_account)
  }

  fn find_by_public_key(
    &self,
    public_key: &PublicKey,
    tx: &SqliteTransaction,
  ) -> Result<AssetRow, StorageError> {
    let db_account = schema::assets::table
      .filter(assets::asset_public_key.eq(Vec::from(public_key.as_bytes())))
      .first(tx.connection())?;

    SqliteAssetsTableGateway::convert_asset(&db_account)
  }
}

impl SqliteAssetsTableGateway {
  fn convert_asset(r: &models::Asset) -> Result<AssetRow, StorageError> {
    let mut committee = Vec::with_capacity(usize::try_from(r.committee_length).unwrap());
    for i in 0..usize::try_from(r.committee_length).unwrap() {
      committee.push(PublicKey::from_bytes(&r.committee_pub_keys[i * 32..(i + 1) * 32]).unwrap());
    }
    Ok(AssetRow {
      id: Uuid::from_slice(&r.id).unwrap(),
      asset_public_key: PublicKey::from_bytes(&r.asset_public_key).unwrap(),
      name: r.name.clone(),
      description: r.description.clone(),
      image: r.image.clone(),
      committee: if committee.is_empty() {
        None
      } else {
        Some(committee)
      },
    })
  }
}
