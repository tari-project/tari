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

use std::{convert::TryFrom, sync::Arc, time::Duration};

use log::*;
use prost::Message;
use tari_common::log_if_error;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{BAN_DURATION_LONG, connectivity::ConnectivityRequester, message::MessageExt};
use tari_p2p::services::liveness::{LivenessEvent, LivenessHandle, MetadataKey, PingPongEvent};
use tokio::sync::broadcast;

use super::{LOG_TARGET, error::ChainMetadataSyncError};
use crate::{
    base_node::{
        chain_metadata_service::handle::{ChainMetadataEvent, PeerChainMetadata},
        comms_interface::{BlockEvent, LocalNodeCommsInterface},
    },
    chain_storage::BlockAddResult,
    proto::base_node as proto,
};

const NUM_ROUNDS_NETWORK_SILENCE: u16 = 3;
/// Maximum encoded byte length of a `MetadataKey::ChainMetadata` entry (~130 bytes for a valid `proto::ChainMetadata`).
pub(super) const MAX_CHAIN_METADATA_BYTES: usize = 512;

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
        let liveness_event_stream = self.liveness.get_event_stream();
        let block_event_stream = self.base_node.get_block_event_stream();

        // Wait briefly for the base node to create chain metadata on startup without blocking a Tokio worker thread.
        tokio::time::sleep(Duration::from_secs(2)).await;
        log_if_error!(
            target: LOG_TARGET,
            "Error when updating liveness chain metadata: '{}'",
            self.update_liveness_chain_metadata().await
        );

        self.run_event_loop(liveness_event_stream, block_event_stream).await;
    }

    async fn run_event_loop(
        &mut self,
        mut liveness_event_stream: broadcast::Receiver<Arc<LivenessEvent>>,
        mut block_event_stream: broadcast::Receiver<Arc<BlockEvent>>,
    ) {
        loop {
            tokio::select! {
                block_event = block_event_stream.recv() => {
                    match block_event {
                        Ok(block_event) => {
                            log_if_error!(
                                level: info,
                                target: LOG_TARGET,
                                "Failed to handle block event because '{}'",
                                self.handle_block_event(&block_event).await
                            );
                        },
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(
                                target: LOG_TARGET,
                                "ChainMetadataService lagged behind by {n} block event(s); refreshing chain metadata"
                            );
                            log_if_error!(
                                target: LOG_TARGET,
                                "Error when refreshing liveness chain metadata after lag: '{}'",
                                self.update_liveness_chain_metadata().await
                            );
                        },
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!(target: LOG_TARGET, "Block event stream closed; shutting down ChainMetadataService");
                            break;
                        },
                    }
                },

                liveness_event = liveness_event_stream.recv() => {
                    match liveness_event {
                        Ok(event) => {
                            if let Err(e) = self.handle_liveness_event(&event).await {
                                info!(target: LOG_TARGET, "Failed to handle liveness event because '{e}'");
                                if let ChainMetadataSyncError::ReceivedInvalidChainMetadata(node_id, reason) = e {
                                    log_if_error!(
                                        level: info,
                                        target: LOG_TARGET,
                                        "Failed to ban node '{}'",
                                        self.connectivity.ban_peer_until(node_id, BAN_DURATION_LONG, reason).await
                                    );
                                }
                            }
                        },
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(target: LOG_TARGET, "ChainMetadataService lagged behind by {n} liveness event(s)");
                        },
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!(target: LOG_TARGET, "Liveness event stream closed; shutting down ChainMetadataService");
                            break;
                        },
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
                self.number_of_rounds_no_pings = 0;
                if event.metadata.has(MetadataKey::ChainMetadata) {
                    self.send_chain_metadata_to_event_publisher(event).await?;
                }
            },
            // Received a pong, check if our neighbour sent it and it contains ChainMetadata
            LivenessEvent::ReceivedPong(event) => {
                self.number_of_rounds_no_pings = 0;
                if event.metadata.has(MetadataKey::ChainMetadata) {
                    self.send_chain_metadata_to_event_publisher(event).await?;
                }
            },
            // New ping round has begun
            LivenessEvent::PingRoundBroadcast(num_peers) => {
                trace!(
                    target: LOG_TARGET,
                    "New chain metadata round sent to {num_peers} peer(s)"
                );
                // If there were no pings for a while, we are probably alone.
                if *num_peers == 0 {
                    self.number_of_rounds_no_pings = self.number_of_rounds_no_pings.saturating_add(1);
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

        if chain_metadata_bytes.len() > MAX_CHAIN_METADATA_BYTES {
            return Err(ChainMetadataSyncError::ReceivedInvalidChainMetadata(
                event.node_id.clone(),
                format!(
                    "ChainMetadata entry of {} bytes exceeds limit of {MAX_CHAIN_METADATA_BYTES}",
                    chain_metadata_bytes.len()
                ),
            ));
        }

        let proto_metadata = proto::ChainMetadata::decode(chain_metadata_bytes.as_slice()).map_err(|err| {
            ChainMetadataSyncError::ReceivedInvalidChainMetadata(
                event.node_id.clone(),
                format!("Failed to decode ChainMetadata protobuf: {err}"),
            )
        })?;
        let chain_metadata = ChainMetadata::try_from(proto_metadata)
            .map_err(|err| ChainMetadataSyncError::ReceivedInvalidChainMetadata(event.node_id.clone(), err))?;
        debug!(
            target: LOG_TARGET,
            "Received chain metadata from NodeId '{}' #{}, Acc_diff {}",
            event.node_id,
            chain_metadata.best_block_height(),
            chain_metadata.accumulated_difficulty(),
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
    use primitive_types::U512;
    use tari_comms::{peer_manager::NodeId, test_utils::mocks::create_connectivity_mock};
    use tari_p2p::services::liveness::{
        LivenessRequest,
        Metadata,
        mock::{LivenessMockState, create_p2p_liveness_mock},
    };
    use tari_service_framework::reply_channel;
    use tari_test_utils::unpack_enum;
    use tari_utilities::epoch_time::EpochTime;
    use tokio::task;

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
        let diff: U512 = 1.into();
        let mut bytes = [0u8; 64];
        diff.to_big_endian(&mut bytes);
        proto::ChainMetadata {
            best_block_height: 1,
            best_block_hash: vec![
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
                28, 29, 30, 31,
            ],
            pruned_height: 0,
            accumulated_difficulty_low: bytes[32..=63].to_vec(),
            accumulated_difficulty_high: bytes[0..=31].to_vec(),
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
        proto_chain_metadata.best_block_height = 123;
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
        assert_eq!(chain_metadata.best_block_height, 123);
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
            nonce: 0,
        };

        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        service.handle_liveness_event(&sample_event).await.unwrap();
        let metadata = events_rx.recv().await.unwrap().peer_metadata().unwrap();
        assert_eq!(*metadata.node_id(), node_id);
        assert_eq!(
            metadata.claimed_chain_metadata().best_block_height(),
            proto_chain_metadata.best_block_height
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
            nonce: 0,
        };

        let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
        service.handle_liveness_event(&sample_event).await.unwrap();
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn handle_liveness_event_bad_metadata() {
        let (mut service, _, _, mut event_rx) = setup();

        for payload in [b"no-good".to_vec(), vec![
            0u8;
            MAX_CHAIN_METADATA_BYTES.saturating_add(1)
        ]] {
            let mut metadata = Metadata::new();
            metadata.insert(MetadataKey::ChainMetadata, payload);
            let node_id = NodeId::new();
            let pong_event = PingPongEvent {
                metadata,
                node_id: node_id.clone(),
                latency: None,
                nonce: 0,
            };

            let sample_event = LivenessEvent::ReceivedPong(Box::new(pong_event));
            let err = service.handle_liveness_event(&sample_event).await.unwrap_err();
            unpack_enum!(ChainMetadataSyncError::ReceivedInvalidChainMetadata(offender, _reason) = err);
            assert_eq!(offender, node_id);
            assert!(event_rx.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn run_event_loop_bans_peers_sending_corrupt_or_oversized_chain_metadata_and_exits_on_stream_close() {
        let (liveness_handle, liveness_mock, _unused_liveness_tx) = create_p2p_liveness_mock(8);
        task::spawn(liveness_mock.run());

        let (base_node, _base_node_receiver) = create_base_node_nci();
        let (mut connectivity, connectivity_mock) = create_connectivity_mock();
        let connectivity_state = connectivity_mock.spawn();
        let (publisher, _event_rx) = broadcast::channel(10);

        let mut service = ChainMetadataService::new(liveness_handle, base_node, connectivity.clone(), publisher);
        let (liveness_events_tx, liveness_events_rx) = broadcast::channel(8);
        let (_block_events_tx, block_events_rx) = broadcast::channel(8);

        let bad_peer_decode = NodeId::new();
        let mut meta1 = Metadata::new();
        meta1.insert(MetadataKey::ChainMetadata, b"corrupt-protobuf".to_vec());
        liveness_events_tx
            .send(Arc::new(LivenessEvent::ReceivedPing(Box::new(PingPongEvent {
                metadata: meta1,
                node_id: bad_peer_decode.clone(),
                latency: None,
                nonce: 1,
            }))))
            .unwrap();

        let bad_peer_oversized = NodeId::new();
        let mut meta2 = Metadata::new();
        meta2.insert(MetadataKey::ChainMetadata, vec![
            0xaa;
            MAX_CHAIN_METADATA_BYTES
                .saturating_add(1)
        ]);
        liveness_events_tx
            .send(Arc::new(LivenessEvent::ReceivedPong(Box::new(PingPongEvent {
                metadata: meta2,
                node_id: bad_peer_oversized.clone(),
                latency: None,
                nonce: 2,
            }))))
            .unwrap();

        // Dropping the sender causes `recv()` to yield both queued events and then `RecvError::Closed`, exiting cleanly
        drop(liveness_events_tx);
        tokio::time::timeout(
            Duration::from_secs(5),
            service.run_event_loop(liveness_events_rx, block_events_rx),
        )
        .await
        .expect("run_event_loop must exit cleanly when liveness stream closes");

        connectivity.get_connectivity_status().await.unwrap();
        let banned = connectivity_state.take_banned_peers().await;
        let [first_ban, second_ban] = banned.as_slice() else {
            panic!("expected 2 banned peers, got {banned:?}");
        };
        assert_eq!(first_ban.0, bad_peer_decode);
        assert_eq!(first_ban.1, BAN_DURATION_LONG);
        assert_eq!(second_ban.0, bad_peer_oversized);
        assert_eq!(second_ban.1, BAN_DURATION_LONG);
    }

    #[tokio::test]
    async fn run_event_loop_refreshes_liveness_metadata_after_lagged_block_events() {
        let (liveness_handle, liveness_mock, _unused_liveness_tx) = create_p2p_liveness_mock(8);
        let liveness_mock_state = liveness_mock.get_mock_state();
        task::spawn(liveness_mock.run());

        let (base_node, mut base_node_receiver) = create_base_node_nci();
        let sample_chain_metadata: ChainMetadata = create_sample_proto_chain_metadata().try_into().unwrap();
        task::spawn(async move {
            while let Some(req) = base_node_receiver.next().await {
                let _ignore = req.reply(Ok(NodeCommsResponse::ChainMetadata(sample_chain_metadata.clone())));
            }
        });

        let (connectivity, connectivity_mock) = create_connectivity_mock();
        connectivity_mock.spawn();
        let (publisher, _event_rx) = broadcast::channel(10);

        let mut service = ChainMetadataService::new(liveness_handle, base_node, connectivity, publisher);
        let (_liveness_events_tx, liveness_events_rx) = broadcast::channel(8);
        // Capacity 1: sending 3 events queues `RecvError::Lagged(2)` followed by 1 `BlockSyncRewind` (a no-op on `Ok`)
        let (block_events_tx, block_events_rx) = broadcast::channel(1);
        for _ in 0..3 {
            let _ignore = block_events_tx.send(Arc::new(BlockEvent::BlockSyncRewind(Vec::new())));
        }
        drop(block_events_tx);

        tokio::time::timeout(
            Duration::from_secs(5),
            service.run_event_loop(liveness_events_rx, block_events_rx),
        )
        .await
        .expect("run_event_loop must exit cleanly when block event stream closes");

        assert_eq!(liveness_mock_state.call_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn run_updates_initial_metadata_after_non_blocking_startup_delay() {
        let (liveness_handle, liveness_mock, _liveness_events_tx) = create_p2p_liveness_mock(8);
        let liveness_mock_state = liveness_mock.get_mock_state();
        task::spawn(liveness_mock.run());

        let (base_node, mut base_node_receiver) = create_base_node_nci();
        let sample_chain_metadata: ChainMetadata = create_sample_proto_chain_metadata().try_into().unwrap();
        let (updated_tx, updated_rx) = tokio::sync::oneshot::channel();
        task::spawn(async move {
            if let Some(req) = base_node_receiver.next().await {
                let _ignore = req.reply(Ok(NodeCommsResponse::ChainMetadata(sample_chain_metadata)));
                let _ignore = updated_tx.send(());
            }
        });

        let (connectivity, connectivity_mock) = create_connectivity_mock();
        connectivity_mock.spawn();
        let (publisher, _event_rx) = broadcast::channel(10);

        let service = ChainMetadataService::new(liveness_handle, base_node, connectivity, publisher);
        let run_handle = task::spawn(service.run());

        tokio::task::yield_now().await;
        assert_eq!(liveness_mock_state.call_count(), 0);

        tokio::time::timeout(Duration::from_secs(5), async {
            updated_rx.await.unwrap();
            while liveness_mock_state.call_count() < 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("startup metadata update should complete once virtual clock advances 2s");
        assert_eq!(liveness_mock_state.call_count(), 1);

        run_handle.abort();
    }
}
