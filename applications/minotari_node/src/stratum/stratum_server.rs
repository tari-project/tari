use std::{marker::PhantomData, path::MAIN_SEPARATOR};

use anyhow::Error;
use log::{debug, info, warn};
use serde::Serialize;
use tari_core::{
    base_node::LocalNodeCommsInterface,
    consensus::{self, ConsensusManager},
};
use tari_shutdown::ShutdownSignal;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    select,
};
use tonic::async_trait;

use crate::stratum::{
    self,
    block_template_repository::DefaultBlockTemplateRepository,
    job_repository_service::{JobRepositoryClient, JobRepositoryService},
    memory_job_repository::MemoryJobRepository,
    tari_sha3x_stratum_handler::TariSha3xStratumHandler,
    LatestBlockBroadcastReceiver,
};

const LOG_TARGET: &str = "minotari::base_node::stratum::server";

pub(crate) struct TariStratumServer {
    port: u16,
}

impl TariStratumServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn start(
        &self,
        shutdown: ShutdownSignal,
        consensus_manager: ConsensusManager,
        local_node: LocalNodeCommsInterface,
    ) -> Result<(), Error> {
        let mem_repo = MemoryJobRepository::default();
        let (job_repository_tx, job_repository_rx) = tokio::sync::mpsc::channel(100);

        let job_repository_service = JobRepositoryService::new(mem_repo, job_repository_rx);
        let repository_client = JobRepositoryClient::new(job_repository_tx);

        let mut servers = vec![];
        let shutdown1 = shutdown.clone();
        let stratum_port = self.port;
        let min_difficulty = 10_000_000; // Default minimum difficulty
        let (submit_tx, submit_job_queue_rx) = tokio::sync::mpsc::channel(100);
        let block_creater = DefaultBlockTemplateRepository::new(local_node, consensus_manager);
        let block_creater_clone = block_creater.clone();
        servers.push(tokio::spawn(async move {
            let job_handler = TariSha3xStratumHandler::new(block_creater_clone, repository_client, submit_tx);
            let stratum_server = StratumServerBuilder::<_, MultiVersionStratumStreamAdapter>::new()
                .with_port(stratum_port)
                .with_job_handler(job_handler)
                // .with_min_difficulty(min_difficulty)
                .build();

            stratum_server.start(shutdown1).await
        }));

        let shutdown2 = shutdown.clone();
        servers.push(tokio::spawn(async move {
            block_creater.start(shutdown2, submit_job_queue_rx).await
        }));
        let shutdown3 = shutdown.clone();
        servers.push(tokio::spawn(
            async move { job_repository_service.start(shutdown3).await },
        ));
        Ok(())
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
        info!(target: LOG_TARGET, "Starting Stratum server on port {}", self.port);
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
                                        info!(target: LOG_TARGET, "Accepted connection from {}", stream.peer_addr()?);
                                        let handler = self.hander.clone();
                                        // self.hander.handle_connection(stream).await?;
                                        tokio::spawn(async move {
                                            let (reader, mut writer) = stream.into_split();
                                            let mut reader = BufReader::new(reader).lines();
                                            let mut subscription_ids = vec![];


                                            while let Ok(Some(line)) = reader.next_line().await {
                                                // if let Ok(msg): Result<Value, _> = serde_json::from_str(&line) {
                                                    // handle 'login', 'submit', etc.
                                                    debug!(target: LOG_TARGET, "Received line: {}", line);
                                                    match TAdapter::try_convert(line) {
                                                        Ok(request) => {
                                                            let id = request.id().to_string();

                                                            info!(target:LOG_TARGET, "Parsed request with id: {}", id);

                                                            // Handle the request based on its type
                                                            match request {
                                                                StratumRequest::Login { id, login, pass, agent, algo } => {

                                                                    // let algo = algo.first().cloned().unwrap_or_else(|| "sha3x".to_string());
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
                                                                    debug!(target: LOG_TARGET, "Login address: {}, difficulty: {}", login_address, login_difficulty);

                                                                    if login_difficulty < self.min_difficulty {
                                                                        info!(target: LOG_TARGET, "Login difficulty {} is less than minimum difficulty {}", login_difficulty, self.min_difficulty);
                                                                        let _res = writer.write_all(format!("{{\"id\": \"{}\", \"jsonrpc\": \"2.0\", \"error\": \"Login difficulty {} is less than minimum difficulty {}\", \"result\": null}}\n", id, login_difficulty, self.min_difficulty).as_bytes()).await;
                                                                        continue;
                                                                    }

                                                                    let response = handler.login(id.clone(), login_address, true, &algo, pass, agent, login_difficulty).await;
                                                                    match response {
                                                                        Ok(resp) => {
                                                                            info!(target: LOG_TARGET, "Handled login request with id: {}", id);
                                                                            let json_response = serde_json::to_string(&resp).unwrap();
                                                                            let res_packet = format!("{{\"id\": {}, \"jsonrpc\": \"2.0\", \"result\": {}}}\n", id, json_response);
                                                                            debug!(target: LOG_TARGET, "Login response: {}", res_packet);
                                                                            let _res = writer.write_all(res_packet.as_bytes()).await;
                                                                        },
                                                                        Err(e) => {
                                                                            warn!(target: LOG_TARGET, "Failed to handle login request: {}", e);
                                                                            let _res = writer.write_all(format!("{{\"id\": \"{}\", \"jsonrpc\": \"2.0\", \"error\": \"Failed to handle login request:{}\", \"result\": null}}\n", id, e.to_string()).as_bytes()).await;
                                                                        }
                                                                    }
                                                                },
                                                                StratumRequest::Submit { job_id, nonce, result, id } => {
                                                                    let nonce = match u64::from_str_radix(&nonce, 16) {
                                                                        Ok(n) => n,
                                                                        Err(e) => {
                                                                            warn!(target: LOG_TARGET, "Failed to parse nonce: {}", e);
                                                                            let _res = writer.write_all(format!("{{\"id\": \"{}\", \"jsonrpc\": \"2.0\", \"error\": \"Failed to handle submit request:{}\", \"result\": null}}\n", id, "Nonce is not valid").as_bytes()).await;
                                                                            continue;
                                                                        }
                                                                    };
                                                                    let response = handler.submit(job_id, nonce, result, id.clone()).await;
                                                                    match response {
                                                                        Ok(resp) => {
                                                                            info!(target: LOG_TARGET, "Handled submit request with id: {}", id);
                                                                            let json_response = serde_json::to_string(&resp).unwrap();
                                                                            let _res = writer.write_all(format!("{{\"id\": \"{}\", \"result\": {}, \"error\": null}}\n", id, json_response).as_bytes()).await.inspect_err(|e| {
                                                                                warn!(target: LOG_TARGET, "Failed to write response: {}", e);
                                                                            }       );
                                                                        },
                                                                        Err(e) => {
                                                                            warn!(target: LOG_TARGET, "Failed to handle submit request: {}", e);
                                                                            let _res = writer.write_all(format!("{{\"id\": \"{}\", \"error\": \"Failed to handle submit request:{}\", \"result\": null}}\n", id, e.to_string()).as_bytes()).await;
                                                                        }
                                                                    }
                                                                },
                                                                StratumRequest::Subscribe { id, agent } => {
                                                                    info!(target: LOG_TARGET, "Subscribe request with id: {}, agent: {}", id, agent);
                                                                     println!("here");
                                                                     let response = handler.subscribe(id.clone(), agent).await;
                                                                     println!("here"    );
                                                                    match response {
                                                                        Ok(r) =>  {
                                                                            info!(target: LOG_TARGET, "Handled subscribe request with id: {}", id);

                                                                            subscription_ids.push((r.subscription_id.clone(), r.extra_nonce.clone()));
            // [2025-07-24T11:20:11.431049200+00:00] {"id":1,"result":[[["mining.set_difficulty","68656c6c6f2c6d696e65722d002779c8"],["mining.notify","68656c6c6f2c6d696e65722d002779c8"]],"002779c8",4],"error":null}

                                                                            // let json_response = serde_json::to_string(&r).unwrap();
                                                                            // let difficulty = r.difficulty;
                                                                            // let difficulty = "1".to_string();
                                                                            // let block_template = r.block_template;
                                                                            // let nonce = r.nonce;
                                                                            // let height = r.height;

                                                                            // {"id":null,"method":"mining.notify","params":["1eb6e5","a9d69d884bd093be85f38e8a4ffcf10c8e8e327636bcf9c3d4f017a96ef00ee7",1153674,"1f02dc3c",false]
                                                                            let res = format!(
                                                                                "{{\"id\": \"{}\", \"jsonrpc\": \"2.0\", \"result\": [[[\"mining.set_difficulty\", \"{}\"],[\"mining.notify\", \"{}\"]], \"{}\", {}]}}\n",
                                                                                id, r.subscription_id, r.subscription_id, r.extra_nonce, r.extra_nonce.len()/2 // length in hex
                                                                            );
                                                                            info!(target: LOG_TARGET, "Subscribe response: {}", res);
                                                                            let _res = writer.write_all(
                                                                                res.as_bytes(),
                                                                            ).await;
                                                                            // let _res = writer.write_all(format!("{{\"id\": \"{}\", \"result\": {}, \"error\": null}}\n", id, json_response).as_bytes()).await;
                                                                        },
                                                                        Err(e) => {
                                                                            warn!(target: LOG_TARGET, "Failed to handle subscribe request: {}", e);
                                                                            let _res = writer.write_all(format!("{{\"id\": \"{}\", \"error\": \"Failed to handle subscribe request:{}\", \"result\": null}}\n", id, e.to_string()).as_bytes()).await;
                                                                        }
                                                                    }
                                                                },
                                                                StratumRequest::Authorize { id, login, pass } => {
                                                                    let _res = writer.write_all(format!("{{\"id\": \"{}\", \"jsonrpc\": \"2.0\", \"error\": \"Not supported\"}}\n", id).as_bytes()).await;
                                                                },

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
                                                            info!( target: LOG_TARGET, "Failed to parse request: {}", e);
                                                        }
                                                    }
                                                // }
                                            }

                                        });
                                    },

                                    Err(e) => {
                                        info!( target: LOG_TARGET, "Failed to accept connection: {}", e);
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
        is_solo: bool,
        algo: &[String],
        pass: String,
        agent: String,
        endpoint_difficulty: u64,
    ) -> anyhow::Result<LoginResponse>;

    async fn submit(&self, job_id: String, nonce: u64, result: String, id: String) -> anyhow::Result<SubmitResponse>;

    async fn subscribe(
        &self,
        id: String,
        // main_algo: String,
        // address: String,
        // is_solo: bool,
        agent: String,
        // worker: Option<String>,
    ) -> anyhow::Result<SubscribeResponse>;
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LoginResponse {
    pub id: String,
    pub job: StratumJob,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StratumJob {
    pub job_id: String,
    pub algo: String,
    pub blob: String,
    pub height: u64,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_hash: Option<String>,
}

pub(crate) struct SubscribeResponse {
    // pub difficulty: String,
    // pub block_template: String,
    // pub nonce: String,
    // pub height: u64,
    pub subscription_id: String,
    pub extra_nonce: String,
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
                let algo = params["algo"]
                    .as_array()
                    .ok_or(anyhow::anyhow!("Invalid JSON. algo missing"))?
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();

                Ok(StratumRequest::Login {
                    id,
                    login,
                    pass,
                    agent,
                    algo,
                })
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
        algo: Vec<String>,
    },
    Submit {
        id: String,
        job_id: String,
        nonce: String,
        result: String,
    },
    Subscribe {
        id: String,
        agent: String,
        // address: String,
        // worker: Option<String>,
    },
    Authorize {
        id: String,
        login: String,
        worker_name: Option<String>,
        pass: String,
    },
}

impl StratumRequest {
    pub fn id(&self) -> &str {
        match self {
            StratumRequest::Login { id, .. } => id.as_str(),
            StratumRequest::Submit { id, .. } => id.as_str(),
            StratumRequest::Subscribe { id, .. } => id.as_str(),
            StratumRequest::Authorize { id, .. } => id.as_str(),
        }
    }
}

struct StratumV1StreamAdapter {}

impl StratumStreamAdapter for StratumV1StreamAdapter {
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
            "mining.subscribe" => {
                dbg!("here");
                let params = json["params"]
                    .as_array()
                    .ok_or(anyhow::anyhow!("Invalid JSON.params missing"))?;
                let agent = params.get(0);
                let agent = agent.and_then(|v| v.as_str()).map(|s| s.to_string());

                // let address_and_worker = params
                //     .get(1)
                //     .and_then(|v| v.as_str())
                //     .ok_or(anyhow::anyhow!("Invalid JSON. address missing"))?
                //     .to_string();
                // let address_parts = address_and_worker.split('.').collect::<Vec<_>>();
                // let address = address_parts[0].to_string();
                // let worker = if address_parts.len() > 1 {
                //     Some(address_parts[1].to_string())
                // } else {
                //     None
                // };
                Ok(StratumRequest::Subscribe {
                    id,
                    agent: agent.unwrap_or_default(),
                    // address,
                    // worker,
                })
            },
            "mining.authorize" => {
                let params = json["params"]
                    .as_array()
                    .ok_or(anyhow::anyhow!("Invalid JSON.params missing"))?;
                let login = params
                    .get(0)
                    .and_then(|v| v.as_str())
                    .ok_or(anyhow::anyhow!("Invalid JSON. login missing"))?
                    .to_string();
                let (worker_name, login) = if login.contains(".") {
                    let parts: Vec<&str> = login.split('.').collect();
                    (Some(parts[1].to_string()), parts[0].to_string())
                } else {
                    (None, login)
                };
                let pass = params
                    .get(1)
                    .and_then(|v| v.as_str())
                    .ok_or(anyhow::anyhow!("Invalid JSON. pass missing"))?
                    .to_string();
                Ok(StratumRequest::Authorize {
                    id,
                    login,
                    pass,
                    worker_name,
                })
            },

            _ => Err(anyhow::anyhow!("Unknown method")),
        }
    }
}

struct MultiVersionStratumStreamAdapter {}

impl StratumStreamAdapter for MultiVersionStratumStreamAdapter {
    fn try_convert(line: String) -> anyhow::Result<StratumRequest> {
        dbg!("converting line: {}", &line);
        // Try NiceHash style first
        if let Ok(request) = NiceHashStyleStatumStreamAdapter::try_convert(line.clone()) {
            dbg!("here");
            Ok(request)
        } else {
            dbg!("here2");
            // Fallback to Stratum V1 style
            StratumV1StreamAdapter::try_convert(line)
        }
    }
}
