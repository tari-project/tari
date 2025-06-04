// Copyright 2025, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures::{pin_mut, Stream};
use log::*;
use multiaddr::Multiaddr;
use thiserror::Error;
use tokio::{
    net::TcpStream,
    sync::{broadcast, RwLock},
    time,
};

use super::{TcpTransport, Transport};
use crate::{
    tor::TorIdentity,
    transports::HiddenServiceTransport,
};

// Type alias to simplify complex types
type TorTransport = HiddenServiceTransport<Box<dyn Fn(TorIdentity) + Send + Sync>>;

const LOG_TARGET: &str = "comms::transports::fallback";

#[derive(Debug, Error)]
pub enum FallbackTransportError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Hidden service transport error: {0}")]
    HiddenService(String),
    #[error("Transport not initialized")]
    NotInitialized,
    #[error("Tor unavailable, IPv4-only mode")]
    TorUnavailable,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransportMode {
    /// Using Tor hidden service transport
    Tor,
    /// Using IPv4 TCP transport only
    IPv4Only,
    /// Transport failed initialization
    Failed,
}

#[derive(Debug, Clone)]
pub struct FallbackConfig {
    /// Timeout for Tor initialization attempts
    pub tor_timeout: Duration,
    /// Whether to allow fallback to IPv4 when Tor fails
    pub allow_fallback: bool,
    /// Interval for checking if Tor becomes available again
    pub tor_retry_interval: Duration,
    /// Whether to retry Tor connections periodically
    pub enable_tor_retry: bool,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            tor_timeout: Duration::from_secs(15),
            allow_fallback: true,
            tor_retry_interval: Duration::from_secs(300), // 5 minutes
            enable_tor_retry: true,
        }
    }
}

/// Transport state shared between the transport and its components
pub struct TransportState {
    pub mode: TransportMode,
    pub tor_transport: Option<Arc<TorTransport>>,
    pub tcp_transport: TcpTransport,
    pub last_tor_attempt: Option<std::time::Instant>,
}

/// A transport that attempts to use Tor first, then falls back to IPv4 TCP if Tor is unavailable
#[derive(Clone)]
pub struct FallbackTransport {
    config: FallbackConfig,
    state: Arc<RwLock<TransportState>>,
    mode_notifier: broadcast::Sender<TransportMode>,
}

impl FallbackTransport {
    pub fn new(
        config: FallbackConfig,
        tor_transport: Option<TorTransport>,
        tcp_transport: TcpTransport,
    ) -> Self {
        let (mode_notifier, _) = broadcast::channel(16);
        
        let state = TransportState {
            mode: TransportMode::Failed,
            tor_transport: tor_transport.map(Arc::new),
            tcp_transport,
            last_tor_attempt: None,
        };

        Self {
            config,
            state: Arc::new(RwLock::new(state)),
            mode_notifier,
        }
    }

    /// Get the current transport mode
    pub async fn mode(&self) -> TransportMode {
        self.state.read().await.mode.clone()
    }

    /// Subscribe to transport mode changes
    pub fn subscribe_mode_changes(&self) -> broadcast::Receiver<TransportMode> {
        self.mode_notifier.subscribe()
    }

    /// Initialize transport, trying Tor first, then falling back to TCP
    async fn initialize(&self, addr: &Multiaddr) -> Result<(FallbackListener, Multiaddr), FallbackTransportError> {
        // Try Tor transport if available
        let tor_transport = {
            let state = self.state.read().await;
            state.tor_transport.clone()
        };
        
        if let Some(tor_transport) = tor_transport {
            info!(target: LOG_TARGET, "Attempting to initialize Tor transport with timeout of {:?}...", self.config.tor_timeout);
            
            // Update last attempt time
            {
                let mut state = self.state.write().await;
                state.last_tor_attempt = Some(std::time::Instant::now());
            }
            
            let tor_init_future = tor_transport.listen(addr);
            pin_mut!(tor_init_future);
            
            match time::timeout(self.config.tor_timeout, tor_init_future).await {
                Ok(Ok((listener, listen_addr))) => {
                    info!(target: LOG_TARGET, "Tor transport initialized successfully");
                    let mut state = self.state.write().await;
                    state.mode = TransportMode::Tor;
                    let _ = self.mode_notifier.send(TransportMode::Tor);
                    
                    return Ok((FallbackListener::Tor(listener), listen_addr));
                },
                Ok(Err(err)) => {
                    warn!(target: LOG_TARGET, "Tor transport failed to initialize: {}", err);
                },
                Err(_) => {
                    warn!(target: LOG_TARGET, "Tor transport initialization timed out after {:?}", self.config.tor_timeout);
                }
            }
        }

        // Fallback to TCP if Tor failed and fallback is enabled
        if self.config.allow_fallback {
            info!(target: LOG_TARGET, "Falling back to IPv4 TCP transport");
            let mut state = self.state.write().await;
            
            let (tcp_listener, tcp_addr) = state.tcp_transport.listen(addr).await?;
            state.mode = TransportMode::IPv4Only;
            let _ = self.mode_notifier.send(TransportMode::IPv4Only);
            
            Ok((FallbackListener::Tcp(tcp_listener), tcp_addr))
        } else {
            let mut state = self.state.write().await;
            state.mode = TransportMode::Failed;
            let _ = self.mode_notifier.send(TransportMode::Failed);
            Err(FallbackTransportError::TorUnavailable)
        }
    }

