//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{convert::TryFrom, fmt, time::Duration};

use bitflags::bitflags;
use bytes::Bytes;

use crate::{
    body::{Body, IntoBody},
    error::HandshakeRejectReason,
    max_response_payload_size,
    proto,
    proto::rpc_session_reply::SessionResult,
    RpcError,
    RpcStatusCode,
};

#[derive(Debug)]
pub struct Request<T> {
    inner: BaseRequest<T>,
}

impl Request<Bytes> {
    pub fn decode<T: prost::Message + Default>(mut self) -> Result<Request<T>, RpcError> {
        let message = T::decode(&mut self.inner.message)?;
        Ok(Request {
            inner: BaseRequest::new(self.inner.method, message),
        })
    }
}

impl<T> Request<T> {
    pub(super) fn new(method: RpcMethod, message: T) -> Self {
        Self {
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

    #[allow(dead_code)]
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
    }
}
impl RpcMessageFlags {
    pub fn is_fin(self) -> bool {
        self.contains(Self::FIN)
    }

    pub fn is_ack(self) -> bool {
        self.contains(Self::ACK)
    }
}

impl Default for RpcMessageFlags {
    fn default() -> Self {
        RpcMessageFlags::empty()
    }
}

//---------------------------------- RpcRequest --------------------------------------------//

impl proto::RpcRequest {
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

impl fmt::Display for proto::RpcRequest {
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
    pub fn to_proto(&self) -> proto::RpcResponse {
        proto::RpcResponse {
            request_id: self.request_id,
            status: self.status as u32,
            flags: self.flags.bits().into(),
            payload: self.payload.to_vec(),
        }
    }

    pub fn exceeded_message_size(self) -> RpcResponse {
        let msg = format!(
            "The response size exceeded the maximum allowed payload size. Max = {} bytes, Got = {} bytes",
            max_response_payload_size() as f32,
            self.payload.len() as f32,
        );
        RpcResponse {
            request_id: self.request_id,
            status: RpcStatusCode::MalformedResponse,
            flags: RpcMessageFlags::FIN,
            payload: msg.into_bytes().into(),
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

impl proto::RpcResponse {
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

impl fmt::Display for proto::RpcResponse {
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
impl proto::RpcSessionReply {
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
