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

use std::{fmt, fmt::Display};

use log::*;
use thiserror::Error;

use super::RpcError;
use crate::{proto, traits::OrOptional};

const LOG_TARGET: &str = "comms::rpc::status";

/// The most bytes of a peer's error details that are kept. The details come from the peer and end up in logs and in
/// stored ban reasons, so a peer must not be able to make them arbitrarily large.
const MAX_PEER_DETAILS_BYTES: usize = 512;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub struct RpcStatus {
    code: RpcStatusCode,
    details: String,
}

impl RpcStatus {
    pub fn ok() -> Self {
        Self {
            code: RpcStatusCode::Ok,
            details: Default::default(),
        }
    }

    pub fn unsupported_method<T: ToString + ?Sized>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::UnsupportedMethod,
            details: details.to_string(),
        }
    }

    pub fn not_implemented<T: ToString + ?Sized>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::NotImplemented,
            details: details.to_string(),
        }
    }

    pub fn bad_request<T: ToString + ?Sized>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::BadRequest,
            details: details.to_string(),
        }
    }

    /// Returns a general error. As with all other errors care should be taken not to leak sensitive data to remote
    /// peers through error messages.
    pub fn general<T: ToString + ?Sized>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::General,
            details: details.to_string(),
        }
    }

    pub fn general_default() -> Self {
        Self::general(&"General error")
    }

    pub fn timed_out<T: ToString + ?Sized>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::Timeout,
            details: details.to_string(),
        }
    }

    pub fn not_found<T: ToString + ?Sized>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::NotFound,
            details: details.to_string(),
        }
    }

    pub fn forbidden<T: ToString + ?Sized>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::Forbidden,
            details: details.to_string(),
        }
    }

    pub fn conflict<T: ToString + ?Sized>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::Conflict,
            details: details.to_string(),
        }
    }

    /// Returns a closure that logs the given error and returns a generic general error that does not leak any
    /// potentially sensitive error information. Use this function with map_err to catch "miscellaneous" errors.
    pub fn log_internal_error<'a, E: std::error::Error + 'a>(target: &'a str) -> impl Fn(E) -> Self + 'a {
        move |err| {
            log::error!(target: target, "Internal error: {err}");
            Self::general_default()
        }
    }

    pub(super) fn protocol_error<T: ToString>(details: &T) -> Self {
        Self {
            code: RpcStatusCode::ProtocolError,
            details: details.to_string(),
        }
    }

    pub fn as_code(&self) -> u32 {
        self.code.as_u32()
    }

    pub fn as_status_code(&self) -> RpcStatusCode {
        self.code
    }

    pub fn details(&self) -> &str {
        &self.details
    }

    pub fn to_details_bytes(&self) -> Vec<u8> {
        self.details.as_bytes().to_vec()
    }

    pub fn is_ok(&self) -> bool {
        self.code.is_ok()
    }

    pub fn is_not_found(&self) -> bool {
        self.code.is_not_found()
    }
}

impl Display for RpcStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.details)
    }
}

impl From<RpcError> for RpcStatus {
    fn from(err: RpcError) -> Self {
        match err {
            RpcError::DecodeError(_) => Self::bad_request("Failed to decode request"),
            RpcError::DecodeBudgetExceeded { .. } => Self::bad_request("Request exceeds the decode budget"),
            RpcError::RequestFailed(status) => status,
            err => {
                error!(target: LOG_TARGET, "Internal error: {err}");
                Self::general(&err.to_string())
            },
        }
    }
}

impl<'a> From<&'a proto::rpc::RpcResponse> for RpcStatus {
    fn from(resp: &'a proto::rpc::RpcResponse) -> Self {
        let status_code = RpcStatusCode::from(resp.status);
        if status_code.is_ok() {
            return RpcStatus::ok();
        }

        // Slice the bytes before decoding, so that a huge payload is never converted in full. Control characters
        // (including newlines) are replaced so that the details cannot forge log lines.
        let prefix = resp.payload.get(..MAX_PEER_DETAILS_BYTES).unwrap_or(&resp.payload);
        let mut details = String::from_utf8_lossy(prefix)
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>();
        if resp.payload.len() > MAX_PEER_DETAILS_BYTES {
            details.push_str(&format!("… (truncated, {} bytes)", resp.payload.len()));
        }

        RpcStatus {
            code: status_code,
            details,
        }
    }
}

impl From<prost::DecodeError> for RpcStatus {
    fn from(_: prost::DecodeError) -> Self {
        Self::bad_request("Failed to decode request")
    }
}

pub trait RpcStatusResultExt<T> {
    fn rpc_status_internal_error(self, target: &str) -> Result<T, RpcStatus>;
    fn rpc_status_not_found<S: ToString>(self, message: S) -> Result<T, RpcStatus>;
    fn rpc_status_bad_request<S: ToString>(self, message: S) -> Result<T, RpcStatus>;
}

impl<T, E: std::error::Error> RpcStatusResultExt<T> for Result<T, E> {
    fn rpc_status_internal_error(self, target: &str) -> Result<T, RpcStatus> {
        self.map_err(RpcStatus::log_internal_error(target))
    }

    fn rpc_status_not_found<S: ToString>(self, message: S) -> Result<T, RpcStatus> {
        self.map_err(|_| RpcStatus::not_found(&message))
    }

    fn rpc_status_bad_request<S: ToString>(self, message: S) -> Result<T, RpcStatus> {
        self.map_err(|_| RpcStatus::bad_request(&message))
    }
}

impl<T> OrOptional<T> for Result<T, RpcStatus> {
    type Error = RpcStatus;

