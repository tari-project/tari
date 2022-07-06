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
use tari_common_types::types::{FixedHash, PublicKey};
use tari_comms::{
    connectivity::ConnectivityError,
    protocol::rpc::{RpcError, RpcStatus},
    types::CommsPublicKey,
};
use tari_comms_dht::DhtActorError;
use tari_dan_common_types::TemplateId;
use tari_dan_engine::state::models::{SchemaState, StateOpLogEntry};

use crate::{
    models::{Node, SideChainBlock, TreeNodeHash},
    services::infrastructure_services::NodeAddressable,
};

pub trait ValidatorNodeClientFactory: Send + Sync {
    type Addr: NodeAddressable;
    type Client: ValidatorNodeRpcClient;
    fn create_client(&self, address: &Self::Addr) -> Self::Client;
}

#[async_trait]
pub trait ValidatorNodeRpcClient: Send + Sync {
    async fn invoke_read_method(
        &mut self,
        contract_id: &FixedHash,
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
        sender: PublicKey,
    ) -> Result<Option<Vec<u8>>, ValidatorNodeClientError>;

    async fn invoke_method(
        &mut self,
        contract_id: &FixedHash,
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
        sender: PublicKey,
    ) -> Result<Option<Vec<u8>>, ValidatorNodeClientError>;

    async fn get_sidechain_blocks(
        &mut self,
        contract_id: &FixedHash,
        start_hash: TreeNodeHash,
        end_hash: Option<TreeNodeHash>,
    ) -> Result<Vec<SideChainBlock>, ValidatorNodeClientError>;

    async fn get_sidechain_state(
        &mut self,
        contract_id: &FixedHash,
    ) -> Result<Vec<SchemaState>, ValidatorNodeClientError>;

    async fn get_op_logs(
        &mut self,
        contract_id: &FixedHash,
        height: u64,
    ) -> Result<Vec<StateOpLogEntry>, ValidatorNodeClientError>;

    async fn get_tip_node(&mut self, contract_id: &FixedHash) -> Result<Option<Node>, ValidatorNodeClientError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ValidatorNodeClientError {
    #[error("Protocol violations for peer {peer}: {details}")]
    ProtocolViolation { peer: CommsPublicKey, details: String },
    #[error("Peer sent an invalid message: {0}")]
    InvalidPeerMessage(String),
    #[error("Connectivity error:{0}")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("RpcError: {0}")]
    RpcError(#[from] RpcError),
    #[error("Remote node returned error: {0}")]
    RpcStatusError(#[from] RpcStatus),
    #[error("Dht error: {0}")]
    DhtError(#[from] DhtActorError),
}
