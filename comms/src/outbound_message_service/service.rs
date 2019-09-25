// Copyright 2019 The Tari Project
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

use super::messages::OutboundRequest;
use crate::{
    connection_manager::ConnectionManagerRequester,
    message::{Frame, Message, MessageEnvelope, MessageFlags, MessageHeader, NodeDestination},
    middleware::MiddlewareError,
    outbound_message_service::{
        broadcast_strategy::BroadcastStrategy,
        error::OutboundServiceError,
        messages::OutboundMessage,
        worker::OutboundMessageWorker,
    },
    peer_manager::{NodeIdentity, PeerManager},
};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
    StreamExt,
};
use log::*;
use std::{error::Error, sync::Arc};
use tari_utilities::message_format::MessageFormat;
use tokio::runtime::TaskExecutor;
use tower::Service;

const LOG_TARGET: &'static str = "comms::outbound_message_service::service";

/// Configuration for the OutboundService
pub struct OutboundServiceConfig {
    pub max_attempts: usize,
}

impl Default for OutboundServiceConfig {
    fn default() -> Self {
        Self { max_attempts: 10 }
    }
}

#[derive(Clone)]
pub struct OutboundServiceRequester {
    sender: mpsc::UnboundedSender<OutboundRequest>,
}

impl OutboundServiceRequester {
    pub fn new(sender: mpsc::UnboundedSender<OutboundRequest>) -> Self {
        Self { sender }
    }

    /// Send a comms message
    pub async fn send_message<T, MType>(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        message_type: MType,
        message: T,
    ) -> Result<(), OutboundServiceError>
    where
        MessageHeader<MType>: MessageFormat,
        T: MessageFormat,
    {
        let frame = serialize_message(message_type, message)?;
        self.send_raw(broadcast_strategy, flags, frame).await
    }

    /// Send a raw comms message
    pub async fn send_raw(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        frame: Frame,
    ) -> Result<(), OutboundServiceError>
    {
        self.sender
            .send(OutboundRequest::SendMsg {
                broadcast_strategy,
                flags,
                body: Box::new(frame),
            })
            .await
            .map_err(Into::into)
    }

    /// Forward a comms message
    pub async fn forward_message(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        envelope: MessageEnvelope,
    ) -> Result<(), OutboundServiceError>
    {
        self.sender
            .send(OutboundRequest::Forward {
                broadcast_strategy,
                message_envelope: Box::new(envelope),
            })
            .await
            .map_err(Into::into)
    }
}

fn serialize_message<T, MType>(message_type: MType, message: T) -> Result<Frame, OutboundServiceError>
where
    T: MessageFormat,
    MessageHeader<MType>: MessageFormat,
{
    let header = MessageHeader::new(message_type)?;
    let msg = Message::from_message_format(header, message)?;

    msg.to_binary().map_err(Into::into)
}

/// Responsible for constructing messages using a broadcast strategy and passing them on to
/// the worker task.
pub struct OutboundMessageService<TMiddleware> {
    executor: TaskExecutor,
    outbound_tx: mpsc::UnboundedSender<Vec<OutboundMessage>>,
    request_rx: mpsc::UnboundedReceiver<OutboundRequest>,
    worker: Option<OutboundMessageWorker<TMiddleware, mpsc::UnboundedReceiver<Vec<OutboundMessage>>>>,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    worker_shutdown_tx: oneshot::Sender<()>,
}

