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
  models::Tip002Info,
};
use uuid::Uuid;

#[tauri::command]
pub async fn tip002_get_info(
  _account_id: Uuid,
  _asset_public_key: String,
  _state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Option<Tip002Info>, String> {
  // let db = state
  //   .create_db()
  //   .await
  //   .map_err(|e| format!("Could not open DB:{}", e))?;
  // let account = db
  //   .accounts()
  //   .find(account_id)
  //   .map_err(|e| format!("Could not find account: {}", e))?;
  // let committee = account.committee;
  // let validator_client = state.connect_validator_node_client(committee[0]).await?;
  // let asset_public_key = PublicKey::from_hex(asset_public_key
  // let template_data = tari_tips::tip002::InfoRequest{
  //
  // };
  // let template_data = template_data.encode_to_vec();
  // validator_client.call(comittee[0], "tip002", )
  todo!()
}
