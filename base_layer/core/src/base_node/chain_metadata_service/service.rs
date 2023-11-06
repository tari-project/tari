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

use std::{convert::TryFrom, sync::Arc};

use log::*;
use num_format::{Locale, ToFormattedString};
use prost::Message;
use tari_common::log_if_error;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{connectivity::ConnectivityRequester, message::MessageExt, BAN_DURATION_LONG};
use tari_p2p::services::liveness::{LivenessEvent, LivenessHandle, MetadataKey, PingPongEvent};
use tokio::sync::broadcast;

use super::{error::ChainMetadataSyncError, LOG_TARGET};
use crate::{
    base_node::{
        chain_metadata_service::handle::{ChainMetadataEvent, PeerChainMetadata},
        comms_interface::{BlockEvent, LocalNodeCommsInterface},
    },
    chain_storage::BlockAddResult,
    proto::base_node as proto,
};

const NUM_ROUNDS_NETWORK_SILENCE: u16 = 3;

pub(super) struct ChainMetadataService {
    liveness: LivenessHandle,
    base_node: LocalNodeCommsInterface,
    connectivity: ConnectivityRequester,
    event_publisher: broadcast::Sender<Arc<ChainMetadataEvent>>,
    number_of_rounds_no_pings: u16,
}

impl ChainMetadataService {
    /// Create a new ChainMetadataService
    ///
    /// ## Arguments
    /// `liveness` - the liveness service handle
    /// `base_node` - the base node service handle
    /// `event_publisher` - A broadcast sender for chain metadata events
    pub fn new(
        liveness: LivenessHandle,
        base_node: LocalNodeCommsInterface,
        connectivity: ConnectivityRequester,
        event_publisher: broadcast::Sender<Arc<ChainMetadataEvent>>,
    ) -> Self {
        Self {
            liveness,
            base_node,
            event_publisher,
            connectivity,
            number_of_rounds_no_pings: 0,
        }
    }

    /// Run the service
    pub async fn run(mut self) {
        let mut liveness_event_stream = self.liveness.get_event_stream();
        let mut block_event_stream = self.base_node.get_block_event_stream();

        log_if_error!(
            target: LOG_TARGET,
            "Error when updating liveness chain metadata: '{}'",
            self.update_liveness_chain_metadata().await
        );

        loop {
            tokio::select! {
                Ok(block_event) = block_event_stream.recv() => {
                    log_if_error!(
                        level: info,
                        target: LOG_TARGET,
                        "Failed to handle block event because '{}'",
                        self.handle_block_event(&block_event).await
                    );
                },

                Ok(event) = liveness_event_stream.recv() => {
                    match
                        self.handle_liveness_event(&event).await {
                        Ok(_) => {}
                        Err(e) => {
                           info!( target: LOG_TARGET, "Failed to handle liveness event because '{}'", e);
                           if let ChainMetadataSyncError::ReceivedInvalidChainMetadata(node_id,reason) = e {
                               log_if_error!(
                                 level: info,
                                 target: LOG_TARGET, "Failed to ban node '{}'",
                                 self.connectivity.ban_peer_until(node_id, BAN_DURATION_LONG, reason).await);                                           }
                        }
                    }

                },
            }
        }
    }

    /// Handle BlockEvents
    async fn handle_block_event(&mut self, event: &BlockEvent) -> Result<(), ChainMetadataSyncError> {
        match event {
            BlockEvent::ValidBlockAdded(_, BlockAddResult::Ok(_)) |
            BlockEvent::ValidBlockAdded(_, BlockAddResult::ChainReorg { .. }) |
            BlockEvent::BlockSyncComplete(_, _) => {
                self.update_liveness_chain_metadata().await?;
            },
            _ => {},
        }

        Ok(())
    }

    /// Tack this node's metadata on to ping/pongs sent by the liveness service
    async fn update_liveness_chain_metadata(&mut self) -> Result<(), ChainMetadataSyncError> {
        let chain_metadata = self.base_node.get_metadata().await?;
        let bytes = proto::ChainMetadata::from(chain_metadata).to_encoded_bytes();
        self.liveness
            .set_metadata_entry(MetadataKey::ChainMetadata, bytes)
            .await?;
        Ok(())
    }

