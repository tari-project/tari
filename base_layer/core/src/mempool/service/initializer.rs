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
use tari_network::NetworkHandle;
use tari_p2p::{message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::{
    async_trait,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::mpsc;

use crate::{
    base_node::comms_interface::LocalNodeCommsInterface,
    mempool::{
        mempool::Mempool,
        service::{
            inbound_handlers::MempoolInboundHandlers,
            local_service::LocalMempoolService,
            service::{MempoolService, MempoolStreams},
            MempoolHandle,
        },
    },
    proto,
    topics::TRANSACTION_TOPIC,
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::bn::mempool_service::initializer";
const SUBSCRIPTION_LABEL: &str = "Mempool";

/// Initializer for the Mempool service and service future.
pub struct MempoolServiceInitializer {
    mempool: Mempool,
}

impl MempoolServiceInitializer {
    /// Create a new MempoolServiceInitializer from the inbound message subscriber.
    pub fn new(mempool: Mempool) -> Self {
        Self { mempool }
    }
}

#[async_trait]
impl ServiceInitializer for MempoolServiceInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        // Connect MempoolOutboundServiceHandle to MempoolService
        let (request_sender, request_receiver) = reply_channel::unbounded();
        let mempool_handle = MempoolHandle::new(request_sender);
        context.register_handle(mempool_handle);

        let (local_request_sender_service, local_request_stream) = reply_channel::unbounded();
        let local_mp_interface = LocalMempoolService::new(local_request_sender_service);
        let inbound_handlers = MempoolInboundHandlers::new(self.mempool.clone());

        context.register_handle(local_mp_interface);

        context.spawn_until_shutdown(move |handles| async move {
            let base_node = handles.expect_handle::<LocalNodeCommsInterface>();
            let mut network = handles.expect_handle::<NetworkHandle>();
            // Mempool does not publish transactions, they are gossiped by the network and published by wallets.
            let (_publisher, subscriber) = match network.subscribe_topic(TRANSACTION_TOPIC).await {
                Ok(x) => x,
                Err(err) => {
                    error!(target: LOG_TARGET, "⚠️ Failed to subscribe to transactions: {}. THE MEMPOOL SERVICE WILL NOT START.", err);
                    return ;
                },
            };

            let streams = MempoolStreams {
                transaction_subscription: subscriber,
                local_request_stream,
                block_event_stream: base_node.get_block_event_stream(),
                request_receiver,
            };
            debug!(target: LOG_TARGET, "Mempool service started");
            if let Err(err) = MempoolService::new(inbound_handlers).start(streams).await {
                error!(target: LOG_TARGET, "Mempool service error: {}", err);
            }
        });

        Ok(())
    }
}
