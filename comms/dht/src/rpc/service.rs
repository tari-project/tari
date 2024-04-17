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

use std::{cmp, convert::TryInto, sync::Arc};

use log::*;
use tari_comms::{
    peer_manager::{NodeId, Peer, PeerFeatures},
    protocol::rpc::{Request, RpcError, RpcStatus, Streaming},
    utils,
    PeerManager,
};
use tari_utilities::{hex::Hex, ByteArray};
use tokio::{sync::mpsc, task};

use crate::{
    proto::rpc::{GetCloserPeersRequest, GetPeersRequest, GetPeersResponse},
    rpc::{DhtRpcService, UnvalidatedPeerInfo},
};

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

    pub fn stream_peers(
        &self,
        peers: Vec<Peer>,
        max_claims: usize,
        max_addresses_per_claim: usize,
    ) -> Streaming<GetPeersResponse> {
        if peers.is_empty() {
            return Streaming::empty();
        }

        // A maximum buffer size of 10 is selected arbitrarily and is to allow the producer/consumer some room to
        // buffer.
        let (tx, rx) = mpsc::channel(cmp::min(10, peers.len()));
        task::spawn(async move {
            let iter = peers
                .into_iter()
                .filter_map(|peer| {
                    let mut peer_info =
                        UnvalidatedPeerInfo::from_peer_limited_claims(peer, max_claims, max_addresses_per_claim);

                    // Filter out all identity claims with invalid signatures
                    let count = peer_info.claims.len();
                    let peer_public_key = peer_info.public_key.clone();
                    peer_info.claims.retain(|claim| {
                        claim
                            .signature
                            .is_valid(&peer_public_key, claim.features, claim.addresses.as_slice())
                    });
                    if count != peer_info.claims.len() {
                        warn!(
                            target: LOG_TARGET,
                            "Peer `{}` provided {} claims but only {} were valid",
                            peer_info.public_key.to_hex(),
                            count,
                            peer_info.claims.len()
                        );
                    }

                    if peer_info.claims.is_empty() {
                        None
                    } else {
                        Some(GetPeersResponse {
                            peer: Some(peer_info.into()),
                        })
                    }
                })
                .map(Ok);

            let _result = utils::mpsc::send_all(&tx, iter).await;
        });

        Streaming::new(rx)
    }
}

#[tari_comms::async_trait]
impl DhtRpcService for DhtRpcServiceImpl {
    async fn get_closer_peers(
        &self,
        request: Request<GetCloserPeersRequest>,
    ) -> Result<Streaming<GetPeersResponse>, RpcStatus> {
        let message = request.message();
        if message.n == 0 {
            return Err(RpcStatus::bad_request("Requesting zero peers is invalid"));
        }

        if message.n as usize > MAX_NUM_PEERS {
            return Err(RpcStatus::bad_request(&format!(
                "Requested too many peers ({}). Cannot request more than `{}` peers",
                message.n, MAX_NUM_PEERS
            )));
        }

        let max_claims = message.max_claims.try_into().map_err(|_|
            // This can't happen on a >= 32-bit arch
            RpcStatus::bad_request("max_claims is too large"))?;

        if max_claims == 0 {
            return Err(RpcStatus::bad_request("max_claims must be greater than zero"));
        }

        let max_addresses_per_claim = message.max_addresses_per_claim.try_into().map_err(|_|
            // This can't happen on a >= 32-bit arch
            RpcStatus::bad_request("max_addresses_per_claim is too large"))?;

        if max_addresses_per_claim == 0 {
            return Err(RpcStatus::bad_request(
                "max_addresses_per_claim must be greater than zero",
            ));
        }

        let node_id = if message.closer_to.is_empty() {
            request.context().peer_node_id().clone()
        } else {
            NodeId::from_canonical_bytes(&message.closer_to)
                .map_err(|_| RpcStatus::bad_request("`closer_to` did not contain a valid NodeId"))?
        };

        if message.excluded.len() > MAX_EXCLUDED_PEERS {
            return Err(RpcStatus::bad_request(&format!(
                "Sending more than {} to the exclude list is not supported",
                MAX_EXCLUDED_PEERS
            )));
        }

        let mut excluded = message
            .excluded
            .iter()
            .filter_map(|node_id| NodeId::from_canonical_bytes(node_id).ok())
            .collect::<Vec<_>>();

        if excluded.len() != message.excluded.len() {
            return Err(RpcStatus::bad_request("Invalid NodeId in excluded list"));
        }

        // Don't return the requesting peer back to itself
        excluded.push(request.context().peer_node_id().clone());

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

        Ok(self.stream_peers(peers, max_claims, max_addresses_per_claim))
    }

    async fn get_peers(&self, request: Request<GetPeersRequest>) -> Result<Streaming<GetPeersResponse>, RpcStatus> {
        let message = request.message();
        let excluded_peers = vec![request.context().peer_node_id().clone()];
        let mut features = Some(PeerFeatures::COMMUNICATION_NODE);
        if message.include_clients {
            features = None;
        }

        let max_claims = message.max_claims.try_into().map_err(|_|
            // This can't happen on a >= 32-bit arch
            RpcStatus::bad_request("max_claims is too large"))?;
        if max_claims == 0 {
            return Err(RpcStatus::bad_request("max_claims must be greater than zero"));
        }
        let max_addresses_per_claim = message.max_addresses_per_claim.try_into().map_err(|_|
            // This can't happen on a >= 32-bit arch
            RpcStatus::bad_request("max_addresses_per_claim is too large"))?;

        if max_addresses_per_claim == 0 {
            return Err(RpcStatus::bad_request(
                "max_addresses_per_claim must be greater than zero",
            ));
        }

        let peers = self
            .peer_manager
            .discovery_syncing(message.n as usize, &excluded_peers, features)
            .await
            .map_err(RpcError::from)?;

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

        Ok(self.stream_peers(peers, max_claims, max_addresses_per_claim))
    }
}