    async fn handle_liveness_event(&mut self, event: &LivenessEvent) -> Result<(), ChainMetadataSyncError> {
        match event {
            // Received a ping, check if it contains ChainMetadata
            LivenessEvent::ReceivedPing(event) => {
                debug!(
                    target: LOG_TARGET,
                    "Received ping from neighbouring node '{}'.", event.node_id
                );
                self.number_of_rounds_no_pings = 0;
                if event.metadata.has(MetadataKey::ChainMetadata) {
                    self.send_chain_metadata_to_event_publisher(event).await?;
                }
            },
            // Received a pong, check if our neighbour sent it and it contains ChainMetadata
            LivenessEvent::ReceivedPong(event) => {
                trace!(
                    target: LOG_TARGET,
                    "Received pong from neighbouring node '{}'.",
                    event.node_id
                );
                self.number_of_rounds_no_pings = 0;
                if event.metadata.has(MetadataKey::ChainMetadata) {
                    self.send_chain_metadata_to_event_publisher(event).await?;
                }
            },
            // New ping round has begun
            LivenessEvent::PingRoundBroadcast(num_peers) => {
                debug!(
                    target: LOG_TARGET,
                    "New chain metadata round sent to {} peer(s)", num_peers
                );
                // If there were no pings for awhile, we are probably alone.
                if *num_peers == 0 {
                    self.number_of_rounds_no_pings += 1;
                    if self.number_of_rounds_no_pings >= NUM_ROUNDS_NETWORK_SILENCE {
                        self.send_network_silence().await?;
                        self.number_of_rounds_no_pings = 0;
                    }
                }
            },
        }

        Ok(())
    }

    async fn send_network_silence(&mut self) -> Result<(), ChainMetadataSyncError> {
        let _size = self.event_publisher.send(Arc::new(ChainMetadataEvent::NetworkSilence));
        Ok(())
    }

