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
    chain_storage::BlockchainBackend,
    mempool::{
        sync_protocol::{MempoolSyncProtocol, MEMPOOL_SYNC_PROTOCOL},
        Mempool,
        MempoolServiceConfig,
    },
};
use futures::channel::mpsc;
use tari_comms::protocol::{ProtocolExtension, ProtocolExtensionContext, ProtocolExtensionError};
use tokio::task;

pub struct MempoolSyncProtocolExtension<T> {
    config: MempoolServiceConfig,
    mempool: Mempool<T>,
}

impl<T> MempoolSyncProtocolExtension<T> {
    pub fn new(config: MempoolServiceConfig, mempool: Mempool<T>) -> Self {
        Self { mempool, config }
    }
}

impl<T: BlockchainBackend + 'static> ProtocolExtension for MempoolSyncProtocolExtension<T> {
    fn install(self: Box<Self>, context: &mut ProtocolExtensionContext) -> Result<(), ProtocolExtensionError> {
        let (notif_tx, notif_rx) = mpsc::channel(3);

        context.add_protocol(&[MEMPOOL_SYNC_PROTOCOL.clone()], notif_tx);

        task::spawn(
            MempoolSyncProtocol::new(
                self.config,
                notif_rx,
                context.connectivity().subscribe_event_stream(),
                self.mempool.clone(),
            )
            .run(),
        );

        Ok(())
    }
}
