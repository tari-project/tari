// Copyright 2020. The Tari Project
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

use futures::{pin_mut, Future, StreamExt};
use std::time::Duration;
use tari_service_framework::{
    reply_channel,
    reply_channel::SenderService,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tari_shutdown::ShutdownSignal;
use tokio::time::delay_for;
use tower::Service;

pub struct ServiceB {
    response_msg: String,
    request_stream: Option<reply_channel::Receiver<String, String>>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl ServiceB {
    pub fn new(
        response_msg: String,
        request_stream: reply_channel::Receiver<String, String>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            response_msg,
            request_stream: Some(request_stream),
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub async fn run(mut self) {
        println!("Starting Service B");
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Service B initialized without shutdown signal");
        let request_stream = self
            .request_stream
            .take()
            .expect("Service B initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        loop {
            futures::select! {
                //Incoming request
                request_context = request_stream.select_next_some() => {
                    println!("Handling Service B API Request");
                    let (request, reply_tx) = request_context.split();
                    let mut response = self.response_msg.clone();
                    response.push_str(request.clone().as_str());
                    let _ = reply_tx.send(response);
                },
                _ = shutdown_signal => {
                    println!("Service B shutting down because the shutdown signal was received");
                    break;
                }
            }
        }

        println!("Service B is shutdown");
    }
}

#[derive(Clone)]
pub struct ServiceBHandle {
    request_tx: SenderService<String, String>,
}

impl ServiceBHandle {
    pub fn new(request_tx: SenderService<String, String>) -> Self {
        Self { request_tx }
    }

    pub async fn send_msg(&mut self, msg: String) -> String {
        self.request_tx.call(msg).await.unwrap()
    }
}

pub struct ServiceBInitializer {
    response_msg: String,
}

impl ServiceBInitializer {
    pub fn new(response_msg: String) -> Self {
        Self { response_msg }
    }
}

impl ServiceInitializer for ServiceBInitializer {
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        let (sender, receiver) = reply_channel::unbounded();

        let service_b_handle = ServiceBHandle::new(sender);

        println!("Service B is going to wait to register its handle");

        println!("Service B is registering its handle now");
        context.register_handle(service_b_handle);

        let response_msg = self.response_msg.clone();

        println!("Service B initialized waiting on Handles Future to complete");
        context.spawn_when_ready(move |handles| async move {
            println!("Service B got the handles");
            let service = ServiceB::new(response_msg, receiver, handles.get_shutdown_signal());
            service.run().await;
            println!("Service B has shutdown and initializer spawned task is now ending");
        });

        async {
            delay_for(Duration::from_secs(10)).await;
            Ok(())
        }
    }
}
