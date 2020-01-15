// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use super::{error::ChainMetadataSyncError, LOG_TARGET};
use crate::{
    base_node::{
        chain_metadata_service::handle::ChainMetadataEvent,
        comms_interface::{BlockEvent, LocalNodeCommsInterface},
        proto,
    },
    chain_storage::{BlockAddResult, ChainMetadata},
};
use chrono::{NaiveDateTime, Utc};
use futures::{stream::StreamExt, SinkExt};
use log::*;
use prost::Message;
use tari_broadcast_channel::Publisher;
use tari_common::log_if_error;
use tari_comms::{message::MessageExt, peer_manager::NodeId};
use tari_p2p::services::liveness::{LivenessEvent, LivenessHandle, Metadata, MetadataKey};

pub(super) struct ChainMetadataService {
    liveness: LivenessHandle,
    base_node: LocalNodeCommsInterface,
    peer_chain_metadata: Vec<PeerChainMetadata>,
    last_chainstate_flushed_at: NaiveDateTime,
    event_publisher: Publisher<ChainMetadataEvent>,
}

impl ChainMetadataService {
    /// Create a new ChainMetadataService
    ///
    /// ## Arguments
    /// `liveness` - the liveness service handle
    /// `base_node` - the base node service handle
    pub fn new(
        liveness: LivenessHandle,
        base_node: LocalNodeCommsInterface,
        event_publisher: Publisher<ChainMetadataEvent>,
    ) -> Self
    {
        Self {
            liveness,
            base_node,
            peer_chain_metadata: Vec::new(),
            last_chainstate_flushed_at: Utc::now().naive_utc(),
            event_publisher,
        }
    }

    /// Run the service
    pub async fn run(mut self) {
        let mut liveness_event_stream = self.liveness.get_event_stream_fused();
        let mut base_node_event_stream = self.base_node.get_block_event_stream_fused();

        log_if_error!(
            target: LOG_TARGET,
            "Error when updating liveness chain metadata: '{}'",
            self.update_liveness_chain_metadata().await
        );

        loop {
            futures::select! {
                event = base_node_event_stream.select_next_some() => {
                    log_if_error!(
                        level: debug,
                        target: LOG_TARGET,
                        "Failed to handle base node event because '{}'",
                        self.handle_block_event(&event).await
                    );
                },

                liveness_event = liveness_event_stream.select_next_some() => {
                    log_if_error!(
                        target: LOG_TARGET,
                        "Failed to handle liveness event because '{}'",
                        self.handle_liveness_event(&liveness_event).await
                    );
                },

                complete => {
                    info!(target: LOG_TARGET, "ChainStateSyncService is exiting because all tasks have completed");
                    break;
                }
            }
        }
    }

    /// Handle BlockEvents
    async fn handle_block_event(&mut self, event: &BlockEvent) -> Result<(), ChainMetadataSyncError> {
        match event {
            BlockEvent::Verified((_, BlockAddResult::Ok)) => {
                self.update_liveness_chain_metadata().await?;
            },
            BlockEvent::Verified(_) | BlockEvent::Invalid(_) => {},
        }

        Ok(())
    }

    /// Send this node's metadata to
    async fn update_liveness_chain_metadata(&mut self) -> Result<(), ChainMetadataSyncError> {
        let chain_metadata = self.base_node.get_metadata().await?;
        let bytes = proto::ChainMetadata::from(chain_metadata).to_encoded_bytes()?;
        self.liveness
            .set_pong_metadata_entry(MetadataKey::ChainMetadata, bytes)
            .await?;
        Ok(())
    }

    async fn handle_liveness_event(&mut self, event: &LivenessEvent) -> Result<(), ChainMetadataSyncError> {
        match event {
            // Received a pong, check if our neighbour sent it and it contains ChainMetadata
            LivenessEvent::ReceivedPong(event) => {
                if event.is_neighbour {
                    self.collect_chain_state_from_pong(&event.node_id, &event.metadata)?;

                    // All peers have responded in this round, send the chain metadata to the base node service
                    if self.peer_chain_metadata.len() == self.peer_chain_metadata.capacity() {
                        self.flush_chain_metadata_to_event_publisher().await?;
                    }
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "Received pong from non-neighbouring node '{}'. Pong ignored...", event.node_id
                    )
                }
            },
            // New ping round has begun
            LivenessEvent::BroadcastedNeighbourPings(num_peers) => {
                // If we have chain metadata to send to the base node service, send them now
                // because the next round of pings is happening.
                // TODO: It's pretty easy for this service to require either a percentage of peers
                //       to respond or, a time limit before assuming some peers will never respond
                //       between rounds (even if this time limit is larger than one or more ping rounds)
                //       before publishing the chain metadata event.
                //       The following will send the chain metadata at the start of a new round if at
                //       least one node has responded.
                if self.peer_chain_metadata.len() > 0 {
                    self.flush_chain_metadata_to_event_publisher().await?;
                }
                // Ensure that we're waiting for the correct amount of peers to respond
                // and have allocated space for their replies
                self.resize_chainstate_buffer(*num_peers);
            },
            _ => {},
        }

