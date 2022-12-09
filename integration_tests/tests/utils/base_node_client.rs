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
use log::trace;
use tari_app_grpc::tari_rpc::{self as grpc, ConsensusConstants, GetShardKeyRequest};
use tari_base_node_grpc_client::BaseNodeGrpcClient;
use tari_common_types::types::{FixedHash, PublicKey};
use tari_comms::types::CommsPublicKey;
use tari_core::{blocks::BlockHeader, transactions::transaction_components::CodeTemplateRegistration};
use tari_crypto::tari_utilities::ByteArray;
use tari_validator_node::{error::GrpcBaseNodeError, types::BaseLayerMetadata};

use crate::utils::base_node::BaseNodeClient;

const LOG_TARGET: &str = "tari::validator_node::app";

type Client = BaseNodeGrpcClient<tonic::transport::Channel>;

#[derive(Clone)]
pub struct GrpcBaseNodeClient {
    endpoint: SocketAddr,
    client: Option<Client>,
}

impl GrpcBaseNodeClient {
    pub fn new(endpoint: SocketAddr) -> GrpcBaseNodeClient {
        Self { endpoint, client: None }
    }

    async fn connection(&mut self) -> Result<&mut Client, GrpcBaseNodeError> {
        if self.client.is_none() {
            let url = format!("http://{}", self.endpoint);
            let inner = Client::connect(url).await?;
            self.client = Some(inner);
        }

        self.client.as_mut().ok_or(GrpcBaseNodeError::ConnectionError)
    }

    pub async fn get_consensus_constants(
        &mut self,
        block_height: u64,
    ) -> Result<ConsensusConstants, GrpcBaseNodeError> {
        let inner = self.connection().await?;

        let request = grpc::BlockHeight { block_height };
        let result = inner.get_constants(request).await?.into_inner();
        let consensus_constants = result.into();

        Ok(consensus_constants)
    }
}

#[async_trait]
impl BaseNodeClient for GrpcBaseNodeClient {
    async fn test_connection(&mut self) -> Result<(), GrpcBaseNodeError> {
        self.connection().await?;
        Ok(())
    }

    async fn get_tip_info(&mut self) -> Result<BaseLayerMetadata, GrpcBaseNodeError> {
        let inner = self.connection().await?;
        let request = grpc::Empty {};
        let result = inner.get_tip_info(request).await?.into_inner();
        let metadata = result
            .metadata
            .ok_or_else(|| GrpcBaseNodeError::InvalidPeerMessage("Base node returned no metadata".to_string()))?;

        Ok(BaseLayerMetadata {
            height_of_longest_chain: metadata.height_of_longest_chain,
            tip_hash: metadata.best_block.try_into().map_err(|_| {
                GrpcBaseNodeError::InvalidPeerMessage("best_block was not a valid fixed hash".to_string())
            })?,
        })
    }

    async fn get_header_by_hash(&mut self, block_hash: FixedHash) -> Result<BlockHeader, GrpcBaseNodeError> {
        let inner = self.connection().await?;
        let request = grpc::GetHeaderByHashRequest {
            hash: block_hash.to_vec(),
        };
        let result = inner.get_header_by_hash(request).await?.into_inner();
        let header = result
            .header
            .ok_or_else(|| GrpcBaseNodeError::InvalidPeerMessage("Base node returned no header".to_string()))?;
        let header = header.try_into().map_err(GrpcBaseNodeError::InvalidPeerMessage)?;
        Ok(header)
    }
}