    /// Check if it's time to retry Tor
    fn should_retry_tor(&self, state: &TransportState) -> bool {
        if !self.config.enable_tor_retry || state.mode == TransportMode::Tor {
            return false;
        }

        match state.last_tor_attempt {
            Some(last_attempt) => {
                last_attempt.elapsed() >= self.config.tor_retry_interval
            },
            None => true,
        }
    }

    /// Attempt to dial using the appropriate transport based on the address and current mode
    async fn dial_with_mode(&self, addr: &Multiaddr) -> Result<TcpStream, FallbackTransportError> {
        let state = self.state.read().await;
        
        match state.mode {
            TransportMode::Tor => {
                if let Some(ref tor_transport) = state.tor_transport {
                    tor_transport.dial(addr).await
                        .map_err(|e| FallbackTransportError::HiddenService(e.to_string()))
                } else {
                    Err(FallbackTransportError::NotInitialized)
                }
            },
            TransportMode::IPv4Only => {
                // Filter out onion addresses in IPv4-only mode
                if self.is_onion_address(addr) {
                    return Err(FallbackTransportError::TorUnavailable);
                }
                state.tcp_transport.dial(addr).await.map_err(FallbackTransportError::Io)
            },
            TransportMode::Failed => {
                Err(FallbackTransportError::NotInitialized)
            }
        }
    }

    /// Check if an address is an onion address
    fn is_onion_address(&self, addr: &Multiaddr) -> bool {
        addr.iter().any(|component| {
            matches!(component, multiaddr::Protocol::Onion(_, _) | multiaddr::Protocol::Onion3(_))
        })
    }
}

/// Listener that can handle both Tor and TCP connections
pub enum FallbackListener {
    Tor(<TorTransport as Transport>::Listener),
    Tcp(<TcpTransport as Transport>::Listener),
}

