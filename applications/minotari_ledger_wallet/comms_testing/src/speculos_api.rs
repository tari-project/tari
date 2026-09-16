// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Speculos' HTTP control API: what the device is showing, and how to press it.
//!
//! The APDU socket in [`crate`] carries the *protocol*. This module carries the *user interface*: the text the
//! device has drawn, and the buttons and taps that answer it. They are two different sockets on the same
//! simulator, and the whole point of this spec is that a test can hold both at once - one thread blocked in an
//! APDU exchange that the device will not answer until somebody presses a button, another thread watching the
//! screen and pressing it.
//!
//! # Why this is hand rolled rather than a HTTP client crate
//!
//! Four endpoints, no TLS, no redirects, no authentication, no connection pooling, and a server that closes the
//! connection after every reply. A general HTTP client would bring a runtime, a TLS stack and a hundred transitive
//! crates into a test-only crate that deliberately keeps its dependency surface small enough to read - the same
//! reasoning that has BIP-32 and BIP-39 written out by hand in [`crate::oracle`] rather than taken off the shelf.
//! The framing this needs is about eighty lines, and it is all in [`request`] and [`read_chunked_line`].
//!
//! # Waiting is event driven, never polled
//!
//! [`SpeculosApi::open_event_stream`] opens `GET /events?stream=true`, which is a Server-Sent Events stream: the
//! server holds the connection open and writes one line per text the device draws. A caller that wants to wait for
//! the device to do *something* blocks in [`EventStream::next_event`], which returns the instant the device draws
//! and not before.
//!
//! That is the only waiting primitive this harness has, and it is deliberate. A `sleep` long enough to be reliable
//! on a loaded CI runner is far longer than the device usually takes, so it is paid on every screen of every
//! scenario; a `sleep` short enough to be quick is a race. A retry loop around either is worse: the nonce store
//! and the script offset context are exactly the state a second attempt disturbs, so a test that goes green on a
//! retry has usually destroyed the evidence that it was ever red. See the crate docs in
//! [`crate::approver`] for what that buys.
//!
//! # What the event stream does and does not tell you
//!
//! Each event is a [`ScreenText`] - a string with the rectangle it was drawn in. Two things are worth knowing:
//!
//! * **Speculos does not forward screen clears.** Its broadcaster consumes the `clear` event to reset its own notion of
//!   the current screen and returns before handing anything to a subscriber, so a stream subscriber sees an unbroken
//!   run of texts with no screen boundaries in it. Boundaries have to come from [`SpeculosApi::current_screen`], which
//!   reports the texts drawn since the last clear.
//! * **On BAGL models the text is reconstructed from pixels, and it is lossy.** See [`crate::review::UiToolkit::Bagl`]
//!   - this is not a detail that can be papered over, it changes what a screen assertion is allowed to claim.