impl<TMiddleware> OutboundMessageService<TMiddleware>
where
    TMiddleware: Service<OutboundMessage, Response = Option<OutboundMessage>, Error = MiddlewareError> + Send + 'static, /* Unpin + 'static */
    TMiddleware::Future: Send,
{
    pub fn new(
        config: OutboundServiceConfig,
        executor: TaskExecutor,
        middleware: TMiddleware,
        request_rx: mpsc::UnboundedReceiver<OutboundRequest>,
        peer_manager: Arc<PeerManager>,
        conn_manager: ConnectionManagerRequester,
        node_identity: Arc<NodeIdentity>,
    ) -> Self
    {
        let (outbound_tx, outbound_rx) = mpsc::unbounded();
        let (worker_shutdown_tx, worker_shutdown_rx) = oneshot::channel();
        let worker = OutboundMessageWorker::new(config, middleware, outbound_rx, conn_manager, worker_shutdown_rx);

        Self {
            executor,
            outbound_tx,
            worker: Some(worker),
            request_rx,
            peer_manager,
            node_identity,
            worker_shutdown_tx,
        }
    }

    pub async fn start(mut self) {
        self.start_outbound_worker();

        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => {
                    if let Err(err) = self.handle_request(request).await {
                        error!(target: LOG_TARGET, "{}", err.description());
                    }
                },

                complete => {
                    info!(target: LOG_TARGET, "OutboundService shutting down");
                    let _ = self.worker_shutdown_tx.send(());
                    break;
                }
            }
        }
    }

    fn start_outbound_worker(&mut self) {
        let worker = self.worker.take().expect("start_outbound_worker called more than once");
        self.executor.spawn(worker.start())
    }

    async fn handle_request(&mut self, request: OutboundRequest) -> Result<(), OutboundServiceError> {
        match request {
            OutboundRequest::SendMsg {
                broadcast_strategy,
                flags,
                body,
            } => self.send_msg(broadcast_strategy, flags, body).await,

            OutboundRequest::Forward {
                broadcast_strategy,
                message_envelope,
            } => self.forward_message(broadcast_strategy, message_envelope).await,
        }
    }

    async fn send_msg(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        body: Box<Frame>,
    ) -> Result<(), OutboundServiceError>
    {
        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then construct and send a
        // individually wrapped MessageEnvelope to each selected peer
        let selected_node_identities = self.peer_manager.get_broadcast_identities(broadcast_strategy)?;

        // Construct a MessageEnvelope for each recipient
        let mut outbound_messages = Vec::with_capacity(selected_node_identities.len());
        for dest_node_identity in selected_node_identities {
            let message_envelope = MessageEnvelope::construct(
                &self.node_identity,
                dest_node_identity.public_key.clone(),
                NodeDestination::NodeId(dest_node_identity.node_id.clone()),
                *body.clone(),
                flags,
            )?;

            outbound_messages.push(OutboundMessage::new(
                dest_node_identity.node_id,
                message_envelope.into_frame_set(),
            ));
        }

        if !outbound_messages.is_empty() {
            self.outbound_tx
                .send(outbound_messages)
                .await
                .map_err(OutboundServiceError::SendError)?;
        }

        Ok(())
    }

    async fn forward_message(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        envelope: Box<MessageEnvelope>,
    ) -> Result<(), OutboundServiceError>
    {
        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then forward the
        // received message to each selected peer
        let selected_node_identities = self.peer_manager.get_broadcast_identities(broadcast_strategy)?;
        // Modify MessageEnvelope for forwarding
        let message_envelope = MessageEnvelope::forward_construct(&self.node_identity, *envelope)?;
        let message_envelope_frames = message_envelope.into_frame_set();

        let outbound_messages = selected_node_identities
            .into_iter()
            .map(|dest_node_identity| OutboundMessage::new(dest_node_identity.node_id, message_envelope_frames.clone()))
            .collect::<Vec<_>>();

        if !outbound_messages.is_empty() {
            self.outbound_tx
                .send(outbound_messages)
                .await
                .map_err(OutboundServiceError::SendError)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::NetAddress,
        connection_manager::actor::ConnectionManagerRequest,
        middleware::IdentityOutboundMiddleware,
        peer_manager::{NodeId, Peer, PeerFlags},
        test_utils::node_identity,
        types::{CommsDatabase, CommsPublicKey},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn send_msg_then_shutdown() {
        let rt = Runtime::new().unwrap();
        let (request_tx, request_rx) = mpsc::unbounded();
        let mut sender = OutboundServiceRequester::new(request_tx);

        let (conn_man_tx, mut conn_man_rx) = mpsc::channel(1);
        let conn_manager = ConnectionManagerRequester::new(conn_man_tx);

        let peer_manager = PeerManager::new(CommsDatabase::new()).map(Arc::new).unwrap();
        let pk = CommsPublicKey::default();
        let example_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk).unwrap(),
            vec!["127.0.0.1:9999".parse::<NetAddress>().unwrap()].into(),
            PeerFlags::empty(),
        );
        peer_manager.add_peer(example_peer.clone()).unwrap();

        let node_identity = Arc::new(node_identity::random(None));
        let service = OutboundMessageService::new(
            Default::default(),
            rt.executor(),
            IdentityOutboundMiddleware::new(),
            request_rx,
            peer_manager,
            conn_manager,
            node_identity,
        );
        rt.spawn(service.start());

        rt.block_on(sender.send_message(
            BroadcastStrategy::Flood,
            MessageFlags::NONE,
            "custom_msg".to_string(),
            "Hi everyone!".to_string(),
        ))
        .unwrap();

        let msg = rt.block_on(conn_man_rx.next()).unwrap();
        match msg {
            ConnectionManagerRequest::DialPeer(boxed) => {
                let (node_id, _) = *boxed;
                assert_eq!(node_id, example_peer.node_id);
            },
        }

        drop(sender);
        rt.shutdown_on_idle();
    }

    #[test]
    fn forward_msg_then_shutdown() {
        let rt = Runtime::new().unwrap();
        let (request_tx, request_rx) = mpsc::unbounded();
        let mut sender = OutboundServiceRequester::new(request_tx);

        let (conn_man_tx, mut conn_man_rx) = mpsc::channel(1);
        let conn_manager = ConnectionManagerRequester::new(conn_man_tx);

        let peer_manager = PeerManager::new(CommsDatabase::new()).map(Arc::new).unwrap();
        let pk = CommsPublicKey::default();
        let example_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk).unwrap(),
            vec!["127.0.0.1:9999".parse::<NetAddress>().unwrap()].into(),
            PeerFlags::empty(),
        );
        peer_manager.add_peer(example_peer.clone()).unwrap();

        let node_identity = Arc::new(node_identity::random(None));
        let service = OutboundMessageService::new(
            Default::default(),
            rt.executor(),
            IdentityOutboundMiddleware::new(),
            request_rx,
            peer_manager,
            conn_manager,
            Arc::clone(&node_identity),
        );
        rt.spawn(service.start());
        let envelope = MessageEnvelope::construct(
            &node_identity,
            example_peer.public_key.clone(),
            NodeDestination::Unknown,
            vec![],
            MessageFlags::empty(),
        )
        .unwrap();
        rt.block_on(sender.forward_message(BroadcastStrategy::Flood, envelope))
            .unwrap();

        let msg = rt.block_on(conn_man_rx.next()).unwrap();
        match msg {
            ConnectionManagerRequest::DialPeer(boxed) => {
                let (node_id, _) = *boxed;
                assert_eq!(node_id, example_peer.node_id);
            },
        }

        drop(sender);
        rt.shutdown_on_idle();
    }
}
