use crate::stratum_types as types;
use bufstream::BufStream;
use chrono::Local;
use log::*;
use native_tls::{TlsConnector, TlsStream};
use std::{
    self,
    io::{self, BufRead, ErrorKind, Read, Write},
    net::TcpStream,
    sync::mpsc,
    thread,
};

#[derive(Debug)]
pub enum Error {
    ConnectionError(String),
    RequestError(String),
    // ResponseError(String),
    JsonError(String),
    GeneralError(String),
}

impl From<serde_json::error::Error> for Error {
    fn from(error: serde_json::error::Error) -> Self {
        Error::JsonError(format!("Failed to parse JSON: {:?}", error))
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(error: std::sync::PoisonError<T>) -> Self {
        Error::GeneralError(format!("Failed to get lock: {:?}", error))
    }
}

impl<T> From<std::sync::mpsc::SendError<T>> for Error {
    fn from(error: std::sync::mpsc::SendError<T>) -> Self {
        Error::GeneralError(format!("Failed to send to a channel: {:?}", error))
    }
}

struct Stream {
    stream: Option<BufStream<TcpStream>>,
    tls_stream: Option<BufStream<TlsStream<TcpStream>>>,
}

impl Stream {
    fn new() -> Stream {
        Stream {
            stream: None,
            tls_stream: None,
        }
    }

    fn try_connect(&mut self, server_url: &str, tls: Option<bool>) -> Result<(), Error> {
        match TcpStream::connect(server_url) {
            Ok(conn) => {
                if tls.is_some() && tls.unwrap() {
                    let connector = TlsConnector::new()
                        .map_err(|e| Error::ConnectionError(format!("Can't create TLS connector: {:?}", e)))?;
                    let url_port: Vec<&str> = server_url.split(':').collect();
                    let split_url: Vec<&str> = url_port[0].split('.').collect();
                    let base_host = format!("{}.{}", split_url[split_url.len() - 2], split_url[split_url.len() - 1]);
                    let mut stream = connector
                        .connect(&base_host, conn)
                        .map_err(|e| Error::ConnectionError(format!("Can't establish TLS connection: {:?}", e)))?;
                    stream
                        .get_mut()
                        .set_nonblocking(true)
                        .map_err(|e| Error::ConnectionError(format!("Can't switch to nonblocking mode: {:?}", e)))?;
                    self.tls_stream = Some(BufStream::new(stream));
                } else {
                    conn.set_nonblocking(true)
                        .map_err(|e| Error::ConnectionError(format!("Can't switch to nonblocking mode: {:?}", e)))?;
                    self.stream = Some(BufStream::new(conn));
                }
                Ok(())
            },
            Err(e) => Err(Error::ConnectionError(format!("{}", e))),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, b: &[u8]) -> Result<usize, std::io::Error> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().write(b)
        } else {
            self.stream.as_mut().unwrap().write(b)
        }
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().flush()
        } else {
            self.stream.as_mut().unwrap().flush()
        }
    }
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().read(buf)
        } else {
            self.stream.as_mut().unwrap().read(buf)
        }
    }
}

impl BufRead for Stream {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().fill_buf()
        } else {
            self.stream.as_mut().unwrap().fill_buf()
        }
    }

    fn consume(&mut self, amt: usize) {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().consume(amt)
        } else {
            self.stream.as_mut().unwrap().consume(amt)
        }
    }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> io::Result<usize> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().read_until(byte, buf)
        } else {
            self.stream.as_mut().unwrap().read_until(byte, buf)
        }
    }

    fn read_line(&mut self, string: &mut String) -> io::Result<usize> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().read_line(string)
        } else {
            self.stream.as_mut().unwrap().read_line(string)
        }
    }
}