    fn or_optional(self) -> Result<Option<T>, Self::Error> {
        self.map(Some)
            .or_else(|status| if status.is_not_found() { Ok(None) } else { Err(status) })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcStatusCode {
    /// Request succeeded
    Ok = 0,
    /// Request is incorrect
    BadRequest = 1,
    /// The method is not recognised
    UnsupportedMethod = 2,
    /// Method is not implemented
    NotImplemented = 3,
    /// The timeout was reached before a response was received (client only)
    Timeout = 4,
    /// Received malformed response
    MalformedResponse = 5,
    /// Misc. errors
    General = 6,
    /// Entity not found
    NotFound = 7,
    /// RPC protocol error
    ProtocolError = 8,
    /// RPC forbidden error
    Forbidden = 9,
    /// RPC conflict error
    Conflict = 10,
    // The following status represents anything that is not recognised (i.e not one of the above codes).
    /// Unrecognised RPC status code
    InvalidRpcStatusCode,
}

impl RpcStatusCode {
    pub fn is_ok(self) -> bool {
        self == Self::Ok
    }

    pub fn is_not_found(self) -> bool {
        self == Self::NotFound
    }

    pub fn is_timeout(self) -> bool {
        self == Self::Timeout
    }

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }

    pub fn to_debug_string(&self) -> String {
        format!("{self:?}")
    }
}

impl From<u32> for RpcStatusCode {
    fn from(code: u32) -> Self {
        #[allow(clippy::enum_glob_use)]
        use RpcStatusCode::*;
        match code {
            0 => Ok,
            1 => BadRequest,
            2 => UnsupportedMethod,
            3 => NotImplemented,
            4 => Timeout,
            5 => MalformedResponse,
            6 => General,
            7 => NotFound,
            8 => ProtocolError,
            9 => Forbidden,
            10 => Conflict,
            _ => InvalidRpcStatusCode,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn response(status: RpcStatusCode, payload: Vec<u8>) -> proto::rpc::RpcResponse {
        proto::rpc::RpcResponse {
            request_id: 1,
            status: status as u32,
            flags: 0,
            payload,
        }
    }

    #[test]
    fn peer_details_are_bounded() {
        // Invalid UTF-8 expands to 3 bytes per byte when decoded lossily
        let resp = response(RpcStatusCode::General, vec![0xff; 8 * 1024 * 1024]);
        let status = RpcStatus::from(&resp);
        assert_eq!(status.as_status_code(), RpcStatusCode::General);
        assert!(
            status.details().len() <= 3 * MAX_PEER_DETAILS_BYTES + 64,
            "{}",
            status.details().len()
        );
        assert!(
            status
                .details()
                .ends_with(&format!("(truncated, {} bytes)", 8 * 1024 * 1024))
        );
    }

    #[test]
    fn short_peer_details_are_unchanged() {
        let resp = response(RpcStatusCode::NotFound, b"What does 'x' mean?".to_vec());
        let status = RpcStatus::from(&resp);
        assert_eq!(status.as_status_code(), RpcStatusCode::NotFound);
        assert_eq!(status.details(), "What does 'x' mean?");

        // Exactly at the limit is not truncated
        let resp = response(RpcStatusCode::General, vec![b'a'; MAX_PEER_DETAILS_BYTES]);
        assert_eq!(RpcStatus::from(&resp).details(), "a".repeat(MAX_PEER_DETAILS_BYTES));
    }

    #[test]
    fn control_characters_in_peer_details_are_replaced() {
        let resp = response(RpcStatusCode::General, b"line 1\nFAKE LOG LINE\r\x1b[31m".to_vec());
        assert_eq!(RpcStatus::from(&resp).details(), "line 1 FAKE LOG LINE  [31m");
    }

    #[test]
    fn an_ok_response_is_ok() {
        let resp = response(RpcStatusCode::Ok, vec![0xff; 1024]);
        assert_eq!(RpcStatus::from(&resp), RpcStatus::ok());
    }

    #[test]
    fn rpc_status_code_conversions() {
        #[allow(clippy::enum_glob_use)]
        use RpcStatusCode::*;
        assert_eq!(RpcStatusCode::from(Ok as u32), Ok);
        assert_eq!(RpcStatusCode::from(BadRequest as u32), BadRequest);
        assert_eq!(RpcStatusCode::from(UnsupportedMethod as u32), UnsupportedMethod);
        assert_eq!(RpcStatusCode::from(General as u32), General);
        assert_eq!(RpcStatusCode::from(NotImplemented as u32), NotImplemented);
        assert_eq!(RpcStatusCode::from(MalformedResponse as u32), MalformedResponse);
        assert_eq!(RpcStatusCode::from(Timeout as u32), Timeout);
        assert_eq!(RpcStatusCode::from(NotFound as u32), NotFound);
        assert_eq!(RpcStatusCode::from(InvalidRpcStatusCode as u32), InvalidRpcStatusCode);
        assert_eq!(RpcStatusCode::from(ProtocolError as u32), ProtocolError);
        assert_eq!(RpcStatusCode::from(Forbidden as u32), Forbidden);
        assert_eq!(RpcStatusCode::from(Conflict as u32), Conflict);
        assert_eq!(RpcStatusCode::from(123), InvalidRpcStatusCode);
    }

    #[test]
    fn rpc_status_or_optional() {
        assert!(Result::<(), RpcStatus>::Ok(()).or_optional().is_ok());
        assert_eq!(
            Result::<(), _>::Err(RpcStatus::not_found("foo")).or_optional(),
            Ok(None)
        );
        assert_eq!(
            Result::<(), _>::Err(RpcStatus::general("foo")).or_optional(),
            Err(RpcStatus::general("foo"))
        );
    }
}
