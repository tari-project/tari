//  Copyright 2024. The Tari Project
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
use std::{
    fmt,
    io::{ErrorKind, Read, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use log::*;
use serde::Deserialize;

use crate::stratum::{
    error::Error,
    stratum_controller::controller::{MAX_JOB_BLOB_LEN, SHARE_LOG_INTERVAL},
    stratum_types as types,
    stream::Stream,
};

pub const LOG_TARGET: &str = "minotari::miner::stratum::controller";
pub const LOG_TARGET_FILE: &str = "minotari::logging::miner::stratum::controller";

/// Longest line accepted from the pool. Covers the base64 of a maximum-size header (`u16::MAX` bytes of pow_data)
/// plus the JSON around it; `MAX_JOB_BLOB_LEN` is a second guard.
const MAX_LINE_LEN: usize = 128 * 1024;
/// Messages the mining controller can queue for the client thread; more are dropped.
const CLIENT_CHANNEL_CAPACITY: usize = 16;
/// Lines read from the pool per pass of the client loop before queued shares and keepalives are sent.
const MAX_LINES_PER_PASS: usize = 16;
/// Longest worker identifier accepted from the pool; it is echoed in every request we send.
const MAX_REQUEST_ID_LEN: usize = 256;
/// Number of characters of pool-supplied text written to the log.
const MAX_LOGGED_TEXT_LEN: usize = 256;

pub struct Controller {
    server_url: String,
    server_login: Option<String>,
    server_password: Option<String>,
    server_tls_enabled: Option<bool>,
    stream: Option<Stream>,
    read_buf: Vec<u8>,
    /// Bytes at the front of `read_buf` already searched for a newline.
    read_scanned: usize,
    rx: mpsc::Receiver<types::client_message::ClientMessage>,
    pub tx: mpsc::SyncSender<types::client_message::ClientMessage>,
    miner_tx: mpsc::SyncSender<types::miner_message::MinerMessage>,
    last_request_id: String,
    submits: SubmitCounts,
    /// Top-level RPC errors from the pool since the last log summary; only the first is logged in full.
    pool_errors: u64,
    /// Pool messages that could not be parsed or handled since the last log summary; only the first is logged in full.
    bad_messages: u64,
    last_logged_job_height: Option<u64>,
    share_log_time: Instant,
}

// fn invalid_error_response() -> types::RpcError {
// types::RpcError {
// code: 0,
// message: "Invalid error response received".to_owned(),
// }
// }

impl Controller {
    pub fn new(
        server_url: &str,
        server_login: Option<String>,
        server_password: Option<String>,
        server_tls_enabled: Option<bool>,
        miner_tx: mpsc::SyncSender<types::miner_message::MinerMessage>,
    ) -> Result<Controller, Error> {
        let (tx, rx) = mpsc::sync_channel::<types::client_message::ClientMessage>(CLIENT_CHANNEL_CAPACITY);
        Ok(Controller {
            server_url: server_url.to_string(),
            server_login,
            server_password,
            server_tls_enabled,
            stream: None,
            read_buf: Vec::new(),
            read_scanned: 0,
            tx,
            rx,
            miner_tx,
            last_request_id: "".to_string(),
            submits: SubmitCounts::default(),
            pool_errors: 0,
            bad_messages: 0,
            last_logged_job_height: None,
            share_log_time: Instant::now(),
        })
    }

    pub fn try_connect(&mut self) -> Result<(), Error> {
        self.stream = None;
        self.read_buf.clear();
        self.read_scanned = 0;
        // Answers to submits sent on the old connection will not arrive
        self.submits.pending = 0;
        let stream = Stream::try_connect(&self.server_url, self.server_tls_enabled)?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stream(&mut self) -> Result<&mut Stream, Error> {
        self.stream.as_mut().ok_or(Error::NotConnected)
    }

    /// Returns the next complete line from the pool, or `None` if no full line has arrived yet. Partial lines are
    /// kept in `read_buf` for the next call.
    fn read_message(&mut self) -> Result<Option<String>, Error> {
        let mut chunk = [0u8; 8192];
        loop {
            if let Some(line) = take_line(&mut self.read_buf, &mut self.read_scanned)? {
                return match String::from_utf8(line) {
                    Ok(line) => Ok(Some(line)),
                    Err(e) => {
                        error!(target: LOG_TARGET, "Communication error with stratum server: {e}");
                        Err(Error::Connection("invalid UTF-8 from server".to_string()))
                    },
                };
            }
            match self.stream()?.read(&mut chunk) {
                // stream is not returning a proper error on disconnect
                Ok(0) => return Err(Error::Connection("broken pipe".to_string())),
                Ok(n) => self.read_buf.extend_from_slice(chunk.get(..n).unwrap_or(&[])),
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {},
                Err(ref e) if e.kind() == ErrorKind::BrokenPipe => {
                    return Err(Error::Connection("broken pipe".to_string()));
                },
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(e) => {
                    error!(target: LOG_TARGET, "Communication error with stratum server: {e}");
                    return Err(Error::Connection("broken pipe".to_string()));
                },
            }
        }
    }

    fn send_message(&mut self, message: &str) -> Result<(), Error> {
        let stream = self.stream()?;
        debug!(target: LOG_TARGET_FILE, "sending request: {}", loggable(message));
        stream.write_all(message.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        Ok(())
    }

    fn send_message_get_job_template(&mut self) -> Result<(), Error> {
        let params = types::worker_identifier::WorkerIdentifier {
            id: self.last_request_id.clone(),
        };
        let req = types::rpc_request::RpcRequest {
            id: Some(self.last_request_id.clone()),
            jsonrpc: "2.0".to_string(),
            method: "getjob".to_string(),
            params: Some(serde_json::to_value(params)?),
        };
        let req_str = serde_json::to_string(&req)?;
        self.send_message(&req_str)
    }

    fn send_login(&mut self) -> Result<(), Error> {
        // only send the login request if a login string is configured
        let login_str = match self.server_login.clone() {
            None => "".to_string(),
            Some(server_login) => server_login,
        };
        if login_str.is_empty() {
            return Ok(());
        }
        let password_str = match self.server_password.clone() {
            None => "".to_string(),
            Some(server_password) => server_password,
        };
        let params = types::login_params::LoginParams {
            login: login_str,
            pass: password_str,
            agent: "minotari-miner".to_string(),
        };
        let req_id = self.last_request_id.to_string();
        let req = types::rpc_request::RpcRequest {
            id: if req_id.is_empty() {
                Some("0".to_string())
            } else {
                Some(req_id)
            },
            jsonrpc: "2.0".to_string(),
            method: "login".to_string(),
            params: Some(serde_json::to_value(params)?),
        };
        let req_str = serde_json::to_string(&req)?;
        self.send_message(&req_str)
    }

    fn send_keepalive(&mut self) -> Result<(), Error> {
        let req = types::rpc_request::RpcRequest {
            id: Some(self.last_request_id.to_string()),
            jsonrpc: "2.0".to_string(),
            method: "keepalive".to_string(),
            params: None,
        };
        let req_str = serde_json::to_string(&req)?;
        self.send_message(&req_str)
    }

    fn send_message_submit(&mut self, job_id: u64, hash: String, nonce: u64) -> Result<(), Error> {
        trace!(
            target: LOG_TARGET,
            "Submitting share with hash {hash} and nonce {nonce}"
        );
        let params_in = types::submit_params::SubmitParams {
            id: self.last_request_id.to_string(),
            job_id,
            hash,
            nonce,
        };
        let params = serde_json::to_string(&params_in)?;
        let req = types::rpc_request::RpcRequest {
            id: Some(self.last_request_id.to_string()),
            jsonrpc: "2.0".to_string(),
            method: "submit".to_string(),
            params: Some(serde_json::from_str(&params)?),
        };
        let req_str = serde_json::to_string(&req)?;
        self.send_message(&req_str)?;
        self.submits.submitted();
        Ok(())
    }

    fn send_miner_job(&mut self, job: types::job_params::JobParams) -> Result<(), Error> {
        use base64::{Engine, prelude::BASE64_STANDARD};
        let blob_bytes = BASE64_STANDARD
            .decode(&job.blob)
            .map_err(|_| Error::General("Invalid base64 byte string received".to_string()))?;
        if blob_bytes.len() > MAX_JOB_BLOB_LEN {
            return Err(Error::General(format!(
                "Job blob of {} bytes is larger than {MAX_JOB_BLOB_LEN} bytes",
                blob_bytes.len()
            )));
        }
        let target = job.target.parse::<u64>()?;
        if target == 0 {
            return Err(Error::General("Job target difficulty is 0".to_string()));
        }
        let miner_message =
            types::miner_message::MinerMessage::ReceivedJob(job.height, job.job_id.parse::<u64>()?, target, blob_bytes);
        self.miner_tx.send(miner_message).map_err(Error::from)
    }

    fn send_miner_stop(&mut self) -> Result<(), Error> {
        let miner_message = types::miner_message::MinerMessage::StopJob;
        self.miner_tx.send(miner_message).map_err(Error::from)
    }

    fn send_miner_resume(&mut self) -> Result<(), Error> {
        let miner_message = types::miner_message::MinerMessage::ResumeJob;
        self.miner_tx.send(miner_message).map_err(Error::from)
    }

    pub fn handle_request(&mut self, req: types::rpc_request::RpcRequest) -> Result<(), Error> {
        debug!(target: LOG_TARGET_FILE, "Received request type: {}", loggable(&req.method));
        match req.method.as_str() {
            "job" => match req.params {
                None => Err(Error::Request("No params in job request".to_owned())),
                Some(params) => {
                    let job = serde_json::from_value::<types::job_params::JobParams>(params)?;
                    self.log_new_job(job.height, &job.target);
                    self.send_miner_job(job)
                },
            },
            _ => Err(Error::Request("Unknown method".to_owned())),
        }
    }

    fn handle_error(&mut self, error: types::rpc_error::RpcError) {
        if [-1, 24].contains(&error.code) {
            // unauthorized
            let _result = self.send_login();
        } else if [21, 20, 22, 23, 25].contains(&error.code) {
            // problem with template
            let _result = self.send_message_get_job_template();
        } else {
            // unhandled
        }
    }

    #[allow(clippy::cognitive_complexity)]
    pub fn handle_response(&mut self, res: types::rpc_response::RpcResponse) -> Result<(), Error> {
        debug!(target: LOG_TARGET_FILE, "Received response with id: {}", loggable(&res.id));
        match res.result {
            Some(result) => {
                let login_response = types::login_response::LoginResponse::deserialize(&result);
                if let Ok(st) = login_response {
                    let fits = request_id_fits(&st.id);
                    // Only a login that changes the stored identifier is worth an info line
                    let level = if fits && st.id != self.last_request_id {
                        Level::Info
                    } else {
                        Level::Debug
                    };
                    log!(
                        target: LOG_TARGET,
                        level,
                        "Successful login to server, worker identifier is {}",
                        loggable(&st.id)
                    );
                    self.log_new_job(st.job.height, &st.job.target);
                    if fits {
                        self.last_request_id = st.id;
                    } else {
                        self.bad_message(format_args!(
                            "Ignoring worker identifier of {} bytes from the pool: {}",
                            st.id.len(),
                            loggable(&st.id)
                        ));
                    }
                    return self.send_miner_job(st.job);
                };
                let job_response = types::job_params::JobParams::deserialize(&result);
                if let Ok(st) = job_response {
                    self.log_new_job(st.height, &st.target);
                    return self.send_miner_job(st);
                };
                let submit_response = types::submit_response::SubmitResponse::deserialize(&result);
                if let Ok(st) = submit_response {
                    self.submits.answered(st.error.is_some());
                    if let Some(error) = st.error {
                        self.handle_error(error);
                    }
                    return Ok(());
                }
                let rpc_response = types::rpc_response::RpcResponse::deserialize(&result);
                if let Ok(st) = rpc_response {
                    let error = st.error;
                    if let Some(error) = error {
                        self.handle_error(error);
                    }
                    return Ok(());
                } else {
                    debug!(target: LOG_TARGET_FILE, "RPC Response: {}", loggable_debug(&rpc_response));
                };
            },
            None => {
                if self.pool_errors == 0 {
                    error!(target: LOG_TARGET, "RPC error: {}", loggable_debug(&res));
                }
                self.pool_errors = self.pool_errors.saturating_add(1);
                self.submits.answered(true);
            },
        }
        Ok(())
    }

    /// Logs a pool message that could not be parsed or handled, in full only for the first one per log summary.
    fn bad_message(&mut self, details: fmt::Arguments<'_>) {
        if self.bad_messages == 0 {
            error!(target: LOG_TARGET, "{details}");
        }
        self.bad_messages = self.bad_messages.saturating_add(1);
    }

    /// Logs a new job at info level when its height changes, and at debug level for further jobs at that height.
    fn log_new_job(&mut self, height: u64, target: &str) {
        let level = if self.last_logged_job_height == Some(height) {
            Level::Debug
        } else {
            Level::Info
        };
        self.last_logged_job_height = Some(height);
        log!(
            target: LOG_TARGET,
            level,
            "Got a new job for height {} with target difficulty {}",
            height,
            loggable(target)
        );
    }

    fn handle_message(&mut self, m: &str) {
        debug!(target: LOG_TARGET_FILE, "Received message: {}", loggable(m));
        let Ok(v) = serde_json::from_str::<serde_json::Value>(m) else {
            self.bad_message(format_args!("Error parsing message: {}", loggable(m)));
            return;
        };
        if classify_message(&v) == MessageKind::Request {
            match serde_json::from_value::<types::rpc_request::RpcRequest>(v) {
                Err(e) => self.bad_message(format_args!(
                    "Error parsing request {} : {}",
                    loggable(m),
                    loggable_debug(&e)
                )),
                Ok(request) => {
                    if let Err(err) = self.handle_request(request) {
                        self.bad_message(format_args!(
                            "Error handling request {} : :{}",
                            loggable(m),
                            loggable_debug(&err)
                        ))
                    }
                },
            }
        } else {
            match serde_json::from_value::<types::rpc_response::RpcResponse>(v) {
                Err(e) => self.bad_message(format_args!(
                    "Error parsing response {} : {}",
                    loggable(m),
                    loggable_debug(&e)
                )),
                Ok(response) => {
                    if let Err(err) = self.handle_response(response) {
                        self.bad_message(format_args!(
                            "Error handling response {} : :{}",
                            loggable(m),
                            loggable_debug(&err)
                        ))
                    }
                },
            }
        }
    }

    #[allow(clippy::cognitive_complexity)]
    pub fn run(mut self) {
        let server_read_interval = Duration::from_secs(1);
        let server_retry_interval = Duration::from_secs(5);
        let mut next_server_read = deadline_after(server_read_interval);
        let mut next_server_retry = Instant::now();
        // Request the first job template
        thread::sleep(Duration::from_secs(1));
        let mut was_disconnected = true;
        loop {
            // Check our connection status, and try to correct if possible
            if self.stream.is_none() {
                if !was_disconnected {
                    let _result = self.send_miner_stop();
                }
                was_disconnected = true;
                if Instant::now() > next_server_retry {
                    if self.try_connect().is_err() {
                        let status = format!(
                            "Connection Status: Can't establish server connection to {}. Will retry every {} seconds",
                            self.server_url,
                            server_retry_interval.as_secs()
                        );
                        warn!("{status}");
                        self.stream = None;
                    } else {
                        let status = format!("Connection Status: Connected to server at {}.", self.server_url);
                        info!(target: LOG_TARGET, "{status}");
                    }
                    next_server_retry = deadline_after(server_retry_interval);
                    if self.stream.is_none() {
                        thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                }
            } else {
                // get new job template
                if was_disconnected {
                    was_disconnected = false;
                    let _result = self.send_login();
                    let _result = self.send_miner_resume();
                }
                // read messages from server, a bounded number per pass so queued shares still get sent
                if Instant::now() > next_server_read {
                    for _ in 0..MAX_LINES_PER_PASS {
                        match self.read_message() {
                            Ok(Some(m)) => self.handle_message(&m),
                            Ok(None) => {
                                next_server_read = deadline_after(server_read_interval);
                                break;
                            },
                            Err(e) => {
                                error!(target: LOG_TARGET, "Error reading message: {e:?}");
                                self.stream = None;
                                break;
                            },
                        }
                    }
                }
            }

            // Talk to the miner algorithm, a bounded number of messages per pass so the pool is still read
            for _ in 0..CLIENT_CHANNEL_CAPACITY {
                let Ok(message) = self.rx.try_recv() else {
                    break;
                };
                debug!(target: LOG_TARGET_FILE, "Client received message: {message:?}");
                let result = match message {
                    types::client_message::ClientMessage::FoundSolution(job_id, hash, nonce) => {
                        self.send_message_submit(job_id, hash, nonce)
                    },
                    types::client_message::ClientMessage::KeepAlive => self.send_keepalive(),
                    types::client_message::ClientMessage::Shutdown => {
                        debug!(target: LOG_TARGET_FILE, "Shutting down client controller");
                        return;
                    },
                };
                if let Err(e) = result {
                    error!(target: LOG_TARGET, "Mining Controller Error {e:?}");
                    self.stream = None;
                }
            }
            self.log_submit_results();
            thread::sleep(Duration::from_millis(10));
        } // loop
    }

    /// Logs how many shares the pool accepted and rejected, and how many more RPC errors and bad messages it sent, at
    /// most once per `SHARE_LOG_INTERVAL`.
    fn log_submit_results(&mut self) {
        if self.share_log_time.elapsed() < SHARE_LOG_INTERVAL {
            return;
        }
        let secs = self.share_log_time.elapsed().as_secs();
        let more_errors = self.pool_errors.saturating_sub(1);
        let more_bad = self.bad_messages.saturating_sub(1);
        if self.submits.rejected > 0 || more_errors > 0 || more_bad > 0 {
            warn!(
                target: LOG_TARGET,
                "Pool rejected {} and accepted {} shares, and sent {more_errors} more RPC errors and {more_bad} more bad \
                 messages, in the last {secs}s",
                self.submits.rejected,
                self.submits.accepted
            );
        } else {
            if self.submits.accepted > 0 {
                info!(
                    target: LOG_TARGET,
                    "Pool accepted {} shares in the last {secs}s", self.submits.accepted
                );
            }
        }
        self.submits.accepted = 0;
        self.submits.rejected = 0;
        self.pool_errors = 0;
        self.bad_messages = 0;
        self.share_log_time = Instant::now();
    }
}

/// Share submits waiting for an answer, and the answers received since the last log summary.
#[derive(Default)]
struct SubmitCounts {
    pending: u64,
    accepted: u64,
    rejected: u64,
}

impl SubmitCounts {
    fn submitted(&mut self) {
        self.pending = self.pending.saturating_add(1);
    }

    /// Counts the answer to a submit. With no submit pending it answers something else, such as a keepalive.
    fn answered(&mut self, rejected: bool) {
        if self.pending == 0 {
            return;
        }
        self.pending = self.pending.saturating_sub(1);
        if rejected {
            self.rejected = self.rejected.saturating_add(1);
        } else {
            self.accepted = self.accepted.saturating_add(1);
        }
    }
}

fn request_id_fits(id: &str) -> bool {
    id.len() <= MAX_REQUEST_ID_LEN
}

#[derive(Debug, PartialEq, Eq)]
enum MessageKind {
    Request,
    Response,
}

/// A `"job"` notification is the only request the pool sends us; anything else is a response to one of ours.
fn classify_message(v: &serde_json::Value) -> MessageKind {
    if v.get("method").and_then(serde_json::Value::as_str) == Some("job") {
        MessageKind::Request
    } else {
        MessageKind::Response
    }
}

/// Splits the first complete line (without its `\n`) off the front of `buf`. Errors once a line grows past
/// `MAX_LINE_LEN`, whether or not its newline has arrived. `scanned` is how much of `buf` is already known to hold
/// no newline, so each call only searches the newly appended bytes.
fn take_line(buf: &mut Vec<u8>, scanned: &mut usize) -> Result<Option<Vec<u8>>, Error> {
    let found = buf
        .get(*scanned..)
        .unwrap_or(&[])
        .iter()
        .position(|b| *b == b'\n')
        .map(|p| scanned.saturating_add(p));
    match found {
        Some(pos) if pos <= MAX_LINE_LEN => {
            let mut line: Vec<u8> = buf.drain(..=pos).collect();
            line.pop();
            *scanned = 0;
            Ok(Some(line))
        },
        None if buf.len() <= MAX_LINE_LEN => {
            *scanned = buf.len();
            Ok(None)
        },
        _ => {
            error!(
                target: LOG_TARGET,
                "Stratum server sent a line longer than {MAX_LINE_LEN} bytes, reconnecting"
            );
            Err(Error::Connection("line too long".to_string()))
        },
    }
}

/// Pool-supplied text or value for the log: escaped, cut to `MAX_LOGGED_TEXT_LEN` characters, and only formatted
/// when the log record is written.
pub(crate) enum Loggable<'a> {
    Text(&'a str),
    Debug(&'a dyn fmt::Debug),
}

pub(crate) fn loggable(text: &str) -> Loggable<'_> {
    Loggable::Text(text)
}

pub(crate) fn loggable_debug(value: &dyn fmt::Debug) -> Loggable<'_> {
    Loggable::Debug(value)
}

impl fmt::Display for Loggable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut writer = CappedWriter {
            out: f,
            remaining: MAX_LOGGED_TEXT_LEN,
            truncated: false,
        };
        let result = match self {
            Loggable::Text(text) => fmt::Write::write_str(&mut writer, text),
            Loggable::Debug(value) => fmt::Write::write_fmt(&mut writer, format_args!("{value:?}")),
        };
        // The writer fails on purpose once it is full, which stops formatting the rest of the value.
        if writer.truncated { f.write_str("...") } else { result }
    }
}

/// Escapes what is written to it and fails once `remaining` characters have been written.
struct CappedWriter<'a, 'b> {
    out: &'a mut fmt::Formatter<'b>,
    remaining: usize,
    truncated: bool,
}

impl fmt::Write for CappedWriter<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if self.remaining == 0 {
                self.truncated = true;
                return Err(fmt::Error);
            }
            self.remaining = self.remaining.saturating_sub(1);
            self.out.write_fmt(format_args!("{}", c.escape_debug()))?;
        }
        Ok(())
    }
}

