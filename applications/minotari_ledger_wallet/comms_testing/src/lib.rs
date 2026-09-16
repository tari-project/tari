// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Test-harness transport for the Minotari Ledger wallet client.
//!
//! This crate holds the TCP transport that talks to a [Speculos](https://github.com/LedgerHQ/speculos) simulator,
//! and the call to `register_transport` that points the ledger client at it.
//!
//! # Why this is its own crate, in its own workspace
//!
//! `ledger` is a **default** feature of `minotari_console_wallet`. Anything that can redirect the device connection
//! is therefore one feature-unification accident away from shipping to users, if it lives in
//! `minotari_ledger_wallet_comms` behind a flag. So it does not live there. It lives here, and this crate enables
//! `minotari_ledger_wallet_comms/test_transport` - which nothing else does.
//!
//! That is not sufficient on its own. Cargo resolves features **once per package across the selected package set**,
//! so while this crate was a member of the repository root workspace it landed in the default package selection and
//! unified `test_transport` into the single `minotari_ledger_wallet_comms` rlib that every other crate links -
//! including the console wallet. Release builds do exactly that: `.github/workflows/build_binaries.yml` runs
//! `cargo build --bin minotari_console_wallet` with **no `-p`**, which is the leaking form. So this crate is
//! `exclude`d from the root workspace and is its own workspace root. Do not move it back into `members`; see the
//! comment on `[workspace] exclude` in the repository root `Cargo.toml`.
//!
//! # The check that keeps that true
//!
//! Both of these must be run **without `-p`**. A `-p` narrows feature resolution and hides exactly the problem
//! being looked for, so `cargo tree -p minotari_console_wallet | grep comms_testing` is not a check - it passes
//! while the binary is tainted. Grep for the **feature**, not the crate, because the feature is what leaks:
//!
//! ```text
//! # Nothing in the default package selection may pull in `test_transport`. Must print nothing.
//! cargo tree -e features -i minotari_ledger_wallet_comms | grep test_transport
//!
//! # And the console wallet's own unit must resolve `minotari_ledger_wallet_comms` to `["default"]` alone.
//! cargo +nightly build -Z unstable-options --unit-graph --bin minotari_console_wallet
//! ```
//!
//! # Building and testing this crate
//!
//! Being outside the root workspace, it is **not** covered by a plain `cargo build` / `cargo test` / `cargo clippy`
//! at the repository root. It needs its own invocation, which a future CI change should add:
//!
//! ```text
//! cargo test --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml
//! cargo clippy --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml --all-targets
//! ```

