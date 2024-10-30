use std::{str::FromStr, time::Duration};

use futures::future;
use hickory_client::{
    client::{AsyncClient, ClientHandle},
    proto::iocompat::AsyncIoTokioAsStd,
    rr::{DNSClass, Name, RData, Record, RecordType},
    tcp::TcpClientStream,
};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tari_core::{base_node::LocalNodeCommsInterface, blocks::ChainHeader};
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tari_utilities::hex::Hex;
use tokio::{net::TcpStream as TokioTcpStream, time};

const LOG_TARGET: &str = "c::bn::tari_pulse";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointConfig {
    pub check_interval: Option<Duration>,
}

pub struct CheckpointService {
    dns_name: Name,
    config: CheckpointConfig,
}

impl CheckpointService {
    pub async fn new() -> Result<Self, anyhow::Error> {
        let dns_name = Name::from_str("checkpoints-nextnet.tari.com").unwrap();
        let config = CheckpointConfig {
            check_interval: Some(Duration::from_secs(60)),
        };
        Ok(Self { dns_name, config })
    }

    async fn get_dns_client(&self) -> Result<AsyncClient, anyhow::Error> {
        let (stream, sender) = TcpClientStream::<AsyncIoTokioAsStd<TokioTcpStream>>::new(([8, 8, 8, 8], 53).into());
        let client = AsyncClient::new(stream, sender, None);
        let (client, bg) = client.await.expect("connection failed");
        tokio::spawn(bg);
        Ok(client)
    }

    async fn run(&mut self, mut base_node_service: LocalNodeCommsInterface) {
        debug!(target: LOG_TARGET, "Running Tari Pulse main loop");
        let mut interval = time::interval(self.config.check_interval.unwrap());
        loop {
            info!(target: LOG_TARGET, "Checking Tari Pulse");
            let dns_checkpoints = match self.fetch_checkpoints().await {
                Ok(checkpoints) => checkpoints,
                Err(e) => {
                    error!(target: LOG_TARGET, "Error fetching checkpoints: {:?}", e);
                    interval.tick().await;
                    continue;
                },
            };
            info!(target: LOG_TARGET, "Fetched {} checkpoints from dns", dns_checkpoints.len());

            let heights = dns_checkpoints.iter().map(|(height, _)| *height).collect();
            let local_checkpoints = match self.get_node_blocks(&mut base_node_service, heights).await {
                Ok(checkpoints) => checkpoints,
                Err(e) => {
                    error!(target: LOG_TARGET, "Error fetching local checkpoints: {:?}", e);
                    interval.tick().await;
                    continue;
                },
            };
            info!(target: LOG_TARGET, "Fetched {} checkpoints from local node", local_checkpoints.len());
            local_checkpoints.iter().for_each(|header| {
                info!(
                    target: LOG_TARGET,
                    "Local checkpoint: {} - {}",
                    header.height(),
                    header.hash().to_hex()
                );
            });

            for (height, hash) in dns_checkpoints {
                info!(target: LOG_TARGET, "Checking checkpoint: {} - {}", height, hash);
                let _ = base_node_service
                    .get_header(height)
                    .await
                    .map_err(|e| error!(target: LOG_TARGET, "Error fetching header: {:?}", e))
                    .and_then(|header| {
                        let header = header.expect("Header not found");
                        info!(
                            target: LOG_TARGET,
                            "Fetched header: {} - {}",
                            header.height(),
                            header.hash().to_hex()
                        );
                        if header.height() != height {
                            error!(
                                target: LOG_TARGET,
                                "Block height mismatch. Expected: {}, Got: {}",
                                height,
                                header.height()
                            );
                            return Err(());
                        }
                        if header.hash().to_hex() != hash {
                            error!(
                                target: LOG_TARGET,
                                "Block hash mismatch. Expected: {}, Got: {}",
                                hash,
                                header.hash().to_hex()
                            );
                            return Err(());
                        }
                        Ok(())
                    });
            }
            debug!(target: LOG_TARGET, "Sleeping for {} seconds", self.config.check_interval.unwrap().as_secs());
            interval.tick().await;
            debug!(target: LOG_TARGET, "Waking up");
        }
    }

    async fn get_node_blocks(
        &mut self,
        base_node_service: &mut LocalNodeCommsInterface,
        heights: Vec<u64>,
    ) -> Result<Vec<ChainHeader>, anyhow::Error> {
        let historical_blocks = future::try_join_all(heights.into_iter().map(|height| {
            let mut node_clone = base_node_service.clone();
            async move {
                node_clone.get_header(height).await.map(|header| {
                    let header = header.expect("Header not found");
                    info!(
                        target: LOG_TARGET,
                        "Fetched header from local node: {} - {}",
                        header.height(),
                        header.hash().to_hex()
                    );
                    header
                })
            }
        }))
        .await?;

        Ok(historical_blocks)
    }

    pub async fn fetch_checkpoints(&mut self) -> Result<Vec<(u64, String)>, anyhow::Error> {
        debug!(target: LOG_TARGET, "Fetching checkpoints");
        let mut client = self.get_dns_client().await?;
        let query = client.query(self.dns_name.clone(), DNSClass::IN, RecordType::TXT);
        let response = query.await.unwrap();
        let answers: &[Record] = response.answers();
        let checkpoints: Vec<(u64, String)> = answers
            .iter()
            .filter_map(|record| {
                if let RData::TXT(txt) = record.data() {
                    let ascii_txt = txt.txt_data().iter().fold(String::new(), |mut acc, bytes| {
                        acc.push_str(&String::from_utf8_lossy(bytes));
                        acc
                    });
                    let (height, hash) = ascii_txt.split_once(':')?;
                    return Some((height.parse().unwrap(), hash.to_string()));
                }
                None
            })
            .collect();

        Ok(checkpoints)
    }
}

pub struct CheckpointHandler;
pub struct CheckpointServiceInitializer;

#[async_trait]
impl ServiceInitializer for CheckpointServiceInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Tari Pulse Service");
        context.register_handle(CheckpointHandler);
        let shutdown_signal = context.get_shutdown_signal();

        context.spawn_when_ready(move |handles| async move {
            let base_node_service = handles.expect_handle::<LocalNodeCommsInterface>();
            let mut checkpoint_service = CheckpointService::new().await.unwrap();
            let tari_pulse_service = checkpoint_service.run(base_node_service);
            futures::pin_mut!(tari_pulse_service);
            future::select(tari_pulse_service, shutdown_signal).await;
            info!(target: LOG_TARGET, "Tari Pulse Service shutdown");
        });
        debug!(target: LOG_TARGET, "Tari Pulse Service initialized");
        Ok(())
    }
}
