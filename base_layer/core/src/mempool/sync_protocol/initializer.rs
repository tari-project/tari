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

use crate::{
    base_node::StateMachineHandle,
    mempool::{
        sync_protocol::{MempoolSyncProtocol, MEMPOOL_SYNC_PROTOCOL},
        Mempool,
        MempoolServiceConfig,
    },
};
use futures::{channel::mpsc, future};
use tari_comms::{
    connectivity::ConnectivityRequester,
    protocol::{ProtocolExtension, ProtocolExtensionContext, ProtocolExtensionError, ProtocolNotification},
    Substream,
};
use tari_service_framework::{ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};

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

    pub fn get_protocol_extension(&self) -> impl ProtocolExtension {
        let notif_tx = self.notif_tx.clone();
        move |context: &mut ProtocolExtensionContext| -> Result<(), ProtocolExtensionError> {
            context.add_protocol(&[MEMPOOL_SYNC_PROTOCOL.clone()], notif_tx);
            Ok(())
        }
    }
}

impl ServiceInitializer for MempoolSyncInitializer {
    type Future = future::Ready<Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        let config = self.config;
        let mempool = self.mempool.clone();
        let notif_rx = self.notif_rx.take().unwrap();

        context.spawn_when_ready(move |handles| {
            let state_machine = handles.expect_handle::<StateMachineHandle>();
            let connectivity = handles.expect_handle::<ConnectivityRequester>();
            MempoolSyncProtocol::new(
                config,
                notif_rx,
                connectivity.get_event_subscription(),
                mempool,
                Some(state_machine),
            )
            .run()
        });

        future::ready(Ok(()))
    }
}
