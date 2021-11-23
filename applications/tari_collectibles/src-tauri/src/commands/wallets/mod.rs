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
  models::{NewWallet, Wallet, WalletInfo},
  storage::{
    models::wallet_row::WalletRow, CollectiblesStorage, StorageTransaction, WalletsTableGateway,
  },
};
use tari_key_manager::{
  cipher_seed::CipherSeed,
  mnemonic::{Mnemonic, MnemonicLanguage},
};
use uuid::Uuid;

#[tauri::command]
pub(crate) async fn wallets_create(
  name: Option<String>,
  passphrase: Option<String>,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<(), String> {
  let new_wallet = WalletRow {
    id: Uuid::new_v4(),
    name,
    // cipher_seed: CipherSeed::new(),
  };

  let db = state
    .create_db()
    .await
    .map_err(|e| format!("Could not connect to DB: {}", e))?;
  let tx = db
    .create_transaction()
    .map_err(|e| format!("Could not start transaction:{}", e))?;
  let result = db
    .wallets()
    .insert(new_wallet, passphrase, &tx)
    .map_err(|e| format!("Could not save wallet: {}", e))?;
  tx.commit()
    .map_err(|e| format!("Could not commit transaction:{}", e))?;
  Ok(())
}

#[tauri::command]
pub(crate) async fn wallets_list(
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<WalletRow>, String> {
  let db = state
    .create_db()
    .await
    .map_err(|e| format!("Could not connect to DB: {}", e))?;

  let result = db
    .wallets()
    .list(None)
    .map_err(|e| format!("Could list wallets from DB: {}", e))?;
  Ok(result)
}

#[tauri::command]
pub(crate) async fn wallets_find(
  id: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<WalletRow, String> {
  let db = state
    .create_db()
    .await
    .map_err(|e| format!("Could not connect to DB: {}", e))?;

  let uuid = Uuid::parse_str(&id).map_err(|e| format!("Failed to parse UUID: {}", e))?;

  let result = db.wallets().find(uuid, None).map_err(|e| e.to_string())?;

  let k = db
    .key_indices()
    .find("assets".into())
    .map_err(|e| e.to_string())?;
  let index = if let Some(k) = k { k.last_index } else { 0 };

  // update the assets key manager for this wallet
  state
    .set_asset_key_manager(result.cipher_seed.clone(), "assets".into(), index)
    .await
    .map_err(|e| e.to_string())?;

  Ok(result)
}

#[tauri::command]
pub(crate) async fn wallets_seed_words(
  id: String,
  passphrase: Option<String>,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<String>, String> {
  let db = state
    .create_db()
    .await
    .map_err(|e| format!("Could not connect to DB: {}", e))?;

  let uuid = Uuid::parse_str(&id).map_err(|e| format!("Failed to parse UUID: {}", e))?;

  let cipher_seed = db
    .wallets()
    .get_cipher_seed(uuid, passphrase.clone(), None)
    .map_err(|e| e.to_string())?;

  let seed_words = cipher_seed
    .to_mnemonic(&MnemonicLanguage::English, passphrase)
    .map_err(|e| format!("Failed to convert cipher seed to seed words: {}", e))?;

  Ok(seed_words)
}
