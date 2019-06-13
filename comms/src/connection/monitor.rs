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

use crate::{
    connection::{
        types::{Result, SocketType},
        zmq::ZmqEndpoint,
        ConnectionError,
        InprocAddress,
        ZmqContext,
    },
    message::FrameSet,
};
use derive_error::Error;
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

#[derive(Debug, Error, PartialEq)]
pub enum ConnectionMonitorError {
    #[error(msg_embedded, non_std, no_from)]
    CreateSocketFailed(String),
    /// Failed to convert integer type to SocketEvent
    SocketEventConversionFailed,
    #[error(msg_embedded, non_std, no_from)]
    ConnectionFailed(String),
    /// Received incorrect number of frames
    IncorrectFrameCount,
}

/// ConnectionMonitor is the read side of the ZMQ_PAIR socket.
/// It allows SocketEvents to be read from the given InprocAddress.
///
/// More details here: http://api.zeromq.org/4-1:zmq-socket-monitor
///
/// ```edition2018
/// # use tari_comms::connection::{ZmqContext, monitor::ConnectionMonitor, Connection, Direction, InprocAddress, NetAddress};
///
/// let ctx = ZmqContext::new();
/// let monitor_addr = InprocAddress::random();
/// let address = "127.0.0.1:9999".parse::<NetAddress>().unwrap();
///
/// // Monitor MUST start before the connection is established
/// let monitor = ConnectionMonitor::connect(&ctx, &monitor_addr).unwrap();
///
/// {
///     Connection::new(&ctx, Direction::Inbound)
///             .set_monitor_addr(monitor_addr)
///             .establish(&address)
///             .unwrap();
/// }
///
/// // Read events
/// while let Ok(event) = monitor.read(100) {
///     println!("Got event: {:?}", event);
/// }
/// ```
pub struct ConnectionMonitor {
    socket: zmq::Socket,
}

impl ConnectionMonitor {
    /// Create a new connected ConnectionMonitor.
    ///
    /// ## Arguments
    /// `context` - Connection context. Must be the same context as the connection being monitored
    /// `address` - The inproc address from which to read socket events
    pub fn connect(context: &ZmqContext, address: &InprocAddress) -> Result<Self> {
        let socket = context.socket(SocketType::Pair).map_err(|e| {
            ConnectionError::MonitorError(ConnectionMonitorError::CreateSocketFailed(format!(
                "Failed to create monitor pair socket: {}",
                e
            )))
        })?;

        socket.connect(&address.to_zmq_endpoint()).map_err(|e| {
            ConnectionError::MonitorError(ConnectionMonitorError::ConnectionFailed(format!(
                "Failed to connect: {}",
                e
            )))
        })?;

        Ok(Self { socket })
    }

    /// Read a SocketEvent within the given timeout.
    /// If the timeout is reached a `Err(ConnectionError::Timeout)` is returned.
    ///
    /// ## Arguments
    /// `timeout_ms` - The timeout to wait in milliseconds
    pub fn read(&self, timeout_ms: u32) -> Result<SocketEvent> {
        let frames = self.read_frames(timeout_ms)?;

        if frames.len() != 2 {
            return Err(ConnectionMonitorError::IncorrectFrameCount.into());
        }

        macro_rules! transmute_value {
            ($data: expr, $start: expr, $end: expr, $type: ty) => {
                unsafe {
                    let mut a: [u8; $end - $start] = Default::default();
                    a.copy_from_slice(&$data[$start..$end]);
                    std::mem::transmute::<[u8; $end - $start], $type>(a).to_le()
                }
            };
        }

        // First 2 bytes are the event type
        let event_type: SocketEventType = transmute_value!(frames[0], 0, 2, u16).try_into()?;
        // Next 4 bytes are the event value
        let event_value = transmute_value!(frames[0], 2, 6, u32);

        let address = String::from_utf8_lossy(&frames[1]).into_owned();

        Ok(SocketEvent {
            event_type,
            event_value,
            address,
        })
    }

    fn read_frames(&self, timeout_ms: u32) -> Result<FrameSet> {
        match self.socket.poll(zmq::POLLIN, timeout_ms as i64) {
            Ok(rc) => {
                match rc {
                    // Internal error when polling connection
                    -1 => Err(ConnectionError::SocketError("Failed to poll socket".to_string())),
                    // Nothing to receive
                    0 => Err(ConnectionError::Timeout),
                    // Ready to receive
                    _ => self
                        .socket
                        .recv_multipart(0)
                        .map_err(|e| ConnectionError::SocketError(format!("Error receiving: {} ({})", e, e.to_raw()))),
                }
            },

            Err(e) => Err(ConnectionError::SocketError(format!("Failed to poll: {}", e))),
        }
    }
}

/// Represents an event for a socket
#[derive(Debug)]
pub struct SocketEvent {
    /// The type of event received
    pub event_type: SocketEventType,
    /// The value of the event. This value depends on the event received.
    /// Usually nothing (zero) or a file descriptor for the monitored socket.
    pub event_value: u32,
    /// The address of the connection which triggered this event
    pub address: String,
}

/// Represents the types of socket events which can occur.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum SocketEventType {
    Connected = 0x0001,
    ConnectDelayed = 0x0002,
    ConnectRetried = 0x0004,
    Listening = 0x0008,
    BindFailed = 0x0010,
    Accepted = 0x0020,
    AcceptFailed = 0x0040,
    Closed = 0x0080,
    CloseFailed = 0x0100,
    Disconnected = 0x0200,
    MonitorStopped = 0x0400,
    HandshakeFailedNoDetail = 0x0800,
    HandshakeSucceeded = 0x1000,
    HandshakeFailedProtocol = 0x2000,
    HandshakeFailedAuth = 0x4000,
}

impl fmt::Display for SocketEventType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &self)
    }
}

impl TryFrom<u16> for SocketEventType {
    type Error = ConnectionError;

    /// Try to convert from a u16 to a SocketEventType
    fn try_from(raw: u16) -> Result<SocketEventType> {
        let event = match raw {
            0x0001 => SocketEventType::Connected,
            0x0002 => SocketEventType::ConnectDelayed,
            0x0004 => SocketEventType::ConnectRetried,
            0x0008 => SocketEventType::Listening,
            0x0010 => SocketEventType::BindFailed,
            0x0020 => SocketEventType::Accepted,
            0x0040 => SocketEventType::AcceptFailed,
            0x0080 => SocketEventType::Closed,
            0x0100 => SocketEventType::CloseFailed,
            0x0200 => SocketEventType::Disconnected,
            0x0400 => SocketEventType::MonitorStopped,
            0x0800 => SocketEventType::HandshakeFailedNoDetail,
            0x1000 => SocketEventType::HandshakeSucceeded,
            0x2000 => SocketEventType::HandshakeFailedProtocol,
            0x4000 => SocketEventType::HandshakeFailedAuth,
            _ => return Err(ConnectionMonitorError::SocketEventConversionFailed.into()),
        };

        Ok(event)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn socket_event_type_try_from() {
        let valid_event_types = vec![
            0x0001, 0x0002, 0x0004, 0x0008, 0x0010, 0x0020, 0x0040, 0x0080, 0x0100, 0x0200, 0x0400, 0x0800, 0x1000,
            0x2000, 0x4000,
        ];

        for raw_evt in valid_event_types {
            let evt_type: Result<SocketEventType> = raw_evt.try_into();
            assert!(evt_type.is_ok());
        }

        let invalid: Result<SocketEventType> = 0xF000u16.try_into();
        assert!(invalid.is_err());
    }
}
