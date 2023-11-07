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

use std::{convert::TryFrom, fmt, time::Duration};

use bitflags::bitflags;
use bytes::Bytes;

use super::RpcError;
use crate::{
    proto,
    proto::rpc::rpc_session_reply::SessionResult,
    protocol::rpc::{
        body::{Body, IntoBody},
        context::RequestContext,
        error::HandshakeRejectReason,
        RpcStatusCode,
    },
};

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

    /// Returns the request context and inner message, consuming this Request.
    ///
    /// ## Panics
    ///
    /// This will panic if this instance was not constructed with `with_context`.
    /// The only time this may not be the case is in tests.
    pub fn into_parts(self) -> (RequestContext, T) {
        (
            self.context
                .expect("Request::context called on request without a context"),
            self.inner.into_message(),
        )
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

    #[allow(dead_code)]
    pub fn method(&self) -> RpcMethod {
        self.method
    }

    pub fn into_message(self) -> T {
        self.message
    }

    pub fn get_ref(&self) -> &T {
        &self.message
    }
}

#[derive(Debug, Clone)]
pub struct Response<T> {
    pub flags: RpcMessageFlags,
    pub payload: T,
}

impl Response<Body> {
    pub fn from_message<T: IntoBody>(message: T) -> Self {
        Self {
            flags: Default::default(),
            payload: message.into_body(),
        }
    }
}

impl<T> Response<T> {
    pub fn new(message: T) -> Self {
        Self {
            payload: message,
            flags: Default::default(),
        }
    }

    pub fn map<F, U>(self, mut f: F) -> Response<U>
    where F: FnMut(T) -> U {
        Response {
            flags: self.flags,
            payload: f(self.payload),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.flags.is_fin()
    }

    pub fn into_message(self) -> T {
        self.payload
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

#[allow(clippy::from_over_into)]
impl Into<u32> for RpcMethod {
    fn into(self) -> u32 {
        self.0
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct RpcMessageFlags: u8 {
        /// Message stream has completed
        const FIN = 0x01;
        /// Typically sent with empty contents and used to confirm a substream is alive.
        const ACK = 0x02;
        /// Another chunk to be received
        const MORE = 0x04;
    }
}
impl RpcMessageFlags {
    pub fn is_fin(self) -> bool {
        self.contains(Self::FIN)
    }

    pub fn is_ack(self) -> bool {
        self.contains(Self::ACK)
    }

    pub fn is_more(self) -> bool {
        self.contains(Self::MORE)
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

    pub fn flags(&self) -> Result<RpcMessageFlags, String> {
        RpcMessageFlags::from_bits(
            u8::try_from(self.flags).map_err(|_| format!("invalid message flag: must be less than {}", u8::MAX))?,
        )
        .ok_or(format!(
            "invalid message flag, does not match any flags ({})",
            self.flags
        ))
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
            self.payload.len()
        )
    }
}
//---------------------------------- RpcResponse --------------------------------------------//

#[derive(Debug, Clone)]
pub struct RpcResponse {
    pub request_id: u32,
    pub status: RpcStatusCode,
    pub flags: RpcMessageFlags,
    pub payload: Bytes,
}

impl RpcResponse {
    pub fn to_proto(&self) -> proto::rpc::RpcResponse {
        proto::rpc::RpcResponse {
            request_id: self.request_id,
            status: self.status as u32,
            flags: self.flags.bits().into(),
            payload: self.payload.to_vec(),
        }
    }
}

impl Default for RpcResponse {
    fn default() -> Self {
        Self {
            request_id: 0,
            status: RpcStatusCode::Ok,
            flags: Default::default(),
            payload: Default::default(),
        }
    }
}

impl proto::rpc::RpcResponse {
    pub fn flags(&self) -> Result<RpcMessageFlags, String> {
        RpcMessageFlags::from_bits(
            u8::try_from(self.flags).map_err(|_| format!("invalid message flag: must be less than {}", u8::MAX))?,
        )
        .ok_or(format!(
            "invalid message flag, does not match any flags ({})",
            self.flags
        ))
    }

    pub fn is_fin(&self) -> bool {
        u8::try_from(self.flags).unwrap() & RpcMessageFlags::FIN.bits() != 0
    }
}

impl fmt::Display for proto::rpc::RpcResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestID={}, Flags={:?}, Message={} byte(s)",
            self.request_id,
            self.flags(),
            self.payload.len()
        )
    }
}

//---------------------------------- RpcSessionReply --------------------------------------------//
impl proto::rpc::RpcSessionReply {
    /// Returns Ok(version) if the session was accepted, otherwise an error is returned with the rejection reason
    /// (`HandshakeRejectReason`)
    pub fn result(&self) -> Result<u32, HandshakeRejectReason> {
        match self.session_result.as_ref() {
            Some(SessionResult::AcceptedVersion(v)) => Ok(*v),
            Some(SessionResult::Rejected(_)) => {
                let reason = HandshakeRejectReason::from_i32(self.reject_reason).unwrap_or(
                    HandshakeRejectReason::Unknown("server returned unrecognised rejection reason"),
                );
                Err(reason)
            },
            None => Err(HandshakeRejectReason::Unknown(
                "handshake reply did not contain a session result",
            )),
        }
    }
}
