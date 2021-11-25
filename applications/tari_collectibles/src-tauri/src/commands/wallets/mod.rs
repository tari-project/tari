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
    models::wallet_row::WalletRow, CollectiblesStorage, StorageTransaction, WalletsTableGateway,
  },
};
use tari_key_manager::mnemonic::{Mnemonic, MnemonicLanguage};
use uuid::Uuid;

#[tauri::command]
pub(crate) async fn wallets_create(
  name: Option<String>,
  _passphrase: Option<String>,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<WalletRow, Status> {
  let new_wallet = WalletRow {
    id: Uuid::new_v4(),
    name,
    // cipher_seed: CipherSeed::new(),
  };

  let db = state.create_db().await?;
  let tx = db.create_transaction()?;
  let _result = db.wallets().insert(&new_wallet, None, &tx)?;
  tx.commit()?;
  state.set_current_wallet_id(new_wallet.id).await;
  Ok(new_wallet)
}

#[tauri::command]
pub(crate) async fn wallets_list(
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<WalletRow>, Status> {
  let db = state.create_db().await?;

  let result = db.wallets().list(None)?;
  Ok(result)
}

#[tauri::command]
pub(crate) async fn wallets_unlock(
  id: Uuid,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<WalletRow, Status> {
  let db = state.create_db().await?;

  let result = db.wallets().find(id, None)?;
  // TODO: decrypt using wallet password
  state.set_current_wallet_id(id).await;
  Ok(result)
}

#[tauri::command]
pub(crate) async fn wallets_seed_words(
  id: Uuid,
  passphrase: Option<String>,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<String>, Status> {
  let db = state.create_db().await?;

  let cipher_seed = db.wallets().get_cipher_seed(id, passphrase.clone(), None)?;

  let seed_words = cipher_seed
    .to_mnemonic(&MnemonicLanguage::English, passphrase)
    .map_err(|e| Status::internal(format!("Could not get seed words:{}", e)))?;

  Ok(seed_words)
}
