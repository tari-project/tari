use std::{error, net::SocketAddr, str::FromStr, time::Duration};

use futures::future;
use hickory_client::{
    client::{AsyncClient, ClientHandle},
    proto::iocompat::AsyncIoTokioAsStd,
    rr::{DNSClass, Name, RData, Record, RecordType},
    tcp::TcpClientStream,
};
use hickory_resolver::system_conf;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::{net::TcpStream as TokioTcpStream, sync::watch, time};

use super::LocalNodeCommsInterface;

const LOG_TARGET: &str = "c::bn::tari_pulse";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TariPulseConfig {
    pub check_interval: Option<Duration>,
}

pub struct TariPulseService {
    dns_name: Name,
    config: TariPulseConfig,
}

impl TariPulseService {
    pub async fn new() -> Result<Self, anyhow::Error> {
        let dns_name = Name::from_str("checkpoints-nextnet.tari.com").unwrap();
        let config = TariPulseConfig {
            check_interval: Some(Duration::from_secs(60)),
        };
        Ok(Self { dns_name, config })
    }

    fn socket_addr_and_dns_name(&self) -> Result<(SocketAddr, Option<String>), anyhow::Error> {
        let (conf, _opts) = system_conf::read_system_conf()?;
        let found = conf
            .name_servers()
            .iter()
            .find(|ns| ns.tls_dns_name.is_some())
            .or_else(|| conf.name_servers().first())
            .ok_or_else(|| anyhow::anyhow!("No name servers found"))?;
        Ok((found.socket_addr, found.tls_dns_name.clone()))
    }

    async fn get_dns_client(&self) -> Result<AsyncClient, anyhow::Error> {
        // let (socket_addr, dns_name) = self.socket_addr_and_dns_name();
        let (stream, sender) = TcpClientStream::<AsyncIoTokioAsStd<TokioTcpStream>>::new(([8, 8, 8, 8], 53).into());
        let client = AsyncClient::new(stream, sender, None);
        let (client, bg) = client.await.expect("connection failed");
        tokio::spawn(bg);
        Ok(client)
    }

    async fn run(
        &mut self,
        mut base_node_service: LocalNodeCommsInterface,
        notify_passed_checkpoints: watch::Sender<bool>,
    ) {
        let mut interval = time::interval(self.config.check_interval.unwrap());
        loop {
            let dns_checkpoints = match self.fetch_checkpoints().await {
                Ok(checkpoints) => checkpoints,
                Err(e) => {
                    error!(target: LOG_TARGET, "Error fetching DNS checkpoints: {:?}", e);
                    interval.tick().await;
                    continue;
                },
            };

            let heights = dns_checkpoints.iter().map(|(height, _)| *height).collect();
            let local_checkpoints = match self.get_node_blocks(&mut base_node_service, heights).await {
                Ok(checkpoints) => checkpoints,
                Err(e) => {
                    error!(target: LOG_TARGET, "Error fetching local checkpoints: {:?}", e);
                    interval.tick().await;
                    continue;
                },
            };

            dns_checkpoints.iter().for_each(|(height, hash)| {
                info!(target: LOG_TARGET, "DNS Checkpoint: Height: {}, Hash: {}", height, hash);
            });
            local_checkpoints.iter().for_each(|(height, hash)| {
                info!(target: LOG_TARGET, "Local Checkpoint: Height: {}, Hash: {}", height, hash);
            });
            let passed_checkpoints = dns_checkpoints.iter().all(|(height, hash)| {
                local_checkpoints
                    .iter()
                    .any(|(local_height, local_hash)| height == local_height && hash == local_hash)
            });
            notify_passed_checkpoints.send(!passed_checkpoints).unwrap();
            interval.tick().await;
        }
    }

    async fn get_node_blocks(
        &mut self,
        base_node_service: &mut LocalNodeCommsInterface,
        heights: Vec<u64>,
    ) -> Result<Vec<(u64, String)>, anyhow::Error> {
        let historical_blocks = future::try_join_all(heights.into_iter().map(|height| {
            let mut node_clone = base_node_service.clone();
            async move {
                node_clone.get_header(height).await.map(|header| {
                    error!(target: LOG_TARGET, "Header not found for height: {}", height);
                    let header = header.expect("Header not found");
                    (header.height(), header.hash().to_hex())
                })
            }
        }))
        .await?;

        Ok(historical_blocks)
    }

    pub async fn fetch_checkpoints(&mut self) -> Result<Vec<(u64, String)>, anyhow::Error> {
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

        debug!(target: LOG_TARGET, "DNS Checkpoints: {:?}", checkpoints);
        Ok(checkpoints)
    }
}

#[derive(Clone)]
pub struct TariPulseHandle {
    pub shutdown_signal: ShutdownSignal,
    pub failed_checkpoints_notifier: watch::Receiver<bool>,
}

impl TariPulseHandle {
    pub fn get_failed_checkpoints_notifier(&self) -> watch::Ref<'_, bool> {
        self.failed_checkpoints_notifier.borrow()
    }
}

pub struct TariPulseServiceInitializer;

#[async_trait]
impl ServiceInitializer for TariPulseServiceInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        info!(target: LOG_TARGET, "Initializing Tari Pulse Service");
        let shutdown_signal = context.get_shutdown_signal();
        let (sender, receiver) = watch::channel(false);
        context.register_handle(TariPulseHandle {
            shutdown_signal: shutdown_signal.clone(),
            failed_checkpoints_notifier: receiver,
        });

        context.spawn_when_ready(move |handles| async move {
            let base_node_service = handles.expect_handle::<LocalNodeCommsInterface>();
            let mut tari_pulse_service = TariPulseService::new().await.unwrap();
            let tari_pulse_service = tari_pulse_service.run(base_node_service, sender);
            futures::pin_mut!(tari_pulse_service);
            future::select(tari_pulse_service, shutdown_signal).await;
            info!(target: LOG_TARGET, "Tari Pulse Service shutdown");
        });
        info!(target: LOG_TARGET, "Tari Pulse Service initialized");
        Ok(())
    }
}
