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

use futures::future;
use std::sync::mpsc;
use tari_p2p::{
    executor::{transport, ServiceInitializationError, ServiceInitializer},
    services::{
        comms_outbound::{CommsOutboundHandle, CommsOutboundRequest},
        ServiceHandlesFuture,
        ServiceName,
    },
};
use tower_util::service_fn;

pub struct TestCommsOutboundInitializer {
    sender: Option<mpsc::Sender<CommsOutboundRequest>>,
}

impl TestCommsOutboundInitializer {
    pub fn new(sender: mpsc::Sender<CommsOutboundRequest>) -> Self {
        Self { sender: Some(sender) }
    }
}

impl ServiceInitializer<ServiceName> for TestCommsOutboundInitializer {
    fn initialize(mut self: Box<Self>, handles: ServiceHandlesFuture) -> Result<(), ServiceInitializationError> {
        let sender = self.sender.take().expect("cannot be None");
        let (oms_requester, oms_responder) = transport::channel(service_fn(move |req| {
            sender.send(req).unwrap();
            future::ok::<_, ()>(Ok(()))
        }));
        tokio::spawn(oms_responder);
        handles.insert(ServiceName::CommsOutbound, CommsOutboundHandle::new(oms_requester));
        Ok(())
    }
}
