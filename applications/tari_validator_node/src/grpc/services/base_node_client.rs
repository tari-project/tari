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

use std::{convert::TryFrom, net::SocketAddr};

use async_trait::async_trait;
use log::*;
use tari_app_grpc::tari_rpc as grpc;
use tari_common_types::types::{PublicKey, COMMITTEE_DEFINITION_ID};
use tari_core::transactions::transaction_components::OutputFeatures;
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use tari_dan_core::{
    models::{AssetDefinition, BaseLayerMetadata, BaseLayerOutput},
    services::BaseNodeClient,
    DigitalAssetError,
};

const LOG_TARGET: &str = "tari::validator_node::app";

type Inner = grpc::base_node_client::BaseNodeClient<tonic::transport::Channel>;

#[derive(Clone)]
pub struct GrpcBaseNodeClient {
    endpoint: SocketAddr,
    inner: Option<Inner>,
}

impl GrpcBaseNodeClient {
    pub fn new(endpoint: SocketAddr) -> GrpcBaseNodeClient {
        Self { endpoint, inner: None }
    }

    pub async fn connection(&mut self) -> Result<&mut Inner, DigitalAssetError> {
        if self.inner.is_none() {
            let url = format!("http://{}", self.endpoint);
            let inner = Inner::connect(url).await?;
            self.inner = Some(inner);
        }
        self.inner
            .as_mut()
            .ok_or_else(|| DigitalAssetError::FatalError("no connection".into()))
    }
}
#[async_trait]
impl BaseNodeClient for GrpcBaseNodeClient {
    async fn get_tip_info(&mut self) -> Result<BaseLayerMetadata, DigitalAssetError> {
        let inner = self.connection().await?;
        let request = grpc::Empty {};
        let result = inner.get_tip_info(request).await?.into_inner();
        Ok(BaseLayerMetadata {
            height_of_longest_chain: result.metadata.unwrap().height_of_longest_chain,
        })
    }

    async fn get_current_checkpoint(
        &mut self,
        _height: u64,
        asset_public_key: PublicKey,
        checkpoint_unique_id: Vec<u8>,
    ) -> Result<Option<BaseLayerOutput>, DigitalAssetError> {
        let inner = self.connection().await?;
        let request = grpc::GetTokensRequest {
            asset_public_key: asset_public_key.as_bytes().to_vec(),
            unique_ids: vec![checkpoint_unique_id],
        };
        let mut result = inner.get_tokens(request).await?.into_inner();
        let mut outputs = vec![];
        while let Some(r) = result.message().await? {
            outputs.push(r);
        }
        let output = outputs.first().and_then(|o| o.features.clone());
        self.extract_base_layer_output(output)
    }

    async fn get_assets_for_dan_node(
        &mut self,
        dan_node_public_key: PublicKey,
    ) -> Result<Vec<AssetDefinition>, DigitalAssetError> {
        let inner = self.connection().await?;
        // TODO: probably should use output mmr indexes here
        let request = grpc::ListAssetRegistrationsRequest { offset: 0, count: 100 };
        let mut result = inner.list_asset_registrations(request).await?.into_inner();
        let mut assets: Vec<AssetDefinition> = vec![];
        let tip = self.get_tip_info().await?;
        while let Some(r) = result.message().await? {
            if let Ok(asset_public_key) = PublicKey::from_bytes(r.asset_public_key.as_bytes()) {
                if let Some(checkpoint) = self
                    .get_current_checkpoint(
                        tip.height_of_longest_chain,
                        asset_public_key.clone(),
                        COMMITTEE_DEFINITION_ID.into(),
                    )
                    .await?
                {
                    if let Some(committee) = checkpoint.get_side_chain_committee() {
                        if committee.contains(&dan_node_public_key) {
                            info!(
                                target: LOG_TARGET,
                                "Node is on committee for asset : {}", asset_public_key
                            );
                            let committee = committee.iter().map(Hex::to_hex).collect::<Vec<String>>();
                            assets.push(AssetDefinition {
                                committee,
                                public_key: asset_public_key,
                                template_parameters: r
                                    .features
                                    .unwrap()
                                    .asset
                                    .unwrap()
                                    .template_parameters
                                    .into_iter()
                                    .map(|tp| tp.into())
                                    .collect(),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        Ok(assets)
    }

    async fn get_asset_registration(
        &mut self,
        asset_public_key: PublicKey,
    ) -> Result<Option<BaseLayerOutput>, DigitalAssetError> {
        let inner = self.connection().await?;
        let req = grpc::GetAssetMetadataRequest {
            asset_public_key: asset_public_key.to_vec(),
        };
        let output = inner.get_asset_metadata(req).await.unwrap().into_inner();
        self.extract_base_layer_output(output.features)
    }
}

impl GrpcBaseNodeClient {
    fn extract_base_layer_output(
        &self,
        output: Option<tari_app_grpc::tari_rpc::OutputFeatures>,
    ) -> Result<Option<BaseLayerOutput>, DigitalAssetError> {
        let res = output
            .map(OutputFeatures::try_from)
            .transpose()
            .map_err(DigitalAssetError::ConversionError)?
            .map(BaseLayerOutput::from);
        Ok(res)
    }
}
