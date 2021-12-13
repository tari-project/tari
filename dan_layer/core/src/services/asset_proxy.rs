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

use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use log::*;
use tari_common_types::types::PublicKey;
use tari_crypto::tari_utilities::hex::Hex;
use tokio_stream::StreamExt;

use crate::{
    models::TemplateId,
    services::{validator_node_rpc_client::ValidatorNodeRpcClient, BaseNodeClient, ValidatorNodeClientFactory},
    DigitalAssetError,
};

const LOG_TARGET: &str = "tari::dan_layer::core::services::asset_proxy";

#[async_trait]
pub trait AssetProxy {
    async fn invoke_read_method(
        &self,
        asset_public_key: &PublicKey,
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError>;
}

pub struct ConcreteAssetProxy<TBaseNodeClient: BaseNodeClient, TValidatorNodeClientFactory: ValidatorNodeClientFactory>
{
    base_node_client: TBaseNodeClient,
    validator_node_client_factory: TValidatorNodeClientFactory,
    max_clients_to_ask: usize,
}

impl<TBaseNodeClient: BaseNodeClient, TValidatorNodeClientFactory: ValidatorNodeClientFactory<Addr = PublicKey>>
    ConcreteAssetProxy<TBaseNodeClient, TValidatorNodeClientFactory>
{
    pub fn new(
        base_node_client: TBaseNodeClient,
        validator_node_client_factory: TValidatorNodeClientFactory,
        max_clients_to_ask: usize,
    ) -> Self {
        Self {
            base_node_client,
            validator_node_client_factory,
            max_clients_to_ask,
        }
    }

    async fn send_request_to_node(
        &self,
        member: &PublicKey,
        asset_public_key: &PublicKey,
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        let client = self.validator_node_client_factory.create_client(member);
        client
            .invoke_read_method(asset_public_key, template_id, method, args)
            .await
    }
}

#[async_trait]
impl<
        TBaseNodeClient: BaseNodeClient + Clone + Sync + Send,
        TValidatorNodeClientFactory: ValidatorNodeClientFactory<Addr = PublicKey> + Sync,
    > AssetProxy for ConcreteAssetProxy<TBaseNodeClient, TValidatorNodeClientFactory>
{
    async fn invoke_read_method(
        &self,
        asset_public_key: &PublicKey,
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        // find the base layer committee

        let mut base_node_client = self.base_node_client.clone();
        let tip = base_node_client.get_tip_info().await?;
        let last_checkpoint = base_node_client
            .get_current_checkpoint(
                tip.height_of_longest_chain,
                asset_public_key.clone(),
                // TODO: read this from the chain maybe?
                vec![3u8; 32],
            )
            .await?;

        let last_checkpoint = match last_checkpoint {
            None => {
                return Err(DigitalAssetError::NotFound {
                    entity: "checkpoint",
                    id: asset_public_key.to_hex(),
                })
            },
            Some(chk) => chk,
        };

        if let Some(committee) = last_checkpoint.get_side_chain_committee() {
            let mut tasks = FuturesUnordered::new();
            for member in committee.iter().take(self.max_clients_to_ask) {
                tasks.push(self.send_request_to_node(
                    member,
                    asset_public_key,
                    template_id,
                    method.clone(),
                    args.clone(),
                ));
            }
            for result in tasks.next().await {
                match result {
                    Ok(data) => return Ok(data),
                    Err(err) => {
                        error!(target: LOG_TARGET, "Committee member responded with error:{}", err);
                    },
                }
            }

            Err(DigitalAssetError::NoResponsesFromCommittee)
        } else {
            Err(DigitalAssetError::NoCommitteeForAsset)
        }
    }
}
