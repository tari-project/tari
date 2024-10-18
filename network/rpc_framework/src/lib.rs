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

//! # RPC protocol
//!
//! Provides a request/response protocol that supports streaming.
//! Available with the `rpc` crate feature.

// #[cfg(test)]
// mod test;

/// Maximum frame size of each RPC message. This is enforced in tokio's length delimited codec.
/// This can be thought of as the hard limit on message size.
pub const RPC_MAX_FRAME_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

/// The maximum request payload size
const fn max_request_size() -> usize {
    RPC_MAX_FRAME_SIZE
}

/// The maximum size for a single RPC response message
const fn max_response_size() -> usize {
    RPC_MAX_FRAME_SIZE
}

/// The maximum size for a single RPC response excluding overhead
const fn max_response_payload_size() -> usize {
    // RpcResponse overhead is:
    // - 4 varint protobuf fields, each field ID is 1 byte
    // - 3 u32 fields, VarInt(u32::MAX) is 5 bytes
    // - 1 length varint for the payload, allow for 5 bytes to be safe (max_payload_size being technically too small is
    //   fine, being too large isn't)
    const MAX_HEADER_SIZE: usize = 4 + 4 * 5;
    max_response_size() - MAX_HEADER_SIZE
}

mod body;
pub use body::{Body, ClientStreaming, IntoBody, Streaming};

mod server;
pub use server::{NamedProtocolService, RpcServer, RpcServerBuilder, RpcServerError, RpcServerHandle};

mod client;
pub use client::*;

mod either;

mod message;
pub use message::{Request, Response};

mod error;
pub use error::RpcError;

mod handshake;
pub use handshake::{Handshake, RpcHandshakeError};

mod status;
pub use status::{RpcStatus, RpcStatusCode, RpcStatusResultExt};

mod proto;

pub mod bounded_executor;
pub mod framing;

mod not_found;
mod notify;
pub mod optional;

// Re-exports used to keep things orderly in the #[tari_rpc] proc macro
pub mod __macro_reexports {
    pub use bytes::Bytes;
    pub use futures::{future, future::BoxFuture};
    pub use libp2p::{PeerId, StreamProtocol};
    pub use prost;
    pub use tokio::io::{AsyncRead, AsyncWrite};
    pub use tower::Service;

    pub use crate::{
        framing::CanonicalFraming,
        message::{Request, Response},
        pool::RpcPoolClient,
        server::{NamedProtocolService, RpcServerError},
        Body,
        ClientStreaming,
        IntoBody,
        RpcClient,
        RpcClientBuilder,
        RpcError,
        RpcStatus,
    };
}

pub use async_trait::async_trait;

// TODO: We could fairly easily abstract this and make this framework independent of libp2p
pub type Substream = libp2p::Stream;
