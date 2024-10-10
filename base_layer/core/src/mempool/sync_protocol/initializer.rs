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

use std::time::Duration;

use libp2p_substream::{ProtocolNotification, Substream};
use log::*;
use tari_network::NetworkHandle;
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tokio::{sync::mpsc, time::sleep};

use crate::{
    base_node::{comms_interface::LocalNodeCommsInterface, StateMachineHandle},
    mempool::{
        sync_protocol::{MempoolSyncProtocol, MEMPOOL_SYNC_PROTOCOL},
        Mempool,
        MempoolServiceConfig,
    },
};

const LOG_TARGET: &str = "c::mempool::sync_protocol";

pub struct MempoolSyncInitializer {
    config: MempoolServiceConfig,
    mempool: Mempool,
}

impl MempoolSyncInitializer {
    pub fn new(config: MempoolServiceConfig, mempool: Mempool) -> Self {
        Self { mempool, config }
    }
}

#[async_trait]
impl ServiceInitializer for MempoolSyncInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Mempool Sync Service");
        let config = self.config.clone();
        let mempool = self.mempool.clone();

        let mut mdc = vec![];
        log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
        context.spawn_until_shutdown(move |handles| async move {
            log_mdc::extend(mdc.clone());
            let (notif_tx, notif_rx) = mpsc::unbounded_channel();
            let network = handles.expect_handle::<NetworkHandle>();
            if let Err(err) = network.add_protocol_notifier(&[MEMPOOL_SYNC_PROTOCOL.clone()], notif_tx).await {
                error!(target: LOG_TARGET, "Failed to register protocol notifier for mempool sync: {err}. Peers will not be able to initiate mempool sync with this node")
            }
            let state_machine = handles.expect_handle::<StateMachineHandle>();
            let base_node = handles.expect_handle::<LocalNodeCommsInterface>();

            let mut status_watch = state_machine.get_status_info_watch();
            if !status_watch.borrow().state_info.is_synced() {
                debug!(target: LOG_TARGET, "Waiting for node to do initial sync...");
                while status_watch.changed().await.is_ok() {
                    log_mdc::extend(mdc.clone());
                    if status_watch.borrow().state_info.is_synced() {
                        debug!(
                            target: LOG_TARGET,
                            "Initial sync is done. Starting mempool sync protocol"
                        );
                        break;
                    }
                    trace!(
                        target: LOG_TARGET,
                        "Mempool sync still on hold, waiting for node to do initial sync",
                    );
                    sleep(Duration::from_secs(30)).await;
                }
                log_mdc::extend(mdc.clone());
            }
            let base_node_events = base_node.get_block_event_stream();

            MempoolSyncProtocol::new(config, notif_rx, mempool, network, base_node_events)
                .run()
                .await;
        });

        debug!(target: LOG_TARGET, "Mempool sync service initialized");
        Ok(())
    }
}
