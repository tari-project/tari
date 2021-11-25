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
  app_state::ConcurrentAppState,
  status::Status,
  storage::{
    models::{asset_row::AssetRow, asset_wallet_row::AssetWalletRow},
    AssetWalletsTableGateway, AssetsTableGateway, CollectiblesStorage, StorageTransaction,
  },
};
use prost::Message;
use tari_common_types::types::PublicKey;
use tari_dan_common_types::proto::tips::tip002;
use tari_utilities::{hex::Hex, ByteArray};
use uuid::Uuid;

#[tauri::command]
pub(crate) async fn asset_wallets_create(
  asset_public_key: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<(), Status> {
  let wallet_id = state
    .current_wallet_id()
    .await
    .ok_or_else(Status::unauthorized)?;
  let asset_public_key = PublicKey::from_hex(asset_public_key.as_str())?;
  let mut new_account = AssetRow {
    id: Uuid::new_v4(),
    asset_public_key: asset_public_key.clone(),
    name: None,
    description: None,
    image: None,
    committee: None,
  };

  let mut client = state.connect_base_node_client().await?;
  let chain_registration_data = client.get_asset_metadata(&asset_public_key).await?;
  new_account.name = chain_registration_data.name.clone();
  new_account.description = chain_registration_data.description.clone();
  new_account.image = chain_registration_data.image.clone();

  let sidechain_committee = match client.get_sidechain_committee(&asset_public_key).await {
    Ok(s) => {
      if s.is_empty() {
        None
      } else {
        Some(s)
      }
    }
    Err(e) => {
      dbg!(e);
      None
    }
  };
  new_account.committee = sidechain_committee;
  let db = state.create_db().await?;
  let tx = db.create_transaction()?;
  db.assets().insert(&new_account, &tx)?;
  let new_asset_wallet = AssetWalletRow {
    id: Uuid::new_v4(),
    asset_id: new_account.id,
    wallet_id,
  };
  db.asset_wallets().insert(&new_asset_wallet, &tx)?;
  tx.commit()?;
  Ok(())
}

#[tauri::command]
pub(crate) async fn asset_wallets_get_balance(
  asset_public_key: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<u64, Status> {
  dbg!(&asset_public_key);
  let asset_public_key = PublicKey::from_hex(&asset_public_key)?;

  let owner = PublicKey::default();

  let mut client = state.connect_validator_node_client().await?;
  let args = tip002::BalanceOfRequest {
    owner: Vec::from(owner.as_bytes()),
  };
  dbg!(&args);
  let mut args_bytes = vec![];
  args.encode(&mut args_bytes)?;
  // let req = grpc::InvokeReadMethodRequest{
  //   asset_public_key: Vec::from(asset_public_key.as_bytes()),
  //   template_id: 2,
  //   method: "BalanceOf",
  //   args
  // };

  let resp = client
    .invoke_read_method(asset_public_key, 2, "BalanceOf".to_string(), args_bytes)
    .await?;

  dbg!(&resp);
  match resp {
    Some(resp) => {
      let proto_resp: tip002::BalanceOfResponse = Message::decode(&*resp)?;
      Ok(proto_resp.balance)
    }
    None => Ok(0),
  }
}

#[tauri::command]
pub(crate) async fn asset_wallets_list(
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<AssetRow>, Status> {
  let wallet_id = state
    .current_wallet_id()
    .await
    .ok_or_else(Status::unauthorized)?;
  let db = state.create_db().await?;
  let tx = db.create_transaction()?;
  let mut result = vec![];
  for asset_wallet in db.asset_wallets().find_by_wallet_id(wallet_id, &tx)? {
    result.push(db.assets().find(asset_wallet.asset_id, &tx)?);
  }
  Ok(result)
}
