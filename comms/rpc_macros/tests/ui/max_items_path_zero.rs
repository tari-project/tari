// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![allow(unused_imports)]

use tari_comms::protocol::rpc::{Request, Response, RpcStatus};
use tari_comms_rpc_macros::tari_rpc;

const NO_ITEMS: usize = 0;

#[tari_rpc(protocol_name = b"/test/1", server_struct = TestServer, client_struct = TestClient)]
pub trait Test: Send + Sync + 'static {
    #[rpc(method = 1, max_items = NO_ITEMS)]
    async fn hello(&self, request: Request<u32>) -> Result<Response<u32>, RpcStatus>;
}

fn main() {}
