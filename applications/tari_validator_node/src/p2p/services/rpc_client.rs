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
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId};
use tari_comms_dht::Dht;
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{
    models::TemplateId,
    services::{BaseNodeClient, ValidatorNodeClientFactory, ValidatorNodeRpcClient},
    DigitalAssetError,
};

use crate::p2p::{proto::validator_node as proto, rpc};

pub struct TariCommsValidatorNodeRpcClient {
    connectivity: ConnectivityRequester,
    address: PublicKey,
}

#[async_trait]
impl ValidatorNodeRpcClient for TariCommsValidatorNodeRpcClient {
    async fn invoke_read_method(
        &self,
        asset_public_key: &PublicKey,
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        let mut connection = self.connectivity.dial_peer(NodeId::from(self.address.clone())).await?;
        let mut client = connection.connect_rpc::<rpc::ValidatorNodeRpcClient>().await?;
        let request = proto::InvokeReadMethodRequest {
            asset_public_key: asset_public_key.to_vec(),
            template_id: template_id as u32,
            method,
            args,
        };
        let response = client.invoke_read_method(request).await?;
        Ok(response.result)
    }
}

#[derive()]
pub struct TariCommsValidatorNodeClientFactory {
    connectivity_requester: ConnectivityRequester,
}

impl TariCommsValidatorNodeClientFactory {
    pub fn new(connectivity_requester: ConnectivityRequester) -> Self {
        Self { connectivity_requester }
    }
}

impl ValidatorNodeClientFactory for TariCommsValidatorNodeClientFactory {
    type Addr = PublicKey;
    type Client = TariCommsValidatorNodeRpcClient;

    fn create_client(&self, address: &Self::Addr) -> Self::Client {
        TariCommsValidatorNodeRpcClient {
            connectivity: self.connectivity_requester.clone(),
            address: address.clone(),
        }
    }
}
