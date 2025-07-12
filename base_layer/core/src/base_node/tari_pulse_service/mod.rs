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
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future;
use log::{debug, info, trace, warn};
use serde::{Deserialize, Serialize};
use tari_common::DnsNameServer;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId, Minimized};
use tari_comms_dht::{envelope::NodeDestination, Dht, DhtDiscoveryRequester};
use tari_p2p::{
    dns::DnsClient,
    services::liveness::{LivenessEvent, LivenessHandle},
    Network,
};
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::{
    sync::{broadcast::error::RecvError, watch, Mutex, RwLock},
    task,
    time::{self, timeout, MissedTickBehavior},
};

use super::LocalNodeCommsInterface;
use crate::base_node::comms_interface::CommsInterfaceError;
const LOG_TARGET: &str = "c::bn::tari_pulse";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TariPulseConfig {
    pub dns_check_interval: Duration,
    pub liveness_interval: Duration,
    pub network: Network,
}

#[derive(Debug, Clone)]
pub struct LivenessCheckResult {
    pub peer: NodeId,
    pub discovery_latency: Option<Duration>,
    pub ping_latency: Option<Duration>,
}

impl Default for TariPulseConfig {
    fn default() -> Self {
        Self {
            dns_check_interval: Duration::from_secs(120),
            liveness_interval: Duration::from_secs(60 * 10),
            network: Network::default(),
        }
    }
}

fn get_network_dns_name(network: Network) -> &'static str {
    match network {
        Network::NextNet => "checkpoints-nextnet.tari.com.",
        Network::MainNet => "checkpoints.tari.com.",
        Network::Esmeralda => "checkpoints-esmeralda.tari.com.",
        Network::StageNet => "checkpoints-stagenet.tari.com.",
        Network::Igor => "checkpoints-igor.tari.com.",
        Network::LocalNet => "checkpoints-localnet.tari.com.",
    }
}

pub struct TariPulseService {
    dns_name: &'static str,
    config: TariPulseConfig,
    shutdown_signal: ShutdownSignal,
    node_comms: ConnectivityRequester,
    liveness_handle: LivenessHandle,
    node_discovery: DhtDiscoveryRequester,
}

impl TariPulseService {
    pub async fn new(
        config: TariPulseConfig,
        node_comms: ConnectivityRequester,
        liveness_handle: LivenessHandle,
        node_discovery: DhtDiscoveryRequester,
        shutdown_signal: ShutdownSignal,
    ) -> Result<Self, anyhow::Error> {
        let dns_name = get_network_dns_name(config.clone().network);
        info!(target: LOG_TARGET, "Tari Pulse Service initialized with DNS name: {}", dns_name);
        Ok(Self {
            dns_name,
            config,
            node_comms,
            liveness_handle,
            node_discovery,
            shutdown_signal,
        })
    }

    fn get_dns_client(&self) -> Result<DnsClient, anyhow::Error> {
        let client = DnsClient::connect(DnsNameServer::System)?;
        Ok(client)
    }

    pub async fn run(
        &mut self,
        mut base_node_service: LocalNodeCommsInterface,
        notify_passed_checkpoints: watch::Sender<bool>,
        notify_comms_health: watch::Sender<Vec<LivenessCheckResult>>,
    ) {
        let mut dns_check_interval = time::interval(self.config.dns_check_interval);
        dns_check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        tokio::pin!(dns_check_interval);

        let mut health_check_interval = time::interval(self.config.liveness_interval);
        health_check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        tokio::pin!(health_check_interval);

        let mut shutdown_signal = self.shutdown_signal.clone();
        let mut count = 0u64;
        let health_check_in_progress = Arc::new(Mutex::new(()));

        loop {
            tokio::select! {
                _ = health_check_interval.tick() => {
                    let h_check = health_check_in_progress.clone();
                    let liveness_handle = self.liveness_handle.clone();
                    let comms = self.node_comms.clone();
                    let discovery = self.node_discovery.clone();
                    let notify_channel = notify_comms_health.clone();
                    tokio::spawn(async move {
                        let mut _lock = match h_check.try_lock() {
                            Ok(val) => val,
                            _ => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Could not acquire lock for health check, skipping this tick"
                                );
                                return;
                            },
                        };
                        check_health(comms, liveness_handle, discovery, notify_channel).await;
                    });
                }
                _ = dns_check_interval.tick() => {
                    count += 1;
                    trace!(target: LOG_TARGET, "DNS Checkpoint interval tick: {}", count);
                    let passed_checkpoints = {
                        match self.passed_checkpoints(&mut base_node_service).await {
                            Ok(passed) => {
                                passed
                            },
                            Err(err) => {
                                warn!(target: LOG_TARGET, "Failed to check if node has passed checkpoints: {}", err);
                                continue;
                            },
                        }
                    };

                    let _unused = notify_passed_checkpoints
                        .send(!passed_checkpoints).inspect_err(|e| {
                            warn!(target: LOG_TARGET, "Failed to send passed checkpoints notification: {}", e);
                        });
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
        let dns_checkpoints = match timeout(Duration::from_secs(1), self.fetch_checkpoints()).await {
            Ok(Ok(checkpoints)) => checkpoints,
            Ok(Err(err)) => {
                warn!(target: LOG_TARGET, "Failed to fetch DNS checkpoints: {}", err);
                return Err(err);
            },
            Err(_) => {
                warn!(target: LOG_TARGET, "Timeout fetching DNS checkpoints. We can't tell whether our blockchain has the correct data or not.");
                // We can't connect to DNS, so can't tell if we are behind or not.
                return Ok(true);
            },
        };

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
                None => Err(CommsInterfaceError::InternalError(format!(
                    "Header not found for block height {}",
                    block_height
                ))),
            })?;