    async fn send_chain_metadata_to_event_publisher(
        &mut self,
        event: &PingPongEvent,
    ) -> Result<(), ChainMetadataSyncError> {
        let chain_metadata_bytes = event
            .metadata
            .get(MetadataKey::ChainMetadata)
            .ok_or(ChainMetadataSyncError::NoChainMetadata)?;

        let chain_metadata = ChainMetadata::try_from(proto::ChainMetadata::decode(chain_metadata_bytes.as_slice())?)
            .map_err(|err| ChainMetadataSyncError::ReceivedInvalidChainMetadata(event.node_id.clone(), err))?;
        debug!(
            target: LOG_TARGET,
            "Received chain metadata from NodeId '{}' #{}, Acc_diff {}",
            event.node_id,
            chain_metadata.height_of_longest_chain(),
            chain_metadata.accumulated_difficulty().to_formatted_string(&Locale::en),
        );

        let peer_chain_metadata = PeerChainMetadata::new(event.node_id.clone(), chain_metadata, event.latency);

        // send only fails if there are no subscribers.
        let _size = self
            .event_publisher
            .send(Arc::new(ChainMetadataEvent::PeerChainMetadataReceived(
                peer_chain_metadata,
            )));

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;

    use futures::StreamExt;
    use tari_comms::{peer_manager::NodeId, test_utils::mocks::create_connectivity_mock};
    use tari_p2p::services::liveness::{
        mock::{create_p2p_liveness_mock, LivenessMockState},
        LivenessRequest,
        Metadata,
        PingPongEvent,
    };
    use tari_service_framework::reply_channel;
    use tari_test_utils::unpack_enum;
    use tari_utilities::epoch_time::EpochTime;
    use tokio::{sync::broadcast, task};

    use super::*;
    use crate::base_node::comms_interface::{CommsInterfaceError, NodeCommsRequest, NodeCommsResponse};

    fn create_base_node_nci() -> (
        LocalNodeCommsInterface,
        reply_channel::TryReceiver<NodeCommsRequest, NodeCommsResponse, CommsInterfaceError>,
    ) {
        let (base_node_sender, base_node_receiver) = reply_channel::unbounded();
        let (block_sender, _block_receiver) = reply_channel::unbounded();
        let (block_event_sender, _) = broadcast::channel(50);
        let base_node = LocalNodeCommsInterface::new(base_node_sender, block_sender, block_event_sender);

        (base_node, base_node_receiver)
    }

    fn create_sample_proto_chain_metadata() -> proto::ChainMetadata {
        let diff: u128 = 1;
        proto::ChainMetadata {
            height_of_longest_chain: 1,
            best_block: vec![
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
                28, 29, 30, 31,
            ],
            pruned_height: 0,
            accumulated_difficulty: diff.to_be_bytes().to_vec(),
            timestamp: EpochTime::now().as_u64(),
        }
    }

    fn setup() -> (
        ChainMetadataService,
        LivenessMockState,
        reply_channel::TryReceiver<NodeCommsRequest, NodeCommsResponse, CommsInterfaceError>,
        broadcast::Receiver<Arc<ChainMetadataEvent>>,
    ) {
        let (liveness_handle, mock, _) = create_p2p_liveness_mock(1);
        let liveness_mock_state = mock.get_mock_state();
        task::spawn(mock.run());

        let (base_node, base_node_receiver) = create_base_node_nci();
        let (publisher, event_rx) = broadcast::channel(10);

        let connectivity = create_connectivity_mock();

        let service = ChainMetadataService::new(liveness_handle, base_node, connectivity.0, publisher);

        (service, liveness_mock_state, base_node_receiver, event_rx)
    }

    #[tokio::test]
    async fn update_liveness_chain_metadata() {
        let (mut service, liveness_mock_state, mut base_node_receiver, _) = setup();

        let mut proto_chain_metadata = create_sample_proto_chain_metadata();
        proto_chain_metadata.height_of_longest_chain = 123;
        let chain_metadata = proto_chain_metadata.clone().try_into().unwrap();

        task::spawn(async move {
            if let Some(base_node_req) = base_node_receiver.next().await {
                base_node_req
                    .reply(Ok(NodeCommsResponse::ChainMetadata(chain_metadata)))
                    .unwrap();
            }
        });

        service.update_liveness_chain_metadata().await.unwrap();

        assert_eq!(liveness_mock_state.call_count(), 1);

        let last_call = liveness_mock_state.take_calls().remove(0);
        unpack_enum!(LivenessRequest::SetMetadataEntry(metadata_key, data) = last_call);
        assert_eq!(metadata_key, MetadataKey::ChainMetadata);
        let chain_metadata = proto::ChainMetadata::decode(data.as_slice()).unwrap();
        assert_eq!(chain_metadata.height_of_longest_chain, 123);
    }
    #[tokio::test]
    async fn handle_liveness_event_ok() {
        let (mut service, _, _, mut events_rx) = setup();

        let mut metadata = Metadata::new();
        let proto_chain_metadata = create_sample_proto_chain_metadata();
        metadata.insert(MetadataKey::ChainMetadata, proto_chain_metadata.to_encoded_bytes());

        let node_id = NodeId::new();
        let pong_event = PingPongEvent {
            metadata,
            node_id: node_id.clone(),
            latency: None,
        };

        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        service.handle_liveness_event(&sample_event).await.unwrap();
        let metadata = events_rx.recv().await.unwrap().peer_metadata().unwrap();
        assert_eq!(*metadata.node_id(), node_id);
        assert_eq!(
            metadata.claimed_chain_metadata().height_of_longest_chain(),
            proto_chain_metadata.height_of_longest_chain
        );
    }

    #[tokio::test]
    async fn handle_liveness_event_no_metadata() {
        let (mut service, _, _, mut event_rx) = setup();

        let metadata = Metadata::new();
        let node_id = NodeId::new();
        let pong_event = PingPongEvent {
            metadata,
            node_id,
            latency: None,
        };

        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        service.handle_liveness_event(&sample_event).await.unwrap();
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn handle_liveness_event_bad_metadata() {
        let (mut service, _, _, mut event_rx) = setup();

        let mut metadata = Metadata::new();
        metadata.insert(MetadataKey::ChainMetadata, b"no-good".to_vec());
        let node_id = NodeId::new();
        let pong_event = PingPongEvent {
            metadata,
            node_id,
            latency: None,
        };

        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        let err = service.handle_liveness_event(&sample_event).await.unwrap_err();
        unpack_enum!(ChainMetadataSyncError::DecodeError(_err) = err);
        assert!(event_rx.try_recv().is_err());
    }
}
