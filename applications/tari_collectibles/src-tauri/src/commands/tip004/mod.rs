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
    models::{address_row::AddressRow, tip721_token_row::Tip721TokenRow},
    AddressesTableGateway, AssetsTableGateway, CollectiblesStorage, StorageTransaction,
    Tip721TokensTableGateway,
  },
};
use log::debug;
use prost::Message;
use tari_common_types::types::PublicKey;
use tari_dan_common_types::proto::tips::tip004;
use tari_utilities::{hex::Hex, ByteArray};
use uuid::Uuid;

const LOG_TARGET: &str = "collectibles::tip004";

#[tauri::command]
pub(crate) async fn tip004_mint_token(
  asset_public_key: String,
  token: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<(), Status> {
  let _wallet_id = state
    .current_wallet_id()
    .await
    .ok_or_else(Status::unauthorized)?;
  let asset_public_key = PublicKey::from_hex(&asset_public_key)?;
  let mut client = state.connect_validator_node_client().await?;
  let db = state.create_db().await?;
  let tx = db.create_transaction()?;
  let _asset = db.assets().find_by_public_key(&asset_public_key, &tx)?;
  drop(tx);

  // TODO: get signature
  let args = tip004::MintRequest {
    token,
    owner: Vec::from(asset_public_key.as_bytes()),
  };
  let mut bytes = vec![];
  args.encode(&mut bytes)?;
  let result = client
    .invoke_method(asset_public_key, 4, "mint".to_string(), bytes)
    .await?;
  dbg!(&result);
  Ok(())
}

#[tauri::command]
pub(crate) async fn tip004_list_tokens(
  asset_public_key: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<(Tip721TokenRow, AddressRow)>, Status> {
  let wallet_id = state
    .current_wallet_id()
    .await
    .ok_or_else(Status::unauthorized)?;
  let asset_public_key = PublicKey::from_hex(&asset_public_key)?;
  let db = state.create_db().await?;
  let tx = db.create_transaction()?;
  let asset = db.assets().find_by_public_key(&asset_public_key, &tx)?;
  let addresses = db
    .addresses()
    .find_by_asset_and_wallet(asset.id, wallet_id, &tx)?;
  let mut client = state.connect_validator_node_client().await?;
  let mut token_ids = vec![];
  for address in addresses {
    let args = tip004::BalanceOfRequest {
      owner: Vec::from(address.public_key.as_bytes()),
    };
    let result = client
      .invoke_read_method(
        asset_public_key.clone(),
        4,
        "balance_of".to_string(),
        args.encode_to_vec(),
      )
      .await?;
    debug!(target: LOG_TARGET, "{:?}", result);
    db.tip721_tokens().delete_all_for_address(address.id, &tx)?;
    if !result.is_empty() {
      let balance_of: tip004::BalanceOfResponse = Message::decode(&*result)?;
      for index in 0..balance_of.num_tokens {
        let args = tip004::TokenOfOwnerByIndexRequest {
          owner: Vec::from(address.public_key.as_bytes()),
          index,
        };
        let token_result = client
          .invoke_read_method(
            asset_public_key.clone(),
            4,
            "token_of_owner_by_index".to_string(),
            args.encode_to_vec(),
          )
          .await?;
        if !token_result.is_empty() {
          let token_data: tip004::TokenOfOwnerByIndexResponse = Message::decode(&*token_result)?;

          let token_row = Tip721TokenRow {
            id: Uuid::new_v4(),
            address_id: address.id,
            token_id: token_data.token_id,
            token: token_data.token.clone(),
          };

          db.tip721_tokens().insert(&token_row, &tx)?;
          token_ids.push((token_row, address.clone()));
        }
      }
    }
  }
  tx.commit()?;

  Ok(token_ids)
}
