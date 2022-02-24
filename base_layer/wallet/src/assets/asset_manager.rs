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

use log::*;
use tari_common_types::{
    transaction::TxId,
    types::{Commitment, FixedHash, PublicKey},
};
use tari_core::transactions::transaction_components::{OutputFeatures, OutputFlags, TemplateParameter, Transaction};

use crate::{
    assets::Asset,
    error::WalletError,
    output_manager_service::{
        handle::OutputManagerHandle,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::DbUnblindedOutput,
        },
    },
};

const LOG_TARGET: &str = "wallet::assets::asset_manager";
const ASSET_FPG: u64 = 10;

pub(crate) struct AssetManager<T: OutputManagerBackend + 'static> {
    output_database: OutputManagerDatabase<T>,
    output_manager: OutputManagerHandle,
}
impl<T: OutputManagerBackend + 'static> AssetManager<T> {
    pub fn new(backend: T, output_manager: OutputManagerHandle) -> Self {
        Self {
            output_database: OutputManagerDatabase::new(backend),
            output_manager,
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

    pub async fn create_registration_transaction(
        &mut self,
        name: String,
        public_key: PublicKey,
        description: Option<String>,
        image: Option<String>,
        template_ids_implemented: Vec<u32>,
        template_parameters: Vec<TemplateParameter>,
    ) -> Result<(TxId, Transaction), WalletError> {
        let serializer = V1AssetMetadataSerializer {};

        let metadata = AssetMetadata {
            name,
            description: description.unwrap_or_default(),
            image: image.unwrap_or_default(),
        };
        let mut metadata_bin = vec![1u8];
        metadata_bin.extend(serializer.serialize(&metadata).into_iter());

        // let public_key = self.assets_key_manager.create_and_store_new()?;
        let output = self
            .output_manager
            .create_output_with_features(
                0.into(),
                OutputFeatures::for_asset_registration(
                    metadata_bin,
                    public_key,
                    template_ids_implemented,
                    template_parameters,
                ),
            )
            .await?;
        debug!(target: LOG_TARGET, "Created output: {:?}", output);
        let (tx_id, transaction) = self
            .output_manager
            .create_send_to_self_with_output(vec![output], ASSET_FPG.into(), None, None)
            .await?;
        Ok((tx_id, transaction))
    }

    pub async fn create_minting_transaction(
        &mut self,
        asset_public_key: PublicKey,
        asset_owner_commitment: Commitment,
        features: Vec<(Vec<u8>, Option<OutputFeatures>)>,
    ) -> Result<(TxId, Transaction), WalletError> {
        let mut outputs = Vec::with_capacity(features.len());
        // TODO: generate proof of ownership
        for (unique_id, token_features) in features {
            let output = self
                .output_manager
                .create_output_with_features(
                    0.into(),
                    OutputFeatures::for_minting(
                        asset_public_key.clone(),
                        asset_owner_commitment.clone(),
                        unique_id,
                        token_features,
                    ),
                )
                .await?;
            outputs.push(output);
        }

        let (tx_id, transaction) = self
            .output_manager
            .create_send_to_self_with_output(outputs, ASSET_FPG.into(), None, None)
            .await?;
        Ok((tx_id, transaction))
    }

    pub async fn create_initial_asset_checkpoint(
        &mut self,
        asset_pub_key: PublicKey,
        merkle_root: FixedHash,
        committee_pub_keys: Vec<PublicKey>,
    ) -> Result<(TxId, Transaction), WalletError> {
        let output = self
            .output_manager
            .create_output_with_features(
                0.into(),
                OutputFeatures::for_checkpoint(
                    asset_pub_key,
                    vec![3u8; 32],
                    merkle_root,
                    committee_pub_keys.clone(),
                    true,
                ),
            )
            .await?;
        // TODO: get consensus threshold from somewhere else
        // TODO: Put the multisig script back
        // let n = committee_pub_keys.len();
        // if n > u8::MAX as usize {
        //     return Err(WalletError::ArgumentError {
        //         argument: "committee_pub_keys".to_string(),
        //         message: "Cannot be more than 255".to_string(),
        //         value: n.to_string(),
        //     });
        // }
        // let max_failures = n / 3;
        // let m = max_failures * 2 + 1;
        // let mut msg = [0u8; 32];
        // msg.copy_from_slice("Need a better message12345678901".as_bytes());
        //
        // let output = output.with_script(script!(CheckMultiSig(
        //     m as u8,
        //     n as u8,
        //     committee_pub_keys,
        //     Box::new(msg)
        // )));
        let (tx_id, transaction) = self
            .output_manager
            .create_send_to_self_with_output(vec![output], ASSET_FPG.into(), None, None)
            .await?;
        Ok((tx_id, transaction))
    }

    pub async fn create_follow_on_asset_checkpoint(
        &mut self,
        asset_pub_key: PublicKey,
        unique_id: Vec<u8>,
        merkle_root: FixedHash,
        committee_pub_keys: Vec<PublicKey>,
    ) -> Result<(TxId, Transaction), WalletError> {
        let output = self
            .output_manager
            .create_output_with_features(
                0.into(),
                OutputFeatures::for_checkpoint(
                    asset_pub_key.clone(),
                    unique_id.clone(),
                    merkle_root,
                    committee_pub_keys.clone(),
                    false,
                ),
            )
            .await?;
        // TODO: get consensus threshold from somewhere else
        // TODO: Put the multisig script back
        // let n = committee_pub_keys.len();
        // if n > u8::MAX as usize {
        //     return Err(WalletError::ArgumentError {
        //         argument: "committee_pub_keys".to_string(),
        //         message: "Cannot be more than 255".to_string(),
        //         value: n.to_string(),
        //     });
        // }
        // let max_failures = n / 3;
        // let m = max_failures * 2 + 1;
        // let mut msg = [0u8; 32];
        // msg.copy_from_slice("Need a better message12345678901".as_bytes());
        //
        // let output = output.with_script(script!(CheckMultiSig(
        //     m as u8,
        //     n as u8,
        //     committee_pub_keys,
        //     Box::new(msg)
        // )));
        let (tx_id, transaction) = self
            .output_manager
            .create_send_to_self_with_output(vec![output], ASSET_FPG.into(), Some(unique_id), Some(asset_pub_key))
            .await?;
        Ok((tx_id, transaction))
    }

    pub async fn create_committee_definition(
        &mut self,
        asset_public_key: PublicKey,
        committee_pub_keys: Vec<PublicKey>,
        effective_sidechain_height: u64,
    ) -> Result<(TxId, Transaction), WalletError> {
        let output = self
            .output_manager
            .create_output_with_features(
                0.into(),
                OutputFeatures::for_committee(
                    asset_public_key,
                    vec![2u8; 32],
                    committee_pub_keys.clone(),
                    effective_sidechain_height,
                    true,
                ),
            )
            .await?;

        let (tx_id, transaction) = self
            .output_manager
            .create_send_to_self_with_output(vec![output], ASSET_FPG.into(), None, None)
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
            "".to_string(),
            "".to_string(),
        ));
    }
    let version = unblinded_output.unblinded_output.features.metadata[0];

    let deserializer = get_deserializer(version);

    let metadata = deserializer.deserialize(&unblinded_output.unblinded_output.features.metadata[1..]);
    info!(target: LOG_TARGET, "Metadata: {:?}", metadata);
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
        metadata.description,
        metadata.image,
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

#[derive(Debug)]
pub struct AssetMetadata {
    name: String,
    description: String,
    image: String,
}