use std::{
    env,
    fmt,
    io::{ErrorKind, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::{Duration, Instant},
};

use serde_json::Value;

/// Where the simulator's HTTP control API is. Defaults to [`SPECULOS_DEFAULT_API_ADDRESS`].
///
/// Set but empty is an error, for the same reason [`crate::simulator::SPECULOS_APDU_ADDRESS`] is: it is what
/// `$(./scripts/ledger_speculos.sh api-address ...)` leaves behind when no simulator is running, and falling back
/// to Speculos' conventional port could drive the buttons of an unrelated simulator while asserting against this
/// one.
pub const SPECULOS_API_ADDRESS: &str = "SPECULOS_API_ADDRESS";

/// Speculos' conventional HTTP API port, which is what `docker run -p 5000:5000 speculos ...` gives you.
///
/// Deliberately *not* where `scripts/ledger_speculos.sh` puts a simulator - that lets Docker allocate an ephemeral
/// host port so concurrent runs cannot collide, and prints the address it got.
pub const SPECULOS_DEFAULT_API_ADDRESS: &str = "127.0.0.1:5000";

/// How long to wait for the HTTP connection itself.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a single request/response may take.
///
/// This bounds one control call - reading the current screen, pressing a button. It is not the bound on waiting
/// for the device to *do* something; that is the caller's deadline, passed to [`EventStream::next_event`].
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A sanity bound on a response body, so that a desynchronised stream produces an error rather than an allocation
/// that aborts the test process.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// The address of the HTTP API, given the raw value of [`SPECULOS_API_ADDRESS`] if it was set.
pub fn api_address() -> Result<String, ApiError> {
    api_address_from(env::var(SPECULOS_API_ADDRESS).ok().as_deref())
}

fn api_address_from(value: Option<&str>) -> Result<String, ApiError> {
    match value.map(str::trim) {
        None => Ok(SPECULOS_DEFAULT_API_ADDRESS.to_string()),
        Some(address) if !address.is_empty() => Ok(address.to_string()),
        Some(_) => Err(ApiError(format!(
            "{SPECULOS_API_ADDRESS} is set but empty. It is usually `$(./scripts/ledger_speculos.sh api-address \
             <model> <seed>)` with no simulator running - start one with `./scripts/ledger_speculos.sh up <model> \
             <seed>`. Refusing to fall back to {SPECULOS_DEFAULT_API_ADDRESS}, which is Speculos' default API port \
             and may be some other simulator entirely."
        ))),
    }
}

/// Anything that went wrong talking to Speculos' control API.
///
/// One variant on purpose. Every one of these is "the simulator is not where or what you think it is", and a
/// caller has the same three options for all of them: fail the test, print the message, go and look at the
/// container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError(pub String);

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ApiError {}

/// One text the device drew, and the rectangle it drew it in.
///
/// The geometry is the answer to the Step 0 spike, and it is what makes touch driving possible without a
/// coordinate table: a tap target is found by matching its **text** and tapping the centre of the rectangle
/// Speculos reports for it, so an SDK that moves a button by twenty pixels does not need a test change. See
/// [`ScreenText::centre`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenText {
    pub text: String,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl ScreenText {
    /// The centre of the rectangle this text was drawn in, which is where to tap to press it.
    pub fn centre(&self) -> (i64, i64) {
        (self.x.saturating_add(self.w / 2), self.y.saturating_add(self.h / 2))
    }
}

impl fmt::Display for ScreenText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} at ({}, {}) {}x{}", self.text, self.x, self.y, self.w, self.h)
    }
}

/// Join a screen's texts into the one string a screen assertion works on.
///
/// No separator between texts, because a text is not a word: a value too long for one line arrives as several
/// events, and on BAGL a field's header and its first line arrive as *one*. Inserting anything between them would
/// make a wrapped address unmatchable against the address that was asked for, which is the assertion that matters
/// most. The outer whitespace is trimmed because both toolkits pad with it in places that carry no meaning.
pub fn joined(texts: &[ScreenText]) -> String {
    texts
        .iter()
        .map(|t| t.text.as_str())
        .collect::<String>()
        .trim()
        .to_string()
}

/// Which physical button on a BAGL device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Both,
}

impl Button {
    const fn path(self) -> &'static str {
        match self {
            Button::Left => "/button/left",
            Button::Right => "/button/right",
            Button::Both => "/button/both",
        }
    }
}

/// A touch on an NBGL device.
///
/// `Swipe` is the one gesture that cannot be aimed by text anchor - it is a movement across the whole screen
/// rather than a press on a widget - so its coordinates come from [`crate::review::UiToolkit`]'s per-model table.
/// Every *button* press is a [`Touch::Tap`] aimed at [`ScreenText::centre`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    Tap { x: i64, y: i64 },
    Press { x: i64, y: i64 },
    Release { x: i64, y: i64 },
    Swipe { from: (i64, i64), to: (i64, i64) },
}