use std::{
    io::{ErrorKind, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use ledger_transport::{APDUAnswer, APDUCommand};
use log::debug;
use minotari_ledger_wallet_comms::error::LedgerDeviceError;
pub use minotari_ledger_wallet_comms::ledger_wallet::{
    LedgerTransport,
    clear_registered_transport,
    register_transport,
};

const LOG_TARGET: &str = "ledger_wallet::comms_testing";

/// Where a locally running Speculos exposes its APDU socket by default.
pub const SPECULOS_DEFAULT_APDU_ADDRESS: &str = "127.0.0.1:9999";

/// How long to wait for the TCP connection itself to come up.
///
/// This bounds *connecting*, not exchanging. Failing to connect is a harness problem and should be reported
/// immediately; waiting for a reply is not, and is deliberately unbounded - see [`SpeculosTransport::exchange`].
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest reply this transport will allocate for, as a sanity bound on the peer supplied length prefix.
///
/// A Ledger APDU reply is at most a few hundred bytes, so this is orders of magnitude more headroom than any real
/// answer needs. Without it, a desynced or garbage stream can name a length of nearly 4 GiB and the allocation that
/// follows aborts the test process with no useful diagnostic. With it, the same stream produces a legible error.
const MAX_APDU_REPLY: usize = 64 * 1024;

/// The connection to Speculos, and the reconnect-on-error handling around it.
///
/// Deliberately shaped like `HidManager::refresh_if_needed` in `minotari_ledger_wallet_comms`: the old connection
/// is dropped before a new one is made, and a failed exchange is retried at most once on a fresh connection.
struct SpeculosConnection {
    inner: Option<TcpStream>,
}

impl SpeculosConnection {
    const fn new() -> Self {
        Self { inner: None }
    }

    /// Whether a connection is already open.
    ///
    /// [`SpeculosTransport::exchange`] needs this to decide whether a silent peer is safe to retry: a connection
    /// that was already open before the exchange started may have been hung up on at any point since it was last
    /// used, so nothing was delivered; a connection opened *for* this exchange that then went silent was live when
    /// the request went out, and the device may well have executed it.
    const fn is_connected(&self) -> bool {
        self.inner.is_some()
    }

    /// Get the open connection, opening one if there is none.
    fn get_or_connect(&mut self, address: &str) -> Result<&mut TcpStream, LedgerDeviceError> {
        if self.inner.is_none() {
            self.inner = Some(connect(address)?);
        }
        // `is_none` was just handled, so this cannot fail.
        self.inner
            .as_mut()
            .ok_or_else(|| LedgerDeviceError::TransportConnect("Speculos connection vanished".to_string()))
    }

    /// Drop the current connection without opening a new one, so that the next exchange starts fresh.
    fn disconnect(&mut self) {
        self.inner = None;
    }

    /// Drop the current connection and open a new one.
    ///
    /// The old stream is cleared first so that it is closed before the new one is opened. Speculos serves a single
    /// APDU connection at a time, so a reconnect that overlapped the connection it is replacing would be refused.
    fn refresh(&mut self, address: &str) -> Result<&mut TcpStream, LedgerDeviceError> {
        self.inner = None;
        self.inner = Some(connect(address)?);
        self.inner
            .as_mut()
            .ok_or_else(|| LedgerDeviceError::TransportConnect("Speculos connection vanished".to_string()))
    }
}

fn connect(address: &str) -> Result<TcpStream, LedgerDeviceError> {
    let mut last_error = format!("No address resolved for '{address}'");
    let addresses = address
        .to_socket_addrs()
        .map_err(|e| LedgerDeviceError::TransportConnect(format!("Could not resolve '{address}': {e}")))?;
    for socket_address in addresses {
        match TcpStream::connect_timeout(&socket_address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                // Nagle would sit on the small APDU writes waiting for more to send.
                if let Err(e) = stream.set_nodelay(true) {
                    debug!(target: LOG_TARGET, "Could not disable Nagle on the Speculos connection: {e}");
                }
                // No read timeout: an exchange may legitimately take as long as a human takes to press a button.
                debug!(target: LOG_TARGET, "Connected to Speculos at '{socket_address}'");
                return Ok(stream);
            },
            Err(e) => last_error = format!("Could not connect to '{socket_address}': {e}"),
        }
    }
    Err(LedgerDeviceError::TransportConnect(last_error))
}

/// A [`LedgerTransport`] that talks to a Speculos simulator over its APDU TCP socket.
///
/// Unlike the HID transport, which opens a fresh connection per exchange, this holds one connection open for its
/// whole life and only reconnects when an exchange fails on it. Speculos serves one connection at a time, and the
/// simulator tests drive the device's UI across an exchange that is deliberately left outstanding for seconds - a
/// reconnect-per-exchange transport would race with that.
pub struct SpeculosTransport {
    address: String,
    connection: Mutex<SpeculosConnection>,
}

impl SpeculosTransport {
    /// Create a transport for `address`, connecting lazily on the first exchange.
    pub fn new<S: Into<String>>(address: S) -> Self {
        Self {
            address: address.into(),
            connection: Mutex::new(SpeculosConnection::new()),
        }
    }

    /// Create a transport for `address` and connect to it now, so that a missing simulator is reported at set-up
    /// time rather than as a mystery failure inside the first instruction.
    pub fn connect<S: Into<String>>(address: S) -> Result<Self, LedgerDeviceError> {
        let transport = Self::new(address);
        {
            let mut connection = transport.lock_connection();
            connection.get_or_connect(&transport.address)?;
        }
        Ok(transport)
    }

    /// The address this transport talks to.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Take the connection lock, recovering from poisoning rather than failing on it.
    ///
    /// A poisoned mutex here means some earlier caller panicked while holding it - in practice a test thread
    /// panicking inside an assertion. The guarded state is an `Option<TcpStream>` and cannot be *logically* corrupt;
    /// the worst it can be is a stale stream, which the reconnect path already handles. Refusing to hand it out
    /// would turn one panicking test into "Speculos connection mutex poisoned" for every later test in the process,
    /// burying the panic that actually mattered. So the connection is dropped (the next exchange starts on a fresh
    /// one), the poison flag is cleared so later callers are not punished for it either, and the guard is returned.
    ///
    /// `minotari_ledger_wallet_comms` recovers from poisoning the same way, for the same reason.
    fn lock_connection(&self) -> MutexGuard<'_, SpeculosConnection> {
        match self.connection.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                debug!(
                    target: LOG_TARGET,
                    "Speculos connection mutex was poisoned by a panicking caller; dropping the connection and carrying on"
                );
                let mut guard = poisoned.into_inner();
                guard.disconnect();
                self.connection.clear_poison();
                guard
            },
        }
    }
}

