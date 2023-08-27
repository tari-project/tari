//  Copyright 2022. The Taiji Project
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
#![cfg(feature = "rpc")]
use core::iter;
use std::{cmp, convert::TryFrom, time::Duration};

use taiji_comms::{
    async_trait,
    protocol::rpc::{Request, Response, RpcStatus, Streaming},
    utils,
};
use taiji_comms_rpc_macros::taiji_rpc;
use tokio::{sync::mpsc, task, time};

#[taiji_rpc(protocol_name = b"t/greeting/1", server_struct = GreetingServer, client_struct = GreetingClient)]
pub trait GreetingRpc: Send + Sync + 'static {
    #[rpc(method = 1)]
    async fn say_hello(&self, request: Request<SayHelloRequest>) -> Result<Response<SayHelloResponse>, RpcStatus>;
    #[rpc(method = 2)]
    async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus>;
    #[rpc(method = 3)]
    async fn reply_with_msg_of_size(&self, request: Request<u64>) -> Result<Response<Vec<u8>>, RpcStatus>;
    #[rpc(method = 4)]
    async fn stream_large_items(
        &self,
        request: Request<StreamLargeItemsRequest>,
    ) -> Result<Streaming<Vec<u8>>, RpcStatus>;
    #[rpc(method = 5)]
    async fn slow_response(&self, request: Request<u64>) -> Result<Response<()>, RpcStatus>;
}

pub struct GreetingService {
    greetings: Vec<String>,
}

impl GreetingService {
    pub const DEFAULT_GREETINGS: &'static [&'static str] =
        &["Sawubona", "Jambo", "Bonjour", "Hello", "Molo", "Olá", "سلام", "你好"];

    pub fn new(greetings: &[&str]) -> Self {
        Self {
            greetings: greetings.iter().map(ToString::to_string).collect(),
        }
    }
}

impl Default for GreetingService {
    fn default() -> Self {
        Self::new(Self::DEFAULT_GREETINGS)
    }
}

#[async_trait]
impl GreetingRpc for GreetingService {
    async fn say_hello(&self, request: Request<SayHelloRequest>) -> Result<Response<SayHelloResponse>, RpcStatus> {
        let msg = request.message();
        let greeting = self
            .greetings
            .get(msg.language as usize)
            .ok_or_else(|| RpcStatus::bad_request(&format!("{} is not a valid language identifier", msg.language)))?;

        let greeting = format!("{} {}", greeting, msg.name);
        Ok(Response::new(SayHelloResponse { greeting }))
    }

    async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus> {
        let num = *request.message();
        let (tx, rx) = mpsc::channel(num as usize);
        let greetings = self.greetings[..cmp::min(num as usize + 1, self.greetings.len())].to_vec();
        task::spawn(async move { utils::mpsc::send_all(&tx, greetings.into_iter().map(Ok)).await });

        Ok(Streaming::new(rx))
    }

    async fn reply_with_msg_of_size(&self, request: Request<u64>) -> Result<Response<Vec<u8>>, RpcStatus> {
        let size = usize::try_from(request.into_message()).unwrap();
        Ok(Response::new(iter::repeat(0).take(size).collect()))
    }

    async fn stream_large_items(
        &self,
        request: Request<StreamLargeItemsRequest>,
    ) -> Result<Streaming<Vec<u8>>, RpcStatus> {
        let req_id = request.context().request_id();
        let StreamLargeItemsRequest {
            id,
            item_size,
            num_items,
            delay_ms: delay_secs,
        } = request.into_message();
        let (tx, rx) = mpsc::channel(10);
        let t = std::time::Instant::now();
        task::spawn(async move {
            let item = iter::repeat(0u8)
                .take(usize::try_from(item_size).unwrap())
                .collect::<Vec<_>>();
            for (i, item) in iter::repeat_with(|| Ok(item.clone()))
                .take(usize::try_from(num_items).unwrap())
                .enumerate()
            {
                if delay_secs > 0 {
                    time::sleep(Duration::from_millis(delay_secs)).await;
                }
                if tx.send(item).await.is_err() {
                    log::info!(
                        "[{}] reqid: {} t={:.2?} STREAM INTERRUPTED {}/{}",
                        id,
                        req_id,
                        t.elapsed(),
                        i + 1,
                        num_items
                    );
                    return;
                }
                log::info!(
                    "[{}] reqid: {} t={:.2?} sent {}/{}",
                    id,
                    req_id,
                    t.elapsed(),
                    i + 1,
                    num_items
                );
            }
        });
        Ok(Streaming::new(rx))
    }

    async fn slow_response(&self, request: Request<u64>) -> Result<Response<()>, RpcStatus> {
        time::sleep(Duration::from_secs(request.into_message())).await;
        Ok(Response::new(()))
    }
}

#[derive(prost::Message)]
pub struct SayHelloRequest {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(uint32, tag = "2")]
    pub language: u32,
}

#[derive(prost::Message)]
pub struct SayHelloResponse {
    #[prost(string, tag = "1")]
    pub greeting: String,
}

#[derive(prost::Message)]
pub struct StreamLargeItemsRequest {
    #[prost(uint64, tag = "1")]
    pub id: u64,
    #[prost(uint64, tag = "2")]
    pub num_items: u64,
    #[prost(uint64, tag = "3")]
    pub item_size: u64,
    #[prost(uint64, tag = "4")]
    pub delay_ms: u64,
}