        Ok(())
    }

    async fn flush_chain_metadata_to_event_publisher(&mut self) -> Result<(), ChainMetadataSyncError> {
        let chain_metadata = self
            .peer_chain_metadata
            .drain(..)
            .map(|peer_metadata| peer_metadata.chain_metadata)
            .collect::<Vec<_>>();

        self.event_publisher
            .send(ChainMetadataEvent::PeerChainMetadataReceived(chain_metadata))
            .await
            .map_err(|_| ChainMetadataSyncError::EventPublishFailed)?;

        self.last_chainstate_flushed_at = Utc::now().naive_utc();

        Ok(())
    }

    fn resize_chainstate_buffer(&mut self, n: usize) {
        match self.peer_chain_metadata.capacity() {
            cap if n > cap => {
                let additional = n - self.peer_chain_metadata.len();
                self.peer_chain_metadata.reserve_exact(additional);
            },
            cap if n < cap => {
                self.peer_chain_metadata.shrink_to(cap);
            },
            _ => {},
        }
    }

    fn collect_chain_state_from_pong(
        &mut self,
        node_id: &NodeId,
        metadata: &Metadata,
    ) -> Result<(), ChainMetadataSyncError>
    {
        let chain_metadata_bytes = metadata
            .get(&MetadataKey::ChainMetadata)
            .ok_or(ChainMetadataSyncError::NoChainMetadata)?;

        debug!(target: LOG_TARGET, "Received chain metadata from NodeId '{}'", node_id);
        let chain_metadata = proto::ChainMetadata::decode(chain_metadata_bytes)?.into();

        if let Some(pos) = self
            .peer_chain_metadata
            .iter()
            .position(|peer_chainstate| &peer_chainstate.node_id == node_id)
        {
            self.peer_chain_metadata.remove(pos);
        }

        self.peer_chain_metadata
            .push(PeerChainMetadata::new(node_id.clone(), chain_metadata));

        Ok(())
    }
}

struct PeerChainMetadata {
    node_id: NodeId,
    chain_metadata: ChainMetadata,
}