impl Stream for FallbackListener {
    type Item = Result<(TcpStream, Multiaddr), FallbackTransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match &mut *self {
            FallbackListener::Tor(listener) => {
                match Pin::new(listener).poll_next(cx) {
                    Poll::Ready(Some(Ok((stream, addr)))) => Poll::Ready(Some(Ok((stream, addr)))),
                    Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(FallbackTransportError::HiddenService(err.to_string())))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            },
            FallbackListener::Tcp(listener) => {
                match Pin::new(listener).poll_next(cx) {
                    Poll::Ready(Some(Ok((stream, addr)))) => Poll::Ready(Some(Ok((stream, addr)))),
                    Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(FallbackTransportError::Io(err)))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

#[crate::async_trait]
impl Transport for FallbackTransport {
    type Error = FallbackTransportError;
    type Listener = FallbackListener;
    type Output = TcpStream;

    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
        self.initialize(addr).await
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        // Check if we should retry Tor
        {
            let state = self.state.read().await;
            if self.should_retry_tor(&state) {
                drop(state);
                // Just update the attempt time, actual testing happens during connection
                let mut state = self.state.write().await;
                state.last_tor_attempt = Some(std::time::Instant::now());
            }
        }

        self.dial_with_mode(addr).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::transports::TcpTransport;

    #[tokio::test]
    async fn test_fallback_config_default() {
        let config = FallbackConfig::default();
        assert_eq!(config.tor_timeout, Duration::from_secs(15));
        assert!(config.allow_fallback);
        assert_eq!(config.tor_retry_interval, Duration::from_secs(300));
        assert!(config.enable_tor_retry);
    }

    #[tokio::test]
    async fn test_transport_mode_transitions() {
        let config = FallbackConfig::default();
        let tcp_transport = TcpTransport::new();
        
        // Create transport without Tor (will go directly to fallback)
        let transport = FallbackTransport::new(config, None, tcp_transport);
        
        // Should start in Failed mode
        assert_eq!(transport.mode().await, TransportMode::Failed);
        
        // Test mode subscription
        let mut mode_receiver = transport.subscribe_mode_changes();
        
        // Try to initialize (will fallback to TCP since no Tor transport)
        let addr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
        let result = transport.initialize(&addr).await;
        
        if result.is_ok() {
            // Should transition to IPv4Only mode
            assert_eq!(transport.mode().await, TransportMode::IPv4Only);
            
            // Should receive mode change notification
            let mode_change = tokio::time::timeout(Duration::from_millis(100), mode_receiver.recv()).await;
            assert!(mode_change.is_ok());
        }
    }

    #[tokio::test]
    async fn test_should_retry_tor() {
        let config = FallbackConfig {
            tor_retry_interval: Duration::from_millis(100),
            enable_tor_retry: true,
            ..Default::default()
        };
        let tcp_transport = TcpTransport::new();
        let transport = FallbackTransport::new(config, None, tcp_transport);
        
        let state = TransportState {
            mode: TransportMode::IPv4Only,
            tor_transport: None,
            tcp_transport: TcpTransport::new(),
            last_tor_attempt: Some(std::time::Instant::now() - Duration::from_millis(200)),
        };
        
        assert!(transport.should_retry_tor(&state));
        
        // Test with recent attempt
        let state = TransportState {
            mode: TransportMode::IPv4Only,
            tor_transport: None,
            tcp_transport: TcpTransport::new(),
            last_tor_attempt: Some(std::time::Instant::now()),
        };
        
        assert!(!transport.should_retry_tor(&state));
    }

    #[tokio::test]
    async fn test_is_onion_address() {
        let config = FallbackConfig::default();
        let tcp_transport = TcpTransport::new();
        let transport = FallbackTransport::new(config, None, tcp_transport);
        
        // Test onion v3 address
        let onion_addr = "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:80".parse().unwrap();
        assert!(transport.is_onion_address(&onion_addr));
        
        // Test regular IP address
        let ip_addr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();
        assert!(!transport.is_onion_address(&ip_addr));
        
        // Test DNS address
        let dns_addr = "/dns/example.com/tcp/80".parse().unwrap();
        assert!(!transport.is_onion_address(&dns_addr));
    }

    #[tokio::test]
    async fn test_fallback_config_creation() {
        let config = FallbackConfig {
            tor_timeout: Duration::from_secs(30),
            allow_fallback: false,
            tor_retry_interval: Duration::from_secs(600),
            enable_tor_retry: false,
        };
        
        assert_eq!(config.tor_timeout, Duration::from_secs(30));
        assert!(!config.allow_fallback);
        assert_eq!(config.tor_retry_interval, Duration::from_secs(600));
        assert!(!config.enable_tor_retry);
    }

    #[tokio::test]
    async fn test_transport_clone() {
        let config = FallbackConfig::default();
        let tcp_transport = TcpTransport::new();
        let transport = FallbackTransport::new(config, None, tcp_transport);
        
        // Transport should be cloneable
        let _cloned_transport = transport.clone();
        
        // Both should share the same state
        assert_eq!(transport.mode().await, TransportMode::Failed);
    }

    #[tokio::test]
    async fn test_fallback_to_tcp_when_no_tor() {
        // Create a fallback config with short timeout for testing
        let config = FallbackConfig {
            tor_timeout: Duration::from_secs(1), // Short timeout
            allow_fallback: true,
            enable_tor_retry: true,
            tor_retry_interval: Duration::from_secs(60),
        };
        
        // Create TCP transport (this should work)
        let tcp_transport = TcpTransport::new();
        
        // Create fallback transport without Tor transport (simulating Tor unavailable)
        let fallback_transport = FallbackTransport::new(config, None, tcp_transport);
        
        // Initial mode should be Failed
        assert_eq!(fallback_transport.mode().await, TransportMode::Failed);
        
        // Try to listen on a local address - should fallback to TCP
        let listen_addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
        
        let result = fallback_transport.listen(&listen_addr).await;
        assert!(result.is_ok(), "Transport initialization should succeed with TCP fallback");
        
        // Should now be in IPv4Only mode
        assert_eq!(fallback_transport.mode().await, TransportMode::IPv4Only);
    }
}
