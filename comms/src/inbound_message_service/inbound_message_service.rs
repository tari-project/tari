// Copyright 2019. The Tari Project
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
use crate::{
    inbound_message_service::error::InboundMessageServiceError,
    message::{Envelope, FrameSet, InboundMessage, MessageError},
    peer_manager::{NodeId, Peer, PeerManager, PeerManagerError},
};
use futures::{channel::mpsc, FutureExt, Sink, SinkExt, Stream, StreamExt};
use log::*;
use prost::Message;
use std::{convert::TryInto, sync::Arc};
use tari_shutdown::ShutdownSignal;

const LOG_TARGET: &str = "comms::inbound_message_service";

const EXPECTED_MESSAGE_FRAME_LEN: usize = 2;

pub type InboundMessageService<TSink> = InnerInboundMessageService<mpsc::Receiver<FrameSet>, TSink>;

/// The InboundMessageService contains the logic for processing a raw MessageEnvelope received via a PeerConnection and
/// routing it via one of the InboundMessageRoutes. This pipeline contains the logic for the various validations and
/// check done to decide whether this message should be passed further along the pipeline and, be sent to
/// the given message_sink.
pub struct InnerInboundMessageService<TStream, TSink> {
    raw_message_stream: Option<TStream>,
    message_sink: TSink,
    peer_manager: Arc<PeerManager>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl<TStream, TSink> InnerInboundMessageService<TStream, TSink>
where
    TStream: Stream<Item = FrameSet> + Unpin,
    TSink: Sink<InboundMessage> + Unpin,
    TSink::Error: Into<InboundMessageServiceError>,
{
    pub fn new(
        raw_message_stream: TStream,
        message_sink: TSink,
        peer_manager: Arc<PeerManager>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            raw_message_stream: Some(raw_message_stream),
            message_sink,
            peer_manager,
            shutdown_signal: Some(shutdown_signal),
        }
    }

    /// Run the Inbound Message Service on the set `message_sink_receiver` processing each message in that stream. If
    /// an error occurs while processing a message it is logged and the service will move onto the next message. Most
    /// errors represent a reason why a message didn't make it through the pipeline.
    pub async fn run(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("InboundMessageService initialized without shutdown_rx")
            .fuse();

        let mut raw_message_stream = self
            .raw_message_stream
            .take()
            .expect("InboundMessageService initialized without raw_message_stream")
            .fuse();

        info!(target: LOG_TARGET, "Inbound Message Pipeline started");
        loop {
            futures::select! {
                frames = raw_message_stream.select_next_some() => {
                    if let Err(e) = self.process_message(frames).await {
                        info!(target: LOG_TARGET, "Inbound Message Service Error: {:?}", e);
                    }
                },

                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "Inbound message service shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
    }

    /// Process a single received message from its raw serialized form i.e. a FrameSet
    pub async fn process_message(&mut self, mut frames: FrameSet) -> Result<(), InboundMessageServiceError> {
        if frames.len() < EXPECTED_MESSAGE_FRAME_LEN {
            return Err(InboundMessageServiceError::MessageError(
                MessageError::InvalidMultipartMessageLength,
            ));
        }

        let source_node_id: NodeId = frames.remove(0).try_into()?;
        let envelope = Envelope::decode(&frames.remove(0))?;

        if !envelope.is_valid() {
            return Err(InboundMessageServiceError::InvalidEnvelope);
        }

        trace!(
            target: LOG_TARGET,
            "Received message envelope version {} from NodeId={}",
            envelope.version,
            source_node_id
        );

        if !envelope.verify_signature()? {
            return Err(InboundMessageServiceError::InvalidMessageSignature);
        }

        let peer = self.find_known_peer(&source_node_id)?;

        let public_key = envelope.get_comms_public_key().expect("already checked");

        if peer.public_key != public_key {
            return Err(InboundMessageServiceError::PeerPublicKeyMismatch);
        }

        // -- Message is authenticated --

        let Envelope { header, body, .. } = envelope;

        let inbound_message = InboundMessage::new(peer, header.expect("already checked").try_into().expect(""), body);

        self.message_sink.send(inbound_message).await.map_err(Into::into)?;

        Ok(())
    }

    /// Check whether the the source of the message is known to our Peer Manager, if it is return the peer but otherwise
    /// we discard the message as it should be in our Peer Manager
    fn find_known_peer(&self, source_node_id: &NodeId) -> Result<Peer, InboundMessageServiceError> {
        match self.peer_manager.find_by_node_id(source_node_id) {
            Ok(peer) => Ok(peer),
            Err(PeerManagerError::PeerNotFoundError) => {
                warn!(
                    target: LOG_TARGET,
                    "Received unknown node id from peer connection. Discarding message from NodeId '{}'",
                    source_node_id
                );
                Err(InboundMessageServiceError::CannotFindSourcePeer)
            },
            Err(PeerManagerError::BannedPeer) => {
                warn!(
                    target: LOG_TARGET,
                    "Received banned node id from peer connection. Discarding message from NodeId '{}'", source_node_id
                );
                Err(InboundMessageServiceError::CannotFindSourcePeer)
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Peer manager failed to look up source node id because '{}'", err
                );
                Err(InboundMessageServiceError::PeerManagerError(err))
            },
        }
    }
}
