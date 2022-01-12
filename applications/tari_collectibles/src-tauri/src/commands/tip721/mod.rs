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
  storage::{AddressesTableGateway, AssetsTableGateway, CollectiblesStorage},
};
use prost::Message;
use tari_common_types::types::PublicKey;
use tari_dan_common_types::proto::tips::tip721;
use tari_utilities::{hex::Hex, ByteArray};
use uuid::Uuid;

#[tauri::command]
pub(crate) async fn tip721_transfer_from(
  asset_public_key: String,
  token_id: String,
  send_to_address: String,
  from_address_id: Uuid,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<(), Status> {
  let wallet_id = state
    .current_wallet_id()
    .await
    .ok_or_else(Status::unauthorized)?;
  let asset_public_key = PublicKey::from_hex(&asset_public_key)?;
  let send_to_address = PublicKey::from_hex(&send_to_address)?;
  let db = state.create_db().await?;
  let tx = db.create_transaction()?;
  let asset = db.assets().find_by_public_key(&asset_public_key, &tx)?;
  let asset_addresses = db
    .addresses()
    .find_by_asset_and_wallet(asset.id, wallet_id, &tx)?;
  let from_address = asset_addresses
    .into_iter()
    .find(|aa| aa.id == from_address_id)
    .ok_or_else(|| Status::not_found("address".to_string()))?;
  drop(tx);
  let mut client = state.connect_validator_node_client().await?;
  let transfer_request = tip721::TransferFromRequest {
    from: Vec::from(from_address.public_key.as_bytes()),
    to: Vec::from(send_to_address.as_bytes()),
    token_id: Vec::from_hex(&token_id)?,
  };
  let transfer_request = transfer_request.encode_to_vec();

  let res = client
    .invoke_method(
      asset_public_key.clone(),
      721,
      "transfer_from".to_string(),
      transfer_request,
    )
    .await?;
  dbg!(&res);
  Ok(())
}
