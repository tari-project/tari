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
use std::{
    collections::VecDeque,
    convert::TryInto,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures::{self, pin_mut, Stream, StreamExt};
use log::*;
use tari_common_types::types::PublicKey;
use tari_comms::types::CommsPublicKey;
use tari_dan_core::{
    models::{HotStuffMessage, HotStuffMessageType, TariDanPayload, ViewId},
    services::infrastructure_services::InboundConnectionService,
    DigitalAssetError,
};
use tari_p2p::comms_connector::PeerMessage;
use tari_shutdown::ShutdownSignal;
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    oneshot,
};

use crate::p2p::proto;

const LOG_TARGET: &str = "tari::validator_node::p2p::services::inbound_connection_service";

#[derive(Debug)]
enum WaitForMessageType {
    Message,
    QuorumCertificate,
}

#[derive(Debug)]
enum TariCommsInboundRequest {
    WaitForMessage {
        wait_for_type: WaitForMessageType,
        message_type: HotStuffMessageType,
        view_number: ViewId,
        reply_channel: oneshot::Sender<(CommsPublicKey, HotStuffMessage<TariDanPayload>)>,
    },
}

pub struct TariCommsInboundConnectionService {
    receiver: TariCommsInboundReceiverHandle,
    // sender: Sender<(CommsPublicKey, HotStuffMessage<TariDanPayload>)>,
    request_channel: Receiver<TariCommsInboundRequest>,
    asset_public_key: PublicKey,
    buffered_messages: VecDeque<(CommsPublicKey, HotStuffMessage<TariDanPayload>, Instant)>,
    expiry_time: Duration,
    #[allow(clippy::type_complexity)]
    waiters: VecDeque<(
        HotStuffMessageType,
        ViewId,
        WaitForMessageType,
        oneshot::Sender<(CommsPublicKey, HotStuffMessage<TariDanPayload>)>,
    )>,
    loopback_sender: Sender<(CommsPublicKey, HotStuffMessage<TariDanPayload>)>,
    loopback_receiver: Receiver<(CommsPublicKey, HotStuffMessage<TariDanPayload>)>,
}

impl TariCommsInboundConnectionService {
    pub fn new(asset_public_key: PublicKey) -> Self {
        let (sender, receiver) = channel(1000);
        let (loopback_sender, loopback_receiver) = channel(1);
        Self {
            receiver: TariCommsInboundReceiverHandle::new(sender),
            request_channel: receiver,
            asset_public_key,
            buffered_messages: VecDeque::with_capacity(1000),
            expiry_time: Duration::new(90, 0),
            waiters: VecDeque::new(),
            loopback_receiver,
            loopback_sender,
        }
    }

    pub fn clone_sender(&self) -> Sender<(CommsPublicKey, HotStuffMessage<TariDanPayload>)> {
        self.loopback_sender.clone()
    }

    pub fn get_receiver(&self) -> TariCommsInboundReceiverHandle {
        self.receiver.clone()
    }

    pub async fn run(
        &mut self,
        _shutdown_signal: ShutdownSignal,
        inbound_stream: impl Stream<Item = Arc<PeerMessage>>,
    ) -> Result<(), DigitalAssetError> {
        let inbound_stream = inbound_stream.fuse();
        pin_mut!(inbound_stream);
        loop {
            tokio::select! {
                request = self.request_channel.recv() => {
                    if let Some(request) = request {
                        self.handle_request(request).await?;
                    } else {
                        debug!(target: LOG_TARGET, "All requesters have dropped, stopping.");
                        return Ok(())
                    }
                },
                message = self.loopback_receiver.recv() => {
                    if let Some((from, message)) = message {
                        self.process_message(from, message).await?;
                    } else {
                        debug!(target: LOG_TARGET, "Loopback senders have all dropped, stopping");
                        return Ok(())
                    }
                },
                message = inbound_stream.select_next_some() => {
                    self.forward_message(message).await?;
                },
                // complete => {
                //     dbg!("Tari inbound connector shutting down");
                //     return Ok(());
                // }
                // _ = shutdown_signal => {
                //     dbg!("Shutdown received");
                //     return Ok(())
                // }
            }
        }
    }

    async fn handle_request(&mut self, request: TariCommsInboundRequest) -> Result<(), DigitalAssetError> {
        debug!(target: LOG_TARGET, "Received request: {:?}", request);
        match request {
            TariCommsInboundRequest::WaitForMessage {
                wait_for_type,
                message_type,
                view_number,
                reply_channel,
            } => {
                // Check for already received messages
                let mut indexes_to_remove = vec![];
                let mut result_message = None;
                for (index, (from_pk, message, msg_time)) in self.buffered_messages.iter().enumerate() {
                    if msg_time.elapsed() > self.expiry_time {
                        warn!(
                            target: LOG_TARGET,
                            "Message has expired: ({:.2?}) {:?}",
                            msg_time.elapsed(),
                            message
                        );
                        indexes_to_remove.push(index);
                    } else {
                        match wait_for_type {
                            WaitForMessageType::Message => {
                                if message.message_type() == message_type && message.view_number() == view_number {
                                    result_message = Some((from_pk.clone(), message.clone()));
                                    indexes_to_remove.push(index);
                                    break;
                                }
                            },
                            WaitForMessageType::QuorumCertificate => {
                                if let Some(qc) = message.justify() {
                                    if qc.message_type() == message_type && qc.view_number() == view_number {
                                        result_message = Some((from_pk.clone(), message.clone()));
                                        indexes_to_remove.push(index);
                                        break;
                                    }
                                }
                            },
                        }
                    }
                }
                for i in indexes_to_remove.iter().rev() {
                    self.buffered_messages.remove(*i);
                }
                match result_message {
                    Some(m) => {
                        reply_channel.send(m).expect("Could not send");
                    },
                    None => {
                        self.waiters
                            .push_back((message_type, view_number, wait_for_type, reply_channel));
                    },
                }
            },
        }
        Ok(())
    }