impl Touch {
    fn body(self) -> String {
        match self {
            Touch::Tap { x, y } => format!(r#"{{"action":"press-and-release","x":{x},"y":{y}}}"#),
            Touch::Press { x, y } => format!(r#"{{"action":"press","x":{x},"y":{y}}}"#),
            Touch::Release { x, y } => format!(r#"{{"action":"release","x":{x},"y":{y}}}"#),
            Touch::Swipe {
                from: (x, y),
                to: (x2, y2),
            } => format!(r#"{{"action":"press-and-release","x":{x},"y":{y},"x2":{x2},"y2":{y2}}}"#),
        }
    }
}

/// A client for one running simulator's HTTP control API.
#[derive(Debug, Clone)]
pub struct SpeculosApi {
    address: String,
}

impl SpeculosApi {
    pub fn new<S: Into<String>>(address: S) -> Self {
        Self {
            address: address.into(),
        }
    }

    /// A client for the simulator named by [`SPECULOS_API_ADDRESS`].
    pub fn from_env() -> Result<Self, ApiError> {
        Ok(Self::new(api_address()?))
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// The texts drawn since the device last cleared its screen - that is, what is on it now.
    ///
    /// This is the only way to see a screen *boundary*: Speculos consumes the clear event inside its broadcaster
    /// and never forwards it to a stream subscriber, so the stream alone cannot tell you that one screen ended and
    /// another began.
    pub fn current_screen(&self) -> Result<Vec<ScreenText>, ApiError> {
        parse_events(&self.get("/events?currentscreenonly=true")?)
    }

    /// Every text the device has drawn since the log was last cleared, across all screens.
    ///
    /// This is the diagnostic dump, not a working surface: it grows without bound and spans screens, so an
    /// assertion built on it would match text that has long since been replaced.
    pub fn event_log(&self) -> Result<Vec<ScreenText>, ApiError> {
        parse_events(&self.get("/events")?)
    }

    /// Discard the accumulated event log.
    ///
    /// This clears only the historical log; the *current screen* is untouched, because Speculos keeps the two in
    /// separate lists. That matters: a scenario can clear the log at its start without blinding itself to a review
    /// that the device has already drawn.
    pub fn clear_event_log(&self) -> Result<(), ApiError> {
        self.request("DELETE", "/events", None).map(|_| ())
    }

    /// Press and release one of a BAGL device's buttons.
    pub fn press_button(&self, button: Button) -> Result<(), ApiError> {
        self.request("POST", button.path(), Some(r#"{"action":"press-and-release"}"#))
            .map(|_| ())
    }

    /// Touch an NBGL device's screen.
    pub fn touch(&self, touch: Touch) -> Result<(), ApiError> {
        self.request("POST", "/finger", Some(&touch.body())).map(|_| ())
    }

    /// The current screen as a PNG.
    ///
    /// Only ever used for a failure dump. Nothing in this harness asserts on pixels - see the crate docs in
    /// [`crate::review`] for why screenshot baselines were rejected - but a timeout that says "the device was
    /// showing something unexpected" is far more useful with a picture of it attached.
    pub fn screenshot(&self) -> Result<Vec<u8>, ApiError> {
        self.request("GET", "/screenshot", None)
    }

    /// Subscribe to the device's draw events.
    ///
    /// The stream is opened now and stays open for the life of the returned [`EventStream`], so a caller that
    /// opens it *before* triggering an instruction cannot miss the review appearing between the two.
    pub fn open_event_stream(&self) -> Result<EventStream, ApiError> {
        let mut stream = self.connect()?;
        let request = format!(
            "GET /events?stream=true HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\n\r\n",
            self.address
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| ApiError(format!("Could not ask '{}' for an event stream: {e}", self.address)))?;
        stream.flush().map_err(|e| {
            ApiError(format!(
                "Could not flush the event stream request to '{}': {e}",
                self.address
            ))
        })?;

        // The subscription handshake gets the ordinary request budget; `next_event` replaces the deadline with
        // the caller's own before every wait.
        let handshake_deadline = Instant::now().checked_add(REQUEST_TIMEOUT).unwrap_or_else(Instant::now);
        let mut reader = ByteReader::new(stream, handshake_deadline);
        let (status, headers) = read_head(&mut reader)?;
        if status != 200 {
            return Err(ApiError(format!(
                "'{}' answered {status} to an event stream subscription",
                self.address
            )));
        }
        if !headers
            .iter()
            .any(|h| h.eq_ignore_ascii_case("transfer-encoding: chunked"))
        {
            // Werkzeug has always used chunked framing for this endpoint. If it ever stops, the body parsing
            // below silently reads garbage, so say so rather than misreport what the device drew.
            return Err(ApiError(format!(
                "'{}' did not answer the event stream with chunked framing; headers were {headers:?}",
                self.address
            )));
        }
        Ok(EventStream { reader })
    }

    fn get(&self, path: &str) -> Result<Vec<u8>, ApiError> {
        self.request("GET", path, None)
    }

    fn connect(&self) -> Result<TcpStream, ApiError> {
        let mut last_error = format!("No address resolved for '{}'", self.address);
        let addresses = self
            .address
            .to_socket_addrs()
            .map_err(|e| ApiError(format!("Could not resolve '{}': {e}", self.address)))?;
        for socket_address in addresses {
            match TcpStream::connect_timeout(&socket_address, CONNECT_TIMEOUT) {
                Ok(stream) => {
                    // Both directions get a bound. A control call that hangs would hang the whole scenario, and
                    // the review it was driving would then time out blaming the device.
                    // An opening value only. The real bound is the absolute deadline in `ByteReader`, which
                    // re-derives this before every read; this is here so that a socket that is somehow read
                    // without going through the reader still cannot block for ever.
                    if let Err(e) = stream.set_read_timeout(Some(REQUEST_TIMEOUT)) {
                        last_error = format!("Could not set a read timeout on '{socket_address}': {e}");
                        continue;
                    }
                    if let Err(e) = stream.set_write_timeout(Some(REQUEST_TIMEOUT)) {
                        last_error = format!("Could not set a write timeout on '{socket_address}': {e}");
                        continue;
                    }
                    return Ok(stream);
                },
                Err(e) => last_error = format!("Could not connect to '{socket_address}': {e}"),
            }
        }
        Err(ApiError(last_error))
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<Vec<u8>, ApiError> {
        let mut stream = self.connect()?;
        let head = match body {
            Some(body) => format!(
                "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: \
                 {length}\r\nConnection: close\r\n\r\n{body}",
                address = self.address,
                length = body.len()
            ),
            None => format!(
                "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n",
                address = self.address
            ),
        };
        stream
            .write_all(head.as_bytes())
            .map_err(|e| ApiError(format!("Could not send {method} {path} to '{}': {e}", self.address)))?;
        stream
            .flush()
            .map_err(|e| ApiError(format!("Could not flush {method} {path} to '{}': {e}", self.address)))?;

        // One deadline for the whole reply, rather than one per `read`. See `ByteReader`.
        let deadline = Instant::now().checked_add(REQUEST_TIMEOUT).unwrap_or_else(Instant::now);
        let mut reader = ByteReader::new(stream, deadline);
        let (status, headers) = read_head(&mut reader)?;
        let body = read_body(&mut reader, &headers)?;
        if !(200..300).contains(&status) {
            return Err(ApiError(format!(
                "'{}' answered {status} to {method} {path}: {}",
                self.address,
                String::from_utf8_lossy(&body)
            )));
        }
        Ok(body)
    }
}

/// An open subscription to the device's draw events.
///
/// Dropping it closes the connection, which is how a scenario unsubscribes.
pub struct EventStream {
    reader: ByteReader,
}

impl EventStream {
    /// Block until the device draws something, or `deadline` passes.
    ///
    /// Returns `Ok(None)` at the deadline rather than an error, because "the device did not draw anything" is a
    /// normal thing for a caller to want to reason about - a caller waiting for a review may decide the deadline
    /// means a timeout with a full diagnostic dump, while a caller draining the stream may not care.
    pub fn next_event(&mut self, deadline: Instant) -> Result<Option<ScreenText>, ApiError> {
        // Handed to the reader rather than armed on the socket here. A single chunk can take many reads, and a
        // socket timeout set once per iteration of the loop below would be granted again, in full, to every one of
        // them - which is how a bound that looks absolute turns out not to be.
        self.reader.set_deadline(deadline);
        loop {
            if Instant::now() >= deadline {
                return Ok(None);
            }
            let line = match read_chunked_line(&mut self.reader) {
                Ok(Some(line)) => line,
                Ok(None) => {
                    return Err(ApiError(
                        "Speculos closed the event stream; the simulator has probably exited".to_string(),
                    ));
                },
                Err(e) if e.timed_out => return Ok(None),
                Err(e) => return Err(ApiError(e.message)),
            };
            // Server-Sent Events framing: `data: <payload>` lines, blank lines between records, and anything else
            // (comments, `event:`, `id:`) is not ours to interpret.
            let Some(payload) = line.strip_prefix("data:") else {
                continue;
            };
            if let Some(event) = parse_event(payload.trim())? {
                return Ok(Some(event));
            }
        }
    }
}

fn parse_events(body: &[u8]) -> Result<Vec<ScreenText>, ApiError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| ApiError(format!("Speculos answered with something that is not JSON: {e}")))?;
    let events = value
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError(format!("Speculos' answer has no 'events' array: {value}")))?;
    events.iter().map(screen_text_from).collect()
}

fn parse_event(payload: &str) -> Result<Option<ScreenText>, ApiError> {
    if payload.is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(payload).map_err(|e| {
        ApiError(format!(
            "Speculos streamed something that is not JSON ({payload:?}): {e}"
        ))
    })?;
    screen_text_from(&value).map(Some)
}

fn screen_text_from(value: &Value) -> Result<ScreenText, ApiError> {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError(format!("A Speculos event has no 'text': {value}")))?;
    let number = |key: &str| value.get(key).and_then(Value::as_i64).unwrap_or_default();
    Ok(ScreenText {
        text: text.to_string(),
        x: number("x"),
        y: number("y"),
        w: number("w"),
        h: number("h"),
    })
}

/// A buffered reader over the socket, because HTTP framing needs to read a line at a time and then exactly N bytes.
///
/// # The bound is an absolute deadline, not a per-read timeout
///
/// A socket's `SO_RCVTIMEO` bounds one `read`, and it restarts on every successful one. That is no bound at all
/// against a peer that dribbles: a byte just inside the timeout resets the clock, and the sanity cap's worth of
/// that is weeks. So the timeout is *derived* from an absolute deadline on each read, and the read that finds the
/// deadline already gone fails instead of arming a new timeout.
///
/// `SpeculosTransport::read_filling` in this crate's `lib.rs` solves the same problem the same way, for the same
/// peer, and its comment about "one byte an hour" applies here word for word.
struct ByteReader {
    stream: TcpStream,
    buffer: Vec<u8>,
    position: usize,
    /// When to give up, however many individual reads it has taken to get here.
    deadline: Instant,
}

struct ReadError {
    message: String,
    timed_out: bool,
}

impl ByteReader {
    fn new(stream: TcpStream, deadline: Instant) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
            position: 0,
            deadline,
        }
    }

    /// Move the deadline, for a reader whose lifetime spans several independent waits.
    ///
    /// [`EventStream`] is exactly that: it is opened once and then waited on many times, each wait with its own
    /// budget decided by the caller, so the deadline cannot be fixed when the socket is opened.
    fn set_deadline(&mut self, deadline: Instant) {
        self.deadline = deadline;
    }

    /// Pull more bytes from the socket. `Ok(false)` means the peer closed.
    fn fill(&mut self) -> Result<bool, ReadError> {
        // Re-derived on every read, so that a peer feeding one byte at a time cannot keep resetting the clock.
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ReadError {
                message: "Speculos did not finish answering within the deadline".to_string(),
                timed_out: true,
            });
        }
        if let Err(e) = self.stream.set_read_timeout(Some(remaining)) {
            return Err(ReadError {
                message: format!("Could not set a read timeout on the Speculos connection: {e}"),
                timed_out: false,
            });
        }
        let mut chunk = [0u8; 4096];
        match self.stream.read(&mut chunk) {
            Ok(0) => Ok(false),
            Ok(read) => {
                // Compact first so that a long-lived stream does not grow a buffer of consumed bytes for ever.
                if self.position > 0 {
                    self.buffer.drain(..self.position);
                    self.position = 0;
                }
                self.buffer.extend_from_slice(chunk.get(..read).unwrap_or_default());
                Ok(true)
            },
            Err(e) if e.kind() == ErrorKind::Interrupted => Ok(true),
            Err(e) => {
                // A read timeout is `WouldBlock` on Unix and `TimedOut` on Windows; both mean the same thing, and
                // because the timeout armed above is what is left of the deadline, both mean the deadline. Saying
                // "resource temporarily unavailable" instead would send the next reader looking for a resource.
                let timed_out = matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut);
                Err(ReadError {
                    message: if timed_out {
                        format!("Speculos did not finish answering within the deadline: {e}")
                    } else {
                        format!("Could not read from Speculos: {e}")
                    },
                    timed_out,
                })
            },
        }
    }

    /// Read one CRLF (or LF) terminated line, without the terminator. `Ok(None)` means the peer closed first.
    fn read_line(&mut self) -> Result<Option<String>, ReadError> {
        loop {
            if let Some(index) = self
                .buffer
                .get(self.position..)
                .and_then(|rest| rest.iter().position(|b| *b == b'\n'))
            {
                let end = self.position.saturating_add(index);
                let line = self.buffer.get(self.position..end).unwrap_or_default();
                let line = String::from_utf8_lossy(line).trim_end_matches('\r').to_string();
                self.position = end.saturating_add(1);
                return Ok(Some(line));
            }
            if self.buffer.len().saturating_sub(self.position) > MAX_RESPONSE_BYTES {
                return Err(ReadError {
                    message: "Speculos sent a line longer than the sanity bound".to_string(),
                    timed_out: false,
                });
            }
            if !self.fill()? {
                return Ok(None);
            }
        }
    }

    /// Read exactly `count` bytes.
    fn read_exact_bytes(&mut self, count: usize) -> Result<Option<Vec<u8>>, ReadError> {
        if count > MAX_RESPONSE_BYTES {
            return Err(ReadError {
                message: format!("Speculos announced {count} bytes, more than the sanity bound"),
                timed_out: false,
            });
        }
        while self.buffer.len().saturating_sub(self.position) < count {
            if !self.fill()? {
                return Ok(None);
            }
        }
        let end = self.position.saturating_add(count);
        let bytes = self.buffer.get(self.position..end).unwrap_or_default().to_vec();
        self.position = end;
        Ok(Some(bytes))
    }

    /// Read until the peer closes.
    fn read_to_end(&mut self) -> Result<Vec<u8>, ReadError> {
        while self.fill()? {
            if self.buffer.len().saturating_sub(self.position) > MAX_RESPONSE_BYTES {
                return Err(ReadError {
                    message: "Speculos sent a body longer than the sanity bound".to_string(),
                    timed_out: false,
                });
            }
        }
        Ok(self.buffer.get(self.position..).unwrap_or_default().to_vec())
    }
}

