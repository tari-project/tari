use std::marker::PhantomData;

use anyhow::Error;
use log::info;
use serde::Serialize;
use tari_shutdown::ShutdownSignal;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    select,
};
use tonic::async_trait;

use crate::stratum::{
    job_repository_service::{JobRepositoryClient, JobRepositoryService},
    memory_job_repository::MemoryJobRepository,
    tari_sha3x_stratum_handler::TariSha3xStratumHandler,
};

const LOG_TARGET: &str = "minotari::base_node::stratum::server";

pub(crate) struct TariStratumServer {
    port: u16,
}

impl TariStratumServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn start(&self, shutdown: ShutdownSignal) -> Result<(), Error> {
        let mem_repo = MemoryJobRepository::default();
        let (job_repository_tx, job_repository_rx) = tokio::sync::mpsc::channel(100);

        let job_repository_service = JobRepositoryService::new(mem_repo, job_repository_rx);
        let repository_client = JobRepositoryClient::new(job_repository_tx);

        todo!()
    }
}

struct StratumServerBuilder<T, TAdapter: StratumStreamAdapter> {
    port: Option<u16>,
    with_job_handler: Option<T>,
    min_difficulty: Option<u64>,
    _marker: PhantomData<TAdapter>,
}

impl<T: StratumJobHandler, TAdapter: StratumStreamAdapter> StratumServerBuilder<T, TAdapter> {
    pub fn new() -> Self {
        StratumServerBuilder {
            port: None,
            min_difficulty: None,
            with_job_handler: None,
            _marker: PhantomData::default(),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn with_job_handler(mut self, handler: T) -> Self {
        self.with_job_handler = Some(handler);
        self
    }

    pub fn with_min_difficulty(mut self, min_difficulty: u64) -> Self {
        self.min_difficulty = Some(min_difficulty);
        self
    }

    pub fn build(self) -> StratumServer<T, TAdapter> {
        StratumServer {
            // Set default port if not provided
            port: self.port.unwrap_or(3333),
            hander: self.with_job_handler.expect("Job handler must be provided"),
            min_difficulty: self.min_difficulty.unwrap_or(10_000_000),
            adapter: Default::default(),
        }
    }
}

struct StratumServer<T: StratumJobHandler, TAdapter: StratumStreamAdapter> {
    port: u16,
    hander: T,
    min_difficulty: u64,
    adapter: PhantomData<TAdapter>,
}

impl<T: StratumJobHandler, TAdapter: StratumStreamAdapter> StratumServer<T, TAdapter> {
    pub async fn start(self, mut shutdown_signal: ShutdownSignal) -> anyhow::Result<()> {
        info!("Starting Stratum server on port {}", self.port);
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port)).await?;
        loop {
            select! {
                _ = shutdown_signal.wait() => {
                    info!( "Shutting down Stratum server");
                    break;
                },
                // Handle incoming connections and jobs here
                res = listener.accept() => {
                    match res {
                        Ok((stream, _)) => {
                            // Handle the connection with the job handler
                            info!( "Accepted connection from {}", stream.peer_addr()?);
                            let handler = self.hander.clone();
                            // self.hander.handle_connection(stream).await?;
                            tokio::spawn(async move {
                                let (reader, mut writer) = stream.into_split();
                                let mut reader = BufReader::new(reader).lines();


                                while let Ok(Some(line)) = reader.next_line().await {
                                    // if let Ok(msg): Result<Value, _> = serde_json::from_str(&line) {
                                        // handle 'login', 'submit', etc.
                                        println!("Received: {:#?}", line);
                                        match TAdapter::try_convert(line) {
                                            Ok(request) => {
                                                let id = request.id().to_string();

                                                info!( "Parsed request with id: {}", id);

                                                // Handle the request based on its type
                                                match request {
                                                    StratumRequest::Login { id, login, pass, agent } => {
                                                        let login_parts = login.split("=").collect::<Vec<_>>();
                                                        let login_address = login_parts[0].to_string();
                                                        let login_difficulty = match login_parts.len() {
                                                            2 => {
                                                                if login_parts[1].ends_with("M") {
                                                                    let difficulty = login_parts[1].replace("M", "");
                                                                    let difficulty = difficulty.parse::<f64>().unwrap_or(self.min_difficulty as f64);
                                                                    (difficulty * 1_000_000.0).floor() as u64
                                                                } else if login_parts[1].ends_with("G") {
                                                                    let difficulty = login_parts[1].replace("G", "");
                                                                    let difficulty = difficulty.parse::<f64>().unwrap_or(self.min_difficulty as f64);
                                                                    (difficulty * 1_000_000_000.0).floor() as u64
                                                                } else {
                                                                    login_parts[1].parse::<u64>().unwrap_or(self.min_difficulty)
                                                                }
                                                            }
                                                            _ => self.min_difficulty,
                                                        };

                                                        if login_difficulty < self.min_difficulty {
                                                            info!( "Login difficulty {} is less than minimum difficulty {}", login_difficulty, self.min_difficulty);
                                                            writer.write_all(format!("{{\"id\": \"{}\", \"error\": \"Login difficulty {} is less than minimum difficulty {}\", \"result\": null}}\n", id, login_difficulty, self.min_difficulty).as_bytes()).await.unwrap();
                                                            continue;
                                                        }

                                                        let response = handler.login(id.clone(), login_address, pass, agent, login_difficulty).await;
                                                        match response {
                                                            Ok(resp) => {
                                                                info!( "Handled login request with id: {}", id);
                                                                let json_response = serde_json::to_string(&resp).unwrap();
                                                                writer.write_all(format!("{{\"id\": \"{}\", \"result\": {}, \"error\": null}}\n", id, json_response).as_bytes()).await.unwrap();
                                                            },
                                                            Err(e) => {
                                                                info!( "Failed to handle login request: {}", e);
                                                                writer.write_all(format!("{{\"id\": \"{}\", \"error\": \"Failed to handle login request:{}\", \"result\": null}}\n", id, e.to_string()).as_bytes()).await.unwrap();
                                                            }
                                                        }
                                                    },
                                                    StratumRequest::Submit { job_id, nonce, result, id } => {
                                                        let nonce = match u64::from_str_radix(&nonce, 16) {
                                                            Ok(n) => n,
                                                            Err(e) => {
                                                                info!( "Failed to parse nonce: {}", e);
                                                                writer.write_all(format!("{{\"id\": \"{}\", \"error\": \"Failed to handle submit request:{}\", \"result\": null}}\n", id, "Nonce is not valid").as_bytes()).await.unwrap();
                                                                continue;
                                                            }
                                                        };
                                                        let response = handler.submit(job_id, nonce, result, id.clone()).await;
                                                        match response {
                                                            Ok(resp) => {
                                                                info!( "Handled submit request with id: {}", id);
                                                                let json_response = serde_json::to_string(&resp).unwrap();
                                                                writer.write_all(format!("{{\"id\": \"{}\", \"result\": {}, \"error\": null}}\n", id, json_response).as_bytes()).await.unwrap();
                                                            },
                                                            Err(e) => {
                                                                info!( "Failed to handle submit request: {}", e);
                                                                writer.write_all(format!("{{\"id\": \"{}\", \"error\": \"Failed to handle submit request:{}\", \"result\": null}}\n", id, e.to_string()).as_bytes()).await.unwrap();
                                                            }
                                                        }
                                                    }
                                                }
                                                // match handler.handle_request(request) {
                                                //     Ok(resp) => {
                                                //         info!( "Handled request with id: {}", id);
                                                //         let json_response = serde_json::to_string(&resp).unwrap();
                                                //         writer.write_all(format!("{{\"id\": \"{}\", \"result\": {}, \"error\": null}}\n", id, json_response).as_bytes()).await.unwrap();
                                                //     },
                                                //     Err(e) => {
                                                //         info!("Failed to handle request: {}", e);
                                                //         writer.write_all(format!("{{\"id\": \"{}\", \"error\": \"Failed to handle request:{}\", \"result\": null}}\n", id, e.to_string()).as_bytes()).await.unwrap();
                                                //     }
                                                // }
                                            },
                                            Err(e) => {
                                                info!( "Failed to parse request: {}", e);
                                            }
                                        }
                                    // }
                                }

                            });
                        },

                        Err(e) => {
                            info!("Failed to accept connection: {}", e);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait StratumJobHandler: Clone + Send + Sync + 'static {
    // fn handle_request(&self, request: StratumRequest) -> anyhow::Result<Value>;
    async fn login(
        &self,
        id: String,
        login: String,
        pass: String,
        agent: String,
        endpoint_difficulty: u64,
    ) -> anyhow::Result<LoginResponse>;

    async fn submit(&self, job_id: String, nonce: u64, result: String, id: String) -> anyhow::Result<SubmitResponse>;
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LoginResponse {
    pub id: String,
    pub job: StratumJob,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StratumJob {
    pub job_id: String,
    pub algo: String,
    pub blob: String,
    pub height: u64,
    pub target: String,
    pub xn: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SubmitResponse {
    pub id: String,
    pub result: bool,
}

pub trait StratumStreamAdapter {
    fn try_convert(line: String) -> anyhow::Result<StratumRequest>;
}

pub struct NiceHashStyleStatumStreamAdapter {}

impl StratumStreamAdapter for NiceHashStyleStatumStreamAdapter {
    fn try_convert(line: String) -> anyhow::Result<StratumRequest> {
        let json: serde_json::Value = serde_json::from_str(&line)?;
        let method = json["method"]
            .as_str()
            .ok_or(anyhow::anyhow!("Json missing method field"))?;
        let id = json["id"]
            .as_i64()
            .ok_or(anyhow::anyhow!("Invalid JSON. Json missing id field"))?
            .to_string();
        match method {
            "login" => {
                let params = json["params"]
                    .as_object()
                    .ok_or(anyhow::anyhow!("Invalid JSON.params missing"))?;
                let login = params["login"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. login missing"))?
                    .to_string();
                let pass = params["pass"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. pass missing"))?
                    .to_string();
                let agent = params["agent"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. agent missing"))?
                    .to_string();

                Ok(StratumRequest::Login { id, login, pass, agent })
            },
            "submit" => {
                let params = json["params"].as_object().ok_or(anyhow::anyhow!("Invalid JSON"))?;
                let job_id = params["job_id"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. job_id missing"))?
                    .to_string();
                let nonce = params["nonce"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. nonce missing"))?
                    .to_string();
                let result = params["result"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. result missing"))?
                    .to_string();
                Ok(StratumRequest::Submit {
                    id,
                    job_id,
                    nonce,
                    result,
                })
            },
            _ => Err(anyhow::anyhow!("Unknown method")),
        }
    }
}

#[derive(Debug, Clone)]
pub enum StratumRequest {
    Login {
        id: String,
        login: String,
        pass: String,
        agent: String,
    },
    Submit {
        id: String,
        job_id: String,
        nonce: String,
        result: String,
    },
}

impl StratumRequest {
    pub fn id(&self) -> &str {
        match self {
            StratumRequest::Login { id, .. } => id.as_str(),
            StratumRequest::Submit { id, .. } => id.as_str(),
        }
    }
}
