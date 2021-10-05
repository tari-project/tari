// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    assets::Asset,
    error::WalletError,
    output_manager_service::storage::database::{OutputManagerBackend, OutputManagerDatabase},
};
use tari_core::transactions::transaction::{OutputFeatures, OutputFlags, Transaction};

use crate::{
    output_manager_service::{handle::OutputManagerHandle, storage::models::DbUnblindedOutput},
    types::PersistentKeyManager,
};
use log::*;
use tari_common_types::types::{Commitment, PublicKey};
use tari_core::transactions::transaction_protocol::TxId;
use tari_crypto::tari_utilities::ByteArray;

const LOG_TARGET: &str = "wallet::assets::asset_manager";

pub(crate) struct AssetManager<T: OutputManagerBackend + 'static, TPersistentKeyManager: PersistentKeyManager> {
    output_database: OutputManagerDatabase<T>,
    output_manager: OutputManagerHandle,
    assets_key_manager: TPersistentKeyManager, // transaction_service: TransactionServiceHandle
}
impl<T: OutputManagerBackend + 'static, TPersistentKeyManager: PersistentKeyManager>
    AssetManager<T, TPersistentKeyManager>
{
    pub fn new(backend: T, output_manager: OutputManagerHandle, assets_key_manager: TPersistentKeyManager) -> Self {
        Self {
            output_database: OutputManagerDatabase::new(backend),
            output_manager,
            assets_key_manager,
        }
    }

    pub async fn list_owned(&self) -> Result<Vec<Asset>, WalletError> {
        let outputs = self
            .output_database
            .fetch_with_features(OutputFlags::ASSET_REGISTRATION)
            .await
            .map_err(|err| WalletError::OutputManagerError(err.into()))?;

        debug!(
            target: LOG_TARGET,
            "Found {} owned outputs that contain assets",
            outputs.len()
        );
        let assets: Vec<Asset> = outputs.into_iter().map(convert_to_asset).collect::<Result<_, _>>()?;
        Ok(assets)
    }

    pub async fn get_owned_asset_by_pub_key(&self, public_key: PublicKey) -> Result<Asset, WalletError> {
        let output = self
            .output_database
            .fetch_by_features_asset_public_key(public_key)
            .map_err(|err| WalletError::OutputManagerError(err.into()))?;
        Ok(convert_to_asset(output)?)
    }

    pub async fn create_registration_transaction(&mut self, name: String) -> Result<(TxId, Transaction), WalletError> {
        let serializer = V1AssetMetadataSerializer {};

        let metadata = AssetMetadata { name };
        let mut metadata_bin = vec![1u8];
        metadata_bin.extend(serializer.serialize(&metadata).into_iter());

        let public_key = self.assets_key_manager.create_and_store_new()?;
        let public_key_bytes = public_key.to_vec();
        let output = self
            .output_manager
            .create_output_with_features(
                0.into(),
                OutputFeatures::for_asset_registration(metadata_bin, public_key),
                Some(public_key_bytes),
                None,
            )
            .await?;
        debug!(target: LOG_TARGET, "Created output: {:?}", output);
        let (tx_id, transaction) = self
            .output_manager
            .create_send_to_self_with_output(0.into(), vec![output], 100.into())
            .await?;
        Ok((tx_id, transaction))
    }

    pub async fn create_minting_transaction(
        &mut self,
        asset_public_key: PublicKey,
        asset_owner_commitment: Commitment,
        unique_ids: Vec<Vec<u8>>,
    ) -> Result<(TxId, Transaction), WalletError> {
        let mut outputs = Vec::with_capacity(unique_ids.len());
        // TODO: generate proof of ownership
        for id in unique_ids {
            let output = self
                .output_manager
                .create_output_with_features(
                    0.into(),
                    OutputFeatures::for_minting(vec![], asset_public_key.clone(), asset_owner_commitment.clone()),
                    Some(id),
                    Some(asset_public_key.clone()),
                )
                .await?;
            outputs.push(output);
        }

        let (tx_id, transaction) = self
            .output_manager
            .create_send_to_self_with_output(0.into(), outputs, 100.into())
            .await?;
        Ok((tx_id, transaction))
    }

    pub async fn create_initial_asset_checkpoint(
        &mut self,
        asset: Asset,
        merkle_root: Vec<u8>,
    ) -> Result<(TxId, Transaction), WalletError> {
        let output = self
            .output_manager
            .create_output_with_features(
                0.into(),
                OutputFeatures::for_checkpoint(merkle_root),
                Some([0u8; 64].to_vec()),
                Some(asset.public_key().clone()),
            )
            .await?;
        let (tx_id, transaction) = self
            .output_manager
            .create_send_to_self_with_output(0.into(), vec![output], 0.into())
            .await?;
        Ok((tx_id, transaction))
    }
}

fn convert_to_asset(unblinded_output: DbUnblindedOutput) -> Result<Asset, WalletError> {
    if unblinded_output.unblinded_output.features.metadata.is_empty() {
        // TODO: sort out unwraps
        return Ok(Asset::new(
            "<Invalid metadata:empty>".to_string(),
            unblinded_output.status.to_string(),
            unblinded_output
                .unblinded_output
                .features
                .asset
                .as_ref()
                .map(|a| a.public_key.clone())
                .unwrap(),
            unblinded_output.commitment,
        ));
    }
    let version = unblinded_output.unblinded_output.features.metadata[0];

    let deserializer = get_deserializer(version);

    let metadata = deserializer.deserialize(&unblinded_output.unblinded_output.features.metadata[1..]);
    Ok(Asset::new(
        metadata.name,
        unblinded_output.status.to_string(),
        unblinded_output
            .unblinded_output
            .features
            .asset
            .as_ref()
            .map(|a| a.public_key.clone())
            .unwrap(),
        unblinded_output.commitment,
    ))
}

fn get_deserializer(_version: u8) -> impl AssetMetadataDeserializer {
    V1AssetMetadataSerializer {}
}

pub trait AssetMetadataDeserializer {
    fn deserialize(&self, metadata: &[u8]) -> AssetMetadata;
}
pub trait AssetMetadataSerializer {
    fn serialize(&self, model: &AssetMetadata) -> Vec<u8>;
}

pub struct V1AssetMetadataSerializer {}

// TODO: Replace with proto serializer
impl AssetMetadataDeserializer for V1AssetMetadataSerializer {
    fn deserialize(&self, metadata: &[u8]) -> AssetMetadata {
        AssetMetadata {
            name: String::from_utf8(Vec::from(metadata)).unwrap(),
        }
    }
}

impl AssetMetadataSerializer for V1AssetMetadataSerializer {
    fn serialize(&self, model: &AssetMetadata) -> Vec<u8> {
        model.name.clone().into_bytes()
    }
}

pub struct AssetMetadata {
    name: String,
}
