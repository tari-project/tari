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
            check_interval: Some(Duration::from_secs(120)),
        };
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
        // let shutdown = Shutdown::new();
        let timeout: Duration = Duration::from_secs(5);
        let trust_anchor = Self::default_trust_anchor();

        let (stream, handle) = TcpClientStream::<AsyncIoTokioAsStd<TokioTcpStream>>::new(([1, 1, 1, 1], 53).into());
        let dns_muxer = DnsMultiplexer::<_, SigSigner>::with_timeout(stream, handle, timeout, None);
        let (client, bg) = AsyncDnssecClient::builder(dns_muxer)
            .trust_anchor(trust_anchor)
            .build()
            .await?;

        tokio::spawn(bg);
        // task::spawn(future::select(shutdown.to_signal(), bg.fuse()));

        Ok(client)
    }

    async fn run(
        &mut self,
        mut base_node_service: LocalNodeCommsInterface,
        notify_passed_checkpoints: watch::Sender<bool>,
    ) {
        let mut interval = time::interval(self.config.check_interval.unwrap());
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

            let heights = dns_checkpoints.iter().map(|(height, _)| *height).collect();
            let local_checkpoints = match self.get_node_blocks(&mut base_node_service, heights).await {
                Ok(checkpoints) => checkpoints,
                Err(e) => {
                    error!(target: LOG_TARGET, "Error fetching local checkpoints: {:?}", e);
                    interval_failed.tick().await;
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
            info!(target: LOG_TARGET, "Checkpoints match: {}", passed_checkpoints);
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
                node_clone.get_header(height).await.and_then(|header| match header {
                    Some(header) => Ok((header.height(), header.hash().to_hex())),
                    None => {
                        error!(target: LOG_TARGET, "Header not found for height: {}", height);
                        Err(CommsInterfaceError::InternalError("Header not found".to_string()).into())
                    },
                })
            }
        }))
        .await?;

        Ok(historical_blocks)
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
