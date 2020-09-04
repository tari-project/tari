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

use super::RpcError;
use crate::{
    proto,
    proto::rpc::rpc_session_reply::SessionResult,
    protocol::rpc::{
        body::{Body, IntoBody},
        context::RequestContext,
    },
};
use bitflags::bitflags;
use bytes::Bytes;
use std::{fmt, time::Duration};

#[derive(Debug)]
pub struct Request<T> {
    pub(super) context: Option<RequestContext>,
    inner: BaseRequest<T>,
}

impl Request<Bytes> {
    pub fn decode<T: prost::Message + Default>(mut self) -> Result<Request<T>, RpcError> {
        let message = T::decode(&mut self.inner.message)?;
        Ok(Request {
            context: self.context,
            inner: BaseRequest::new(self.inner.method, message),
        })
    }
}

impl<T> Request<T> {
    pub(super) fn with_context(context: RequestContext, method: RpcMethod, message: T) -> Self {
        Self {
            context: Some(context),
            inner: BaseRequest::new(method, message),
        }
    }

    pub fn new(method: RpcMethod, message: T) -> Self {
        Self {
            context: None,
            inner: BaseRequest::new(method, message),
        }
    }

    pub fn method(&self) -> RpcMethod {
        self.inner.method
    }

    #[inline]
    pub fn message(&self) -> &T {
        &self.inner.message
    }

    pub fn into_message(self) -> T {
        self.inner.into_message()
    }

    /// Returns the request context that is provided to every service request.
    ///
    /// ## Panics
    ///
    /// This will panic if this instance was not constructed with `with_context`.
    /// The only time this may not be the case is in tests.
    pub fn context(&self) -> &RequestContext {
        self.context
            .as_ref()
            .expect("Request::context called on request without a context")
    }
}

#[derive(Debug, Clone)]
pub struct BaseRequest<T> {
    pub(super) method: RpcMethod,
    pub message: T,
}

impl<T> BaseRequest<T> {
    pub fn new(method: RpcMethod, message: T) -> Self {
        Self { method, message }
    }

    pub fn method(&self) -> RpcMethod {
        self.method
    }

    pub fn into_message(self) -> T {
        self.message
    }

    pub fn map<F, U>(self, mut f: F) -> BaseRequest<U>
    where F: FnMut(T) -> U {
        BaseRequest {
            method: self.method,
            message: f(self.message),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Response<T> {
    pub flags: RpcMessageFlags,
    pub message: T,
}

impl Response<Body> {
    pub fn from_message<T: IntoBody>(message: T) -> Self {
        Self {
            flags: Default::default(),
            message: message.into_body(),
        }
    }
}

impl<T> Response<T> {
    pub fn new(message: T) -> Self {
        Self {
            message,
            flags: Default::default(),
        }
    }

    pub fn map<F, U>(self, mut f: F) -> Response<U>
    where F: FnMut(T) -> U {
        Response {
            flags: self.flags,
            message: f(self.message),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.flags.is_fin()
    }

    pub fn into_message(self) -> T {
        self.message
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RpcMethod(u32);

impl RpcMethod {
    pub fn id(self) -> u32 {
        self.0
    }
}

impl From<u32> for RpcMethod {
    fn from(m: u32) -> Self {
        Self(m)
    }
}

impl Into<u32> for RpcMethod {
    fn into(self) -> u32 {
        self.0
    }
}

bitflags! {
    pub struct RpcMessageFlags: u8 {
        const FIN = 0x01;
    }
}
impl RpcMessageFlags {
    pub fn is_fin(&self) -> bool {
        self.contains(Self::FIN)
    }
}

impl Default for RpcMessageFlags {
    fn default() -> Self {
        RpcMessageFlags::empty()
    }
}

//---------------------------------- RpcRequest --------------------------------------------//

impl proto::rpc::RpcRequest {
    pub fn deadline(&self) -> Duration {
        Duration::from_secs(self.deadline)
    }

    pub fn flags(&self) -> RpcMessageFlags {
        RpcMessageFlags::from_bits_truncate(self.flags as u8)
    }
}

impl fmt::Display for proto::rpc::RpcRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestID={}, Deadline={:.0?}, Flags={:?}, Message={} byte(s)",
            self.request_id,
            self.deadline(),
            self.flags(),
            self.message.len()
        )
    }
}
//---------------------------------- RpcResponse --------------------------------------------//

impl proto::rpc::RpcResponse {
    pub fn flags(&self) -> RpcMessageFlags {
        RpcMessageFlags::from_bits_truncate(self.flags as u8)
    }
}

impl fmt::Display for proto::rpc::RpcResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestID={}, Flags={:?}, Message={} byte(s)",
            self.request_id,
            self.flags(),
            self.message.len()
        )
    }
}

//---------------------------------- RpcSessionReply --------------------------------------------//
impl proto::rpc::RpcSessionReply {
    /// Returns the accepted version from the reply. If the session was rejected, None is returned.
    pub fn accepted_version(&self) -> Option<u32> {
        match self.session_result.as_ref()? {
            SessionResult::AcceptedVersion(v) => Some(*v),
            SessionResult::Rejected(_) => None,
        }
    }
}
