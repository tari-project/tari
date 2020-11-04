// Copyright 2020, The Tari Project
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

use super::{error::MessagingProtocolError, MessagingEvent, MessagingProtocol, SendFailReason};
use crate::{
    connection_manager::{NegotiatedSubstream, PeerConnection},
    connectivity::{ConnectivityError, ConnectivityRequester},
    message::OutboundMessage,
    multiplexing::Substream,
    peer_manager::NodeId,
    protocol::messaging::protocol::MESSAGING_PROTOCOL,
};
use futures::{channel::mpsc, future::Either, SinkExt, StreamExt};
use log::*;
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::stream as tokio_stream;

const LOG_TARGET: &str = "comms::protocol::messaging::outbound";
/// The number of times to retry sending a failed message before publishing a SendMessageFailed event.
/// This should only need to be 1 to handle the case where the pending dial is cancelled due to to tie breaking
/// and because the connection manager already retries dialing a number of times for each requested dial.
const MAX_SEND_RETRIES: usize = 1;

pub struct OutboundMessaging {
    connectivity: ConnectivityRequester,
    request_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    messaging_events_tx: mpsc::Sender<MessagingEvent>,
    peer_node_id: NodeId,
    inactivity_timeout: Option<Duration>,
}

impl OutboundMessaging {
    pub fn new(
        connectivity: ConnectivityRequester,
        messaging_events_tx: mpsc::Sender<MessagingEvent>,
        request_rx: mpsc::UnboundedReceiver<OutboundMessage>,
        peer_node_id: NodeId,
        inactivity_timeout: Option<Duration>,
    ) -> Self
    {
        Self {
            connectivity,
            request_rx,
            messaging_events_tx,
            peer_node_id,
            inactivity_timeout,
        }
    }

    pub async fn run(self) {
        debug!(
            target: LOG_TARGET,
            "Attempting to dial peer '{}' if required",
            self.peer_node_id.short_str()
        );
        let peer_node_id = self.peer_node_id.clone();
        let mut messaging_events_tx = self.messaging_events_tx.clone();
        match self.run_inner().await {
            Ok(_) => {
                debug!(
                    target: LOG_TARGET,
                    "Outbound messaging for peer '{}' has stopped because the stream was closed",
                    peer_node_id.short_str()
                );
            },
            Err(MessagingProtocolError::Inactivity) => {
                debug!(
                    target: LOG_TARGET,
                    "Outbound messaging for peer '{}' has stopped because it was inactive",
                    peer_node_id.short_str()
                );
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Outbound messaging substream failed: {}", err);
            },
        }

        let _ = messaging_events_tx
            .send(MessagingEvent::OutboundProtocolExited(peer_node_id))
            .await;
    }

    async fn run_inner(mut self) -> Result<(), MessagingProtocolError> {
        let mut attempts = 0;
        let substream = loop {
            match self.try_establish().await {
                Ok(substream) => break substream,
                Err(err) => {
                    assert!(
                        attempts <= MAX_SEND_RETRIES,
                        "Attempt count was greater than the maximum"
                    );
                    if attempts == MAX_SEND_RETRIES {
                        debug!(
                            target: LOG_TARGET,
                            "Error establishing messaging protocol: {}. Aborting because maximum retries reached.", err
                        );
                        self.fail_all_pending_messages(SendFailReason::PeerDialFailed).await;
                        return Err(err);
                    }
                    debug!(
                        target: LOG_TARGET,
                        "Error establishing messaging protocol: {}. Retrying...", err
                    );
                    attempts += 1;
                },
            }
        };
        self.start_forwarding_messages(substream).await?;

        Ok(())
    }

    async fn try_dial_peer(&mut self) -> Result<PeerConnection, MessagingProtocolError> {
        loop {
            match self.connectivity.dial_peer(self.peer_node_id.clone()).await {
                Ok(conn) => break Ok(conn),
                Err(ConnectivityError::DialCancelled) => {
                    debug!(
                        target: LOG_TARGET,
                        "Dial was cancelled for peer '{}'. This is probably because of connection tie-breaking. \
                         Retrying...",
                        self.peer_node_id.short_str(),
                    );
                    continue;
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "MessagingProtocol failed to dial peer '{}' because '{:?}'",
                        self.peer_node_id.short_str(),
                        err
                    );

                    break Err(MessagingProtocolError::PeerDialFailed);
                },
            }
        }
    }

    async fn try_establish(&mut self) -> Result<NegotiatedSubstream<Substream>, MessagingProtocolError> {
        debug!(
            target: LOG_TARGET,
            "Attempting to establish messaging protocol connection to peer `{}`",
            self.peer_node_id.short_str()
        );
        let start = Instant::now();
        let conn = self.try_dial_peer().await?;
        debug!(
            target: LOG_TARGET,
            "Connection succeeded for peer `{}` in {:.0?}",
            self.peer_node_id.short_str(),
            start.elapsed()
        );
        let substream = self.try_open_substream(conn).await?;
        debug!(
            target: LOG_TARGET,
            "Substream established for peer `{}`",
            self.peer_node_id.short_str(),
        );
        Ok(substream)
    }

    async fn try_open_substream(
        &mut self,
        mut conn: PeerConnection,
    ) -> Result<NegotiatedSubstream<Substream>, MessagingProtocolError>
    {
        match conn.open_substream(&MESSAGING_PROTOCOL).await {
            Ok(substream) => Ok(substream),
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "MessagingProtocol failed to open a substream to peer '{}' because '{}'",
                    self.peer_node_id.short_str(),
                    err
                );
                Err(err.into())
            },
        }
    }

    async fn start_forwarding_messages(
        self,
        substream: NegotiatedSubstream<Substream>,
    ) -> Result<(), MessagingProtocolError>
    {
        debug!(
            target: LOG_TARGET,
            "Starting direct message forwarding for peer `{}`",
            self.peer_node_id.short_str()
        );
        let substream = substream.stream;

        let (sink, _) = MessagingProtocol::framed(substream).split();

        let Self {
            request_rx,
            inactivity_timeout,
            ..
        } = self;

        let stream = match inactivity_timeout {
            Some(timeout) => {
                let s = tokio_stream::StreamExt::timeout(request_rx, timeout).map(|r| match r {
                    Ok(s) => Ok(s),
                    Err(_) => Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        MessagingProtocolError::Inactivity,
                    )),
                });
                Either::Left(s)
            },
            None => Either::Right(request_rx.map(Ok)),
        };

        stream
            .map(|msg| {
                msg.map(|mut out_msg| {
                    trace!(target: LOG_TARGET, "Message buffered for sending {}", out_msg);
                    out_msg.reply_success();
                    out_msg.body
                })
            })
            .forward(sink)
            .await?;

        debug!(
            target: LOG_TARGET,
            "Direct message forwarding successfully completed for peer `{}`.",
            self.peer_node_id.short_str()
        );
        Ok(())
    }

    async fn fail_all_pending_messages(&mut self, reason: SendFailReason) {
        // Close the request channel so that we can read all the remaining messages and flush them
        // to a failed event
        self.request_rx.close();
        while let Some(mut out_msg) = self.request_rx.next().await {
            out_msg.reply_fail(reason);
            let _ = self
                .messaging_events_tx
                .send(MessagingEvent::SendMessageFailed(out_msg, reason))
                .await;
        }
    }
}
