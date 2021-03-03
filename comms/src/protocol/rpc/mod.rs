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

// TODO: Remove once in use
#![allow(dead_code)]

#[cfg(test)]
mod test;

mod body;
pub use body::{Body, ClientStreaming, IntoBody, Streaming};

mod context;

mod server;
pub use server::{NamedProtocolService, RpcServer};

mod client;
pub use client::{RpcClient, RpcClientBuilder, RpcClientConfig};

mod either;

mod message;
pub use message::{Request, Response};

mod error;
pub use error::RpcError;

mod router;

mod handshake;
pub use handshake::Handshake;

mod status;
pub use status::{RpcStatus, RpcStatusCode};

mod not_found;

pub mod mock;

/// Maximum frame size of each RPC message. This is enforced in tokio's length delimited codec.
pub const RPC_MAX_FRAME_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

// Re-exports used to keep things orderly in the #[tari_rpc] proc macro
pub mod __macro_reexports {
    pub use crate::{
        framing::CanonicalFraming,
        protocol::{
            rpc::{
                message::{Request, Response},
                server::NamedProtocolService,
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
        Bytes,
    };
    pub use futures::{future, future::BoxFuture, AsyncRead, AsyncWrite};
    pub use tower::Service;
}
