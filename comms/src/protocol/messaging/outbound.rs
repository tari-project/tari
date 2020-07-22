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

use super::{error::MessagingProtocolError, MessagingEvent, MessagingProtocol, SendFailReason, MESSAGING_PROTOCOL};
use crate::{
    connection_manager::{ConnectionManagerError, ConnectionManagerRequester, NegotiatedSubstream, PeerConnection},
    message::{MessageTag, OutboundMessage},
    multiplexing::Substream,
    peer_manager::NodeId,
};
use bytes::Bytes;
use futures::{
    channel::mpsc,
    future::Either,
    ready,
    stream::FusedStream,
    task::{Context, Poll},
    Sink,
    SinkExt,
    Stream,
    StreamExt,
};
use log::*;
use pin_project::pin_project;
use std::{io, pin::Pin, time::Duration};
use tokio::stream as tokio_stream;

const LOG_TARGET: &str = "comms::protocol::messaging::outbound";

pub struct OutboundMessaging {
    conn_man_requester: ConnectionManagerRequester,
    request_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    messaging_events_tx: mpsc::Sender<MessagingEvent>,
    peer_node_id: NodeId,
    inactivity_timeout: Option<Duration>,
}

impl OutboundMessaging {
    pub fn new(
        conn_man_requester: ConnectionManagerRequester,
        messaging_events_tx: mpsc::Sender<MessagingEvent>,
        request_rx: mpsc::UnboundedReceiver<OutboundMessage>,
        peer_node_id: NodeId,
        inactivity_timeout: Option<Duration>,
    ) -> Self
    {
        Self {
            conn_man_requester,
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
    }

    async fn run_inner(mut self) -> Result<(), MessagingProtocolError> {
        let conn = self.try_dial_peer().await?;
        let substream = self.try_open_substream(conn).await?;
        debug_assert_eq!(substream.protocol, MESSAGING_PROTOCOL);
        self.start_forwarding_messages(substream.stream).await?;

        Ok(())
    }

    async fn try_dial_peer(&mut self) -> Result<PeerConnection, MessagingProtocolError> {
        loop {
            match self.conn_man_requester.dial_peer(self.peer_node_id.clone()).await {
                Ok(conn) => break Ok(conn),
                Err(ConnectionManagerError::DialCancelled) => {
                    debug!(
                        target: LOG_TARGET,
                        "Dial was cancelled for peer '{}'. This is probably because of connection tie-breaking. \
                         Retrying...",
                        self.peer_node_id.short_str(),
                    );
                    continue;
                },
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "MessagingProtocol failed to dial peer '{}' because '{:?}'",
                        self.peer_node_id.short_str(),
                        err
                    );
                    self.flush_all_messages_to_failed_event(SendFailReason::PeerDialFailed)
                        .await;
                    break Err(MessagingProtocolError::PeerDialFailed);
                },
            }
        }
    }

    async fn try_open_substream(
        &mut self,
        mut conn: PeerConnection,
    ) -> Result<NegotiatedSubstream<Substream>, MessagingProtocolError>
    {
        match conn.open_substream(&MESSAGING_PROTOCOL).await {
            Ok(substream) => Ok(substream),
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "MessagingProtocol failed to open a substream to peer '{}' because '{:?}'",
                    self.peer_node_id.short_str(),
                    err
                );
                self.flush_all_messages_to_failed_event(SendFailReason::SubstreamOpenFailed)
                    .await;
                Err(err.into())
            },
        }
    }

    async fn start_forwarding_messages(self, substream: Substream) -> Result<(), MessagingProtocolError> {
        let framed = MessagingProtocol::framed(substream);

        let Self {
            request_rx,
            inactivity_timeout,
            peer_node_id,
            messaging_events_tx,
            ..
        } = self;

        let stream = MessageForwarderStream::new(peer_node_id.clone(), request_rx, framed);

        let stream = match inactivity_timeout {
            Some(timeout) => {
                let s = tokio_stream::StreamExt::timeout(stream, timeout).map(|r| match r {
                    Ok(s) => s,
                    Err(_) => Err(MessageSendFailure {
                        item: None,
                        error: MessagingProtocolError::Inactivity,
                    }),
                });
                Either::Left(s)
            },
            None => Either::Right(stream),
        };

        stream
            .fold(Result::<(), MessagingProtocolError>::Ok(()), move |_, result| {
                let mut messaging_events_tx = messaging_events_tx.clone();
                let peer_node_id = peer_node_id.clone();
                async move {
                    match result {
                        Ok(tag) => {
                            debug!(
                                target: LOG_TARGET,
                                "(peer = {}, tag = {}) Message sent",
                                peer_node_id.short_str(),
                                tag,
                            );
                            let _ = messaging_events_tx.send(MessagingEvent::MessageSent(tag)).await;
                            Ok(())
                        },
                        Err(failure) => {
                            match failure.item {
                                Some(out_msg) => {
                                    debug!(
                                        target: LOG_TARGET,
                                        "(peer = {}, tag = {}, {} bytes()) Message failed to send: {}",
                                        peer_node_id.short_str(),
                                        out_msg.tag,
                                        out_msg.body.len(),
                                        failure.error,
                                    );
                                    let _ = messaging_events_tx
                                        .send(MessagingEvent::SendMessageFailed(
                                            out_msg,
                                            SendFailReason::SubstreamSendFailed,
                                        ))
                                        .await;
                                },
                                None => {
                                    debug!(
                                        target: LOG_TARGET,
                                        "(peer = {}): {}",
                                        peer_node_id.short_str(),
                                        failure.error,
                                    );
                                },
                            }

                            Err(failure.error)
                        },
                    }
                }
            })
            .await?;

        Ok(())
    }

    async fn flush_all_messages_to_failed_event(&mut self, reason: SendFailReason) {
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

#[derive(Debug)]
struct MessageSendFailure {
    pub item: Option<OutboundMessage>,
    pub error: MessagingProtocolError,
}

impl From<io::Error> for MessageSendFailure {
    fn from(err: io::Error) -> Self {
        Self {
            item: None,
            error: err.into(),
        }
    }
}

#[pin_project(project = StateProj)]
#[derive(Debug)]
enum State {
    Read,
    Write(Option<OutboundMessage>),
    FlushPending(bool),
    Flush(bool),
    Errored(bool),
    Complete(bool),
}

/// This stream forwards messages from the mpsc receiver to the given `Sink`.
#[pin_project(project = MessageForwarderStreamProj)]
#[derive(Debug)]
#[must_use = "streams do nothing unless you poll them"]
struct MessageForwarderStream<Si> {
    peer_node_id: NodeId,
    #[pin]
    sink: Option<Si>,
    #[pin]
    stream: mpsc::UnboundedReceiver<OutboundMessage>,
    #[pin]
    state: State,
    pending_queue: Vec<OutboundMessage>,
}

impl<Si> MessageForwarderStream<Si>
where Si: Sink<Bytes, Error = io::Error> + Unpin
{
    pub fn new(peer_node_id: NodeId, stream: mpsc::UnboundedReceiver<OutboundMessage>, sink: Si) -> Self {
        Self {
            peer_node_id,
            stream,
            sink: Some(sink),
            state: State::Read,
            // Capacity is chosen to match yamux's internal channel buffer
            pending_queue: Vec::with_capacity(32),
        }
    }
}

impl<Si> Stream for MessageForwarderStream<Si>
where Si: Sink<Bytes, Error = io::Error> + Unpin
{
    type Item = Result<MessageTag, MessageSendFailure>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let MessageForwarderStreamProj {
            mut sink,
            mut stream,
            peer_node_id,
            mut state,
            pending_queue,
        } = self.project();

        loop {
            match state.as_mut().project() {
                StateProj::Read => match stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(item)) => {
                        debug!(
                            target: LOG_TARGET,
                            "Buffering outbound message (tag = {}, {} bytes) for peer `{}`",
                            item.tag,
                            item.body.len(),
                            peer_node_id.short_str()
                        );
                        *state = State::Write(Some(item));
                    },
                    Poll::Ready(None) => {
                        *state = State::Complete(false);
                    },
                    Poll::Pending => *state = State::Flush(true),
                },
                StateProj::Flush(is_pending) => {
                    let si = sink
                        .as_mut()
                        .as_pin_mut()
                        .expect("polled `MessageForwarderStream` after completion");
                    if let Err(err) = ready!(si.poll_flush(cx)) {
                        *state = State::Errored(false);
                        return Poll::Ready(Some(Err(err.into())));
                    }
                    *state = State::FlushPending(*is_pending);
                },
                StateProj::FlushPending(is_pending) => match pending_queue.pop() {
                    Some(mut item) => {
                        item.reply_success();
                        return Poll::Ready(Some(Ok(item.tag)));
                    },
                    None => {
                        let is_pending = *is_pending;
                        *state = State::Read;
                        if is_pending {
                            return Poll::Pending;
                        }
                    },
                },
                StateProj::Write(item) => {
                    let mut si = sink
                        .as_mut()
                        .as_pin_mut()
                        .expect("polled `MessageForwarderStream` after completion");
                    match ready!(si.as_mut().poll_ready(cx)) {
                        Ok(_) => {
                            let item = item.take().expect("State::Write without an item to write");
                            match si.as_mut().start_send(item.body.clone()) {
                                Ok(_) => {
                                    pending_queue.push(item);
                                    if pending_queue.len() >= pending_queue.capacity() {
                                        *state = State::Flush(false);
                                    } else {
                                        *state = State::Read
                                    }
                                },
                                Err(err) => {
                                    *state = State::Errored(false);
                                    let err = MessageSendFailure {
                                        item: Some(item),
                                        error: err.into(),
                                    };
                                    return Poll::Ready(Some(Err(err)));
                                },
                            }
                        },
                        Err(err) => {
                            let item = item.take().expect("State::Write without an item to write");
                            *state = State::Errored(false);
                            let err = MessageSendFailure {
                                item: Some(item),
                                error: err.into(),
                            };
                            return Poll::Ready(Some(Err(err)));
                        },
                    }
                },
                StateProj::Errored(is_complete) => {
                    // Close stream and flush
                    if !stream.is_terminated() {
                        stream.as_mut().close();
                    }
                    if let Some(item) = pending_queue.pop() {
                        let err = MessageSendFailure {
                            item: Some(item),
                            error: MessagingProtocolError::MessageSendFailed,
                        };
                        return Poll::Ready(Some(Err(err)));
                    }

                    if *is_complete {
                        sink.set(None);
                        return Poll::Ready(None);
                    }

                    return match ready!(stream.as_mut().poll_next(cx)) {
                        Some(item) => {
                            let err = MessageSendFailure {
                                item: Some(item),
                                error: MessagingProtocolError::MessageSendFailed,
                            };
                            Poll::Ready(Some(Err(err)))
                        },
                        None => {
                            sink.set(None);
                            Poll::Ready(None)
                        },
                    };
                },
                StateProj::Complete(has_closed) => {
                    let si = sink
                        .as_mut()
                        .as_pin_mut()
                        .expect("polled `MessageForwarderStream` after completion");
                    if !*has_closed {
                        if let Err(err) = ready!(si.poll_close(cx)) {
                            *state = State::Errored(true);
                            return Poll::Ready(Some(Err(err.into())));
                        }

                        *state = State::Complete(true);
                    }

                    if let Some(mut item) = pending_queue.pop() {
                        item.reply_success();
                        return Poll::Ready(Some(Ok(item.tag)));
                    }
                    sink.set(None);
                    return Poll::Ready(None);
                },
            }
        }
    }
}
