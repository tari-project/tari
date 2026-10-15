// Copyright 2020, The Tari Project
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
    collections::HashSet,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::{SinkExt, StreamExt, future};
use log::*;
use multiaddr::{Multiaddr, Protocol};
use rand::Rng;
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    sync::watch,
    time,
    time::Instant,
};
use tokio_util::codec::{Framed, LinesCodec, LinesCodecError};

use crate::{connection_manager::wire_mode::WireMode, transports::Transport};

/// Max line length accepted by the liveness session. Fits the 64 hex characters of a self-check token.
const MAX_LINE_LENGTH: usize = 80;
/// How long an admitted liveness session may go without receiving a line before it is closed.
const LIVENESS_SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// The longest an admitted liveness session may stay open, whatever it sends.
const LIVENESS_SESSION_MAX_LIFETIME: Duration = Duration::from_secs(5 * 60);
/// How long the listener waits for the first line of a liveness connection.
pub(crate) const LIVENESS_FIRST_LINE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the self-check waits for its token to be echoed back.
const SELF_CHECK_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long opening a self-check session may take in total: the dial, the wire byte and the token exchange. Expiry is
/// treated like a dial failure.
const SELF_CHECK_DIAL_TIMEOUT: Duration = Duration::from_secs(30);
/// The same bound for an onion address. Dialling our own onion service through Tor can be slow (cold descriptor,
/// slow rendezvous) and the SOCKS transport has no dial timeout of its own.
const SELF_CHECK_ONION_DIAL_TIMEOUT: Duration = Duration::from_secs(120);

/// Returns the bound on opening a self-check session to `address`.
fn self_check_dial_timeout(address: &Multiaddr) -> Duration {
    for protocol in address {
        if matches!(protocol, Protocol::Onion(..) | Protocol::Onion3(_)) {
            return SELF_CHECK_ONION_DIAL_TIMEOUT;
        }
    }
    SELF_CHECK_DIAL_TIMEOUT
}
/// The self-check runs one dial at a time, so it holds at most one token. This is a defensive bound.
const MAX_LIVENESS_TOKENS: usize = 8;
const LOG_TARGET: &str = "comms::connection_manager::self_liveness";

/// One-time tokens issued by this node's self-check. The listener admits a liveness connection whose first line is
/// one of these as a self-check session without consulting the allowlist or the session limit, because only this node
/// could know the token.
pub type LivenessTokens = Arc<Mutex<HashSet<[u8; 32]>>>;

/// Removes the token in `line` from `tokens`. Returns true if `line` was a registered token.
pub(crate) fn take_liveness_token(tokens: &LivenessTokens, line: &str) -> bool {
    let bytes = match hex::from_hex(line) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let token: [u8; 32] = match bytes.try_into() {
        Ok(token) => token,
        Err(_) => return false,
    };
    let mut tokens = tokens.lock().expect("liveness token lock poisoned");
    tokens.remove(&token)
}

/// Echo server for liveness checks
pub struct SelfLivenessSession<TSocket> {
    framed: Framed<TSocket, LinesCodec>,
}

