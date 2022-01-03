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
use tari_common_types::types::PublicKey;
use tari_comms::{
    connection_manager::ConnectionManagerError,
    connectivity::{ConnectivityError, ConnectivityRequester},
    peer_manager::NodeId,
    PeerConnection,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester};
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{
    models::TemplateId,
    services::{ValidatorNodeClientFactory, ValidatorNodeRpcClient},
    DigitalAssetError,
};

use crate::p2p::{proto::validator_node as proto, rpc};
// const LOG_TARGET: &str = "tari::validator_node::p2p::services::rpc_client";

pub struct TariCommsValidatorNodeRpcClient {
    connectivity: ConnectivityRequester,
    dht_discovery: DhtDiscoveryRequester,
    address: PublicKey,
}

impl TariCommsValidatorNodeRpcClient {
    async fn create_connection(&mut self) -> Result<PeerConnection, DigitalAssetError> {
        match self.connectivity.dial_peer(NodeId::from(self.address.clone())).await {
            Ok(connection) => Ok(connection),
            Err(connectivity_error) => {
                dbg!(&connectivity_error);
                match &connectivity_error {
                    ConnectivityError::ConnectionFailed(err) => {
                        match err {
                            ConnectionManagerError::PeerConnectionError(_) |
                            ConnectionManagerError::DialConnectFailedAllAddresses |
                            ConnectionManagerError::PeerIdentityNoValidAddresses => {
                                // Try discover, then dial again
                                // TODO: Should make discovery and connect the responsibility of the DHT layer
                                self.dht_discovery
                                    .discover_peer(
                                        Box::new(self.address.clone()),
                                        NodeDestination::PublicKey(Box::new(self.address.clone())),
                                    )
                                    .await?;
                                if let Some(conn) = self
                                    .connectivity
                                    .get_connection(NodeId::from(self.address.clone()))
                                    .await?
                                {
                                    return Ok(conn);
                                }
                                Ok(self.connectivity.dial_peer(NodeId::from(self.address.clone())).await?)
                            },
                            _ => Err(connectivity_error.into()),
                        }
                    },
                    _ => Err(connectivity_error.into()),
                }
            },
        }
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
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
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
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        let mut connection = self.create_connection().await?;
        let mut client = connection.connect_rpc::<rpc::ValidatorNodeRpcClient>().await?;
        let request = proto::InvokeMethodRequest {
            asset_public_key: asset_public_key.to_vec(),
            template_id: template_id as u32,
            method,
            args,
        };
        let response = client.invoke_method(request).await?;
        Ok(if response.result.is_empty() {
            None
        } else {
            Some(response.result)
        })
    }
}

#[derive(Clone)]
pub struct TariCommsValidatorNodeClientFactory {
    connectivity_requester: ConnectivityRequester,
    dht_discovery: DhtDiscoveryRequester,
}

impl TariCommsValidatorNodeClientFactory {
    pub fn new(connectivity_requester: ConnectivityRequester, dht_discovery: DhtDiscoveryRequester) -> Self {
        Self {
            connectivity_requester,
            dht_discovery,
        }
    }
}

impl ValidatorNodeClientFactory for TariCommsValidatorNodeClientFactory {
    type Addr = PublicKey;
    type Client = TariCommsValidatorNodeRpcClient;

    fn create_client(&self, address: &Self::Addr) -> Self::Client {
        TariCommsValidatorNodeRpcClient {
            connectivity: self.connectivity_requester.clone(),
            dht_discovery: self.dht_discovery.clone(),
            address: address.clone(),
        }
    }
}