    async fn forward_message(&mut self, message: Arc<PeerMessage>) -> Result<(), DigitalAssetError> {
        // let from = message.authenticated_origin.as_ref().unwrap().clone();
        let from = message.source_peer.public_key.clone();
        let proto_message: proto::consensus::HotStuffMessage = message.decode_message().unwrap();
        let hot_stuff_message: HotStuffMessage<TariDanPayload> = proto_message
            .try_into()
            .map_err(DigitalAssetError::InvalidPeerMessage)?;
        if hot_stuff_message.asset_public_key() == &self.asset_public_key {
            dbg!(&hot_stuff_message);
            // self.sender.send((from, hot_stuff_message)).await.unwrap();
            self.process_message(from, hot_stuff_message).await?;
        } else {
            dbg!("filtered");
        }
        Ok(())
    }

    async fn process_message(
        &mut self,
        from: CommsPublicKey,
        message: HotStuffMessage<TariDanPayload>,
    ) -> Result<(), DigitalAssetError> {
        debug!(target: "messages::inbound::validator_node", "Inbound message received:{} {:?}", from, message);
        debug!(target: LOG_TARGET, "Inbound message received:{} {:?}", from, message);

        // Loop until we have sent to a waiting call, or buffer the message
        // dbg!(&self.waiters);
        loop {
            // Check for waiters
            let mut waiter_index = None;
            for (index, waiter) in self.waiters.iter().enumerate() {
                let (message_type, view_number, wait_for_type, _) = waiter;
                match wait_for_type {
                    WaitForMessageType::Message => {
                        if message.message_type() == *message_type && message.view_number() == *view_number {
                            waiter_index = Some(index);
                            break;
                        }
                    },
                    WaitForMessageType::QuorumCertificate => {
                        if let Some(qc) = message.justify() {
                            if qc.message_type() == *message_type && qc.view_number() == *view_number {
                                waiter_index = Some(index);
                                break;
                            }
                        }
                    },
                }
            }

            if let Some(index) = waiter_index {
                debug!(
                    target: LOG_TARGET,
                    "Found waiter for this message, waking task... {:?}",
                    message.message_type()
                );
                if let Some((_, _, _, reply)) = self.waiters.swap_remove_back(index) {
                    // The receiver on the other end of this channel may have dropped naturally
                    // as it moves out of scope and is not longer interested in receiving the message
                    if reply.send((from.clone(), message.clone())).is_ok() {
                        return Ok(());
                    }
                }
            } else {
                break;
            }
        }

        debug!(
            target: LOG_TARGET,
            "No waiters for this message, buffering message: {:?}",
            message.message_type()
        );
        // Otherwise, buffer it
        self.buffered_messages.push_back((from, message, Instant::now()));
        Ok(())
    }
}

#[derive(Clone)]
pub struct TariCommsInboundReceiverHandle {
    sender: Sender<TariCommsInboundRequest>,
}

impl TariCommsInboundReceiverHandle {
    fn new(sender: Sender<TariCommsInboundRequest>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl InboundConnectionService for TariCommsInboundReceiverHandle {
    type Addr = CommsPublicKey;
    type Payload = TariDanPayload;

    async fn wait_for_message(
        &self,
        message_type: HotStuffMessageType,
        view_number: ViewId,
    ) -> Result<(CommsPublicKey, HotStuffMessage<TariDanPayload>), DigitalAssetError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(TariCommsInboundRequest::WaitForMessage {
                wait_for_type: WaitForMessageType::Message,
                message_type,
                view_number,
                reply_channel: tx,
            })
            .await
            .map_err(|e| DigitalAssetError::FatalError(format!("Error sending request to channel:{}", e)))?;
        rx.await
            .map_err(|e| DigitalAssetError::FatalError(format!("Error receiving from wait_for oneshot channel:{}", e)))
    }

    async fn wait_for_qc(
        &self,
        message_type: HotStuffMessageType,
        view_number: ViewId,
    ) -> Result<(CommsPublicKey, HotStuffMessage<TariDanPayload>), DigitalAssetError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(TariCommsInboundRequest::WaitForMessage {
                wait_for_type: WaitForMessageType::QuorumCertificate,
                message_type,
                view_number,
                reply_channel: tx,
            })
            .await
            .map_err(|e| DigitalAssetError::FatalError(format!("Error sending request to channel:{}", e)))?;
        rx.await
            .map_err(|e| DigitalAssetError::FatalError(format!("Error receiving from qc oneshot channel:{}", e)))
    }
}
