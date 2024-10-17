//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, fmt::Display};

use log::*;
use thiserror::Error;

use super::RpcError;
use crate::{optional::OrOptional, proto};

const LOG_TARGET: &str = "comms::rpc::status";

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

    pub fn unsupported_method<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::UnsupportedMethod,
            details: details.into(),
        }
    }

    pub fn not_implemented<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::NotImplemented,
            details: details.into(),
        }
    }

    pub fn bad_request<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::BadRequest,
            details: details.into(),
        }
    }

    /// Returns a general error. As with all other errors care should be taken not to leak sensitive data to remote
    /// peers through error messages.
    pub fn general<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::General,
            details: details.into(),
        }
    }

    pub fn general_default() -> Self {
        Self::general("General error")
    }

    pub fn timed_out<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::Timeout,
            details: details.into(),
        }
    }

    pub fn not_found<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::NotFound,
            details: details.into(),
        }
    }

    pub fn forbidden<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::Forbidden,
            details: details.into(),
        }
    }

    pub fn conflict<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::Conflict,
            details: details.into(),
        }
    }

    /// Returns a closure that logs the given error and returns a generic general error that does not leak any
    /// potentially sensitive error information. Use this function with map_err to catch "miscellaneous" errors.
    pub fn log_internal_error<'a, E: std::error::Error + 'a>(target: &'a str) -> impl Fn(E) -> Self + 'a {
        move |err| {
            log::error!(target: target, "Internal error: {}", err);
            Self::general_default()
        }
    }

    pub(super) fn protocol_error<T: Into<String>>(details: T) -> Self {
        Self {
            code: RpcStatusCode::ProtocolError,
            details: details.into(),
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
        write!(f, "{:?}: {}", self.code, &self.details)
    }
}

impl From<RpcError> for RpcStatus {
    fn from(err: RpcError) -> Self {
        match err {
            RpcError::DecodeError(_) => Self::bad_request("Failed to decode request"),
            RpcError::RequestFailed(status) => status,
            err => {
                error!(target: LOG_TARGET, "Internal error: {}", err);
                Self::general(err.to_string())
            },
        }
    }
}

impl<'a> From<&'a proto::RpcResponse> for RpcStatus {
    fn from(resp: &'a proto::RpcResponse) -> Self {
        let status_code = RpcStatusCode::from(resp.status);
        if status_code.is_ok() {
            return RpcStatus::ok();
        }

        RpcStatus {
            code: status_code,
            details: String::from_utf8_lossy(&resp.payload).to_string(),
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
    fn rpc_status_not_found<S: Into<String>>(self, message: S) -> Result<T, RpcStatus>;
    fn rpc_status_bad_request<S: Into<String>>(self, message: S) -> Result<T, RpcStatus>;
}

impl<T, E: std::error::Error> RpcStatusResultExt<T> for Result<T, E> {
    fn rpc_status_internal_error(self, target: &str) -> Result<T, RpcStatus> {
        self.map_err(RpcStatus::log_internal_error(target))
    }

    fn rpc_status_not_found<S: Into<String>>(self, message: S) -> Result<T, RpcStatus> {
        self.map_err(|_| RpcStatus::not_found(message))
    }

    fn rpc_status_bad_request<S: Into<String>>(self, message: S) -> Result<T, RpcStatus> {
        self.map_err(|_| RpcStatus::bad_request(message))
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
    /// RPC handshake denied
    HandshakeDenied = 11,
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

    pub fn is_handshake_denied(self) -> bool {
        self == Self::HandshakeDenied
    }

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }

    pub fn to_debug_string(&self) -> String {
        format!("{:?}", self)
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
            11 => HandshakeDenied,
            _ => InvalidRpcStatusCode,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
