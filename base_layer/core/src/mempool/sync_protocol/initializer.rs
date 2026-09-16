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

use log::*;
use tari_comms::{
    Substream,
    connectivity::ConnectivityRequester,
    protocol::{ProtocolExtension, ProtocolExtensionContext, ProtocolExtensionError, ProtocolNotification},
};
use tari_service_framework::{ServiceInitializationError, ServiceInitializer, ServiceInitializerContext, async_trait};
use tokio::{sync::mpsc, time::sleep};

use crate::{
    base_node::{StateMachineHandle, comms_interface::LocalNodeCommsInterface},
    mempool::{
        Mempool,
        MempoolServiceConfig,
        sync_protocol::{MEMPOOL_SYNC_PROTOCOL, MempoolSyncProtocol},
    },
};

const LOG_TARGET: &str = "c::mempool::sync_protocol";

pub struct MempoolSyncInitializer {
    config: MempoolServiceConfig,
    mempool: Mempool,
    notif_rx: Option<mpsc::Receiver<ProtocolNotification<Substream>>>,
    notif_tx: mpsc::Sender<ProtocolNotification<Substream>>,
}

impl MempoolSyncInitializer {
    pub fn new(config: MempoolServiceConfig, mempool: Mempool) -> Self {
        let (notif_tx, notif_rx) = mpsc::channel(3);
        Self {
            mempool,
            config,
            notif_tx,
            notif_rx: Some(notif_rx),
        }
    }

    pub fn get_protocol_extension(&self) -> impl ProtocolExtension + use<> {
        let notif_tx = self.notif_tx.clone();
        move |context: &mut ProtocolExtensionContext| -> Result<(), ProtocolExtensionError> {
            context.add_protocol([MEMPOOL_SYNC_PROTOCOL.clone()], &notif_tx);
            Ok(())
        }
    }
}

#[async_trait]
impl ServiceInitializer for MempoolSyncInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        trace!(target: LOG_TARGET, "Initializing Mempool Sync Service");
        let config = self.config.clone();
        let mempool = self.mempool.clone();
        let notif_rx = self.notif_rx.take().unwrap();

        // `spawn_when_ready`, not `spawn_until_shutdown`: the latter races this whole future against
        // the shutdown signal with `future::select`, which polls the signal first and drops the
        // future without polling it again. That would cut `run()` short before it can abort and
        // join its peer protocol tasks, which is precisely the guarantee we need at shutdown. This
        // service therefore handles the signal itself, at both points where it can wait forever.
        context.spawn_when_ready(move |handles| async move {
            // `get_handle` rather than `expect_handle`: the ready signal also fires when the stack
            // builder returns early on an initializer error, dropping the notifier. In that case no
            // handles were ever registered, and unlike `spawn_until_shutdown` — which could drop
            // this future before its body ran — `spawn_when_ready` always runs it. Panicking here
            // would add noise to an already-failing startup rather than reporting anything new.
            let (Some(state_machine), Some(connectivity), Some(base_node)) = (
                handles.get_handle::<StateMachineHandle>(),
                handles.get_handle::<ConnectivityRequester>(),
                handles.get_handle::<LocalNodeCommsInterface>(),
            ) else {
                debug!(
                    target: LOG_TARGET,
                    "Service handles are unavailable, so the node is shutting down before it \
                     finished starting. Mempool sync protocol will not run."
                );
                return;
            };
            let shutdown_signal = handles.get_shutdown_signal();

            let mut status_watch = state_machine.get_status_info_watch();
            if !status_watch.borrow().state_info.is_synced() {
                debug!(target: LOG_TARGET, "Waiting for node to do initial sync...");
                let wait_for_initial_sync = async {
                    while status_watch.changed().await.is_ok() {
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
                };
                let mut shutdown = shutdown_signal.clone();
                tokio::pin!(wait_for_initial_sync);
                tokio::select! {
                    () = &mut wait_for_initial_sync => {},
                    _ = &mut shutdown => {
                        debug!(
                            target: LOG_TARGET,
                            "Shutdown requested before initial sync completed; mempool sync will not start"
                        );
                        return;
                    },
                }
            }
            let base_node_events = base_node.get_block_event_stream();

            MempoolSyncProtocol::new(
                config,
                notif_rx,
                mempool,
                connectivity,
                base_node_events,
                shutdown_signal,
            )
            .run()
            .await;
        });

        trace!(target: LOG_TARGET, "Mempool sync service initialized");
        Ok(())
    }
}
