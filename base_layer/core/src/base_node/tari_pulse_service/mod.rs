use std::{str::FromStr, time::Duration};

use futures::future;
use hickory_client::{
    client::{AsyncDnssecClient, ClientHandle},
    proto::{
        iocompat::AsyncIoTokioAsStd,
        rr::dnssec::{public_key::Rsa, SigSigner, TrustAnchor},
        xfer::DnsMultiplexer,
    },
    rr::{DNSClass, Name, RData, Record, RecordType},
    tcp::TcpClientStream,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use tari_p2p::Network;
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::{net::TcpStream as TokioTcpStream, sync::watch, time};

use super::LocalNodeCommsInterface;
use crate::base_node::comms_interface::CommsInterfaceError;

const LOG_TARGET: &str = "c::bn::tari_pulse";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TariPulseConfig {
    pub check_interval: Duration,
    pub network: Network,
}

impl Default for TariPulseConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(120),
            network: Network::default(),
        }
    }
}

fn get_network_dns_name(network: &Network) -> Name {
    match network {
        Network::NextNet => Name::from_str("checkpoints-nextnet.tari.com").unwrap(),
        _ => panic!("Network not supported"),
    }
}

pub struct TariPulseService {
    dns_name: Name,
    config: TariPulseConfig,
}

impl TariPulseService {
    pub async fn new(config: TariPulseConfig) -> Result<Self, anyhow::Error> {
        let dns_name: Name = get_network_dns_name(&config.clone().network);
        info!(target: LOG_TARGET, "Tari Pulse Service initialized with DNS name: {}", dns_name);
        Ok(Self { dns_name, config })
    }

    pub fn default_trust_anchor() -> TrustAnchor {
        const ROOT_ANCHOR_ORIG: &[u8] = include_bytes!("20326.rsa");
        const ROOT_ANCHOR_CURRENT: &[u8] = include_bytes!("38696.rsa");

        let mut anchor = TrustAnchor::new();
        anchor.insert_trust_anchor(&Rsa::from_public_bytes(ROOT_ANCHOR_ORIG).expect("Invalid ROOT_ANCHOR_ORIG"));
        anchor.insert_trust_anchor(&Rsa::from_public_bytes(ROOT_ANCHOR_CURRENT).expect("Invalid ROOT_ANCHOR_CURRENT"));
        anchor
    }

    async fn get_dns_client(&self) -> Result<AsyncDnssecClient, anyhow::Error> {
        let timeout: Duration = Duration::from_secs(5);
        let trust_anchor = Self::default_trust_anchor();

        let (stream, handle) = TcpClientStream::<AsyncIoTokioAsStd<TokioTcpStream>>::new(([1, 1, 1, 1], 53).into());
        let dns_muxer = DnsMultiplexer::<_, SigSigner>::with_timeout(stream, handle, timeout, None);
        let (client, bg) = AsyncDnssecClient::builder(dns_muxer)
            .trust_anchor(trust_anchor)
            .build()
            .await?;

        tokio::spawn(bg);

        Ok(client)
    }

    async fn run(
        &mut self,
        mut base_node_service: LocalNodeCommsInterface,
        notify_passed_checkpoints: watch::Sender<bool>,
    ) {
        let mut interval = time::interval(self.config.check_interval);
        let mut interval_failed = time::interval(Duration::from_millis(100));
        loop {
            let dns_checkpoints = match self.fetch_checkpoints().await {
                Ok(checkpoints) => checkpoints,
                Err(e) => {
                    error!(target: LOG_TARGET, "Error fetching DNS checkpoints: {:?}", e);
                    interval_failed.tick().await;
                    continue;
                },
            };

            let max_height_block = dns_checkpoints
                .iter()
                .max_by(|a, b| a.0.cmp(&b.0))
                .ok_or(CommsInterfaceError::InternalError("No checkpoints found".to_string()))
                .unwrap();
            let local_checkpoints = match self.get_node_block(&mut base_node_service, max_height_block.0).await {
                Ok(checkpoints) => checkpoints,
                Err(e) => {
                    error!(target: LOG_TARGET, "Error fetching local checkpoints: {:?}", e);
                    interval_failed.tick().await;
                    continue;
                },
            };
            let passed_checkpoints = local_checkpoints.1 == max_height_block.1;

            notify_passed_checkpoints.send(!passed_checkpoints).unwrap();
            interval.tick().await;
        }
    }

    async fn get_node_block(
        &mut self,
        base_node_service: &mut LocalNodeCommsInterface,
        block_height: u64,
    ) -> Result<(u64, String), anyhow::Error> {
        let historical_block = base_node_service
            .get_header(block_height)
            .await
            .and_then(|header| match header {
                Some(header) => Ok((header.height(), header.hash().to_hex())),
                None => {
                    error!(target: LOG_TARGET, "Header not found for height: {}", block_height);
                    Err(CommsInterfaceError::InternalError("Header not found".to_string()).into())
                },
            })?;

        Ok(historical_block)
    }

    pub async fn fetch_checkpoints(&mut self) -> Result<Vec<(u64, String)>, anyhow::Error> {
        let mut client = self.get_dns_client().await?;
        let query = client.query(self.dns_name.clone(), DNSClass::IN, RecordType::TXT);
        let response = query.await?;
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
                    return Some((height.parse().ok()?, hash.to_string()));
                }
                None
            })
            .collect();

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

pub struct TariPulseServiceInitializer {
    interval: Option<Duration>,
    network: Network,
}

impl TariPulseServiceInitializer {
    pub fn new(interval: Option<Duration>, network: Network) -> Self {
        Self { interval, network }
    }
}

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
        let config = TariPulseConfig {
            check_interval: self.interval.unwrap_or_default(),
            network: self.network,
        };

        context.spawn_when_ready(move |handles| async move {
            let base_node_service = handles.expect_handle::<LocalNodeCommsInterface>();
            let mut tari_pulse_service = TariPulseService::new(config).await.unwrap();
            let tari_pulse_service = tari_pulse_service.run(base_node_service, sender);
            futures::pin_mut!(tari_pulse_service);
            future::select(tari_pulse_service, shutdown_signal).await;
            info!(target: LOG_TARGET, "Tari Pulse Service shutdown");
        });
        info!(target: LOG_TARGET, "Tari Pulse Service initialized");
        Ok(())
    }
}
