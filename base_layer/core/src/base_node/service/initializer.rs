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

use std::{sync::Arc, time::Duration};

use futures::future;
use log::*;
use tari_network::NetworkHandle;
use tari_p2p::{tari_message::TariMessageType, Dispatcher};
use tari_service_framework::{
    async_trait,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::{broadcast, mpsc};

use crate::{
    base_node::{
        comms_interface::{InboundNodeCommsHandlers, LocalNodeCommsInterface, OutboundNodeCommsInterface},
        service::service::{BaseNodeService, BaseNodeStreams},
        BaseNodeStateMachineConfig,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    consensus::ConsensusManager,
    mempool::Mempool,
    proof_of_work::randomx_factory::RandomXFactory,
    topics::BLOCK_TOPIC,
};

const LOG_TARGET: &str = "c::bn::service::initializer";
const SUBSCRIPTION_LABEL: &str = "Base Node";

/// Initializer for the Base Node service handle and service future.
pub struct BaseNodeServiceInitializer<T> {
    blockchain_db: AsyncBlockchainDb<T>,
    mempool: Mempool,
    consensus_manager: ConsensusManager,
    service_request_timeout: Duration,
    randomx_factory: RandomXFactory,
    base_node_config: BaseNodeStateMachineConfig,
}

impl<T> BaseNodeServiceInitializer<T>
where T: BlockchainBackend
{
    /// Create a new BaseNodeServiceInitializer from the inbound message subscriber.
    pub fn new(
        blockchain_db: AsyncBlockchainDb<T>,
        mempool: Mempool,
        consensus_manager: ConsensusManager,
        service_request_timeout: Duration,
        randomx_factory: RandomXFactory,
        base_node_config: BaseNodeStateMachineConfig,
    ) -> Self {
        Self {
            blockchain_db,
            mempool,
            consensus_manager,
            service_request_timeout,
            randomx_factory,
            base_node_config,
        }
    }
}

#[async_trait]
impl<T> ServiceInitializer for BaseNodeServiceInitializer<T>
where T: BlockchainBackend + 'static
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Base Node Service");
        // Connect InboundNodeCommsInterface and OutboundNodeCommsInterface to BaseNodeService
        let (outbound_request_sender_service, outbound_request_stream) = reply_channel::unbounded();
        let (local_request_sender_service, local_request_stream) = reply_channel::unbounded();
        let (local_block_sender_service, local_block_stream) = reply_channel::unbounded();
        let (block_event_sender, _) = broadcast::channel(50);
        let local_nci = LocalNodeCommsInterface::new(
            local_request_sender_service,
            local_block_sender_service,
            block_event_sender.clone(),
        );

        context.register_handle(local_nci);

        let service_request_timeout = self.service_request_timeout;
        let blockchain_db = self.blockchain_db.clone();
        let mempool = self.mempool.clone();
        let consensus_manager = self.consensus_manager.clone();
        let randomx_factory = self.randomx_factory.clone();
        let config = self.base_node_config.clone();

        context.spawn_when_ready(move |handles| async move {
            let network = handles.expect_handle::<NetworkHandle>();
            let (block_publisher, block_subscription) = match network.subscribe_topic(BLOCK_TOPIC).await {
                Ok(x) => x,
                Err(err) => {
                    error!(target: LOG_TARGET, "⚠️ Failed to subscribe to BLOCK_TOPIC: {err}. THE BASE NODE SERVICE WILL NOT START.");
                    return;
                },
            };

            let dispatcher = handles.expect_handle::<Dispatcher>();
            let (inbound_msgs_tx, inbound_msgs_rx) = mpsc::unbounded_channel();
            dispatcher.register(TariMessageType::BaseNodeRequest, inbound_msgs_tx.clone());
            dispatcher.register(TariMessageType::BaseNodeResponse, inbound_msgs_tx);


            let state_machine = handles.expect_handle();
            let outbound_messaging = handles.expect_handle();

            let outbound_nci =
                OutboundNodeCommsInterface::new(outbound_request_sender_service, block_publisher);

            let inbound_nch = InboundNodeCommsHandlers::new(
                block_event_sender,
                blockchain_db,
                mempool,
                consensus_manager,
                outbound_nci.clone(),
                randomx_factory,
            );

            let streams = BaseNodeStreams {
                outbound_request_stream,
                inbound_messages: inbound_msgs_rx,
                block_subscription,
                local_request_stream,
                local_block_stream,
            };
            let service = BaseNodeService::new(
                outbound_messaging,
                inbound_nch,
                service_request_timeout,
                state_machine,
                network,
                config,
            )
            .start(streams);
            futures::pin_mut!(service);
            future::select(service, handles.get_shutdown_signal()).await;
            info!(target: LOG_TARGET, "Base Node Service shutdown");
        });

        debug!(target: LOG_TARGET, "Base Node Service initialized");
        Ok(())
    }
}