/// Read a HTTP status line and headers, returning the status code and the header lines.
fn read_head(reader: &mut ByteReader) -> Result<(u16, Vec<String>), ApiError> {
    let status_line = reader
        .read_line()
        .map_err(|e| ApiError(e.message))?
        .ok_or_else(|| ApiError("Speculos closed the connection without answering".to_string()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| {
            ApiError(format!(
                "Speculos answered with an unparseable status line: {status_line:?}"
            ))
        })?;

    let mut headers = Vec::new();
    loop {
        let line = reader
            .read_line()
            .map_err(|e| ApiError(e.message))?
            .ok_or_else(|| ApiError("Speculos closed the connection inside its headers".to_string()))?;
        if line.is_empty() {
            return Ok((status, headers));
        }
        headers.push(line);
    }
}

fn read_body(reader: &mut ByteReader, headers: &[String]) -> Result<Vec<u8>, ApiError> {
    let length = headers.iter().find_map(|header| {
        let (name, value) = header.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    });
    match length {
        Some(length) => reader
            .read_exact_bytes(length)
            .map_err(|e| ApiError(e.message))?
            .ok_or_else(|| ApiError("Speculos closed the connection inside its body".to_string())),
        // Werkzeug closes the connection after every non-streaming reply, so read-to-EOF is the framing whenever
        // there is no length. `/screenshot` in particular arrives this way.
        None => reader.read_to_end().map_err(|e| ApiError(e.message)),
    }
}

/// Read one line of a chunked body. `Ok(None)` means the stream ended cleanly.
fn read_chunked_line(reader: &mut ByteReader) -> Result<Option<String>, ReadError> {
    let Some(size_line) = reader.read_line()? else {
        return Ok(None);
    };
    // A chunk size may carry extensions after a `;`, which nothing here uses but which must not break parsing.
    let size_text = size_line.split(';').next().unwrap_or_default().trim();
    if size_text.is_empty() {
        // A stray blank line between chunks. Harmless, and reading on is the only sensible response.
        return Ok(Some(String::new()));
    }
    let size = usize::from_str_radix(size_text, 16).map_err(|e| ReadError {
        message: format!("Speculos sent an unparseable chunk size {size_text:?}: {e}"),
        timed_out: false,
    })?;
    if size == 0 {
        return Ok(None);
    }
    let Some(bytes) = reader.read_exact_bytes(size)? else {
        return Ok(None);
    };
    // The CRLF that terminates the chunk.
    reader.read_line()?;
    Ok(Some(String::from_utf8_lossy(&bytes).trim_end().to_string()))
}

#[cfg(test)]
mod test {
    use std::{
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    use super::*;

    /// A peer that answers, and then dribbles, must be given up on - not followed until the sanity cap.
    ///
    /// This is the failure a per-read socket timeout cannot catch. `SO_RCVTIMEO` restarts on every successful
    /// read, so a byte delivered just inside it resets the clock for ever; at the four megabyte cap that is weeks
    /// of "not technically hung". The bound has to be an absolute deadline, and this is the test that says so.
    ///
    /// The `sleep` below is the *fake server* choosing to be slow, which is the behaviour under test. There is no
    /// sleep on the harness side of the socket, and there is no retry.
    #[test]
    fn a_trickling_peer_is_given_up_on_rather_than_followed_to_the_sanity_bound() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut discard = [0u8; 1024];
            let _ = stream.read(&mut discard);
            // A complete set of headers promising a body, and then one byte of it at a time, for ever.
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\n");
            let _ = stream.flush();
            while !server_stop.load(Ordering::Relaxed) {
                if stream.write_all(b"x").is_err() || stream.flush().is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        });

        let api = SpeculosApi::new(address);
        let started = Instant::now();
        let error = api
            .current_screen()
            .expect_err("a peer that never finishes its body must not be followed for ever");
        let elapsed = started.elapsed();
        stop.store(true, Ordering::Relaxed);
        let _ = server.join();

        // Comfortably inside the request budget's own slack, and nowhere near the millions of reads the sanity
        // cap alone would have allowed.
        assert!(
            elapsed < REQUEST_TIMEOUT.saturating_mul(2),
            "gave up after {elapsed:?}, which is not a bound"
        );
        assert!(
            error.to_string().contains("deadline"),
            "the error should say the deadline passed, got: {error}"
        );
    }

    #[test]
    fn an_unset_api_address_is_the_default() {
        assert_eq!(api_address_from(None).unwrap(), SPECULOS_DEFAULT_API_ADDRESS);
    }

    #[test]
    fn an_api_address_is_taken_as_given() {
        assert_eq!(api_address_from(Some("10.0.0.1:5000")).unwrap(), "10.0.0.1:5000");
        assert_eq!(api_address_from(Some(" 127.0.0.1:55012\n")).unwrap(), "127.0.0.1:55012");
    }

    /// Set but empty is the shape `$(... api-address ...)` leaves behind when nothing is running, and falling back
    /// to the conventional port could press the buttons of an unrelated simulator. Same rule, same reason, as
    /// `SPECULOS_APDU_ADDRESS`.
    #[test]
    fn a_set_but_empty_api_address_is_an_error() {
        for value in ["", " ", "\n", "  \t\n "] {
            let message = api_address_from(Some(value))
                .expect_err("an empty API address must not silently become the default")
                .to_string();
            assert!(message.contains(SPECULOS_API_ADDRESS), "got: {message}");
            assert!(message.contains(SPECULOS_DEFAULT_API_ADDRESS), "got: {message}");
        }
    }

    #[test]
    fn events_are_parsed_with_their_geometry() {
        let body = br#"{"events": [{"text": "Reject", "x": 43, "y": 610, "w": 73, "h": 31, "clear": false}]}"#;
        let events = parse_events(body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text, "Reject");
        // The Step 0 spike: NBGL events carry geometry, so a tap is aimed at a widget's own rectangle.
        assert_eq!(events[0].centre(), (79, 625));
    }

    #[test]
    fn an_event_without_geometry_still_parses() {
        // Not every producer fills every field, and a missing coordinate must not lose the text - the text is the
        // part an assertion needs.
        let body = br#"{"events": [{"text": "Amount"}]}"#;
        let events = parse_events(body).unwrap();
        assert_eq!(events[0].text, "Amount");
        assert_eq!(events[0].centre(), (0, 0));
    }

    #[test]
    fn a_body_that_is_not_events_is_an_error() {
        assert!(parse_events(b"{}").is_err());
        assert!(parse_events(b"not json").is_err());
    }

    /// Texts are joined with nothing between them: a value too long for one line arrives as several events, and a
    /// separator would make the wrapped address unmatchable against the one that was asked for.
    #[test]
    fn a_screen_joins_its_texts_without_separators() {
        let texts = vec![
            ScreenText {
                text: " Receiver".to_string(),
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            ScreenText {
                text: "2311".to_string(),
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            ScreenText {
                text: "4wBq ".to_string(),
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
        ];
        assert_eq!(joined(&texts), "Receiver23114wBq");
    }

    #[test]
    fn touch_bodies_are_the_shapes_speculos_validates() {
        assert_eq!(
            Touch::Tap { x: 115, y: 516 }.body(),
            r#"{"action":"press-and-release","x":115,"y":516}"#
        );
        assert_eq!(
            Touch::Swipe {
                from: (300, 350),
                to: (80, 350)
            }
            .body(),
            r#"{"action":"press-and-release","x":300,"y":350,"x2":80,"y2":350}"#
        );
        assert_eq!(Touch::Press { x: 1, y: 2 }.body(), r#"{"action":"press","x":1,"y":2}"#);
        assert_eq!(
            Touch::Release { x: 1, y: 2 }.body(),
            r#"{"action":"release","x":1,"y":2}"#
        );
    }
}
