//  Copyright 2020, The Tari Project
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

use crate::{
    proto::rpc::{GetPeersRequest, GetPeersResponse},
    rpc::DhtRpcService,
};
use futures::{channel::mpsc, stream, SinkExt};
use log::*;
use std::{cmp, sync::Arc};
use tari_comms::{
    peer_manager::PeerFeatures,
    protocol::rpc::{Request, RpcError, RpcStatus, Streaming},
    NodeIdentity,
    PeerManager,
};
use tokio::task;

const LOG_TARGET: &str = "comms::dht::rpc";

const MAX_NUM_PEERS: usize = 100;

pub struct DhtRpcServiceImpl {
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
}

impl DhtRpcServiceImpl {
    pub fn new(node_identity: Arc<NodeIdentity>, peer_manager: Arc<PeerManager>) -> Self {
        Self {
            node_identity,
            peer_manager,
        }
    }
}

#[tari_comms::async_trait]
impl DhtRpcService for DhtRpcServiceImpl {
    async fn get_peers(&self, request: Request<GetPeersRequest>) -> Result<Streaming<GetPeersResponse>, RpcStatus> {
        if !self.node_identity.has_peer_features(PeerFeatures::COMMUNICATION_NODE) {
            debug!(target: LOG_TARGET, "get_peers request is not valid for client nodes");
            // TODO: #banheuristic - nodes should never call this method for client nodes
            return Err(RpcStatus::unsupported_method("get_peers is not supported"));
        }

        let message = request.message();
        let node_id = request.context().peer_node_id();
        if message.n == 0 {
            return Err(RpcStatus::bad_request("Requesting zero peers is invalid"));
        }
        if message.n as usize > MAX_NUM_PEERS {
            return Err(RpcStatus::bad_request(format!(
                "Requested too many peers ({}). Cannot request more than `{}` peers",
                message.n, MAX_NUM_PEERS
            )));
        }

        let peers = self
            .peer_manager
            .closest_peers(node_id, message.n as usize, &[], Some(PeerFeatures::COMMUNICATION_NODE))
            .await
            .map_err(RpcError::from)?;
        debug!(
            target: LOG_TARGET,
            "[get_peers] Returning {}/{} peer(s) to peer `{}`",
            peers.len(),
            message.n,
            node_id.short_str()
        );

        if peers.is_empty() {
            return Ok(Streaming::empty());
        }

        // A maximum buffer size of 10 is selected arbitrarily and is to allow the producer/consumer some room to
        // buffer.
        let (mut tx, rx) = mpsc::channel(cmp::min(10, peers.len() as usize));
        task::spawn(async move {
            let iter = peers
                .into_iter()
                .map(|peer| GetPeersResponse {
                    peer: Some(peer.into()),
                })
                .map(Ok)
                .map(Ok);
            let mut stream = stream::iter(iter);
            let _ = tx.send_all(&mut stream).await;
        });

        Ok(Streaming::new(rx))
    }
}