impl LedgerTransport for SpeculosTransport {
    /// Send one APDU to Speculos and wait for its answer.
    ///
    /// May block indefinitely: an instruction that puts a review screen up does not answer until something drives
    /// the simulator's UI. That is why there is no read timeout on the socket. See [`LedgerTransport`].
    ///
    /// # Retries never replay an instruction the device may have run
    ///
    /// Many instructions are not idempotent - `GetRawSchnorrSignature` and friends consume a device nonce - so a
    /// blind "retry once on error" would be able to burn a second nonce behind the test's back, and the test would
    /// then see device state that nothing it did explains. A retry therefore only happens when the request
    /// provably did not reach the device: see [`ExchangeFailure`].
    fn exchange(&self, command: &APDUCommand<Vec<u8>>) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        let request = command.serialize();
        let mut connection = self.lock_connection();

        let was_connected = connection.is_connected();
        let first_attempt = {
            let stream = connection.get_or_connect(&self.address)?;
            exchange_on(stream, &request)
        };
        let failure = match first_attempt {
            Ok(answer) => return Ok(answer),
            Err(f) => f,
        };

        if !failure.is_safe_to_retry(was_connected) {
            // The device may have executed this instruction, so it must not be sent again. The stream is in an
            // unknown state, so drop it; the next exchange will start on a new one.
            debug!(
                target: LOG_TARGET,
                "Speculos exchange failed ({failure}) and cannot be safely retried - the device may have run it"
            );
            connection.disconnect();
            return Err(failure.into_error());
        }

        // Nothing was delivered, so re-sending is not a replay. The old stream is in an unknown state - possibly a
        // half written request - so the retry has to be on a brand new one.
        debug!(target: LOG_TARGET, "Speculos exchange failed ({failure}), reconnecting and retrying once");
        let stream = connection.refresh(&self.address)?;
        exchange_on(stream, &request).map_err(ExchangeFailure::into_error)
    }
}

/// How far an exchange got before it failed, which is what decides whether re-sending it is a retry or a replay.
enum ExchangeFailure {
    /// The request could not be written to the socket. Nothing, or an incomplete prefix of the request, reached the
    /// peer. An incomplete request is not a command: the peer is still waiting for the bytes the length prefix
    /// promised and cannot have executed anything. Safe to retry - but only on a new connection, since the old one
    /// is left mid-frame.
    WriteFailed(LedgerDeviceError),
    /// The request went out and the peer closed without sending a single byte back.
    ///
    /// Safe to retry **only if the connection was already open before this exchange started**: such a connection may
    /// have been hung up on at any time since it was last used, and the write then succeeded into a socket buffer
    /// nobody was ever going to read. On a connection opened for this very exchange, the peer was demonstrably alive
    /// when the request went out, so it may have run the instruction and died before answering - that is a replay
    /// risk, not a retry.
    NoReply(LedgerDeviceError),
    /// The peer answered, but the answer was unusable - a partial reply, an implausible length prefix, or something
    /// `APDUAnswer` could not parse. An answer means the instruction ran. **Never** retry this.
    BadAnswer(LedgerDeviceError),
}

impl ExchangeFailure {
    const fn is_safe_to_retry(&self, connection_was_already_open: bool) -> bool {
        match self {
            ExchangeFailure::WriteFailed(_) => true,
            ExchangeFailure::NoReply(_) => connection_was_already_open,
            ExchangeFailure::BadAnswer(_) => false,
        }
    }

    fn into_error(self) -> LedgerDeviceError {
        match self {
            ExchangeFailure::WriteFailed(e) | ExchangeFailure::NoReply(e) | ExchangeFailure::BadAnswer(e) => e,
        }
    }
}

impl std::fmt::Display for ExchangeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExchangeFailure::WriteFailed(e) | ExchangeFailure::NoReply(e) | ExchangeFailure::BadAnswer(e) => {
                write!(f, "{e}")
            },
        }
    }
}

