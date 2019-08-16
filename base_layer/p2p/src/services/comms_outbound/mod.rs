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

use crate::{
    executor::{
        transport::{self, Responder},
        MakeServicePair,
    },
    services::ServiceName,
    tari_message::TariMessageType,
};
use std::sync::Arc;
use tari_comms::builder::CommsServices;

use self::service::CommsOutboundService;
use crate::services::{comms_outbound::messages::CommsOutboundRequest, ServiceHandles};
pub use error::CommsOutboundServiceError;
pub use handle::CommsOutboundHandle;

pub struct MakeCommsOutboundService<'a> {
    comms: &'a CommsServices<TariMessageType>,
}

impl<'a> MakeCommsOutboundService<'a> {
    pub fn new(comms: &'a CommsServices<TariMessageType>) -> Self {
        Self { comms }
    }
}

impl<'a> MakeServicePair<ServiceName> for MakeCommsOutboundService<'a> {
    type Future = Responder<CommsOutboundService, CommsOutboundRequest>;
    type Handle = CommsOutboundHandle;

    fn make_pair(self, _: Arc<ServiceHandles>) -> (Self::Handle, Self::Future) {
        let oms = self.comms.outbound_message_service();
        let service = CommsOutboundService::new(oms);
        let (requester, responder) = transport::channel(service);
        (CommsOutboundHandle::new(requester), responder)
    }
}
