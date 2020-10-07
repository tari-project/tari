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
    proto::rpc::{GetCloserPeersRequest, GetPeersRequest, GetPeersResponse},
    rpc::DhtRpcService,
};
use futures::{channel::mpsc, stream, SinkExt};
use log::*;
use std::{cmp, sync::Arc};
use tari_comms::{
    peer_manager::{NodeId, Peer, PeerFeatures, PeerQuery},
    protocol::rpc::{Request, RpcError, RpcStatus, Streaming},
    PeerManager,
};
use tari_utilities::ByteArray;
use tokio::task;

const LOG_TARGET: &str = "comms::dht::rpc";

const MAX_NUM_PEERS: usize = 100;
const MAX_EXCLUDED_PEERS: usize = 1000;

pub struct DhtRpcServiceImpl {
    peer_manager: Arc<PeerManager>,
}

impl DhtRpcServiceImpl {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self { peer_manager }
    }

    pub fn stream_peers(&self, peers: Vec<Peer>) -> Streaming<GetPeersResponse> {
        if peers.is_empty() {
            return Streaming::empty();
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

        Streaming::new(rx)
    }
}

#[tari_comms::async_trait]
impl DhtRpcService for DhtRpcServiceImpl {
    async fn get_closer_peers(
        &self,
        request: Request<GetCloserPeersRequest>,
    ) -> Result<Streaming<GetPeersResponse>, RpcStatus>
    {
        let message = request.message();
        if message.n == 0 {
            return Err(RpcStatus::bad_request("Requesting zero peers is invalid"));
        }

        if message.n as usize > MAX_NUM_PEERS {
            return Err(RpcStatus::bad_request(format!(
                "Requested too many peers ({}). Cannot request more than `{}` peers",
                message.n, MAX_NUM_PEERS
            )));
        }

        let node_id = if message.closer_to.is_empty() {
            request.context().peer_node_id().clone()
        } else {
            NodeId::from_bytes(&message.closer_to)
                .map_err(|_| RpcStatus::bad_request("`closer_to` did not contain a valid NodeId"))?
        };

        if message.excluded.len() > MAX_EXCLUDED_PEERS {
            return Err(RpcStatus::bad_request(format!(
                "Sending more than {} to the exclude list is not supported",
                MAX_EXCLUDED_PEERS
            )));
        }

        let excluded = message
            .excluded
            .iter()
            .filter_map(|node_id| NodeId::from_bytes(node_id).ok())
            .collect::<Vec<_>>();

        if excluded.len() != message.excluded.len() {
            return Err(RpcStatus::bad_request("Invalid NodeId in excluded list"));
        }

        let mut features = Some(PeerFeatures::COMMUNICATION_NODE);
        if message.include_clients {
            features = None;
        }

        let peers = self
            .peer_manager
            .closest_peers(&node_id, message.n as usize, &excluded, features)
            .await
            .map_err(RpcError::from)?;

        debug!(
            target: LOG_TARGET,
            "[get_closest_peers] Returning {}/{} peer(s) to peer `{}`",
            peers.len(),
            message.n,
            node_id.short_str()
        );

        Ok(self.stream_peers(peers))
    }

    async fn get_peers(&self, request: Request<GetPeersRequest>) -> Result<Streaming<GetPeersResponse>, RpcStatus> {
        let message = request.message();

        let mut query = PeerQuery::new()
            .select_where(|peer| (message.include_clients || !peer.features.is_client()) && !peer.is_banned());
        if message.n > 0 {
            query = query.limit(message.n as usize);
        }

        // TODO: This result set can/will be large
        //       Ideally, we'd need a lazy-loaded iterator, however that requires a long-lived read transaction and
        //       the lifetime of that transaction is proportional on the time it takes to send the peers.
        //       Either we should not need to return all peers, or we can find a way to do an iterator which does not
        //       require a long-lived read transaction (we don't strictly care about read consistency in this case).
        let peers = self.peer_manager.perform_query(query).await.map_err(RpcError::from)?;

        let node_id = request.context().peer_node_id();
        debug!(
            target: LOG_TARGET,
            "[get_peers] Returning {}/{} peer(s) to peer `{}`",
            peers.len(),
            Some(message.n)
                .filter(|n| *n > 0)
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "∞".into()),
            node_id.short_str()
        );

        Ok(self.stream_peers(peers))
    }
}
