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

use std::fmt;

use tokio::sync::mpsc;
use tower::Service;

use super::MessagingProtocol;
use crate::{
    bounded_executor::{BoundedExecutor, OptionallyBoundedExecutor},
    message::InboundMessage,
    pipeline,
    protocol::{
        messaging::{protocol::MESSAGING_PROTOCOL, MessagingEventSender},
        ProtocolExtension,
        ProtocolExtensionContext,
        ProtocolExtensionError,
    },
    runtime::task,
};

/// Buffer size for inbound messages from _all_ peers. This should be large enough to buffer quite a few incoming
/// messages before creating backpressure on peers speaking the messaging protocol.
pub const INBOUND_MESSAGE_BUFFER_SIZE: usize = 100;
/// Buffer size notifications that a peer wants to speak /tari/messaging. This buffer is used for all peers, but a low
/// value is ok because this events happen once (or less) per connecting peer. For e.g. a value of 10 would allow 10
/// peers to concurrently request to speak /tari/messaging.
pub const MESSAGING_PROTOCOL_EVENTS_BUFFER_SIZE: usize = 30;

/// Buffer size for requests to the messaging protocol. All outbound messages will be sent along this channel. Some
/// buffering may be required if the node needs to send many messages out at the same time.
pub const MESSAGING_REQUEST_BUFFER_SIZE: usize = 50;

pub struct MessagingProtocolExtension<TInPipe, TOutPipe, TOutReq> {
    event_tx: MessagingEventSender,
    pipeline: pipeline::Config<TInPipe, TOutPipe, TOutReq>,
}

impl<TInPipe, TOutPipe, TOutReq> MessagingProtocolExtension<TInPipe, TOutPipe, TOutReq> {
    pub fn new(event_tx: MessagingEventSender, pipeline: pipeline::Config<TInPipe, TOutPipe, TOutReq>) -> Self {
        Self { event_tx, pipeline }
    }
}

impl<TInPipe, TOutPipe, TOutReq> ProtocolExtension for MessagingProtocolExtension<TInPipe, TOutPipe, TOutReq>
where
    TOutPipe: Service<TOutReq, Response = ()> + Clone + Send + 'static,
    TOutPipe::Error: fmt::Display + Send,
    TOutPipe::Future: Send + 'static,
    TInPipe: Service<InboundMessage> + Clone + Send + 'static,
    TInPipe::Error: fmt::Display + Send,
    TInPipe::Future: Send + 'static,
    TOutReq: Send + 'static,
{
    fn install(self: Box<Self>, context: &mut ProtocolExtensionContext) -> Result<(), ProtocolExtensionError> {
        let (proto_tx, proto_rx) = mpsc::channel(MESSAGING_PROTOCOL_EVENTS_BUFFER_SIZE);
        context.add_protocol(&[MESSAGING_PROTOCOL.clone()], proto_tx);

        let (messaging_request_tx, messaging_request_rx) = mpsc::channel(MESSAGING_REQUEST_BUFFER_SIZE);
        let (inbound_message_tx, inbound_message_rx) = mpsc::channel(INBOUND_MESSAGE_BUFFER_SIZE);

        let messaging = MessagingProtocol::new(
            context.connectivity(),
            proto_rx,
            messaging_request_rx,
            self.event_tx,
            inbound_message_tx,
            context.shutdown_signal(),
        );

        context.register_complete_signal(messaging.complete_signal());

        // Spawn messaging protocol
        task::spawn(messaging.run());

        // Spawn inbound pipeline
        let bounded_executor = BoundedExecutor::from_current(self.pipeline.max_concurrent_inbound_tasks);
        let inbound = pipeline::Inbound::new(
            bounded_executor,
            inbound_message_rx,
            self.pipeline.inbound,
            context.shutdown_signal(),
        );
        task::spawn(inbound.run());

        let executor = OptionallyBoundedExecutor::from_current(self.pipeline.max_concurrent_outbound_tasks);
        // Spawn outbound pipeline
        let outbound = pipeline::Outbound::new(executor, self.pipeline.outbound, messaging_request_tx);
        task::spawn(outbound.run());

        Ok(())
    }
}
