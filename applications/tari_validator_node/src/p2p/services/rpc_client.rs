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

use std::convert::TryInto;

use async_trait::async_trait;
use log::*;
use tari_common_types::types::PublicKey;
use tari_comms::PeerConnection;
use tari_comms_dht::DhtRequester;
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{
    models::{Node, SchemaState, SideChainBlock, StateOpLogEntry, TemplateId, TreeNodeHash},
    services::{ValidatorNodeClientError, ValidatorNodeClientFactory, ValidatorNodeRpcClient},
};
use tokio_stream::StreamExt;

use crate::p2p::{proto::validator_node as proto, rpc};

const LOG_TARGET: &str = "tari::validator_node::p2p::services::rpc_client";

pub struct TariCommsValidatorNodeRpcClient {
    dht: DhtRequester,
    address: PublicKey,
}

impl TariCommsValidatorNodeRpcClient {
    async fn create_connection(&mut self) -> Result<PeerConnection, ValidatorNodeClientError> {
        let conn = self.dht.dial_or_discover_peer(self.address.clone()).await?;
        Ok(conn)
    }
}

#[async_trait]
impl ValidatorNodeRpcClient for TariCommsValidatorNodeRpcClient {
    async fn invoke_read_method(
        &mut self,
        asset_public_key: &PublicKey,
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, ValidatorNodeClientError> {
        debug!(
            target: LOG_TARGET,
            r#"Invoking read method "{}" for asset '{}'"#, method, asset_public_key
        );
        let mut connection = self.create_connection().await?;
        let mut client = connection.connect_rpc::<rpc::ValidatorNodeRpcClient>().await?;
        let request = proto::InvokeReadMethodRequest {
            asset_public_key: asset_public_key.to_vec(),
            template_id: template_id as u32,
            method,
            args,
        };
        let response = client.invoke_read_method(request).await?;

        Ok(if response.result.is_empty() {
            None
        } else {
            Some(response.result)
        })
    }

    async fn invoke_method(
        &mut self,
        asset_public_key: &PublicKey,
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, ValidatorNodeClientError> {
        debug!(
            target: LOG_TARGET,
            r#"Invoking method "{}" for asset '{}'"#, method, asset_public_key
        );
        let mut connection = self.create_connection().await?;
        let mut client = connection.connect_rpc::<rpc::ValidatorNodeRpcClient>().await?;
        let request = proto::InvokeMethodRequest {
            asset_public_key: asset_public_key.to_vec(),
            template_id: template_id as u32,
            method,
            args,
        };
        let response = client.invoke_method(request).await?;

        debug!(
            target: LOG_TARGET,
            "Validator node '{}' returned status '{}' for asset '{}'", self.address, response.status, asset_public_key
        );
        if response.result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(response.result))
        }
    }

    async fn get_sidechain_blocks(
        &mut self,
        asset_public_key: &PublicKey,
        start_hash: TreeNodeHash,
        end_hash: Option<TreeNodeHash>,
    ) -> Result<Vec<SideChainBlock>, ValidatorNodeClientError> {
        let mut connection = self.create_connection().await?;
        let mut client = connection.connect_rpc::<rpc::ValidatorNodeRpcClient>().await?;
        let request = proto::GetSidechainBlocksRequest {
            asset_public_key: asset_public_key.to_vec(),
            start_hash: start_hash.as_bytes().to_vec(),
            end_hash: end_hash.map(|h| h.as_bytes().to_vec()).unwrap_or_default(),
        };

        let stream = client.get_sidechain_blocks(request).await?;
        // TODO: By first collecting all the blocks, we lose the advantage of streaming. Since you cannot return
        //       `Result<impl Stream<..>, _>`, and the Map type is private in tokio-stream, its a little tricky to
        //       return the stream and not leak the RPC response type out of the client.
        //       Copying the tokio_stream::Map stream into our code / creating a custom conversion stream wrapper would
        // solve this.
        let blocks = stream
            .map(|result| {
                let resp = result?;
                let block: SideChainBlock = resp
                    .block
                    .ok_or_else(|| {
                        ValidatorNodeClientError::InvalidPeerMessage("Node returned empty block".to_string())
                    })?
                    .try_into()
                    .map_err(ValidatorNodeClientError::InvalidPeerMessage)?;
                Ok(block)
            })
            .collect::<Result<_, ValidatorNodeClientError>>()
            .await?;

        Ok(blocks)
    }

