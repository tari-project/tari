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

use std::{fmt, time::Duration};

use tokio::sync::mpsc;
use tower::Service;

use super::MessagingProtocol;
use crate::{
    bounded_executor::BoundedExecutor,
    message::InboundMessage,
    pipeline,
    protocol::{
        messaging::{protocol::MESSAGING_PROTOCOL_ID, MessagingEventSender},
        ProtocolExtension,
        ProtocolExtensionContext,
        ProtocolExtensionError,
    },
};

/// Buffer size for inbound messages from _all_ peers. If the message consumer is slow to get through this queue,
/// sending peers will start to experience backpressure (this is a good thing).
pub const INBOUND_MESSAGE_BUFFER_SIZE: usize = 10;
/// Buffer size notifications that a peer wants to speak /tari/messaging. This buffer is used for all peers, but a low
/// value is ok because this events happen once (or less) per connecting peer. For e.g. a value of 10 would allow 10
/// peers to concurrently request to speak /tari/messaging.
pub const MESSAGING_PROTOCOL_EVENTS_BUFFER_SIZE: usize = 30;

/// Installs the messaging protocol
pub struct MessagingProtocolExtension<TInPipe, TOutPipe, TOutReq> {
    event_tx: MessagingEventSender,
    pipeline: pipeline::Config<TInPipe, TOutPipe, TOutReq>,
    enable_message_received_event: bool,
    ban_duration: Duration,
}

impl<TInPipe, TOutPipe, TOutReq> MessagingProtocolExtension<TInPipe, TOutPipe, TOutReq> {
    pub fn new(event_tx: MessagingEventSender, pipeline: pipeline::Config<TInPipe, TOutPipe, TOutReq>) -> Self {
        Self {
            event_tx,
            pipeline,
            enable_message_received_event: false,
            ban_duration: Duration::from_secs(10 * 60),
        }
    }

    /// Enables the MessageReceived event which is disabled by default. This will enable sending the MessageReceived
    /// event per message received. This is typically used in tests. If unused it should be disabled to reduce memory
    /// usage (not reading the event from the channel).
    pub fn enable_message_received_event(mut self) -> Self {
        self.enable_message_received_event = true;
        self
    }

    /// Sets the ban duration for peers that violate protocol. Default is 10 minutes.
    pub fn with_ban_duration(mut self, ban_duration: Duration) -> Self {
        self.ban_duration = ban_duration;
        self
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
    fn install(mut self: Box<Self>, context: &mut ProtocolExtensionContext) -> Result<(), ProtocolExtensionError> {
        let (proto_tx, proto_rx) = mpsc::channel(MESSAGING_PROTOCOL_EVENTS_BUFFER_SIZE);
        context.add_protocol(&[MESSAGING_PROTOCOL_ID.clone()], &proto_tx);

        let (inbound_message_tx, inbound_message_rx) = mpsc::channel(INBOUND_MESSAGE_BUFFER_SIZE);

        let message_receiver = self.pipeline.outbound.out_receiver.take().unwrap();
        let messaging = MessagingProtocol::new(
            context.connectivity(),
            proto_rx,
            message_receiver,
            self.event_tx,
            inbound_message_tx,
            context.shutdown_signal(),
        )
        .set_message_received_event_enabled(self.enable_message_received_event)
        .with_ban_duration(self.ban_duration);

        context.register_complete_signal(messaging.complete_signal());

        // Spawn messaging protocol
        tokio::spawn(messaging.run());

        // Spawn inbound pipeline
        let bounded_executor = BoundedExecutor::new(self.pipeline.max_concurrent_inbound_tasks);
        let inbound = pipeline::Inbound::new(
            bounded_executor,
            inbound_message_rx,
            self.pipeline.inbound,
            context.shutdown_signal(),
        );
        tokio::spawn(inbound.run());

        let executor = BoundedExecutor::new(
            self.pipeline
                .max_concurrent_outbound_tasks
                .unwrap_or_else(BoundedExecutor::max_theoretical_tasks),
        );
        // Spawn outbound pipeline
        let outbound = pipeline::Outbound::new(executor, self.pipeline.outbound);
        tokio::spawn(outbound.run());

        Ok(())
    }
}