impl PeerChainMetadata {
    fn new(node_id: NodeId, chain_metadata: ChainMetadata) -> Self {
        Self {
            node_id,
            chain_metadata,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::base_node::comms_interface::{CommsInterfaceError, NodeCommsRequest, NodeCommsResponse};
    use std::convert::TryInto;
    use tari_broadcast_channel as broadcast_channel;
    use tari_p2p::services::liveness::{mock::create_p2p_liveness_mock, LivenessRequest, PongEvent};
    use tari_service_framework::reply_channel;
    use tari_test_utils::{runtime, unpack_enum};

    fn create_base_node_nci() -> (
        LocalNodeCommsInterface,
        reply_channel::Receiver<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>,
    ) {
        let (base_node_sender, base_node_receiver) = reply_channel::unbounded();
        let (block_sender, _block_receiver) = reply_channel::unbounded();
        let (_base_node_publisher, subscriber) = broadcast_channel::bounded(1);
        let base_node = LocalNodeCommsInterface::new(base_node_sender, block_sender, subscriber);

        (base_node, base_node_receiver)
    }

    fn create_sample_proto_chain_metadata() -> proto::ChainMetadata {
        proto::ChainMetadata {
            height_of_longest_chain: Some(1),
            best_block: Some(vec![]),
            pruning_horizon: 64,
        }
    }

    #[test]
    fn update_liveness_chain_metadata() {
        runtime::test_async(|rt| {
            let (liveness_handle, liveness_mock) = create_p2p_liveness_mock(1);
            let liveness_mock_state = liveness_mock.get_mock_state();
            rt.spawn(liveness_mock.run());

            let (base_node, mut base_node_receiver) = create_base_node_nci();

            let (publisher, _subscriber) = broadcast_channel::bounded(1);
            let mut service = ChainMetadataService::new(liveness_handle, base_node, publisher);

            let mut proto_chain_metadata = create_sample_proto_chain_metadata();
            proto_chain_metadata.height_of_longest_chain = Some(123);
            let chain_metadata = proto_chain_metadata.clone().try_into().unwrap();

            rt.spawn(async move {
                let base_node_req = base_node_receiver.select_next_some().await;
                let (_req, reply_tx) = base_node_req.split();
                reply_tx
                    .send(Ok(NodeCommsResponse::ChainMetadata(chain_metadata)))
                    .unwrap();
            });

            rt.block_on(service.update_liveness_chain_metadata()).unwrap();

            assert_eq!(liveness_mock_state.call_count(), 1);

            let last_call = liveness_mock_state.take_calls().remove(0);
            unpack_enum!(LivenessRequest::SetPongMetadata(metadata_key, data) = last_call);
            assert_eq!(metadata_key, MetadataKey::ChainMetadata);
            let chain_metadata = proto::ChainMetadata::decode(&data).unwrap();
            assert_eq!(chain_metadata.height_of_longest_chain, Some(123));
        });
    }

    #[tokio_macros::test]
    async fn handle_liveness_event_ok() {
        let (liveness_handle, _) = create_p2p_liveness_mock(1);
        let mut metadata = Metadata::new();
        let proto_chain_metadata = create_sample_proto_chain_metadata();
        metadata.insert(
            MetadataKey::ChainMetadata,
            proto_chain_metadata.to_encoded_bytes().unwrap(),
        );

        let node_id = NodeId::new();
        let pong_event = PongEvent {
            is_neighbour: true,
            metadata,
            node_id: node_id.clone(),
            latency: None,
            is_monitored: false,
        };

        let (base_node, _) = create_base_node_nci();

        let (publisher, _subscriber) = broadcast_channel::bounded(1);
        let mut service = ChainMetadataService::new(liveness_handle, base_node, publisher);

        // To prevent the chain metadata buffer being flushed after receiving a single pong event,
        // extend it's capacity to 2
        service.peer_chain_metadata.reserve_exact(2);
        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        service.handle_liveness_event(&sample_event).await.unwrap();
        assert_eq!(service.peer_chain_metadata.len(), 1);
        let metadata = service.peer_chain_metadata.remove(0);
        assert_eq!(metadata.node_id, node_id);
        assert_eq!(
            metadata.chain_metadata.height_of_longest_chain,
            proto_chain_metadata.height_of_longest_chain
        );
    }

    #[tokio_macros::test]
    async fn handle_liveness_event_no_metadata() {
        let (liveness_handle, _) = create_p2p_liveness_mock(1);
        let metadata = Metadata::new();
        let node_id = NodeId::new();
        let pong_event = PongEvent {
            is_neighbour: true,
            metadata,
            node_id,
            latency: None,
            is_monitored: false,
        };

        let (base_node, _) = create_base_node_nci();
        let (publisher, _subscriber) = broadcast_channel::bounded(1);
        let mut service = ChainMetadataService::new(liveness_handle, base_node, publisher);

        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        let err = service.handle_liveness_event(&sample_event).await.unwrap_err();
        unpack_enum!(ChainMetadataSyncError::NoChainMetadata = err);
        assert_eq!(service.peer_chain_metadata.len(), 0);
    }

    #[tokio_macros::test]
    async fn handle_liveness_event_not_neighbour() {
        let (liveness_handle, _) = create_p2p_liveness_mock(1);
        let metadata = Metadata::new();
        let node_id = NodeId::new();
        let pong_event = PongEvent {
            is_neighbour: false,
            metadata,
            node_id,
            latency: None,
            is_monitored: false,
        };

        let (base_node, _) = create_base_node_nci();
        let (publisher, _subscriber) = broadcast_channel::bounded(1);
        let mut service = ChainMetadataService::new(liveness_handle, base_node, publisher);

        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        service.handle_liveness_event(&sample_event).await.unwrap();
        assert_eq!(service.peer_chain_metadata.len(), 0);
    }

    #[tokio_macros::test]
    async fn handle_liveness_event_bad_metadata() {
        let (liveness_handle, _) = create_p2p_liveness_mock(1);
        let mut metadata = Metadata::new();
        metadata.insert(MetadataKey::ChainMetadata, b"no-good".to_vec());
        let node_id = NodeId::new();
        let pong_event = PongEvent {
            is_neighbour: true,
            metadata,
            node_id,
            latency: None,
            is_monitored: false,
        };

        let (base_node, _) = create_base_node_nci();
        let (publisher, _subscriber) = broadcast_channel::bounded(1);
        let mut service = ChainMetadataService::new(liveness_handle, base_node, publisher);

        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        let err = service.handle_liveness_event(&sample_event).await.unwrap_err();
        unpack_enum!(ChainMetadataSyncError::DecodeError(_err) = err);
        assert_eq!(service.peer_chain_metadata.len(), 0);
    }
}
