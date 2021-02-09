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
use crate::proto;
use log::*;
use std::{fmt, fmt::Display};
use thiserror::Error;

const LOG_TARGET: &str = "comms::rpc::status";

#[derive(Debug, Error, Clone)]
pub struct RpcStatus {
    code: RpcStatusCode,
    details: String,
}

impl RpcStatus {
    pub fn ok() -> Self {
        RpcStatus {
            code: RpcStatusCode::Ok,
            details: Default::default(),
        }
    }

    pub fn unsupported_method<T: ToString>(details: T) -> Self {
        RpcStatus {
            code: RpcStatusCode::UnsupportedMethod,
            details: details.to_string(),
        }
    }

    pub fn not_implemented<T: ToString>(details: T) -> Self {
        RpcStatus {
            code: RpcStatusCode::NotImplemented,
            details: details.to_string(),
        }
    }

    pub fn bad_request<T: ToString>(details: T) -> Self {
        Self {
            code: RpcStatusCode::BadRequest,
            details: details.to_string(),
        }
    }

    pub fn general<T: ToString>(details: T) -> Self {
        Self {
            code: RpcStatusCode::General,
            details: details.to_string(),
        }
    }

    pub fn general_default() -> Self {
        Self::general("General error")
    }

    pub fn timed_out<T: ToString>(details: T) -> Self {
        Self {
            code: RpcStatusCode::Timeout,
            details: details.to_string(),
        }
    }

    pub fn not_found<T: ToString>(details: T) -> Self {
        Self {
            code: RpcStatusCode::NotFound,
            details: details.to_string(),
        }
    }

    pub fn log_internal_error<'a, E: std::error::Error + 'a>(target: &'a str) -> impl Fn(E) -> Self + 'a {
        move |err| {
            log::error!(target: target, "Internal error: {}", err);
            Self::general(err.to_string())
        }
    }

    pub fn as_code(&self) -> u32 {
        self.code as u32
    }

    pub fn status_code(&self) -> RpcStatusCode {
        self.code
    }

    pub fn details(&self) -> &str {
        &self.details
    }

    pub fn details_bytes(&self) -> Vec<u8> {
        self.details.as_bytes().to_vec()
    }

    pub fn is_ok(&self) -> bool {
        self.code.is_ok()
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

impl<'a> From<&'a proto::rpc::RpcResponse> for RpcStatus {
    fn from(resp: &'a proto::rpc::RpcResponse) -> Self {
        let status_code = RpcStatusCode::from(resp.status);
        if status_code.is_ok() {
            return RpcStatus::ok();
        }

        RpcStatus {
            code: status_code,
            details: String::from_utf8_lossy(&resp.message).to_string(),
        }
    }
}

impl From<prost::DecodeError> for RpcStatus {
    fn from(_: prost::DecodeError) -> Self {
        Self::bad_request("Failed to decode request")
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
}

impl From<u32> for RpcStatusCode {
    fn from(code: u32) -> Self {
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
            _ => InvalidRpcStatusCode,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn rpc_status_code_conversions() {
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
        assert_eq!(RpcStatusCode::from(123), InvalidRpcStatusCode);
    }
}
