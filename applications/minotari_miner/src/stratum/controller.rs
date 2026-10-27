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
    io::{ErrorKind, Read, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use log::*;
use serde::Deserialize;

use crate::stratum::{error::Error, stratum_types as types, stream::Stream};

pub const LOG_TARGET: &str = "minotari::miner::stratum::controller";
pub const LOG_TARGET_FILE: &str = "minotari::logging::miner::stratum::controller";

/// Longest line accepted from the pool. Real job blobs are a few KB of header, so this is generous.
const MAX_LINE_LEN: usize = 1024 * 1024;
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
    pub tx: mpsc::Sender<types::client_message::ClientMessage>,
    miner_tx: mpsc::Sender<types::miner_message::MinerMessage>,
    last_request_id: String,
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
        miner_tx: mpsc::Sender<types::miner_message::MinerMessage>,
    ) -> Result<Controller, Error> {
        let (tx, rx) = mpsc::channel::<types::client_message::ClientMessage>();
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
        })
    }

    pub fn try_connect(&mut self) -> Result<(), Error> {
        self.stream = None;
        self.read_buf.clear();
        self.read_scanned = 0;
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
        debug!(
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
        self.send_message(&req_str)
    }

    fn send_miner_job(&mut self, job: types::job_params::JobParams) -> Result<(), Error> {
        use base64::{Engine, prelude::BASE64_STANDARD};
        let blob_bytes = BASE64_STANDARD
            .decode(&job.blob)
            .map_err(|_| Error::General("Invalid base64 byte string received".to_string()))?;
        let miner_message = types::miner_message::MinerMessage::ReceivedJob(
            job.height,
            job.job_id.parse::<u64>()?,
            job.target.parse::<u64>()?,
            blob_bytes,
        );
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
                    info!(
                        target: LOG_TARGET,
                        "Got a new job for height {} with target difficulty {}", job.height, loggable(&job.target)
                    );
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
                    info!(
                        target: LOG_TARGET,
                        "Successful login to server, worker identifier is {}", loggable(&st.id)
                    );
                    info!(
                        target: LOG_TARGET,
                        "Got a new job for height {} with target difficulty {}", st.job.height, loggable(&st.job.target)
                    );
                    self.last_request_id = st.id;
                    let _result = self.send_miner_job(st.job);
                    return Ok(());
                };
                let job_response = types::job_params::JobParams::deserialize(&result);
                if let Ok(st) = job_response {
                    info!(
                        target: LOG_TARGET,
                        "Got a new job for height {} with target difficulty {}", st.height, loggable(&st.target)
                    );
                    let _result = self.send_miner_job(st);
                    return Ok(());
                };
                let submit_response = types::submit_response::SubmitResponse::deserialize(&result);
                if let Ok(st) = submit_response {
                    let error = st.error;
                    if let Some(error) = error {
                        // rejected share
                        self.handle_error(error);
                        warn!(target: LOG_TARGET, "Rejected");
                    } else {
                        // accepted share
                        debug!(target: LOG_TARGET, "Share accepted: {}", loggable(&format!("{:?}", st.status)));
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
                    debug!(target: LOG_TARGET_FILE, "RPC Response: {}", loggable(&format!("{rpc_response:?}")));
                };
            },
            None => {
                error!(target: LOG_TARGET, "RPC error: {}", loggable(&format!("{res:?}")));
            },
        }
        Ok(())
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
                // read messages from server
                if Instant::now() > next_server_read {
                    match self.read_message() {
                        Ok(Some(m)) => {
                            // figure out what kind of message,
                            // and dispatch appropriately
                            debug!(target: LOG_TARGET_FILE, "Received message: {}", loggable(&m));
                            // Deserialize to see what type of object it is
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&m) {
                                if classify_message(&v) == MessageKind::Request {
                                    match serde_json::from_value::<types::rpc_request::RpcRequest>(v) {
                                        Err(e) => {
                                            error!(target: LOG_TARGET, "Error parsing request {} : {}", loggable(&m), loggable(&format!("{e:?}")))
                                        },
                                        Ok(request) => {
                                            if let Err(err) = self.handle_request(request) {
                                                error!(target: LOG_TARGET, "Error handling request {} : :{}", loggable(&m), loggable(&format!("{err:?}")))
                                            }
                                        },
                                    }
                                } else {
                                    // this is a response
                                    match serde_json::from_value::<types::rpc_response::RpcResponse>(v) {
                                        Err(e) => {
                                            error!(target: LOG_TARGET, "Error parsing response {} : {}", loggable(&m), loggable(&format!("{e:?}")))
                                        },
                                        Ok(response) => {
                                            if let Err(err) = self.handle_response(response) {
                                                error!(target: LOG_TARGET, "Error handling response {} : :{}", loggable(&m), loggable(&format!("{err:?}")))
                                            }
                                        },
                                    }
                                }
                                continue;
                            } else {
                                error!(target: LOG_TARGET, "Error parsing message: {}", loggable(&m))
                            }
                        },
                        Ok(None) => {
                            // noop, nothing to read for this interval
                        },
                        Err(e) => {
                            error!(target: LOG_TARGET, "Error reading message: {e:?}");
                            self.stream = None;
                            continue;
                        },
                    }
                    next_server_read = deadline_after(server_read_interval);
                }
            }

            // Talk to the miner algorithm
            while let Some(message) = self.rx.try_iter().next() {
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
            thread::sleep(Duration::from_millis(10));
        } // loop
    }
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

/// Escapes pool-supplied text for the log and cuts it to `MAX_LOGGED_TEXT_LEN` characters.
pub(crate) fn loggable(text: &str) -> String {
    let mut out: String = text
        .chars()
        .take(MAX_LOGGED_TEXT_LEN)
        .flat_map(char::escape_debug)
        .collect();
    if text.chars().nth(MAX_LOGGED_TEXT_LEN).is_some() {
        out.push_str("...");
    }
    out
}

/// Returns the instant `interval` from now, clamped to now if the clock would overflow.
fn deadline_after(interval: Duration) -> Instant {
    Instant::now().checked_add(interval).unwrap_or_else(Instant::now)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn loggable_escapes_control_characters() {
        assert_eq!(loggable("a\nb\r\0"), "a\\nb\\r\\0");
        assert_eq!(loggable("\u{1b}[31mred"), "\\u{1b}[31mred");
    }

    #[test]
    fn loggable_truncates_on_a_char_boundary() {
        let text = "\u{e9}".repeat(MAX_LOGGED_TEXT_LEN + 1);
        let expected = format!("{}...", "\u{e9}".repeat(MAX_LOGGED_TEXT_LEN));
        assert_eq!(loggable(&text), expected);
        assert_eq!(
            loggable(&"\u{e9}".repeat(MAX_LOGGED_TEXT_LEN)),
            "\u{e9}".repeat(MAX_LOGGED_TEXT_LEN)
        );
    }

    #[test]
    fn loggable_truncates_long_input() {
        let text = "x".repeat(10_000);
        assert_eq!(loggable(&text), format!("{}...", "x".repeat(MAX_LOGGED_TEXT_LEN)));
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
