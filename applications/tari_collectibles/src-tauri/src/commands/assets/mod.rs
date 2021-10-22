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

use crate::{app_state::ConcurrentAppState, models::AssetInfo};
use rand::{rngs::OsRng, Rng};
use serde::Serialize;
use tari_crypto::{hash::blake2::Blake256, keys::PublicKey, ristretto::RistrettoPublicKey};
use tari_mmr::{MemBackendVec, MerkleMountainRange};
use tari_utilities::{hex::Hex, Hashable};

#[tauri::command]
pub(crate) async fn assets_create(
  name: String,
  description: String,
  image: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<String, String> {
  // println!("Hello create asset");
  let mut client = state.create_wallet_client().await;
  client.connect().await?;
  let res = client.register_asset(name, description, image).await?;

  Ok(res)
}

#[tauri::command]
pub(crate) async fn assets_list(
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<AssetInfo>, String> {
  println!("Hello list owned assets");
  let mut client = state.create_wallet_client().await;
  client.connect().await?;
  let assets = client.list_owned_assets().await?;
  Ok(
    assets
      .assets
      .into_iter()
      .map(|a| AssetInfo {
        public_key: a.public_key.to_hex(),
        name: a.name,
        description: a.description,
        image: a.image,
      })
      .collect(),
  )
}

#[tauri::command]
pub(crate) async fn assets_issue_simple_tokens(
  asset_pub_key: String,
  num_tokens: u32,
  committee: Vec<String>,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<(), String> {
  println!("Hello issue simple tokens");

  // let mut pubKeys = vec![];
  let mut mmr = MerkleMountainRange::<Blake256, _>::new(MemBackendVec::new());
  let mut rng = OsRng;
  for i in 0..num_tokens {
    let (private, public) = RistrettoPublicKey::random_keypair(&mut rng);
    // lol, best save that private key somewhere...
    // TODO

    println!("key: {}", public);
    mmr.push(public.hash()).unwrap();
  }

  let root = mmr.get_merkle_root().unwrap();

  println!("New root: {}", root.to_hex());

  let mut client = state.create_wallet_client().await;
  client.connect().await?;

  client
    .create_initial_asset_checkpoint(asset_pub_key, root, committee)
    .await;

  Ok(())
}