    async fn get_sidechain_state(
        &mut self,
        asset_public_key: &PublicKey,
    ) -> Result<Vec<SchemaState>, ValidatorNodeClientError> {
        let mut connection = self.create_connection().await?;
        let mut client = connection.connect_rpc::<rpc::ValidatorNodeRpcClient>().await?;
        let request = proto::GetSidechainStateRequest {
            asset_public_key: asset_public_key.to_vec(),
        };

        let mut stream = client.get_sidechain_state(request).await?;
        // TODO: Same issue as get_sidechain_blocks
        let mut schemas = Vec::new();
        let mut current_schema = None;
        while let Some(resp) = stream.next().await {
            let resp = resp?;

            match resp.state {
                Some(proto::get_sidechain_state_response::State::Schema(name)) => {
                    if let Some(schema) = current_schema.take() {
                        schemas.push(schema);
                    }
                    current_schema = Some(SchemaState::new(name, vec![]));
                },
                Some(proto::get_sidechain_state_response::State::KeyValue(kv)) => match current_schema.as_mut() {
                    Some(schema) => {
                        let kv = kv.try_into().map_err(ValidatorNodeClientError::InvalidPeerMessage)?;
                        schema.push_key_value(kv);
                    },
                    None => {
                        return Err(ValidatorNodeClientError::InvalidPeerMessage(format!(
                            "Peer {} sent a key value response without first defining the schema",
                            self.address
                        )))
                    },
                },
                None => {
                    return Err(ValidatorNodeClientError::ProtocolViolation {
                        peer: self.address.clone(),
                        details: "get_sidechain_state: Peer sent response without state".to_string(),
                    })
                },
            }
        }

        if let Some(schema) = current_schema {
            schemas.push(schema);
        }

        Ok(schemas)
    }

    async fn get_op_logs(
        &mut self,
        asset_public_key: &PublicKey,
        height: u64,
    ) -> Result<Vec<StateOpLogEntry>, ValidatorNodeClientError> {
        let mut connection = self.create_connection().await?;
        let mut client = connection.connect_rpc::<rpc::ValidatorNodeRpcClient>().await?;
        let request = proto::GetStateOpLogsRequest {
            asset_public_key: asset_public_key.as_bytes().to_vec(),
            height,
        };

        let resp = client.get_op_logs(request).await?;
        let op_logs = resp
            .op_logs
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ValidatorNodeClientError::InvalidPeerMessage)?;

        Ok(op_logs)
    }

    async fn get_tip_node(&mut self, asset_public_key: &PublicKey) -> Result<Option<Node>, ValidatorNodeClientError> {
        let mut connection = self.create_connection().await?;
        let mut client = connection.connect_rpc::<rpc::ValidatorNodeRpcClient>().await?;
        let request = proto::GetTipNodeRequest {
            asset_public_key: asset_public_key.as_bytes().to_vec(),
        };
        let resp = client.get_tip_node(request).await?;
        resp.tip_node
            .map(TryInto::try_into)
            .transpose()
            .map_err(ValidatorNodeClientError::InvalidPeerMessage)
    }
}

#[derive(Clone)]
pub struct TariCommsValidatorNodeClientFactory {
    dht: DhtRequester,
}

impl TariCommsValidatorNodeClientFactory {
    pub fn new(dht: DhtRequester) -> Self {
        Self { dht }
    }
}

impl ValidatorNodeClientFactory for TariCommsValidatorNodeClientFactory {
    type Addr = PublicKey;
    type Client = TariCommsValidatorNodeRpcClient;

    fn create_client(&self, address: &Self::Addr) -> Self::Client {
        TariCommsValidatorNodeRpcClient {
            dht: self.dht.clone(),
            address: address.clone(),
        }
    }
}
