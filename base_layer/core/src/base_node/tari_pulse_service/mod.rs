// Copyright 2024. The Tari Project
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

use std::{cmp::min, str::FromStr, time::Duration};

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
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use tari_p2p::Network;
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::{net::TcpStream as TokioTcpStream, sync::watch, time, time::MissedTickBehavior};

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

fn get_network_dns_name(network: Network) -> Name {
    match network {
        Network::NextNet => Name::from_str("checkpoints-nextnet.tari.com").expect("infallible"),
        Network::MainNet => Name::from_str("checkpoints-mainnet.tari.com").expect("infallible"),
        Network::Esmeralda => Name::from_str("checkpoints-esmeralda.tari.com").expect("infallible"),
        Network::StageNet => Name::from_str("checkpoints-stagenet.tari.com").expect("infallible"),
        Network::Igor => Name::from_str("checkpoints-igor.tari.com").expect("infallible"),
        Network::LocalNet => Name::from_str("checkpoints-localnet.tari.com").expect("infallible"),
    }
}

pub struct TariPulseService {
    dns_name: Name,
    config: TariPulseConfig,
    shutdown_signal: ShutdownSignal,
}

impl TariPulseService {
    pub async fn new(config: TariPulseConfig, shutdown_signal: ShutdownSignal) -> Result<Self, anyhow::Error> {
        let dns_name: Name = get_network_dns_name(config.clone().network);
        info!(target: LOG_TARGET, "Tari Pulse Service initialized with DNS name: {}", dns_name);
        Ok(Self {
            dns_name,
            config,
            shutdown_signal,
        })
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

    pub async fn run(
        &mut self,
        mut base_node_service: LocalNodeCommsInterface,
        notify_passed_checkpoints: watch::Sender<bool>,
    ) {
        let mut interval = time::interval(self.config.check_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        tokio::pin!(interval);
        let mut shutdown_signal = self.shutdown_signal.clone();
        let mut count = 0u64;
        let mut skip_ticks = 0;
        let mut skipped_ticks = 0;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    count += 1;
                    trace!(target: LOG_TARGET, "Interval tick: {}", count);
                    if skipped_ticks < skip_ticks {
                        skipped_ticks += 1;
                        debug!(target: LOG_TARGET, "Skipping {} of {} ticks", skipped_ticks, skip_ticks);
                        continue;
                    }
                    let passed_checkpoints = {
                        match self.passed_checkpoints(&mut base_node_service).await {
                            Ok(passed) => {
                                skip_ticks = 0;
                                skipped_ticks = 0;
                                passed
                            },
                            Err(err) => {
                                warn!(target: LOG_TARGET, "Failed to check if node has passed checkpoints: {:?}", err);
                                skip_ticks = min(skip_ticks + 1, 30 * 60 / self.config.check_interval.as_secs());
                                skipped_ticks = 0;
                                continue;
                            },
                        }
                    };

                    notify_passed_checkpoints
                        .send(!passed_checkpoints)
                        .expect("Channel should be open");
                },
                _ = shutdown_signal.wait() => {
                    info!(
                        target: LOG_TARGET,
                        "Tari Pulse shutting down because the shutdown signal was received"
                    );
                    break;
                },
            }
        }
    }

    async fn passed_checkpoints(
        &mut self,
        base_node_service: &mut LocalNodeCommsInterface,
    ) -> Result<bool, anyhow::Error> {
        let dns_checkpoints = self.fetch_checkpoints().await?;

        let max_height_block = dns_checkpoints
            .iter()
            .max_by(|a, b| a.0.cmp(&b.0))
            .ok_or(CommsInterfaceError::InternalError("No checkpoints found".to_string()))?;
        let local_checkpoints = self.get_node_block(base_node_service, max_height_block.0).await?;
        let passed = local_checkpoints.1 == max_height_block.1;
        trace!(
            target: LOG_TARGET, "Passed checkpoints: {}, DNS: ({}, {}), Local: ({}, {})",
            passed, max_height_block.0, max_height_block.1, local_checkpoints.0, local_checkpoints.1
        );
        Ok(passed)
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
                    Err(CommsInterfaceError::InternalError("Header not found".to_string()))
                },
            })?;

        Ok(historical_block)
    }

    async fn fetch_checkpoints(&mut self) -> Result<Vec<(u64, String)>, anyhow::Error> {
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
    interval: Duration,
    network: Network,
}

impl TariPulseServiceInitializer {
    pub fn new(interval: Duration, network: Network) -> Self {
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
            check_interval: self.interval,
            network: self.network,
        };

        context.spawn_when_ready(move |handles| async move {
            let base_node_service = handles.expect_handle::<LocalNodeCommsInterface>();
            let mut tari_pulse_service = TariPulseService::new(config, shutdown_signal.clone())
                .await
                .expect("Should be able to get the service");
            let tari_pulse_service = tari_pulse_service.run(base_node_service, sender);
            futures::pin_mut!(tari_pulse_service);
            future::select(tari_pulse_service, shutdown_signal).await;
            info!(target: LOG_TARGET, "Tari Pulse Service shutdown");
        });
        info!(target: LOG_TARGET, "Tari Pulse Service initialized");
        Ok(())
    }
}
