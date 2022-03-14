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
use tari_core::transactions::transaction_components::OutputFlags;

use crate::{
    error::WalletError,
    output_manager_service::{
        handle::OutputManagerHandle,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::DbUnblindedOutput,
        },
    },
    tokens::Token,
};

const LOG_TARGET: &str = "wallet::tokens::token_manager";

pub(crate) struct TokenManager<T: OutputManagerBackend + 'static> {
    output_database: OutputManagerDatabase<T>,
    _output_manager: OutputManagerHandle,
    // transaction_service: TransactionServiceHandle
}
impl<T: OutputManagerBackend + 'static> TokenManager<T> {
    pub fn new(backend: T, output_manager: OutputManagerHandle) -> Self {
        Self {
            output_database: OutputManagerDatabase::new(backend),
            _output_manager: output_manager,
        }
    }

    pub async fn list_owned(&self) -> Result<Vec<Token>, WalletError> {
        let outputs = self
            .output_database
            .fetch_with_features(OutputFlags::NON_FUNGIBLE)
            .await
            .map_err(|err| WalletError::OutputManagerError(err.into()))?;

        // These will include assets registrations

        debug!(
            target: LOG_TARGET,
            "Found {} owned outputs that contain tokens",
            outputs.len()
        );
        let assets: Vec<Token> = outputs
            .into_iter()
            .filter(|ub| {
                // Filter out asset registrations that don't have a parent pub key
                ub.unblinded_output.features.parent_public_key.is_some()
            })
            .map(convert_to_token)
            .collect::<Result<_, _>>()?;
        Ok(assets)
    }
}

fn convert_to_token(unblinded_output: DbUnblindedOutput) -> Result<Token, WalletError> {
    if unblinded_output.unblinded_output.features.metadata.is_empty() {
        // TODO: sort out unwraps
        return Ok(Token::new(
            "<Invalid metadata:empty>".to_string(),
            unblinded_output.status.to_string(),
            unblinded_output
                .unblinded_output
                .features
                .parent_public_key
                .as_ref()
                .cloned()
                .unwrap(),
            unblinded_output.commitment,
            unblinded_output.unblinded_output.features.unique_id.unwrap_or_default(),
        ));
    }
    let version = unblinded_output.unblinded_output.features.metadata[0];

    let deserializer = get_deserializer(version);

    let metadata = deserializer.deserialize(&unblinded_output.unblinded_output.features.metadata[1..]);
    Ok(Token::new(
        metadata.name,
        unblinded_output.status.to_string(),
        unblinded_output
            .unblinded_output
            .features
            .parent_public_key
            .as_ref()
            .cloned()
            .unwrap(),
        unblinded_output.commitment,
        unblinded_output.unblinded_output.features.unique_id.unwrap_or_default(),
    ))
}

fn get_deserializer(_version: u8) -> impl TokenMetadataDeserializer {
    V1TokenMetadataSerializer {}
}

pub trait TokenMetadataDeserializer {
    fn deserialize(&self, metadata: &[u8]) -> TokenMetadata;
}
pub trait TokenMetadataSerializer {
    fn serialize(&self, model: &TokenMetadata) -> Vec<u8>;
}

pub struct V1TokenMetadataSerializer {}

// TODO: Replace with proto serializer
impl TokenMetadataDeserializer for V1TokenMetadataSerializer {
    fn deserialize(&self, metadata: &[u8]) -> TokenMetadata {
        TokenMetadata {
            name: String::from_utf8(Vec::from(metadata)).unwrap(),
        }
    }
}

impl TokenMetadataSerializer for V1TokenMetadataSerializer {
    fn serialize(&self, model: &TokenMetadata) -> Vec<u8> {
        model.name.clone().into_bytes()
    }
}

pub struct TokenMetadata {
    name: String,
}
