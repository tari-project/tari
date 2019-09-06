// Copyright 2019 The Tari Project
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

mod error;
mod handle;
mod messages;
mod service;

use self::service::CommsOutboundService;
use crate::services::comms_outbound::messages::CommsOutboundResponse;
use futures::{
    future::{self, Future},
    task::SpawnExt,
};
use std::sync::Arc;
use tari_comms::outbound_message_service::outbound_message_service::OutboundMessageService;
use tari_service_framework::{
    handles::ServiceHandlesFuture,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
};

pub use self::{error::CommsOutboundServiceError, messages::CommsOutboundRequest};

type CommsOutboundRequestSender =
    reply_channel::SenderService<CommsOutboundRequest, Result<CommsOutboundResponse, CommsOutboundServiceError>>;
/// Convenience type alias for external services that want to use this services handle
pub type CommsOutboundHandle = handle::CommsOutboundHandle<CommsOutboundRequestSender>;

/// Initializer for CommsOutbound service
pub struct CommsOutboundServiceInitializer {
    oms: Arc<OutboundMessageService>,
}

impl CommsOutboundServiceInitializer {
    pub fn new(oms: Arc<OutboundMessageService>) -> Self {
        Self { oms }
    }
}

impl<TExec> ServiceInitializer<TExec> for CommsOutboundServiceInitializer
where TExec: SpawnExt
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, executor: &mut TExec, handles: ServiceHandlesFuture) -> Self::Future {
        let (requester, responder) = reply_channel::unbounded();
        handles.register(CommsOutboundHandle::new(requester));

        let service = CommsOutboundService::new(responder, Arc::clone(&self.oms));
        let spawn_res = executor.spawn(service.run()).map_err(Into::into);

        future::ready(spawn_res)
    }
}
