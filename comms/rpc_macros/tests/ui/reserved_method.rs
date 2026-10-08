// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![allow(unused_imports)]

use tari_comms::protocol::rpc::{Request, Response, RpcStatus};
use tari_comms_rpc_macros::tari_rpc;

#[tari_rpc(
    protocol_name = b"/test/1",
    server_struct = TestServer,
    client_struct = TestClient,
    reserved_methods = [2, 7]
)]
pub trait Test: Send + Sync + 'static {
    #[rpc(method = 1)]
    async fn hello(&self, request: Request<u32>) -> Result<Response<u32>, RpcStatus>;
    #[rpc(method = 7)]
    async fn goodbye(&self, request: Request<u32>) -> Result<Response<u32>, RpcStatus>;
}

fn main() {}
