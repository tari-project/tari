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

use log::*;
use tari_network::NetworkHandle;
use tari_p2p::services::liveness::LivenessHandle;
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tokio::sync::broadcast;

use super::service::ChainMetadataService;
use crate::base_node::{chain_metadata_service::handle::ChainMetadataHandle, comms_interface::LocalNodeCommsInterface};

const LOG_TARGET: &str = "c::bn::chain_metadata_service::initializer";

pub struct ChainMetadataServiceInitializer;

#[async_trait]
impl ServiceInitializer for ChainMetadataServiceInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Chain Metadata Service");
        let (publisher, _) = broadcast::channel(20);

        let handle = ChainMetadataHandle::new(publisher.clone());
        context.register_handle(handle);

        context.spawn_until_shutdown(|handles| {
            let network = handles.expect_handle::<NetworkHandle>();
            let liveness = handles.expect_handle::<LivenessHandle>();
            let base_node = handles.expect_handle::<LocalNodeCommsInterface>();

            ChainMetadataService::new(liveness, base_node, network, publisher).run()
        });

        debug!(target: LOG_TARGET, "Chain Metadata Service initialized");
        Ok(())
    }
}
