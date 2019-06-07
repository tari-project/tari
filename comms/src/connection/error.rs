//  Copyright 2019 The Tari Project
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

use derive_error::Error;

use super::{monitor, NetAddressError, PeerConnectionError};

#[derive(Debug, Error)]
pub enum ConnectionError {
    NetAddressError(NetAddressError),
    #[error(msg_embedded, no_from, non_std)]
    SocketError(String),
    /// Connection timed out
    Timeout,
    #[error(msg_embedded, no_from, non_std)]
    CurveKeypairError(String),
    PeerError(PeerConnectionError),
    MonitorError(monitor::ConnectionMonitorError),
    #[error(msg_embedded, no_from, non_std)]
    InvalidOperation(String),
}

impl ConnectionError {
    /// Returns true if the error is a Timeout error, otherwise false
    pub fn is_timeout(&self) -> bool {
        match *self {
            ConnectionError::Timeout => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn is_timeout() {
        let err = ConnectionError::Timeout;
        assert!(err.is_timeout());

        let err = ConnectionError::SocketError("dummy error".to_string());
        assert!(!err.is_timeout());
    }
}
