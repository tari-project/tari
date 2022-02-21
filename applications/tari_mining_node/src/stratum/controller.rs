// Copyright 2021. The Tari Project
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
//
use std::{
    self,
    io::{BufRead, ErrorKind, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use log::*;

use crate::stratum::{error::Error, stratum_types as types, stream::Stream};

pub const LOG_TARGET: &str = "tari_mining_node::miner::stratum::controller";
pub const LOG_TARGET_FILE: &str = "tari_mining_node::logging::miner::stratum::controller";

pub struct Controller {
    server_url: String,
    server_login: Option<String>,
    server_password: Option<String>,
    server_tls_enabled: Option<bool>,
    stream: Option<Stream>,
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
            tx,
            rx,
            miner_tx,
            last_request_id: "".to_string(),
        })
    }

    pub fn try_connect(&mut self) -> Result<(), Error> {
        self.stream = Some(Stream::new());
        self.stream
            .as_mut()
            .unwrap()
            .try_connect(&self.server_url, self.server_tls_enabled)?;
        Ok(())
    }

    fn read_message(&mut self) -> Result<Option<String>, Error> {
        if self.stream.is_none() {
            return Err(Error::Connection("broken pipe".to_string()));
        }
        let mut line = String::new();
        match self.stream.as_mut().unwrap().read_line(&mut line) {
            Ok(_) => {
                // stream is not returning a proper error on disconnect
                if line.is_empty() {
                    return Err(Error::Connection("broken pipe".to_string()));
                }
                Ok(Some(line))
            },
            Err(ref e) if e.kind() == ErrorKind::BrokenPipe => Err(Error::Connection("broken pipe".to_string())),
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) => {
                error!(target: LOG_TARGET, "Communication error with stratum server: {}", e);
                Err(Error::Connection("broken pipe".to_string()))
            },
        }
    }

    fn send_message(&mut self, message: &str) -> Result<(), Error> {
        if self.stream.is_none() {
            return Err(Error::Connection(String::from("No server connection")));
        }
        debug!(target: LOG_TARGET_FILE, "sending request: {}", message);
        let _ = self.stream.as_mut().unwrap().write(message.as_bytes());
        let _ = self.stream.as_mut().unwrap().write(b"\n");
        let _ = self.stream.as_mut().unwrap().flush();
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
            agent: "tari-miner".to_string(),
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
            "Submitting share with hash {} and nonce {}", hash, nonce
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
        let miner_message = types::miner_message::MinerMessage::ReceivedJob(
            job.height,
            job.job_id.parse::<u64>().unwrap(),
            job.target.parse::<u64>().unwrap(),
            job.blob,
        );
        self.miner_tx.send(miner_message).map_err(|e| e.into())
    }

    fn send_miner_stop(&mut self) -> Result<(), Error> {
        let miner_message = types::miner_message::MinerMessage::StopJob;
        self.miner_tx.send(miner_message).map_err(|e| e.into())
    }

    fn send_miner_resume(&mut self) -> Result<(), Error> {
        let miner_message = types::miner_message::MinerMessage::ResumeJob;
        self.miner_tx.send(miner_message).map_err(|e| e.into())
    }

    pub fn handle_request(&mut self, req: types::rpc_request::RpcRequest) -> Result<(), Error> {
        debug!(target: LOG_TARGET_FILE, "Received request type: {}", req.method);
        match req.method.as_str() {
            "job" => match req.params {
                None => Err(Error::Request("No params in job request".to_owned())),
                Some(params) => {
                    let job = serde_json::from_value::<types::job_params::JobParams>(params)?;
                    info!(
                        target: LOG_TARGET,
                        "Got a new job for height {} with target difficulty {}", job.height, job.target
                    );
                    self.send_miner_job(job)
                },
            },
            _ => Err(Error::Request("Unknown method".to_owned())),
        }
    }

    fn handle_error(&mut self, error: types::rpc_error::RpcError) {
        if vec![-1, 24].contains(&error.code) {
            // unauthorized
            let _ = self.send_login();
        } else if vec![21, 20, 22, 23, 25].contains(&error.code) {
            // problem with template
            let _ = self.send_message_get_job_template();
        }
    }

    #[allow(clippy::cognitive_complexity)]
    pub fn handle_response(&mut self, res: types::rpc_response::RpcResponse) -> Result<(), Error> {
        debug!(target: LOG_TARGET_FILE, "Received response with id: {}", res.id);
        match res.result {
            Some(result) => {
                let login_response = serde_json::from_value::<types::login_response::LoginResponse>(result.clone());
                if let Ok(st) = login_response {
                    info!(
                        target: LOG_TARGET,
                        "Successful login to server, worker identifier is {}", st.id
                    );
                    info!(
                        target: LOG_TARGET,
                        "Got a new job for height {} with target difficulty {}", st.job.height, st.job.target
                    );
                    self.last_request_id = st.id;
                    let _ = self.send_miner_job(st.job);
                    return Ok(());
                };
                let job_response = serde_json::from_value::<types::job_params::JobParams>(result.clone());
                if let Ok(st) = job_response {
                    info!(
                        target: LOG_TARGET,
                        "Got a new job for height {} with target difficulty {}", st.height, st.target
                    );
                    let _ = self.send_miner_job(st);
                    return Ok(());
                };
                let submit_response = serde_json::from_value::<types::submit_response::SubmitResponse>(result.clone());
                if let Ok(st) = submit_response {
                    let error = st.error;
                    if let Some(error) = error {
                        // rejected share
                        self.handle_error(error);
                        warn!(target: LOG_TARGET, "Rejected");
                    } else {
                        // accepted share
                        debug!(target: LOG_TARGET, "Share accepted: {:?}", st.status);
                    }
                    return Ok(());
                }
                let rpc_response = serde_json::from_value::<types::rpc_response::RpcResponse>(result);
                if let Ok(st) = rpc_response {
                    let error = st.error;
                    if let Some(error) = error {
                        self.handle_error(error);
                    }
                    return Ok(());
                } else {
                    debug!(target: LOG_TARGET_FILE, "RPC Response: {:?}", rpc_response);
                };
            },
            None => {
                error!(target: LOG_TARGET, "RPC error: {:?}", res);
            },
        }
        Ok(())
    }

    #[allow(clippy::cognitive_complexity)]
    pub fn run(mut self) {
        let server_read_interval = Duration::from_secs(1);
        let server_retry_interval = Duration::from_secs(5);
        let mut next_server_read = Instant::now() + server_read_interval;
        let mut next_server_retry = Instant::now();
        // Request the first job template
        thread::sleep(Duration::from_secs(1));
        let mut was_disconnected = true;
        loop {
            // Check our connection status, and try to correct if possible
            if self.stream.is_none() {
                if !was_disconnected {
                    let _ = self.send_miner_stop();
                }
                was_disconnected = true;
                if Instant::now() > next_server_retry {
                    if self.try_connect().is_err() {
                        let status = format!(
                            "Connection Status: Can't establish server connection to {}. Will retry every {} seconds",
                            self.server_url,
                            server_retry_interval.as_secs()
                        );
                        warn!("{}", status);
                        self.stream = None;
                    } else {
                        let status = format!("Connection Status: Connected to server at {}.", self.server_url);
                        info!(target: LOG_TARGET, "{}", status);
                    }
                    next_server_retry = Instant::now() + server_retry_interval;
                    if self.stream.is_none() {
                        thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                }
            } else {
                // get new job template
                if was_disconnected {
                    was_disconnected = false;
                    let _ = self.send_login();
                    let _ = self.send_miner_resume();
                }
                // read messages from server
                if Instant::now() > next_server_read {
                    match self.read_message() {
                        Ok(Some(m)) => {
                            // figure out what kind of message,
                            // and dispatch appropriately
                            debug!(target: LOG_TARGET_FILE, "Received message: {}", m);
                            // Deserialize to see what type of object it is
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&m) {
                                // Is this a response or request?
                                if v["method"] == "job" {
                                    // this is a request
                                    match serde_json::from_str::<types::rpc_request::RpcRequest>(&m) {
                                        Err(e) => error!(target: LOG_TARGET, "Error parsing request {} : {:?}", m, e),
                                        Ok(request) => {
                                            if let Err(err) = self.handle_request(request) {
                                                error!(target: LOG_TARGET, "Error handling request {} : :{:?}", m, err)
                                            }
                                        },
                                    }
                                } else {
                                    // this is a response
                                    match serde_json::from_str::<types::rpc_response::RpcResponse>(&m) {
                                        Err(e) => error!(target: LOG_TARGET, "Error parsing response {} : {:?}", m, e),
                                        Ok(response) => {
                                            if let Err(err) = self.handle_response(response) {
                                                error!(target: LOG_TARGET, "Error handling response {} : :{:?}", m, err)
                                            }
                                        },
                                    }
                                }
                                continue;
                            } else {
                                error!(target: LOG_TARGET, "Error parsing message: {}", m)
                            }
                        },
                        Ok(None) => {
                            // noop, nothing to read for this interval
                        },
                        Err(e) => {
                            error!(target: LOG_TARGET, "Error reading message: {:?}", e);
                            self.stream = None;
                            continue;
                        },
                    }
                    next_server_read = Instant::now() + server_read_interval;
                }
            }

            // Talk to the miner algorithm
            while let Some(message) = self.rx.try_iter().next() {
                debug!(target: LOG_TARGET_FILE, "Client received message: {:?}", message);
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
                    error!(target: LOG_TARGET, "Mining Controller Error {:?}", e);
                    self.stream = None;
                }
            }
            thread::sleep(Duration::from_millis(10));
        } // loop
    }
}