/// Returns the instant `interval` from now, clamped to now if the clock would overflow.
fn deadline_after(interval: Duration) -> Instant {
    Instant::now().checked_add(interval).unwrap_or_else(Instant::now)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn job_with_blob(blob_len: usize) -> types::job_params::JobParams {
        use base64::{Engine, prelude::BASE64_STANDARD};
        types::job_params::JobParams {
            job_id: "1".to_string(),
            blob: BASE64_STANDARD.encode(vec![0u8; blob_len]),
            target: "2".to_string(),
            height: 3,
        }
    }

    #[test]
    fn request_id_is_capped() {
        assert!(request_id_fits(&"w".repeat(MAX_REQUEST_ID_LEN)));
        assert!(!request_id_fits(&"w".repeat(MAX_REQUEST_ID_LEN + 1)));
    }

    #[test]
    fn only_answers_to_pending_submits_are_counted() {
        let mut counts = SubmitCounts::default();
        counts.answered(false);
        counts.answered(true);
        assert_eq!((counts.accepted, counts.rejected), (0, 0));

        counts.submitted();
        counts.submitted();
        counts.answered(false);
        counts.answered(true);
        counts.answered(false);
        assert_eq!((counts.pending, counts.accepted, counts.rejected), (0, 1, 1));
    }

    #[test]
    fn send_miner_job_rejects_a_zero_target() {
        let (miner_tx, miner_rx) = mpsc::sync_channel(1);
        let mut controller = Controller::new("127.0.0.1:1", None, None, None, miner_tx).unwrap();
        let mut job = job_with_blob(10);
        job.target = "0".to_string();
        assert!(controller.send_miner_job(job).is_err());
        assert!(miner_rx.try_recv().is_err());
    }

    #[test]
    fn send_miner_job_rejects_an_oversized_blob() {
        let (miner_tx, miner_rx) = mpsc::sync_channel(1);
        let mut controller = Controller::new("127.0.0.1:1", None, None, None, miner_tx).unwrap();
        assert!(controller.send_miner_job(job_with_blob(MAX_JOB_BLOB_LEN + 1)).is_err());
        assert!(miner_rx.try_recv().is_err());

        controller.send_miner_job(job_with_blob(MAX_JOB_BLOB_LEN)).unwrap();
        assert!(matches!(
            miner_rx.try_recv().unwrap(),
            types::miner_message::MinerMessage::ReceivedJob(3, 1, 2, blob) if blob.len() == MAX_JOB_BLOB_LEN
        ));
    }

    #[test]
    fn loggable_escapes_control_characters() {
        assert_eq!(loggable("a\nb\r\0").to_string(), "a\\nb\\r\\0");
        assert_eq!(loggable("\u{1b}[31mred").to_string(), "\\u{1b}[31mred");
    }

    #[test]
    fn loggable_truncates_on_a_char_boundary() {
        let text = "\u{e9}".repeat(MAX_LOGGED_TEXT_LEN + 1);
        let expected = format!("{}...", "\u{e9}".repeat(MAX_LOGGED_TEXT_LEN));
        assert_eq!(loggable(&text).to_string(), expected);
        assert_eq!(
            loggable(&"\u{e9}".repeat(MAX_LOGGED_TEXT_LEN)).to_string(),
            "\u{e9}".repeat(MAX_LOGGED_TEXT_LEN)
        );
    }

    #[test]
    fn loggable_truncates_long_input() {
        let text = "x".repeat(10_000);
        assert_eq!(
            loggable(&text).to_string(),
            format!("{}...", "x".repeat(MAX_LOGGED_TEXT_LEN))
        );
    }

    #[test]
    fn loggable_debug_stops_formatting_a_large_value_at_the_cap() {
        struct Big(std::cell::Cell<usize>);
        impl fmt::Debug for Big {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                for _ in 0..100_000 {
                    f.write_str("y")?;
                    self.0.set(self.0.get().saturating_add(1));
                }
                Ok(())
            }
        }

        let big = Big(std::cell::Cell::new(0));
        assert_eq!(
            loggable_debug(&big).to_string(),
            format!("{}...", "y".repeat(MAX_LOGGED_TEXT_LEN))
        );
        assert_eq!(big.0.get(), MAX_LOGGED_TEXT_LEN);
    }

    #[test]
    fn line_split_across_two_appends_is_joined() {
        let mut buf = b"{\"id\":".to_vec();
        let mut scanned = 0;
        assert!(take_line(&mut buf, &mut scanned).unwrap().is_none());
        buf.extend_from_slice(b"\"1\"}\n");
        assert_eq!(take_line(&mut buf, &mut scanned).unwrap().unwrap(), b"{\"id\":\"1\"}");
        assert!(buf.is_empty());
    }

    #[test]
    fn two_lines_in_one_append_are_returned_in_order() {
        let mut buf = b"first\nsecond\n".to_vec();
        let mut scanned = 0;
        assert_eq!(take_line(&mut buf, &mut scanned).unwrap().unwrap(), b"first");
        assert_eq!(take_line(&mut buf, &mut scanned).unwrap().unwrap(), b"second");
        assert!(take_line(&mut buf, &mut scanned).unwrap().is_none());
    }

    #[test]
    fn trailing_partial_line_is_kept() {
        let mut buf = b"done\npart".to_vec();
        let mut scanned = 0;
        assert_eq!(take_line(&mut buf, &mut scanned).unwrap().unwrap(), b"done");
        assert!(take_line(&mut buf, &mut scanned).unwrap().is_none());
        assert_eq!(buf, b"part");
    }

    #[test]
    fn line_over_many_appends_is_found_past_the_previous_scan() {
        let mut buf = Vec::new();
        let mut scanned = 0;
        for i in 0..100 {
            buf.extend_from_slice(b"0123456789");
            assert!(take_line(&mut buf, &mut scanned).unwrap().is_none());
            assert_eq!(scanned, (i + 1) * 10);
        }
        buf.extend_from_slice(b"end\nnext");
        let line = take_line(&mut buf, &mut scanned).unwrap().unwrap();
        assert_eq!(line.len(), 1003);
        assert!(line.ends_with(b"9end"));
        assert_eq!(buf, b"next");
        assert_eq!(scanned, 0);
    }

    #[test]
    fn line_at_the_cap_is_accepted() {
        let mut buf = vec![b'a'; MAX_LINE_LEN];
        buf.push(b'\n');
        let mut scanned = 0;
        assert_eq!(take_line(&mut buf, &mut scanned).unwrap().unwrap().len(), MAX_LINE_LEN);
    }

    #[test]
    fn oversized_line_is_an_error() {
        let mut buf = vec![b'a'; MAX_LINE_LEN];
        let mut scanned = 0;
        assert!(take_line(&mut buf, &mut scanned).unwrap().is_none());
        buf.push(b'a');
        assert!(matches!(take_line(&mut buf, &mut scanned), Err(Error::Connection(_))));
        buf.push(b'\n');
        assert!(matches!(take_line(&mut buf, &mut scanned), Err(Error::Connection(_))));
    }

    #[test]
    fn login_response_is_a_response() {
        let v = json!({"id": "1", "jsonrpc": "2.0", "result": {"id": "w", "job": null}, "error": null});
        assert_eq!(classify_message(&v), MessageKind::Response);
    }

    #[test]
    fn submit_response_is_a_response() {
        let v = json!({"id": "2", "jsonrpc": "2.0", "result": {"status": "OK"}, "error": null});
        assert_eq!(classify_message(&v), MessageKind::Response);
    }

    #[test]
    fn missing_or_non_string_method_is_a_response() {
        assert_eq!(classify_message(&json!({})), MessageKind::Response);
        assert_eq!(classify_message(&json!({"method": null})), MessageKind::Response);
        assert_eq!(classify_message(&json!({"method": 123})), MessageKind::Response);
    }

    #[test]
    fn response_echoing_a_method_is_a_response() {
        let v = json!({"id": "1", "method": "login", "result": {"id": "w", "job": null}, "error": null});
        assert_eq!(classify_message(&v), MessageKind::Response);
    }

    #[test]
    fn unknown_method_is_a_response() {
        let v = json!({"id": "4", "jsonrpc": "2.0", "method": "weird", "params": {}});
        assert_eq!(classify_message(&v), MessageKind::Response);
    }

    #[test]
    fn job_method_is_a_request() {
        let v = json!({"id": "3", "jsonrpc": "2.0", "method": "job", "params": {}});
        assert_eq!(classify_message(&v), MessageKind::Request);
    }
}