/// The framing used by the Speculos APDU socket, in both directions: a 4-byte big endian length, then that many
/// bytes.
///
/// The one asymmetry is the status word. The length Speculos sends counts the *response data only*, and the two
/// status bytes follow it outside the count - so a reply is `len`, then `len + 2` bytes, the last two of which are
/// the status word. [`APDUAnswer::from_answer`] wants data and status word together, which is exactly those
/// `len + 2` bytes.
const LENGTH_PREFIX_BYTES: usize = 4;
const STATUS_WORD_BYTES: usize = 2;

/// Fill `buffer`, reporting how many bytes made it in if that could not be done.
///
/// This is `Read::read_exact` with the one detail it throws away kept: whether the peer said *nothing at all* or
/// began an answer and then stopped. [`ExchangeFailure`] needs to tell those apart.
fn read_filling(stream: &mut TcpStream, buffer: &mut [u8]) -> Result<(), (usize, std::io::Error)> {
    let mut filled = 0;
    while filled < buffer.len() {
        let remaining = buffer.get_mut(filled..).unwrap_or_default();
        match stream.read(remaining) {
            Ok(0) => {
                return Err((
                    filled,
                    std::io::Error::new(ErrorKind::UnexpectedEof, "the peer closed the connection"),
                ));
            },
            Ok(read) => filled = filled.saturating_add(read),
            Err(e) if e.kind() == ErrorKind::Interrupted => {},
            Err(e) => return Err((filled, e)),
        }
    }
    Ok(())
}

fn exchange_on(stream: &mut TcpStream, request: &[u8]) -> Result<APDUAnswer<Vec<u8>>, ExchangeFailure> {
    let length = u32::try_from(request.len()).map_err(|_| {
        ExchangeFailure::WriteFailed(LedgerDeviceError::TransportExchange(format!(
            "APDU of {} bytes is too large",
            request.len()
        )))
    })?;

    let mut framed = Vec::with_capacity(LENGTH_PREFIX_BYTES.saturating_add(request.len()));
    framed.extend_from_slice(&length.to_be_bytes());
    framed.extend_from_slice(request);
    stream.write_all(&framed).map_err(|e| {
        ExchangeFailure::WriteFailed(LedgerDeviceError::TransportExchange(format!(
            "Could not send the APDU: {e}"
        )))
    })?;
    stream.flush().map_err(|e| {
        ExchangeFailure::WriteFailed(LedgerDeviceError::TransportExchange(format!(
            "Could not flush the APDU: {e}"
        )))
    })?;

    // From here on the request is on the wire, so every failure has to assume the device ran it - except the one
    // case where the peer never said anything at all, which `exchange` weighs against the connection's age.
    let mut length_bytes = [0u8; LENGTH_PREFIX_BYTES];
    if let Err((received, e)) = read_filling(stream, &mut length_bytes) {
        let error = LedgerDeviceError::TransportExchange(format!("Could not read the reply length: {e}"));
        return Err(if received == 0 {
            ExchangeFailure::NoReply(error)
        } else {
            ExchangeFailure::BadAnswer(error)
        });
    }
    let data_length = usize::try_from(u32::from_be_bytes(length_bytes)).map_err(|e| {
        ExchangeFailure::BadAnswer(LedgerDeviceError::TransportExchange(format!(
            "Reply length does not fit in a usize: {e}"
        )))
    })?;
    if data_length > MAX_APDU_REPLY {
        return Err(ExchangeFailure::BadAnswer(LedgerDeviceError::TransportExchange(
            format!("Reply claims {data_length} bytes of data, more than the {MAX_APDU_REPLY} byte sanity bound"),
        )));
    }

    let mut answer = vec![0u8; data_length.saturating_add(STATUS_WORD_BYTES)];
    read_filling(stream, &mut answer).map_err(|(_, e)| {
        ExchangeFailure::BadAnswer(LedgerDeviceError::TransportExchange(format!(
            "Could not read the reply: {e}"
        )))
    })?;

    APDUAnswer::from_answer(answer).map_err(|e| {
        ExchangeFailure::BadAnswer(LedgerDeviceError::TransportExchange(format!(
            "Malformed APDU answer: {e}"
        )))
    })
}

/// Point the ledger client at a Speculos simulator for the rest of this process.
///
/// Returns the transport as well, so that a caller that wants to talk to the simulator directly can.
pub fn register_speculos_transport<S: Into<String>>(
    address: S,
) -> Result<std::sync::Arc<SpeculosTransport>, LedgerDeviceError> {
    let transport = std::sync::Arc::new(SpeculosTransport::connect(address)?);
    register_transport(transport.clone());
    Ok(transport)
}

