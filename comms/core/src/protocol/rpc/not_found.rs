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

use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future;
use tower::Service;

use super::{
    body::Body,
    message::{Request, Response},
    server::RpcServerError,
    RpcStatus,
};
use crate::protocol::ProtocolId;

#[derive(Clone)]
pub struct ProtocolServiceNotFound;

impl Service<ProtocolId> for ProtocolServiceNotFound {
    type Error = RpcServerError;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;
    type Response = NeverService;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, protocol: ProtocolId) -> Self::Future {
        future::ready(Err(RpcServerError::ProtocolServiceNotFound(
            String::from_utf8_lossy(&protocol).to_string(),
        )))
    }
}

// Used to satisfy the ProtocolServiceNotFound: MakeService trait bound. This is never called.
pub struct NeverService;

impl Service<Request<Bytes>> for NeverService {
    type Error = RpcStatus;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;
    type Response = Response<Body>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unimplemented!()
    }

    fn call(&mut self, _: Request<Bytes>) -> Self::Future {
        unimplemented!()
    }
}
