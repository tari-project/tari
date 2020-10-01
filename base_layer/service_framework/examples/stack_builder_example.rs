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
#![feature(type_alias_impl_trait)]
pub mod services;

use crate::services::{ServiceAHandle, ServiceAInitializer, ServiceBHandle, ServiceBInitializer};
use std::time::Duration;
use tari_service_framework::StackBuilder;
use tari_shutdown::Shutdown;
use tokio::time::delay_for;

#[tokio_macros::main]
async fn main() {
    let mut shutdown = Shutdown::new();
    let fut = StackBuilder::new(shutdown.to_signal())
        .add_initializer(ServiceAInitializer::new("Service A response: ".to_string()))
        .add_initializer(ServiceBInitializer::new("Service B response: ".to_string()))
        .build();

    let handles = fut.await.expect("Should get the ServiceHandles from this stack ");

    let mut service_a_handle = handles.expect_handle::<ServiceAHandle>();
    let mut service_b_handle = handles.expect_handle::<ServiceBHandle>();

    delay_for(Duration::from_secs(1)).await;
    println!("----------------------------------------------------");
    let response_b = service_b_handle.send_msg("Hello B".to_string()).await;
    println!("Response from Service B: {}", response_b);
    println!("----------------------------------------------------");
    let response_a = service_a_handle.send_msg("Hello A".to_string()).await;
    println!("Response from Service A: {}", response_a);
    println!("----------------------------------------------------");

    let _ = shutdown.trigger();

    delay_for(Duration::from_secs(5)).await;
}
