// Copyright 2019, The Tari Project
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

use super::{service::ChainMetadataService, LOG_TARGET};
use crate::base_node::{chain_metadata_service::handle::ChainMetadataHandle, comms_interface::LocalNodeCommsInterface};
use futures::{future, future::select, pin_mut};
use log::*;
use std::future::Future;
use tari_broadcast_channel as broadcast_channel;
use tari_p2p::services::liveness::LivenessHandle;
use tari_service_framework::{handles::ServiceHandlesFuture, ServiceInitializationError, ServiceInitializer};
use tari_shutdown::ShutdownSignal;
use tokio::runtime;

// Must be set to 1 to ensure outdated chain metadata is discarded.
const BROADCAST_EVENT_BUFFER_SIZE: usize = 1;

pub struct ChainMetadataServiceInitializer;

impl ServiceInitializer for ChainMetadataServiceInitializer {
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        let (publisher, subscriber) = broadcast_channel::bounded(BROADCAST_EVENT_BUFFER_SIZE);
        let handle = ChainMetadataHandle::new(subscriber);
        handles_fut.register(handle);

        executor.spawn(async move {
            let handles = handles_fut.await;

            let liveness = handles
                .get_handle::<LivenessHandle>()
                .expect("Liveness service required to initialize ChainStateSyncService");

            let base_node = handles
                .get_handle::<LocalNodeCommsInterface>()
                .expect("LocalNodeCommsInterface required to initialize ChainStateSyncService");

            let service_run = ChainMetadataService::new(liveness, base_node, publisher).run();
            pin_mut!(service_run);
            select(service_run, shutdown).await;
            info!(target: LOG_TARGET, "ChainMetadataService has shut down");
        });

        future::ready(Ok(()))
    }
}
