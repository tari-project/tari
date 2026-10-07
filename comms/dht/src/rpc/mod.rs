//  Copyright 2020, The Tari Project
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

//! DHT RPC interface defining RPC methods for peer sharing.
#[cfg(test)]
mod test;

mod service;
pub use service::DhtRpcServiceImpl;
use tari_comms::protocol::rpc::{Request, RpcStatus, Streaming};
use tari_comms_rpc_macros::tari_rpc;

use crate::proto::rpc::{GetPeersRequest, GetPeersResponse};

mod peer_info;
pub use peer_info::UnvalidatedPeerInfo;

// `max_items` is the decode budget for a method: the most embedded message instances its request, response or each
// stream item may carry (see `tari_comms::decode_budget`). 262_144 covers a max-weight block body or transaction (~90k
// message instances); 65_536 covers the batched queries and streams. The default is 16_384.
#[tari_rpc(protocol_name = b"t/dht/1", server_struct = DhtService, client_struct = DhtClient)]
pub trait DhtRpcService: Send + Sync + 'static {
    #[rpc(method = 10, max_items = 65_536)]
    async fn get_peers(&self, request: Request<GetPeersRequest>) -> Result<Streaming<GetPeersResponse>, RpcStatus>;
}