pub struct Controller {
    server_url: String,
    server_login: Option<String>,
    server_password: Option<String>,
    server_tls_enabled: Option<bool>,
    stream: Option<Stream>,
    rx: mpsc::Receiver<types::ClientMessage>,
    pub tx: mpsc::Sender<types::ClientMessage>,
    miner_tx: mpsc::Sender<types::MinerMessage>,
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
        miner_tx: mpsc::Sender<types::MinerMessage>,
    ) -> Result<Controller, Error> {
        let (tx, rx) = mpsc::channel::<types::ClientMessage>();
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
            return Err(Error::ConnectionError("broken pipe".to_string()));
        }
        let mut line = String::new();
        match self.stream.as_mut().unwrap().read_line(&mut line) {
            Ok(_) => {
                // stream is not returning a proper error on disconnect
                if line.is_empty() {
                    return Err(Error::ConnectionError("broken pipe".to_string()));
                }
                Ok(Some(line))
            },
            Err(ref e) if e.kind() == ErrorKind::BrokenPipe => Err(Error::ConnectionError("broken pipe".to_string())),
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) => {
                error!("Communication error with stratum server: {}", e);
                Err(Error::ConnectionError("broken pipe".to_string()))
            },
        }
    }

    fn send_message(&mut self, message: &str) -> Result<(), Error> {
        if self.stream.is_none() {
            return Err(Error::ConnectionError(String::from("No server connection")));
        }
        debug!("sending request: {}", message);
        let _ = self.stream.as_mut().unwrap().write(message.as_bytes());
        let _ = self.stream.as_mut().unwrap().write(b"\n");
        let _ = self.stream.as_mut().unwrap().flush();
        Ok(())
    }

    // TODO: Request new templates when exceeding average solving time in addition to jobs sent from server
    // fn send_message_get_job_template(&mut self) -> Result<(), Error> {
    // let params = types::WorkerIdentifier {
    // id: self.last_request_id.clone(),
    // };
    //
    // let req = types::RpcRequest {
    // id: Some(self.last_request_id.clone()),
    // jsonrpc: "2.0".to_string(),
    // method: "getjob".to_string(),
    // params: Some(serde_json::to_value(params)?),
    // };
    // let req_str = serde_json::to_string(&req)?;
    // self.send_message(&req_str)
    // }

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
        let params = types::LoginParams {
            login: login_str,
            pass: password_str,
            agent: "tari-miner".to_string(),
        };
        let req_id = self.last_request_id.to_string();
        let req = types::RpcRequest {
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

    fn send_message_submit(&mut self, job_id: u64, hash: String, nonce: u64) -> Result<(), Error> {
        info!("Submitting Solution");
        let params_in = types::SubmitParams {
            id: self.last_request_id.to_string(),
            job_id,
            hash,
            nonce,
        };
        let params = serde_json::to_string(&params_in)?;
        let req = types::RpcRequest {
            id: Some(self.last_request_id.to_string()),
            jsonrpc: "2.0".to_string(),
            method: "submit".to_string(),
            params: Some(serde_json::from_str(&params)?),
        };
        let req_str = serde_json::to_string(&req)?;
        self.send_message(&req_str)
    }

    fn send_miner_job(&mut self, job: types::JobParams) -> Result<(), Error> {
        let miner_message = types::MinerMessage::ReceivedJob(
            job.height,
            job.job_id.parse::<u64>().unwrap(),
            job.target.parse::<u64>().unwrap(),
            job.blob,
        );
        self.miner_tx.send(miner_message).map_err(|e| e.into())
    }

    fn send_miner_stop(&mut self) -> Result<(), Error> {
        let miner_message = types::MinerMessage::StopJob;
        self.miner_tx.send(miner_message).map_err(|e| e.into())
    }

    pub fn handle_request(&mut self, req: types::RpcRequest) -> Result<(), Error> {
        debug!("Received request type: {}", req.method);
        match req.method.as_str() {
            "job" => match req.params {
                None => Err(Error::RequestError("No params in job request".to_owned())),
                Some(params) => {
                    let job = serde_json::from_value::<types::JobParams>(params)?;
                    info!("Got a new job: {:?}", job);
                    self.send_miner_job(job)
                },
            },
            _ => Err(Error::RequestError("Unknown method".to_owned())),
        }
    }

    pub fn handle_response(&mut self, res: types::RpcResponse) -> Result<(), Error> {
        debug!("Received response with id: {}", res.id);
        match res.result {
            Some(result) => {
                let login_response = serde_json::from_value::<types::LoginResponse>(result.clone());
                match login_response {
                    Ok(st) => {
                        println!("{:?}", st);
                        let date = Local::now();
                        println!("\r\n{}", date.format("[%Y-%m-%d][%H:%M:%S]"));
                        println!("\r\n\r\n");
                        self.last_request_id = st.id;
                        let _ = self.send_miner_job(st.job);
                        // let _ = self.send_message_get_job_template();
                        return Ok(());
                    },
                    Err(_e) => {},
                }
                let job_response = serde_json::from_value::<types::JobParams>(result);
                match job_response {
                    Ok(st) => {
                        println!("{:?}", st);
                        let date = Local::now();
                        println!("\r\n{}", date.format("[%Y-%m-%d][%H:%M:%S]"));
                        println!("\r\n\r\n");
                        let _ = self.send_miner_job(st);
                        return Ok(());
                    },
                    Err(_e) => {},
                }
            },
            None => {
                println!("{:?}", res);
            },
        }

        // TODO: Implement these
        // match res.id.as_str() {
        // "submit" response
        // "submit" => {
        // if let Some(result) = res.result {
        // info!("Share Accepted!!");
        // let result = serde_json::to_string(&result)?;
        // if result.contains("blockfound") {
        // info!("Block Found!!");
        // stats.client_stats.last_message_received =
        //    "Last Message Received: Block Found!!".to_string();
        // stats.mining_stats.solution_stats.num_blocks_found += 1;
        // }
        // } else {
        // let err = res.error.unwrap_or_else(invalid_error_response);
        // if err.message.contains("too late") {
        // stats.mining_stats.solution_stats.num_staled += 1;
        // } else {
        // stats.mining_stats.solution_stats.num_rejected += 1;
        // }
        // error!("Failed to submit a solution: {:?}", err);
        // }
        // Ok(())
        // }
        // "keepalive" response
        // "keepalive" => {
        // if res.result.is_some() {
        // Nothing to do for keepalive "ok"
        // dont update last_message_received with good keepalive response
        // } else {
        // let err = res.error.unwrap_or_else(invalid_error_response);
        // let mut stats = self.stats.write()?;
        // stats.client_stats.last_message_received = format!(
        //    "Last Message Received: Failed to request keepalive: {:?}",
        //    err
        // );
        // error!("Failed to request keepalive: {:?}", err);
        // }
        // Ok(())
        // }
        // unknown method response
        // _ => {
        // let mut stats = self.stats.write()?;
        // stats.client_stats.last_message_received =
        //    format!("Last Message Received: Unknown Response: {:?}", res);
        // warn!("Unknown Response: {:?}", res);
        // Ok(())
        // }
        // }
        Ok(())
    }

    #[allow(clippy::cognitive_complexity)]
    pub fn run(mut self) {
        let server_read_interval = 1;
        let server_retry_interval = 5;
        let mut next_server_read = time::get_time().sec + server_read_interval;
        let mut next_server_retry = time::get_time().sec;
        // Request the first job template
        thread::sleep(std::time::Duration::from_secs(1));
        let mut was_disconnected = true;
        loop {
            // Check our connection status, and try to correct if possible
            if self.stream.is_none() {
                if !was_disconnected {
                    let _ = self.send_miner_stop();
                }
                was_disconnected = true;
                if time::get_time().sec > next_server_retry {
                    if self.try_connect().is_err() {
                        let status = format!(
                            "Connection Status: Can't establish server connection to {}. Will retry every {} seconds",
                            self.server_url, server_retry_interval
                        );
                        warn!("{}", status);
                        self.stream = None;
                    } else {
                        let status = format!("Connection Status: Connected to server at {}.", self.server_url);
                        warn!("{}", status);
                    }
                    next_server_retry = time::get_time().sec + server_retry_interval;
                    if self.stream.is_none() {
                        thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                }
            } else {
                // get new job template
                if was_disconnected {
                    let _ = self.send_login();
                    was_disconnected = false;
                }
                // read messages from server
                if time::get_time().sec > next_server_read {
                    match self.read_message() {
                        Ok(message) => {
                            if let Some(m) = message {
                                // figure out what kind of message,
                                // and dispatch appropriately
                                debug!("Received message: {}", m);
                                // Deserialize to see what type of object it is
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&m) {
                                    // Is this a response or request?
                                    if v["method"] == "job" {
                                        // this is a request
                                        match serde_json::from_str::<types::RpcRequest>(&m) {
                                            Err(e) => error!("Error parsing request {} : {:?}", m, e),
                                            Ok(request) => {
                                                if let Err(err) = self.handle_request(request) {
                                                    error!("Error handling request {} : :{:?}", m, err)
                                                }
                                            },
                                        }
                                    } else {
                                        // this is a response
                                        match serde_json::from_str::<types::RpcResponse>(&m) {
                                            Err(e) => error!("Error parsing response {} : {:?}", m, e),
                                            Ok(response) => {
                                                println!("{:?}", response);
                                                if let Err(err) = self.handle_response(response) {
                                                    error!("Error handling response {} : :{:?}", m, err)
                                                }
                                            },
                                        }
                                    }
                                    continue;
                                } else {
                                    error!("Error parsing message: {}", m)
                                }
                            }
                        },
                        Err(e) => {
                            error!("Error reading message: {:?}", e);
                            self.stream = None;
                            continue;
                        },
                    }
                    next_server_read = time::get_time().sec + server_read_interval;
                }
            }

            // Talk to the miner algorithm
            while let Some(message) = self.rx.try_iter().next() {
                debug!("Client received message: {:?}", message);
                let result = match message {
                    types::ClientMessage::FoundSolution(job_id, hash, nonce) => {
                        self.send_message_submit(job_id, hash, nonce)
                    },
                    types::ClientMessage::Shutdown => {
                        debug!("Shutting down client controller");
                        return;
                    },
                };
                if let Err(e) = result {
                    error!("Mining Controller Error {:?}", e);
                    self.stream = None;
                }
            }
            thread::sleep(std::time::Duration::from_millis(10));
        } // loop
    }
}
