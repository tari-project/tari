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

#[cfg(test)]
mod test;

/// Maximum frame size of each RPC message. This is enforced in tokio's length delimited codec.
/// This can be thought of as the hard limit on message size.
pub const RPC_MAX_FRAME_SIZE: usize = 3 * 1024 * 1024; // 3 MiB
/// Maximum number of chunks into which a message can be broken up.
const RPC_CHUNKING_MAX_CHUNKS: usize = 16; // 16 x 256 Kib = 4 MiB max combined message size

/// The maximum request payload size
const fn max_request_size() -> usize {
    RPC_MAX_FRAME_SIZE
}

mod body;
pub use body::{Body, ClientStreaming, IntoBody, Streaming};

mod context;

mod server;
pub use server::{mock, NamedProtocolService, RpcServer, RpcServerBuilder, RpcServerError, RpcServerHandle};

mod client;
pub use client::{
    pool,
    pool::{RpcClientLease, RpcClientPool, RpcClientPoolError, RpcPoolClient},
    RpcClient,
    RpcClientBuilder,
    RpcClientConfig,
};

mod either;

mod message;
pub use message::{Request, Response};

mod error;
pub use error::RpcError;

mod handshake;
pub use handshake::{Handshake, RpcHandshakeError};

mod status;
pub use status::{RpcStatus, RpcStatusCode, RpcStatusResultExt};

mod not_found;

// Re-exports used to keep things orderly in the #[tari_rpc] proc macro
pub mod __macro_reexports {
    pub use futures::{future, future::BoxFuture};
    pub use tokio::io::{AsyncRead, AsyncWrite};
    pub use tower::Service;

    pub use crate::{
        framing::CanonicalFraming,
        protocol::{
            rpc::{
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
            },
            ProtocolId,
        },
        stream_id::StreamId,
        Bytes,
    };
}