        Ok(historical_block)
    }

    async fn fetch_checkpoints(&mut self) -> Result<Vec<(u64, String)>, anyhow::Error> {
        let mut client = self.get_dns_client()?;
        let response = client.query_txt(self.dns_name).await?;
        let checkpoints: Vec<(u64, String)> = response
            .iter()
            .filter_map(|ascii_txt| {
                let (height, hash) = ascii_txt.split_once(':')?;
                Some((height.parse().ok()?, hash.to_string()))
            })
            .collect();

        Ok(checkpoints)
    }
}

#[derive(Clone)]
pub struct TariPulseHandle {
    pub shutdown_signal: ShutdownSignal,
    pub failed_checkpoints_notifier: watch::Receiver<bool>,
    pub liveness_checks: watch::Receiver<Vec<LivenessCheckResult>>,
}

impl TariPulseHandle {
    pub fn get_failed_checkpoints_notifier(&self) -> watch::Ref<'_, bool> {
        self.failed_checkpoints_notifier.borrow()
    }

    pub fn get_liveness_checks(&self) -> watch::Ref<'_, Vec<LivenessCheckResult>> {
        self.liveness_checks.borrow()
    }
}

pub struct TariPulseServiceInitializer {
    dns_interval: Duration,
    liveness_interval: Duration,
    network: Network,
}

impl TariPulseServiceInitializer {
    pub fn new(dns_interval: Duration, liveness_interval: Duration, network: Network) -> Self {
        Self {
            dns_interval,
            liveness_interval,
            network,
        }
    }
}

#[async_trait]
impl ServiceInitializer for TariPulseServiceInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        info!(target: LOG_TARGET, "Initializing Tari Pulse Service");
        let shutdown_signal = context.get_shutdown_signal();
        let (dns_sender, dns_receiver) = watch::channel(false);
        let (liveness_sender, liveness_receiver) = watch::channel(vec![]);
        context.register_handle(TariPulseHandle {
            shutdown_signal: shutdown_signal.clone(),
            failed_checkpoints_notifier: dns_receiver,
            liveness_checks: liveness_receiver,
        });
        let config = TariPulseConfig {
            dns_check_interval: self.dns_interval,
            liveness_interval: self.liveness_interval,
            network: self.network,
        };

        context.spawn_when_ready(move |handles| async move {
            let base_node_service = handles.expect_handle::<LocalNodeCommsInterface>();
            let node_comms = handles.expect_handle::<ConnectivityRequester>();
            let liveness = handles.expect_handle::<LivenessHandle>();
            let base_node_dht = handles.expect_handle::<Dht>();
            let node_discovery = base_node_dht.discovery_service_requester();
            let mut tari_pulse_service =
                TariPulseService::new(config, node_comms, liveness, node_discovery, shutdown_signal.clone())
                    .await
                    .expect("Should be able to get the service");
            let tari_pulse_service = tari_pulse_service.run(base_node_service, dns_sender, liveness_sender);
            futures::pin_mut!(tari_pulse_service);
            future::select(tari_pulse_service, shutdown_signal).await;
            info!(target: LOG_TARGET, "Tari Pulse Service shutdown");
        });
        info!(target: LOG_TARGET, "Tari Pulse Service initialized");
        Ok(())
    }
}

async fn check_health(
    mut node_comms: ConnectivityRequester,
    liveness_handle: LivenessHandle,
    node_discovery: DhtDiscoveryRequester,
    notify_comms_health: watch::Sender<Vec<LivenessCheckResult>>,
) {
    let results = Arc::new(RwLock::new(Vec::new()));
    let peers = node_comms.get_seeds().await.unwrap_or_else(|_| vec![]);
    let mut handles = vec![];
    trace!(target: LOG_TARGET, "check_health started contacting {} seed peers", peers.len());
    for peer in &peers {
        let result_clone = results.clone();
        let mut result = LivenessCheckResult {
            peer: peer.node_id.clone(),
            discovery_latency: None,
            ping_latency: None,
        };
        let dest_key = peer.public_key.clone();
        let mut discovery = node_discovery.clone();
        let mut liveness_events = liveness_handle.get_event_stream();
        let mut liveness = liveness_handle.clone();
        let mut comms = node_comms.clone();
        handles.push(task::spawn(async move {
            let start = Instant::now();
            if discovery
                .discover_peer(dest_key.clone(), NodeDestination::PublicKey(dest_key.into()))
                .await
                .is_ok()
            {
                result.discovery_latency = Some(start.elapsed());
            }
            let start2 = Instant::now();
            if let Ok(nonce) = liveness.send_ping(result.peer.clone()).await {
                loop {
                    match liveness_events.recv().await {
                        Ok(event) => {
                            if let LivenessEvent::ReceivedPong(pong) = &*event {
                                if pong.node_id == result.peer && pong.nonce == nonce {
                                    result.ping_latency = Some(start2.elapsed());
                                    break;
                                }
                            }
                        },
                        Err(RecvError::Closed) => {
                            break;
                        },
                        Err(RecvError::Lagged(_)) => {},
                    }
                }
            }
            if let Ok(Some(mut conn)) = comms.get_connection(result.peer.clone()).await {
                if let Err(err) = conn.disconnect_if_unused(Minimized::No, 0, 2, "Health check").await {
                    warn!(target: LOG_TARGET, "Failed to disconnect peer {} ({})", result.peer, err);
                }
            }
            (*result_clone).write().await.push(result);
        }));
    }
    futures::future::join_all(handles).await;
    let inner_result = (*(*results).read().await).clone();
    notify_comms_health.send(inner_result).expect("Channel should be open");
    trace!(target: LOG_TARGET, "check_health ended");
}
