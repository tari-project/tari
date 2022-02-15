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

use std::{convert::TryFrom, sync::Arc};

use futures::{Stream, StreamExt};
use log::*;
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::{PeerMessage, SubscriptionFactory},
    domain_message::DomainMessage,
    tari_message::TariMessageType,
};
use tari_service_framework::{
    async_trait,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::mpsc;

use crate::{
    base_node::{comms_interface::LocalNodeCommsInterface, StateMachineHandle},
    mempool::{
        mempool::Mempool,
        service::{
            inbound_handlers::MempoolInboundHandlers,
            local_service::LocalMempoolService,
            outbound_interface::OutboundMempoolServiceInterface,
            service::{MempoolService, MempoolStreams},
            MempoolHandle,
        },
    },
    proto,
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::bn::mempool_service::initializer";
const SUBSCRIPTION_LABEL: &str = "Mempool";

/// Initializer for the Mempool service and service future.
pub struct MempoolServiceInitializer {
    mempool: Mempool,
    inbound_message_subscription_factory: Arc<SubscriptionFactory>,
}

impl MempoolServiceInitializer {
    /// Create a new MempoolServiceInitializer from the inbound message subscriber.
    pub fn new(mempool: Mempool, inbound_message_subscription_factory: Arc<SubscriptionFactory>) -> Self {
        Self {
            mempool,
            inbound_message_subscription_factory,
        }
    }

    /// Create a stream of 'New Transaction` messages
    fn inbound_transaction_stream(&self) -> impl Stream<Item = DomainMessage<Transaction>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::NewTransaction, SUBSCRIPTION_LABEL)
            .filter_map(extract_transaction)
    }
}

async fn extract_transaction(msg: Arc<PeerMessage>) -> Option<DomainMessage<Transaction>> {
    match msg.decode_message::<proto::types::Transaction>() {
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Could not decode inbound transaction message. {}",
                e.to_string()
            );
            None
        },
        Ok(tx) => {
            let tx = match Transaction::try_from(tx) {
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Inbound transaction message from {} was ill-formed. {}", msg.source_peer.public_key, e
                    );
                    return None;
                },
                Ok(b) => b,
            };
            Some(DomainMessage {
                source_peer: msg.source_peer.clone(),
                dht_header: msg.dht_header.clone(),
                authenticated_origin: msg.authenticated_origin.clone(),
                inner: tx,
            })
        },
    }
}

#[async_trait]
impl ServiceInitializer for MempoolServiceInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        // Create streams for receiving Mempool service requests and response messages from comms
        let inbound_transaction_stream = self.inbound_transaction_stream();

        // Connect MempoolOutboundServiceHandle to MempoolService
        let (request_sender, request_receiver) = reply_channel::unbounded();
        let mempool_handle = MempoolHandle::new(request_sender);
        context.register_handle(mempool_handle);

        let (outbound_tx_sender, outbound_tx_stream) = mpsc::unbounded_channel();
        let (local_request_sender_service, local_request_stream) = reply_channel::unbounded();
        let outbound_mp_interface = OutboundMempoolServiceInterface::new(outbound_tx_sender);
        let local_mp_interface = LocalMempoolService::new(local_request_sender_service);
        let inbound_handlers = MempoolInboundHandlers::new(self.mempool.clone(), outbound_mp_interface.clone());

        // Register handle to OutboundMempoolServiceInterface before waiting for handles to be ready
        context.register_handle(outbound_mp_interface);
        context.register_handle(local_mp_interface);

        context.spawn_until_shutdown(move |handles| {
            let outbound_message_service = handles.expect_handle::<Dht>().outbound_requester();
            let state_machine = handles.expect_handle::<StateMachineHandle>();
            let base_node = handles.expect_handle::<LocalNodeCommsInterface>();

            let streams = MempoolStreams {
                outbound_tx_stream,
                inbound_transaction_stream,
                local_request_stream,
                block_event_stream: base_node.get_block_event_stream(),
                request_receiver,
            };
            debug!(target: LOG_TARGET, "Mempool service started");
            MempoolService::new(outbound_message_service, inbound_handlers, state_machine).start(streams)
        });

        Ok(())
    }
}
