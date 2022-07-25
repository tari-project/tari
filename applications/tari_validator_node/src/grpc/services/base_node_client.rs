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

use std::{convert::TryInto, net::SocketAddr};

use async_trait::async_trait;
use log::*;
use tari_app_grpc::tari_rpc as grpc;
use tari_common_types::types::{FixedHash, PublicKey};
use tari_core::{
    chain_storage::{PrunedOutput, UtxoMinedInfo},
    transactions::transaction_components::OutputType,
};
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{
    models::{BaseLayerMetadata, BaseLayerOutput},
    services::BaseNodeClient,
    DigitalAssetError,
};

const LOG_TARGET: &str = "tari::validator_node::app";

type Client = grpc::base_node_client::BaseNodeClient<tonic::transport::Channel>;

#[derive(Clone)]
pub struct GrpcBaseNodeClient {
    endpoint: SocketAddr,
    client: Option<Client>,
}

impl GrpcBaseNodeClient {
    pub fn new(endpoint: SocketAddr) -> GrpcBaseNodeClient {
        Self { endpoint, client: None }
    }

    pub async fn connection(&mut self) -> Result<&mut Client, DigitalAssetError> {
        if self.client.is_none() {
            let url = format!("http://{}", self.endpoint);
            let inner = Client::connect(url).await?;
            self.client = Some(inner);
        }
        self.client
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
        let metadata = result
            .metadata
            .ok_or_else(|| DigitalAssetError::InvalidPeerMessage("Base node returned no metadata".to_string()))?;
        Ok(BaseLayerMetadata {
            height_of_longest_chain: metadata.height_of_longest_chain,
            tip_hash: metadata.best_block.try_into().map_err(|_| {
                DigitalAssetError::InvalidPeerMessage("best_block was not a valid fixed hash".to_string())
            })?,
        })
    }

    async fn get_current_contract_outputs(
        &mut self,
        _height: u64,
        contract_id: FixedHash,
        output_type: OutputType,
    ) -> Result<Vec<UtxoMinedInfo>, DigitalAssetError> {
        let inner = self.connection().await?;
        let request = grpc::GetCurrentContractOutputsRequest {
            contract_id: contract_id.to_vec(),
            output_type: u32::from(output_type.as_byte()),
        };
        let resp = match inner.get_current_contract_outputs(request).await {
            Ok(resp) => {
                debug!(
                    target: LOG_TARGET,
                    "get_current_contract_outputs: {} output(s) found for contract {}: {:?}",
                    resp.get_ref().outputs.len(),
                    contract_id,
                    resp
                );
                resp.into_inner()
            },
            Err(err) => return Err(err.into()),
        };

        let mut outputs = vec![];
        for mined_info in resp.outputs {
            let output = mined_info
                .output
                .map(TryInto::try_into)
                .transpose()
                .map_err(DigitalAssetError::ConversionError)?
                .ok_or_else(|| DigitalAssetError::InvalidPeerMessage("Mined info contained no output".to_string()))?;

            outputs.push(UtxoMinedInfo {
                output: PrunedOutput::NotPruned { output },
                mmr_position: mined_info.mmr_position,
                mined_height: mined_info.mined_height,
                header_hash: mined_info.header_hash,
                mined_timestamp: mined_info.mined_timestamp,
            });
        }
        Ok(outputs)
    }

    async fn get_constitutions(
        &mut self,
        start_block_hash: Option<FixedHash>,
        dan_node_public_key: &PublicKey,
    ) -> Result<Vec<UtxoMinedInfo>, DigitalAssetError> {
        let conn = self.connection().await?;
        let request = grpc::GetConstitutionsRequest {
            start_block_hash: start_block_hash.map(|h| h.to_vec()).unwrap_or_else(Vec::new),
            dan_node_public_key: dan_node_public_key.as_bytes().to_vec(),
        };
        let mut result = conn.get_constitutions(request).await?.into_inner();
        let mut outputs = vec![];
        while let Some(resp) = result.message().await? {
            let output = resp
                .output
                .map(TryInto::try_into)
                .transpose()
                .map_err(DigitalAssetError::ConversionError)?
                .ok_or_else(|| DigitalAssetError::InvalidPeerMessage("Mined info contained no output".to_string()))?;

            outputs.push(UtxoMinedInfo {
                output: PrunedOutput::NotPruned { output },
                mmr_position: resp.mmr_position,
                mined_height: resp.mined_height,
                header_hash: resp.header_hash,
                mined_timestamp: resp.mined_timestamp,
            });
        }
        Ok(outputs)
    }

    async fn check_if_in_committee(
        &mut self,
        _asset_public_key: PublicKey,
        _dan_node_public_key: PublicKey,
    ) -> Result<(bool, u64), DigitalAssetError> {
        unimplemented!()
        // let tip = self.get_tip_info().await?;
        // if let Some(checkpoint) = self
        //     .get_current_checkpoint(
        //         tip.height_of_longest_chain,
        //         asset_public_key,
        //         COMMITTEE_DEFINITION_ID.into(),
        //     )
        //     .await?
        // {
        //     if let Some(committee) = checkpoint.get_side_chain_committee() {
        //         if committee.contains(&dan_node_public_key) {
        //             // We know it's part of the committee at this height
        //             // TODO: there could be a scenario where it was not part of the committee for one block (or more,
        //             // depends on the config)
        //             Ok((true, checkpoint.height))
        //         } else {
        //             // We know it's no longer part of the committee at this height
        //             // TODO: if the committee changes twice in short period of time, this will cause some glitches
        //             Ok((false, checkpoint.height))
        //         }
        //     } else {
        //         Ok((false, 0))
        //     }
        // } else {
        //     Ok((false, 0))
        // }
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

        let mined_height = output.mined_height;
        let output = output
            .features
            .map(|features| match features.try_into() {
                Ok(f) => Ok(BaseLayerOutput {
                    features: f,
                    height: mined_height,
                }),
                Err(e) => Err(DigitalAssetError::ConversionError(e)),
            })
            .transpose()?;

        Ok(output)
    }
}
