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

use futures::{future, Future, StreamExt};
use std::sync::mpsc;
use tari_comms_dht::outbound::{DhtOutboundError, DhtOutboundRequest, OutboundMessageRequester};
use tari_p2p::{
    executor::{transport, ServiceInitializationError, ServiceInitializer},
    services::{
        comms_outbound::{CommsOutboundHandle, CommsOutboundRequest},
        ServiceHandlesFuture,
        ServiceName,
    },
};
use tari_service_framework::{
    handles::ServiceHandlesFuture,
    reply_channel,
    reply_channel::Receiver,
    ServiceInitializationError,
    ServiceInitializer,
};

async fn oms_reponder(mut oms_receiver: Receiver<DhtOutboundRequest, Result<(), DhtOutboundError>>) {
    while let Some(request_context) = oms_receiver.next() {
        let (request, reply_tx) = request_context.split();
        reply_tx.send(Ok(()));
    }
}

pub struct TestOutboundMessageServiceInitializer {
    sender: Option<mpsc::Sender<DhtOutboundRequest>>,
}

impl TestOutboundMessageServiceInitializer {
    pub fn new(sender: mpsc::Sender<DhtOutboundRequest>) -> Self {
        Self { sender: Some(sender) }
    }
}

impl ServiceInitializer<ServiceName> for TestOutboundMessageServiceInitializer {
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, executor: TaskExecutor, handles_fut: ServiceHandlesFuture) -> Self::Future {
        let sender = self.sender.take().expect("sender cannot be None");
        let (oms_sender, oms_receiver) = reply_channel::unbounded();

        handles_fut.register(OutboundMessageRequester::new(oms_sender));

        executor.spawn(async move {
            // Wait for all handles to become available
            let handles = handles_fut.await;

            oms_reponder(oms_receiver).await;
        });

        future::ready(Ok(()))
    }
}
