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
  models::{Account, NewAccount},
  storage::{AccountsTableGateway, CollectiblesStorage},
};
use tari_common_types::types::PublicKey;
use tari_utilities::hex::Hex;

#[tauri::command]
pub(crate) async fn accounts_create(
  asset_pub_key: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Account, String> {
  let asset_pub_key =
    PublicKey::from_hex(asset_pub_key.as_str()).map_err(|e| format!("Invalid public key:{}", e))?;
  let mut new_account = NewAccount {
    asset_public_key: asset_pub_key.clone(),
    name: None,
    description: None,
    image: None,
    committee: None,
  };

  let mut client = state.connect_base_node_client().await?;
  let chain_registration_data = client.get_asset_metadata(&asset_pub_key).await?;
  new_account.name = chain_registration_data.name.clone();
  new_account.description = chain_registration_data.description.clone();
  new_account.image = chain_registration_data.image.clone();

  let sidechain_committee = match client.get_sidechain_committee(&asset_pub_key).await {
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

  let result = state
    .create_db()
    .await
    .map_err(|e| format!("Could not connect to DB:{}", e))?
    .accounts()
    .insert(new_account)
    .map_err(|e| format!("Could not save account: {}", e))?;
  Ok(result)
}

#[tauri::command]
pub(crate) async fn accounts_list(
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<Account>, String> {
  let db = state
    .create_db()
    .await
    .map_err(|e| format!("Could not connect to DB:{}", e))?;
  let result = db
    .accounts()
    .list()
    .map_err(|e| format!("Could list accounts from DB: {}", e))?;
  Ok(result)
}