impl<TSocket> SelfLivenessSession<TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    pub fn new(socket: TSocket) -> Self {
        Self {
            framed: Framed::new(socket, LinesCodec::new_with_max_length(MAX_LINE_LENGTH)),
        }
    }

    /// Reads one line. Returns None if the line does not arrive within `timeout`, the stream ends or the line is
    /// invalid.
    pub async fn read_line(&mut self, timeout: Duration) -> Option<String> {
        match time::timeout(timeout, self.framed.next()).await {
            Ok(Some(Ok(line))) => Some(line),
            Ok(Some(Err(err))) => {
                debug!(target: LOG_TARGET, "Liveness session received an invalid line: {err}");
                None
            },
            Ok(None) => None,
            Err(_elapsed) => {
                debug!(target: LOG_TARGET, "Liveness session did not send a line within {timeout:.2?}");
                None
            },
        }
    }

    /// Sends one line.
    pub async fn send_line(&mut self, line: String) -> Result<(), LinesCodecError> {
        self.framed.send(line).await
    }

    /// Echoes every line back until the peer closes, the session is idle for longer than the idle timeout, or the
    /// session reaches its maximum lifetime.
    pub async fn run(mut self) -> Result<(), LinesCodecError> {
        match time::timeout(LIVENESS_SESSION_MAX_LIFETIME, self.echo_until_idle()).await {
            Ok(result) => result,
            Err(_elapsed) => {
                debug!(
                    target: LOG_TARGET,
                    "Liveness session reached its maximum lifetime of {LIVENESS_SESSION_MAX_LIFETIME:.2?}"
                );
                Ok(())
            },
        }
    }

    async fn echo_until_idle(&mut self) -> Result<(), LinesCodecError> {
        loop {
            let line = match time::timeout(LIVENESS_SESSION_IDLE_TIMEOUT, self.framed.next()).await {
                Ok(Some(line)) => line?,
                Ok(None) => return Ok(()),
                Err(_elapsed) => {
                    debug!(
                        target: LOG_TARGET,
                        "Liveness session idle for {LIVENESS_SESSION_IDLE_TIMEOUT:.2?}, closing"
                    );
                    return Ok(());
                },
            };
            self.framed.send(line).await?;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SelfLivenessStatus {
    Disabled,
    Checking,
    Unreachable,
    Live(Duration),
}

#[derive(Debug, Error)]
enum SelfCheckError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Codec error: {0}")]
    Codec(#[from] LinesCodecError),
    #[error("No response within {0:.2?}")]
    Timeout(Duration),
    #[error("Connection closed before the token was echoed")]
    Closed,
    #[error("Echoed line did not match the line sent")]
    EchoMismatch,
    #[error("Too many outstanding liveness tokens")]
    TooManyTokens,
    #[error("Failed to dial: {0}")]
    Dial(String),
    #[error("Opening the session took longer than {0:.2?}")]
    DialTimeout(Duration),
}

/// An open self-check connection: the listener admitted it by token and echoes every line sent on it.
struct SelfCheckSession<TSocket> {
    framed: Framed<TSocket, LinesCodec>,
    opened_at: Instant,
    last_echo_at: Instant,
}

impl<TSocket> SelfCheckSession<TSocket> {
    /// True if the listener will have closed (or is about to close) the session because of its idle timeout or
    /// maximum lifetime. Allows one response timeout of margin so a check is not sent into a closing session.
    fn is_expired(&self) -> bool {
        let idle_limit = LIVENESS_SESSION_IDLE_TIMEOUT.saturating_sub(SELF_CHECK_RESPONSE_TIMEOUT);
        let lifetime_limit = LIVENESS_SESSION_MAX_LIFETIME.saturating_sub(SELF_CHECK_RESPONSE_TIMEOUT);
        self.last_echo_at.elapsed() >= idle_limit || self.opened_at.elapsed() >= lifetime_limit
    }
}

/// Periodically checks that this node is reachable on its own public addresses.
///
/// The check dials a public address, writes the liveness wire byte and sends a fresh one-time token as the first line.
/// The listener recognises the token and keeps echoing on that connection, so every later check sends a random line
/// on the same connection and requires it echoed back exactly. A wrong echo, an error or an unexpected close marks
/// the node unreachable and the next check re-dials with a new token.
///
/// The listener closes liveness sessions after 30s without a line and after 5 minutes in total. The check re-dials
/// with a fresh token, without reporting a failure, once a session is (nearly) past either limit. With an interval of
/// 20s or more every check therefore re-dials.
///
/// Before each (re-)dial the status is set to `Checking`. Opening a session (dial, wire byte and token exchange) is
/// bounded by 30s, or 120s for an onion address because dialling our own onion service through Tor can be slow;
/// expiry counts as a dial failure, so the status becomes `Unreachable` and the next dial tries the next address.
///
/// The listener admits only one self-check session at a time, so the old session is dropped before a new dial. If
/// the listener has not yet seen the old session close (e.g. a lost FIN), the new session is refused and that check
/// reports `Unreachable`; the listener's 30s idle timeout then frees the old session and a later re-dial succeeds.
///
/// Because one session is reused, a listener that has started refusing new inbound connections can still be
/// reported `Live` on the existing session until that session expires (up to about 5 minutes).
pub struct SelfLivenessCheck<TTransport> {
    transport: TTransport,
    addresses: Vec<Multiaddr>,
    interval: Duration,
    tokens: LivenessTokens,
    tx_watch: watch::Sender<SelfLivenessStatus>,
    shutdown_signal: ShutdownSignal,
}

impl<TTransport> SelfLivenessCheck<TTransport>
where
    TTransport: Transport + Send + Sync + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Unpin + Send,
{
    pub fn spawn(
        transport: TTransport,
        addresses: Vec<Multiaddr>,
        interval: Duration,
        tokens: LivenessTokens,
        shutdown_signal: ShutdownSignal,
    ) -> watch::Receiver<SelfLivenessStatus> {
        let (tx_watch, rx_watch) = watch::channel(SelfLivenessStatus::Checking);
        let check = Self {
            transport,
            addresses,
            interval,
            tokens,
            tx_watch,
            shutdown_signal,
        };
        tokio::spawn(check.run_until_shutdown());
        rx_watch
    }

    pub async fn run_until_shutdown(self) {
        let shutdown_signal = self.shutdown_signal.clone();
        let run_fut = self.run();
        tokio::pin!(run_fut);
        future::select(run_fut, shutdown_signal).await;
    }

    pub async fn run(self) {
        if self.addresses.is_empty() {
            warn!(target: LOG_TARGET, "🔌️ No addresses to check");
            return;
        }
        info!(
            target: LOG_TARGET,
            "🔌️ Starting liveness self-check with interval {:.2?}", self.interval
        );
        let mut current_address_idx = 0;
        let mut session: Option<SelfCheckSession<TTransport::Output>> = None;
        loop {
            let mut checked = false;
            if let Some(mut open_session) = session.take() {
                if open_session.is_expired() {
                    debug!(
                        target: LOG_TARGET,
                        "🔌 self liveness session reached the listener's limits, re-dialling"
                    );
                    // Close it before dialling so the listener's single self-check permit is free
                    drop(open_session);
                } else {
                    checked = true;
                    match Self::check_session(&mut open_session).await {
                        Ok(latency) => {
                            debug!(target: LOG_TARGET, "⚡️️ self liveness check latency {latency:.2?}");
                            self.tx_watch.send_replace(SelfLivenessStatus::Live(latency));
                            session = Some(open_session);
                        },
                        Err(err) => {
                            warn!(target: LOG_TARGET, "🔌️ self liveness check failed: {err}");
                            self.tx_watch.send_replace(SelfLivenessStatus::Unreachable);
                        },
                    }
                }
            }

            if !checked {
                session = self.dial_session(&mut current_address_idx).await;
            }
            time::sleep(self.interval).await;
        }
    }

    /// Dials the current address and opens a self-check session with a fresh token, updating the status. On a dial
    /// failure or timeout moves on to the next address.
    async fn dial_session(&self, current_address_idx: &mut usize) -> Option<SelfCheckSession<TTransport::Output>> {
        let address = self
            .addresses
            .get(*current_address_idx)
            .expect("index should exist")
            .clone();
        self.tx_watch.send_replace(SelfLivenessStatus::Checking);

        let mut token = [0u8; 32];
        rand::rng().fill_bytes(&mut token);
        {
            let mut tokens = self.tokens.lock().expect("liveness token lock poisoned");
            if tokens.len() >= MAX_LIVENESS_TOKENS {
                drop(tokens);
                warn!(target: LOG_TARGET, "🔌️ self liveness check failed: {}", SelfCheckError::TooManyTokens);
                self.tx_watch.send_replace(SelfLivenessStatus::Unreachable);
                return None;
            }
            tokens.insert(token);
        }

        // The token is registered with the listener only while the session is being opened (the listener consumes it
        // when it admits the session). It is removed here so that a timeout cannot leave it behind.
        let dial_timeout = self_check_dial_timeout(&address);
        let result = time::timeout(dial_timeout, self.open_session(&address, &token))
            .await
            .unwrap_or(Err(SelfCheckError::DialTimeout(dial_timeout)));
        self.tokens.lock().expect("liveness token lock poisoned").remove(&token);

        match result {
            Ok((session, latency)) => {
                debug!(target: LOG_TARGET, "⚡️️ self liveness check latency {latency:.2?}");
                self.tx_watch.send_replace(SelfLivenessStatus::Live(latency));
                Some(session)
            },
            Err(err @ (SelfCheckError::Dial(_) | SelfCheckError::DialTimeout(_))) => {
                *current_address_idx = current_address_idx
                    .saturating_add(1)
                    .checked_rem(self.addresses.len())
                    .unwrap_or(0);
                self.tx_watch.send_replace(SelfLivenessStatus::Unreachable);
                warn!(
                    target: LOG_TARGET,
                    "🔌️ Failed to open self check session to own public address {address}: {err}"
                );
                None
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "🔌️ self liveness check failed: {err}");
                self.tx_watch.send_replace(SelfLivenessStatus::Unreachable);
                None
            },
        }
    }

    /// Dials `address`, writes the liveness wire byte and sends `token` as the first line, waiting for it to be
    /// echoed back.
    async fn open_session(
        &self,
        address: &Multiaddr,
        token: &[u8; 32],
    ) -> Result<(SelfCheckSession<TTransport::Output>, Duration), SelfCheckError> {
        let timer = Instant::now();
        let mut socket = self
            .transport
            .dial(address)
            .await
            .map_err(|err| SelfCheckError::Dial(err.to_string()))?;
        debug!(
            target: LOG_TARGET,
            "🔌 self liveness dial ({}) took {:.2?}",
            address,
            timer.elapsed()
        );
        socket.write_all(&[WireMode::Liveness.as_byte()]).await?;

        let now = Instant::now();
        let mut session = SelfCheckSession {
            framed: Framed::new(socket, LinesCodec::new_with_max_length(MAX_LINE_LENGTH)),
            opened_at: now,
            last_echo_at: now,
        };
        let latency = Self::echo_line(&mut session, hex::to_hex(token)).await?;
        Ok((session, latency))
    }

    /// Sends a fresh random line on an open session and waits for it to be echoed back.
    async fn check_session(session: &mut SelfCheckSession<TTransport::Output>) -> Result<Duration, SelfCheckError> {
        let mut line = [0u8; 32];
        rand::rng().fill_bytes(&mut line);
        Self::echo_line(session, hex::to_hex(&line)).await
    }

    /// Sends `line` and requires the next line received to be exactly `line`. Returns the round trip time.
    async fn echo_line(
        session: &mut SelfCheckSession<TTransport::Output>,
        line: String,
    ) -> Result<Duration, SelfCheckError> {
        let timer = Instant::now();
        session.framed.send(line.clone()).await?;
        let response = time::timeout(SELF_CHECK_RESPONSE_TIMEOUT, session.framed.next())
            .await
            .map_err(|_| SelfCheckError::Timeout(SELF_CHECK_RESPONSE_TIMEOUT))?;
        let received = match response {
            Some(received) => received?,
            None => return Err(SelfCheckError::Closed),
        };
        trace!(target: LOG_TARGET, "Received: {received}");
        if received != line {
            return Err(SelfCheckError::EchoMismatch);
        }
        session.last_echo_at = Instant::now();
        Ok(timer.elapsed())
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use tokio::io::AsyncReadExt;
    use tokio_stream::StreamExt;

    use super::*;
    use crate::{memsocket::MemorySocket, transports::MemoryTransport};

    /// A transport whose dials never complete. Records the addresses dialled.
    #[derive(Clone, Default)]
    struct HangingTransport {
        dialled: Arc<Mutex<Vec<Multiaddr>>>,
    }

    #[crate::async_trait]
    impl Transport for HangingTransport {
        type Error = io::Error;
        type Listener = futures::stream::Empty<io::Result<(MemorySocket, Multiaddr)>>;
        type Output = MemorySocket;

        async fn listen(&self, _addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
            Err(io::ErrorKind::Unsupported.into())
        }

        async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
            self.dialled.lock().unwrap().push(addr.clone());
            future::pending().await
        }

        fn supported_protocols(&self) -> Vec<crate::types::TransportProtocol> {
            vec![crate::types::TransportProtocol::Memory]
        }
    }

    const ONION_ADDRESS: &str = "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:18189";

    /// A transport that takes `delay` to dial and then connects to `target`, whatever address was dialled.
    #[derive(Clone)]
    struct SlowTransport {
        delay: Duration,
        target: Multiaddr,
    }

    #[crate::async_trait]
    impl Transport for SlowTransport {
        type Error = io::Error;
        type Listener = futures::stream::Empty<io::Result<(MemorySocket, Multiaddr)>>;
        type Output = MemorySocket;

        async fn listen(&self, _addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
            Err(io::ErrorKind::Unsupported.into())
        }

        async fn dial(&self, _addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
            time::sleep(self.delay).await;
            MemoryTransport.dial(&self.target).await
        }

        fn supported_protocols(&self) -> Vec<crate::types::TransportProtocol> {
            vec![crate::types::TransportProtocol::Memory]
        }
    }

    #[test]
    fn onion_addresses_get_a_longer_dial_timeout() {
        assert_eq!(
            self_check_dial_timeout(&ONION_ADDRESS.parse().unwrap()),
            SELF_CHECK_ONION_DIAL_TIMEOUT
        );
        assert_eq!(
            self_check_dial_timeout(&"/ip4/203.0.113.7/tcp/18189".parse().unwrap()),
            SELF_CHECK_DIAL_TIMEOUT
        );
        assert_eq!(
            self_check_dial_timeout(&"/memory/1".parse().unwrap()),
            SELF_CHECK_DIAL_TIMEOUT
        );
    }

    #[tokio::test(start_paused = true)]
    async fn slow_onion_self_dial_is_live() {
        let shutdown = tari_shutdown::Shutdown::new();
        let (mut listener, target) = MemoryTransport.listen(&"/memory/0".parse().unwrap()).await.unwrap();

        // An echo server standing in for our own listener
        tokio::spawn(async move {
            let (mut socket, _addr) = listener.next().await.unwrap().unwrap();
            let mut wire_byte = [0u8; 1];
            socket.read_exact(&mut wire_byte).await.unwrap();
            let mut framed = Framed::new(socket, LinesCodec::new());
            while let Some(Ok(line)) = framed.next().await {
                framed.send(line).await.unwrap();
            }
        });

        // A 60s dial is longer than the clearnet bound but within the onion bound
        let transport = SlowTransport {
            delay: Duration::from_secs(60),
            target,
        };
        let start = Instant::now();
        let mut liveness_watch = SelfLivenessCheck::spawn(
            transport,
            vec![ONION_ADDRESS.parse().unwrap()],
            Duration::from_secs(1),
            LivenessTokens::default(),
            shutdown.to_signal(),
        );
        loop {
            time::timeout(Duration::from_secs(200), liveness_watch.changed())
                .await
                .unwrap()
                .unwrap();
            match *liveness_watch.borrow_and_update() {
                SelfLivenessStatus::Live(_) => break,
                SelfLivenessStatus::Unreachable => panic!("slow onion self-dial was reported unreachable"),
                _ => {},
            }
        }
        assert!(start.elapsed() >= Duration::from_secs(60));
    }

    #[tokio::test(start_paused = true)]
    async fn self_check_dial_that_never_completes_times_out() {
        let shutdown = tari_shutdown::Shutdown::new();
        let transport = HangingTransport::default();
        let tokens = LivenessTokens::default();
        let first: Multiaddr = "/memory/1".parse().unwrap();
        let second: Multiaddr = "/memory/2".parse().unwrap();
        let mut liveness_watch = SelfLivenessCheck::spawn(
            transport.clone(),
            vec![first.clone(), second.clone()],
            Duration::from_secs(1),
            tokens.clone(),
            shutdown.to_signal(),
        );

        // Each hung dial is cut at the dial timeout, reported Unreachable, and the next dial (published as Checking
        // first) moves on to the next address
        let start = Instant::now();
        let mut statuses = Vec::new();
        while statuses.len() < 4 {
            time::timeout(Duration::from_secs(120), liveness_watch.changed())
                .await
                .unwrap()
                .unwrap();
            statuses.push(*liveness_watch.borrow_and_update());
        }
        assert!(matches!(statuses[0], SelfLivenessStatus::Checking));
        assert!(matches!(statuses[1], SelfLivenessStatus::Unreachable));
        assert!(matches!(statuses[2], SelfLivenessStatus::Checking));
        assert!(matches!(statuses[3], SelfLivenessStatus::Unreachable));
        assert!(start.elapsed() >= SELF_CHECK_DIAL_TIMEOUT.saturating_mul(2));
        assert_eq!(*transport.dialled.lock().unwrap(), vec![first, second]);
        // The token registered for each attempt is removed when it times out
        assert!(tokens.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn self_check_wrong_echo_marks_unreachable() {
        let shutdown = tari_shutdown::Shutdown::new();
        let (mut listener, address) = MemoryTransport.listen(&"/memory/0".parse().unwrap()).await.unwrap();

        // A server that echoes the first line (the token) and then answers every check with the wrong line
        tokio::spawn(async move {
            let (mut socket, _addr) = listener.next().await.unwrap().unwrap();
            let mut wire_byte = [0u8; 1];
            socket.read_exact(&mut wire_byte).await.unwrap();
            assert_eq!(wire_byte[0], WireMode::Liveness.as_byte());
            let mut framed = Framed::new(socket, LinesCodec::new());
            let token = framed.next().await.unwrap().unwrap();
            framed.send(token).await.unwrap();
            while let Some(Ok(_line)) = framed.next().await {
                framed.send("not the line you sent".to_string()).await.unwrap();
            }
        });

        let tokens = LivenessTokens::default();
        let mut liveness_watch = SelfLivenessCheck::spawn(
            MemoryTransport,
            vec![address],
            Duration::from_secs(1),
            tokens.clone(),
            shutdown.to_signal(),
        );

        // The token exchange succeeds, then the next check on the same connection gets a wrong echo
        time::timeout(
            Duration::from_secs(10),
            liveness_watch.wait_for(|status| matches!(status, SelfLivenessStatus::Live(_))),
        )
        .await
        .unwrap()
        .unwrap();
        time::timeout(
            Duration::from_secs(10),
            liveness_watch.wait_for(|status| matches!(status, SelfLivenessStatus::Unreachable)),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(tokens.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn echos() {
        let (inbound, outbound) = MemorySocket::new_pair();
        let liveness = SelfLivenessSession::new(inbound);
        let join_handle = tokio::spawn(liveness.run());
        let mut outbound = Framed::new(outbound, LinesCodec::new());
        for _ in 0..10usize {
            outbound.send("ECHO".to_string()).await.unwrap()
        }
        let pings = outbound.take(10).collect::<Vec<_>>().await;
        assert_eq!(pings.len(), 10);
        assert!(pings.iter().all(|a| a.as_ref().unwrap() == "ECHO"));

        time::timeout(Duration::from_secs(1), join_handle)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