#[cfg(test)]
mod test {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        thread,
    };

    use super::*;

    /// Read one length prefixed APDU off `stream`.
    fn read_request(stream: &mut TcpStream) -> Option<Vec<u8>> {
        let mut length_bytes = [0u8; 4];
        stream.read_exact(&mut length_bytes).ok()?;
        let length = usize::try_from(u32::from_be_bytes(length_bytes)).unwrap();
        let mut request = vec![0u8; length];
        stream.read_exact(&mut request).ok()?;
        Some(request)
    }

    /// Write one length prefixed reply, with the status word outside the count, the way Speculos does.
    fn write_reply(stream: &mut TcpStream, data: &[u8], status_word: u16) {
        let mut reply = Vec::new();
        reply.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        reply.extend_from_slice(data);
        reply.extend_from_slice(&status_word.to_be_bytes());
        stream.write_all(&reply).unwrap();
        stream.flush().unwrap();
    }

    /// A stand-in for Speculos that speaks the same framing: read a length prefixed APDU, reply with a length
    /// prefixed payload followed by a status word.
    fn serve(replies: Vec<(Vec<u8>, u16)>) -> (String, thread::JoinHandle<Vec<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let handle = thread::spawn(move || {
            let mut received = Vec::new();
            let (mut stream, _) = listener.accept().unwrap();
            for (data, status_word) in replies {
                let Some(request) = read_request(&mut stream) else {
                    break;
                };
                received.push(request);
                write_reply(&mut stream, &data, status_word);
            }
            received
        });
        (address, handle)
    }

    fn command() -> APDUCommand<Vec<u8>> {
        APDUCommand {
            cla: 0x80,
            ins: 0x01,
            p1: 0x02,
            p2: 0x03,
            data: vec![0xaa, 0xbb, 0xcc],
        }
    }

    #[test]
    fn round_trips_a_framed_apdu() {
        let (address, server) = serve(vec![(vec![0x02, 0x10, 0x20], 0x9000)]);
        let transport = SpeculosTransport::connect(address).unwrap();

        let answer = transport.exchange(&command()).unwrap();
        assert_eq!(answer.data(), &[0x02, 0x10, 0x20]);
        assert_eq!(answer.retcode(), 0x9000);

        let received = server.join().unwrap();
        assert_eq!(received, vec![command().serialize()]);
    }

    #[test]
    fn empty_reply_data_is_just_a_status_word() {
        let (address, server) = serve(vec![(vec![], 0x6e00)]);
        let transport = SpeculosTransport::connect(address).unwrap();

        let answer = transport.exchange(&command()).unwrap();
        assert!(answer.data().is_empty());
        assert_eq!(answer.retcode(), 0x6e00);
        server.join().unwrap();
    }

    #[test]
    fn the_connection_is_reused_across_exchanges() {
        // A single `accept` serves both exchanges, so if the transport reconnected per exchange the second one
        // would find nobody listening.
        let (address, server) = serve(vec![(vec![0x02, 0x01], 0x9000), (vec![0x02, 0x02], 0x9000)]);
        let transport = SpeculosTransport::connect(address).unwrap();

        assert_eq!(transport.exchange(&command()).unwrap().data(), &[0x02, 0x01]);
        assert_eq!(transport.exchange(&command()).unwrap().data(), &[0x02, 0x02]);
        server.join().unwrap();
    }

    #[test]
    fn a_stale_connection_is_retried_once() {
        // The connection is opened up front and then hung up on before the exchange starts, which is the one shape
        // that is safe to retry: the peer was already gone, so nothing was delivered.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = thread::spawn(move || {
            // Hang up on the first connection without reading anything.
            let (stream, _) = listener.accept().unwrap();
            drop(stream);
            // Then serve the retry properly.
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream).unwrap();
            write_reply(&mut stream, &[0x02, 0x42], 0x9000);
            request
        });

        let transport = SpeculosTransport::connect(address).unwrap();
        let answer = transport.exchange(&command()).unwrap();
        assert_eq!(answer.data(), &[0x02, 0x42]);
        assert_eq!(answer.retcode(), 0x9000);
        assert_eq!(server.join().unwrap(), command().serialize());
    }

    #[test]
    fn a_device_that_takes_the_request_and_goes_silent_is_not_replayed() {
        // The connection is opened *by the exchange*, so the peer was alive when the request went out and may have
        // executed it. Re-sending would burn a second device nonce for a nonce-consuming instruction. It must fail
        // instead, and the server must see exactly one request.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = thread::spawn(move || {
            let mut received = Vec::new();
            // Take the request, then hang up without answering.
            let (mut stream, _) = listener.accept().unwrap();
            if let Some(request) = read_request(&mut stream) {
                received.push(request);
            }
            drop(stream);
            // Anything that arrives after this is a replay. Record it, and answer it so that a client which did
            // replay does not block here instead of failing the assertion below.
            if let Ok((mut stream, _)) = listener.accept() &&
                let Some(request) = read_request(&mut stream)
            {
                received.push(request);
                write_reply(&mut stream, &[0x02, 0x42], 0x9000);
            }
            received
        });

        // Lazy, so the exchange itself opens the connection.
        let transport = SpeculosTransport::new(address.clone());
        match transport.exchange(&command()) {
            Err(LedgerDeviceError::TransportExchange(_)) => {},
            other => panic!("Expected a TransportExchange error, got {:?}", other.map(|_| ())),
        }

        // Unblock the server's second `accept` so the test can join it.
        drop(TcpStream::connect(&address));
        assert_eq!(
            server.join().unwrap(),
            vec![command().serialize()],
            "the instruction was replayed after the device may already have run it"
        );
    }

    #[test]
    fn a_malformed_reply_length_is_not_replayed() {
        // A reply is an answer, so the instruction ran. A garbage length prefix must become a legible error rather
        // than either a ~4 GiB allocation or a second send.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = thread::spawn(move || {
            let mut received = Vec::new();
            let (mut stream, _) = listener.accept().unwrap();
            if let Some(request) = read_request(&mut stream) {
                received.push(request);
            }
            // A length prefix that no real answer could have.
            stream.write_all(&u32::MAX.to_be_bytes()).unwrap();
            stream.flush().unwrap();
            // Keep the connection alive so that a retry, if there were one, would be visible here.
            if let Some(request) = read_request(&mut stream) {
                received.push(request);
            }
            received
        });

        let transport = SpeculosTransport::connect(address).unwrap();
        match transport.exchange(&command()) {
            Err(LedgerDeviceError::TransportExchange(message)) => {
                assert!(
                    message.contains("sanity bound"),
                    "expected the reply size bound to be named, got: {message}"
                );
            },
            other => panic!("Expected a TransportExchange error, got {:?}", other.map(|_| ())),
        }
        assert_eq!(
            server.join().unwrap(),
            vec![command().serialize()],
            "the instruction was replayed after the device had already answered"
        );
    }

    #[test]
    fn connecting_to_nothing_is_a_transport_connect_error() {
        // Bind and drop, so the port is almost certainly free and nothing is listening on it.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        drop(listener);

        match SpeculosTransport::connect(address).map(|_| ()) {
            Err(LedgerDeviceError::TransportConnect(_)) => {},
            other => panic!("Expected a TransportConnect error, got {other:?}"),
        }
    }

    #[test]
    fn a_panic_while_holding_the_lock_does_not_break_the_transport() {
        // One panicking test thread must not turn every later exchange in the process into "mutex poisoned",
        // burying the panic that actually mattered.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = thread::spawn(move || {
            // The connection `SpeculosTransport::connect` opens, which the poisoning drops.
            let (stream, _) = listener.accept().unwrap();
            drop(stream);
            // The connection the exchange after the panic opens.
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream).unwrap();
            write_reply(&mut stream, &[0x02, 0x99], 0x9000);
            request
        });

        let transport = Arc::new(SpeculosTransport::connect(address).unwrap());

        let panicking = {
            let transport = Arc::clone(&transport);
            thread::spawn(move || {
                let _guard = transport.lock_connection();
                panic!("a test assertion failed while holding the connection lock");
            })
        };
        assert!(panicking.join().is_err(), "the thread was supposed to panic");

        let answer = transport
            .exchange(&command())
            .expect("the transport must still work after a panic poisoned its lock");
        assert_eq!(answer.data(), &[0x02, 0x99]);
        assert_eq!(server.join().unwrap(), command().serialize());

        // And it keeps working: the poison was cleared rather than worked around once.
        assert!(!transport.connection.is_poisoned());
    }
}
