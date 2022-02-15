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
  models::{AssetInfo, RegisteredAssetInfo, TemplateParameter},
  providers::KeyManagerProvider,
  status::Status,
  storage::{
    models::{
      address_row::AddressRow, asset_row::AssetRow, asset_wallet_row::AssetWalletRow,
      tip002_address_row::Tip002AddressRow,
    },
    AddressesTableGateway, AssetWalletsTableGateway, AssetsTableGateway, CollectiblesStorage,
    StorageTransaction, Tip002AddressesTableGateway,
  },
};

use log::debug;
use tari_app_grpc::tari_rpc::{self};
use tari_common_types::types::{Commitment, PublicKey};
use tari_crypto::{hash::blake2::Blake256, ristretto::RistrettoPublicKey};
use tari_mmr::{MemBackendVec, MerkleMountainRange};
use tari_utilities::{hex::Hex, ByteArray};
use uuid::Uuid;

const LOG_TARGET: &str = "collectibles::assets";

#[tauri::command]
pub(crate) async fn assets_create(
  name: String,
  description: String,
  image: String,
  template_ids: Vec<u32>,
  template_parameters: Vec<TemplateParameter>,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<String, Status> {
  let wallet_id = state
    .current_wallet_id()
    .await
    .ok_or_else(Status::unauthorized)?;

  let mut client = state.create_wallet_client().await;
  client.connect().await?;
  let tp = template_parameters
    .into_iter()
    .map(|t| tari_rpc::TemplateParameter {
      template_data: t.template_data,
      template_data_version: 1,
      template_id: t.template_id,
    })
    .collect();

  let db = state.create_db().await?;
  let transaction = db.create_transaction()?;
  let (key_manager_path, _, asset_public_key) = state
    .key_manager()
    .await
    .generate_asset_public_key(wallet_id, None, &transaction)
    .map_err(|e| Status::internal(format!("could not generate asset public key: {}", e)))?;

  // NOTE: we are blocking the database during this time....
  let res = client
    .register_asset(
      name.clone(),
      asset_public_key.clone(),
      description.clone(),
      image.clone(),
      template_ids.clone(),
      tp,
    )
    .await?;

  let asset_id = Uuid::new_v4();
  let asset_row = AssetRow {
    id: asset_id,
    asset_public_key: asset_public_key.clone(),
    name: Some(name),
    description: Some(description),
    image: Some(image),
    committee: None,
  };
  debug!(target: LOG_TARGET, "asset_row {:?}", asset_row);
  db.assets().insert(&asset_row, &transaction)?;
  let asset_wallet_row = AssetWalletRow {
    id: Uuid::new_v4(),
    asset_id,
    wallet_id,
  };
  debug!(
    target: LOG_TARGET,
    "asset_wallet_row {:?}", asset_wallet_row
  );
  db.asset_wallets().insert(&asset_wallet_row, &transaction)?;
  let address = AddressRow {
    id: Uuid::new_v4(),
    asset_wallet_id: asset_wallet_row.id,
    name: Some("Issuer wallet".to_string()),
    public_key: asset_public_key,
    key_manager_path: key_manager_path.clone(),
  };
  debug!(target: LOG_TARGET, "address {:?}", address);
  db.addresses().insert(&address, &transaction)?;
  if template_ids.contains(&2) {
    let row = Tip002AddressRow {
      id: Uuid::new_v4(),
      address_id: address.id,
      balance: 0,
      at_height: None,
    };
    db.tip002_addresses().insert(&row, &transaction)?;
  }
  transaction.commit()?;

  Ok(res)
}

#[tauri::command]
pub(crate) async fn assets_list_owned(
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<AssetInfo>, Status> {
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

// TODO: remove and use better serializer
#[derive(Debug)]
struct AssetMetadata {
  name: String,
  description: String,
  image: String,
}

trait AssetMetadataDeserializer {
  fn deserialize(&self, metadata: &[u8]) -> AssetMetadata;
}
trait AssetMetadataSerializer {
  fn serialize(&self, model: &AssetMetadata) -> Vec<u8>;
}

struct V1AssetMetadataSerializer {}

impl AssetMetadataDeserializer for V1AssetMetadataSerializer {
  fn deserialize(&self, metadata: &[u8]) -> AssetMetadata {
    let m = String::from_utf8(Vec::from(metadata)).unwrap();
    let mut m = m
      .as_str()
      .split('|')
      .map(|s| s.to_string())
      .collect::<Vec<String>>()
      .into_iter();
    let name = m.next();
    let description = m.next();
    let image = m.next();

    AssetMetadata {
      name: name.unwrap_or_else(|| "".to_string()),
      description: description.unwrap_or_else(|| "".to_string()),
      image: image.unwrap_or_else(|| "".to_string()),
    }
  }
}

impl AssetMetadataSerializer for V1AssetMetadataSerializer {
  fn serialize(&self, model: &AssetMetadata) -> Vec<u8> {
    let str = format!("{}|{}|{}", model.name, model.description, model.image);

    str.into_bytes()
  }
}

#[tauri::command]
pub(crate) async fn assets_list_registered_assets(
  offset: u64,
  count: u64,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<Vec<RegisteredAssetInfo>, Status> {
  let mut client = state.connect_base_node_client().await?;
  let assets = client.list_registered_assets(offset, count).await?;
  let serializer = V1AssetMetadataSerializer {};
  assets
    .into_iter()
    .filter_map(|asset| {
      if let Some(ref features) = asset.features {
        let metadata = serializer.deserialize(&features.metadata[1..]);

        // TODO: Find a better way of reading the metadata
        Some(Ok(RegisteredAssetInfo {
          owner_commitment: Commitment::from_bytes(&asset.owner_commitment).ok(),
          asset_public_key: RistrettoPublicKey::from_bytes(&asset.asset_public_key).ok(),
          unique_id: asset.unique_id,
          mined_height: asset.mined_height,
          mined_in_block: asset.mined_in_block,
          features: features.clone().into(),
          name: metadata.name.clone(),
          description: Some(metadata.description.clone()),
          image: Some(metadata.image),
        }))
      } else {
        None
      }
    })
    .collect()
}

#[tauri::command]
pub(crate) async fn assets_create_initial_checkpoint(
  asset_public_key: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<(), Status> {
  let mmr = MerkleMountainRange::<Blake256, _>::new(MemBackendVec::new());

  let merkle_root = mmr.get_merkle_root().unwrap();

  let mut client = state.create_wallet_client().await;
  client.connect().await?;

  // todo: check for enough utxos first

  // create asset reg checkpoint
  client
    .create_initial_asset_checkpoint(&asset_public_key, merkle_root)
    .await?;

  Ok(())
}

#[tauri::command]
pub(crate) async fn assets_create_committee_definition(
  asset_public_key: String,
  committee: Vec<String>,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<(), Status> {
  let mut client = state.create_wallet_client().await;
  client.connect().await?;

  // TODO: effective sidechain height...
  client
    .create_committee_definition(&asset_public_key, committee, 0)
    .await?;

  Ok(())
}

#[tauri::command]
pub(crate) async fn assets_get_registration(
  asset_pub_key: String,
  state: tauri::State<'_, ConcurrentAppState>,
) -> Result<RegisteredAssetInfo, Status> {
  let mut client = state.connect_base_node_client().await?;
  let asset_pub_key = PublicKey::from_hex(&asset_pub_key)?;
  let asset = client.get_asset_metadata(&asset_pub_key).await?;

  debug!(target: LOG_TARGET, "asset {:?}", asset);
  let features = asset.features.unwrap();
  let serializer = V1AssetMetadataSerializer {};
  let metadata = serializer.deserialize(&features.metadata[1..]);

  Ok(RegisteredAssetInfo {
    owner_commitment: Commitment::from_bytes(&asset.owner_commitment).ok(),
    asset_public_key: RistrettoPublicKey::from_bytes(&features.unique_id).ok(),
    unique_id: features.unique_id.clone(),
    mined_height: asset.mined_height,
    mined_in_block: asset.mined_in_block,
    features: features.into(),
    name: metadata.name.clone(),
    description: Some(metadata.description.clone()),
    image: Some(metadata.image),
  })
}
