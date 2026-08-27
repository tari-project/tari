// Copyright 2026. The Tari Project
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

//! # Minotari peer sync
//!
//! Brings up the same comms + DHT stack that the base node uses, lets it do its normal peer sync (the DHT
//! "seed strap": dial the configured/DNS seed peers and stream their peer lists back), then dials every peer that was
//! downloaded and reports how many of them could actually be connected to.
//!
//! Nothing blockchain related is started - no database, no sync, no gRPC - so this only exercises peer discovery and
//! connectivity.

mod cli;
mod config;

use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    fs,
    path::PathBuf,
    process,
    sync::Arc,
    time::{Duration, Instant},
};

use clap::Parser;
use futures::{StreamExt, stream};
use log::*;
use minotari_app_utilities::{
    consts,
    identity_management::{load_from_json, setup_node_identity},
    utilities::setup_runtime,
};
use tari_common::{
    exit_codes::{ExitCode, ExitError},
    initialize_logging,
    load_configuration,
};
use tari_comms::{
    CommsNode,
    Minimized,
    NodeIdentity,
    RefKind,
    UnspawnedCommsNode,
    connectivity::ConnectivityRequester,
    multiaddr::{Multiaddr, Protocol},
    peer_manager::{Peer, PeerFeatures},
};
use tari_comms_dht::{Dht, event::DhtEvent};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{P2pInitializer, spawn_comms_using_transport},
};
use tari_service_framework::StackBuilder;
use tari_shutdown::Shutdown;
use tari_utilities::hex::Hex;
use tokio::{sync::broadcast::error::RecvError, time};

use crate::{cli::Cli, config::PeerSyncConfig};

const LOG_TARGET: &str = "minotari::peer_sync";
/// The pubsub buffer for inbound domain messages. Nothing consumes them here, they are dropped.
const MESSAGE_BUFFER_SIZE: usize = 100;

fn main() {
    match main_inner() {
        Ok(report) => {
            println!("{report}");
        },
        Err(err) => {
            eprintln!("{err:?}");
            let exit_code = err.exit_code;
            if let Some(hint) = exit_code.hint() {
                eprintln!("{hint}");
            }
            error!(target: LOG_TARGET, "Exiting with code ({}): {:?}", exit_code as i32, err);
            process::exit(exit_code as i32);
        },
    }
}

fn main_inner() -> Result<Report, ExitError> {
    let cli = Cli::parse();

    let base_path = cli.common.get_base_path();
    initialize_logging(
        &cli.common.log_config_path("peer_sync"),
        cli.common.log_path.as_ref().unwrap_or(&base_path),
        include_str!("../log4rs_sample.yml"),
    )?;

    info!(target: LOG_TARGET, "Starting Minotari peer sync version: {}", consts::APP_VERSION);

    let cfg = load_configuration(cli.common.config_path(), true, true, &cli, cli.common.network)?;
    let mut config = PeerSyncConfig::load_from(&cfg)?;
    apply_overrides(&cli, &mut config)?;

    let node_identity = build_node_identity(&cli, &config)?;
    info!(
        target: LOG_TARGET,
        "Peer sync node identity: {} ({})",
        node_identity.node_id().to_hex(),
        if cli.use_node_identity { "base node identity" } else { "throw-away identity" }
    );

    let runtime = setup_runtime()?;
    runtime.block_on(run(cli, config, node_identity))
}

/// Applies the peer-sync specific overrides on top of the base node's config. These exist so that a run does not
/// disturb - or get disturbed by - a base node that is running against the same base directory.
fn apply_overrides(cli: &Cli, config: &mut PeerSyncConfig) -> Result<(), ExitError> {
    let p2p = &mut config.base_node.p2p;

    // Keep this run's peers out of the base node's peer database.
    let peer_db_dir = cli
        .peer_db_dir
        .clone()
        .unwrap_or_else(|| p2p.datastore_path.join("peer_sync"));
    fs::create_dir_all(&peer_db_dir)
        .map_err(|e| ExitError::new(ExitCode::ConfigError, format!("Could not create {peer_db_dir:?}: {e}")))?;
    if !cli.reuse_peer_db {
        clear_peer_database(&peer_db_dir, &p2p.peer_database_name)?;
    }
    p2p.datastore_path = peer_db_dir;

    // Bind the listener to an OS-assigned port by default so that we do not fight the base node for its port.
    p2p.transport.tcp.listener_address = with_tcp_port(&p2p.transport.tcp.listener_address, cli.listener_port);
    if let Some(addr) = p2p.transport.tor.listener_address_override.as_ref() {
        p2p.transport.tor.listener_address_override = Some(with_tcp_port(addr, cli.listener_port));
    }

    // We advertise no address of our own (see build_node_identity), so there is nothing to liveness check.
    p2p.listener_self_liveness_check_interval = None;

    Ok(())
}

/// Deletes the sqlite peer database files so that every run starts by actually downloading peers. Note that peer sync
/// is skipped by the DHT when the peer database already holds enough usable peers.
fn clear_peer_database(dir: &PathBuf, database_name: &str) -> Result<(), ExitError> {
    let prefix = format!("{database_name}.db");
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            return Err(ExitError::new(
                ExitCode::ConfigError,
                format!("Could not read {dir:?}: {e}"),
            ));
        },
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(&prefix) {
            fs::remove_file(entry.path()).map_err(|e| {
                ExitError::new(
                    ExitCode::ConfigError,
                    format!("Could not remove old peer database {:?}: {}", entry.path(), e),
                )
            })?;
        }
    }
    Ok(())
}

/// Returns `addr` with its TCP port replaced by `port`.
fn with_tcp_port(addr: &Multiaddr, port: u16) -> Multiaddr {
    addr.iter()
        .map(|p| match p {
            Protocol::Tcp(_) => Protocol::Tcp(port),
            other => other,
        })
        .collect()
}

/// By default a random identity is used: running as the base node's identity while the node itself is online would
/// have both instances claiming the same node id on the network. The random identity advertises no addresses - peers
/// accept an address-less identity claim, and we never expect inbound connections.
fn build_node_identity(cli: &Cli, config: &PeerSyncConfig) -> Result<Arc<NodeIdentity>, ExitError> {
    if cli.use_node_identity {
        return setup_node_identity(
            &config.base_node.identity_file,
            config.base_node.p2p.public_addresses.clone().into_vec(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            config.base_node.p2p.transport.transport_type,
        );
    }
    Ok(Arc::new(NodeIdentity::random_multiple_addresses(
        &mut rand::rng(),
        Vec::new(),
        PeerFeatures::COMMUNICATION_NODE,
    )))
}

async fn run(cli: Cli, config: PeerSyncConfig, node_identity: Arc<NodeIdentity>) -> Result<Report, ExitError> {
    let mut shutdown = Shutdown::new();
    let mut report = Report::new(&cli, &config, &node_identity);

    let (comms, dht_bootstrap) = start_comms(&cli, &config, node_identity, shutdown.to_signal()).await?;
    let peer_manager = comms.peer_manager();
    let connectivity = comms.connectivity();

    // --- Peer sync ------------------------------------------------------------------------------------------------
    let seeds = peer_manager
        .get_seed_peers()
        .await
        .map_err(|e| ExitError::new(ExitCode::NetworkError, e))?;
    report.num_seed_peers = seeds.len();
    println!(
        "Starting peer sync with {} seed peer(s) on {}. This can take a few minutes...",
        seeds.len(),
        config.base_node.network
    );

    let started = Instant::now();
    report.sync = dht_bootstrap.wait(Duration::from_secs(cli.sync_timeout)).await;
    report.sync_time = started.elapsed();
    if cli.settle_time > 0 {
        time::sleep(Duration::from_secs(cli.settle_time)).await;
    }

    let all_peers = peer_manager
        .all(None)
        .await
        .map_err(|e| ExitError::new(ExitCode::NetworkError, e))?;
    report.num_peers_in_db = all_peers.len();
    report.num_downloaded = all_peers.iter().filter(|p| !p.is_seed()).count();

    // --- Dial every peer ------------------------------------------------------------------------------------------
    let mut num_skipped = 0usize;
    let mut candidates: Vec<Peer> = all_peers
        .into_iter()
        .filter(|p| {
            if cli.skip_seeds && p.is_seed() {
                return false;
            }
            if p.deleted_at.is_some() || p.is_banned() {
                num_skipped += 1;
                return false;
            }
            true
        })
        .collect();
    report.num_skipped = num_skipped;
    if let Some(max) = cli.max_peers {
        candidates.truncate(max);
    }

    println!("Peer sync done, dialing {} peer(s)...", candidates.len());
    let started = Instant::now();
    let dial_timeout = Duration::from_secs(cli.dial_timeout);
    let results: Vec<DialOutcome> = stream::iter(candidates.iter())
        .map(|peer| dial_peer(&connectivity, peer, dial_timeout))
        .buffer_unordered(cli.concurrency.max(1))
        .collect()
        .await;
    report.dial_time = started.elapsed();
    report.add_dial_results(results);

    shutdown.trigger();
    if time::timeout(Duration::from_secs(10), comms.wait_until_shutdown())
        .await
        .is_err()
    {
        debug!(target: LOG_TARGET, "Comms did not shut down cleanly within 10s");
    }

    Ok(report)
}

/// Builds and spawns the same comms + DHT stack the base node builds, minus the blockchain services. The DHT starts
/// its network discovery (peer sync) state machine on its own as soon as comms comes online.
async fn start_comms(
    cli: &Cli,
    config: &PeerSyncConfig,
    node_identity: Arc<NodeIdentity>,
    shutdown_signal: tari_shutdown::ShutdownSignal,
) -> Result<(CommsNode, BootstrapWaiter), ExitError> {
    let mut p2p_config = config.base_node.p2p.clone();
    if cli.use_node_identity {
        p2p_config.transport.tor.identity = load_from_json(&config.base_node.tor_identity_file)
            .map_err(|e| ExitError::new(ExitCode::ConfigError, e))?;
    }

    let user_agent = cli
        .user_agent
        .clone()
        .unwrap_or_else(|| format!("tari/basenode/{}", consts::APP_VERSION_NUMBER));
    let (publisher, _subscription_factory) = pubsub_connector(MESSAGE_BUFFER_SIZE);

    let mut handles = StackBuilder::new(shutdown_signal)
        .add_initializer(P2pInitializer::new(
            p2p_config.clone(),
            user_agent,
            config.peer_seeds.clone(),
            config.base_node.network,
            node_identity,
            publisher,
        ))
        .build()
        .await
        .map_err(|e| ExitError::new(ExitCode::NetworkError, e))?;

    // Subscribe before comms is spawned: network discovery only starts once comms is online, so no event is missed.
    let dht = handles.expect_handle::<Dht>();
    let bootstrap = BootstrapWaiter::new(&dht);

    let comms = handles
        .take_handle::<UnspawnedCommsNode>()
        .expect("P2pInitializer did not add UnspawnedCommsNode");
    let comms = spawn_comms_using_transport(comms, p2p_config.transport.clone(), |_identity| {})
        .await
        .map_err(|e| e.to_exit_error())?;

    Ok((comms, bootstrap))
}

/// Watches the DHT event stream for the end of the initial peer sync round.
struct BootstrapWaiter {
    events: tari_comms_dht::event::DhtEventReceiver,
}

impl BootstrapWaiter {
    fn new(dht: &Dht) -> Self {
        Self {
            events: dht.subscribe_dht_events(),
        }
    }

    /// Waits for the DHT to report that its initial bootstrap is done, collecting the peer sync round statistics
    /// along the way.
    async fn wait(mut self, timeout: Duration) -> SyncOutcome {
        let mut outcome = SyncOutcome::default();
        let deadline = time::Instant::now() + timeout;
        loop {
            let event = tokio::select! {
                event = self.events.recv() => event,
                _ = time::sleep_until(deadline) => {
                    warn!(target: LOG_TARGET, "Timed out after {timeout:.0?} waiting for peer sync to complete");
                    outcome.timed_out = true;
                    return outcome;
                },
            };
            match event {
                Ok(event) => match &*event {
                    DhtEvent::NetworkDiscoveryPeersAdded(info) => {
                        info!(target: LOG_TARGET, "Peer sync round complete: {info}");
                        outcome.rounds += 1;
                        outcome.num_new_peers += info.num_new_peers;
                        outcome.num_duplicate_peers += info.num_duplicate_peers;
                        outcome.num_seeds_contacted += info.num_succeeded;
                    },
                    DhtEvent::PrimaryBootstrapComplete => {
                        outcome.completed = true;
                        return outcome;
                    },
                    _ => {},
                },
                Err(RecvError::Lagged(n)) => {
                    debug!(target: LOG_TARGET, "Lagged {n} DHT event(s)");
                },
                Err(RecvError::Closed) => {
                    warn!(target: LOG_TARGET, "DHT event stream closed before bootstrap completed");
                    return outcome;
                },
            }
        }
    }
}

async fn dial_peer(connectivity: &ConnectivityRequester, peer: &Peer, dial_timeout: Duration) -> DialOutcome {
    let started = Instant::now();
    let result = time::timeout(
        dial_timeout,
        connectivity.dial_peer(peer.node_id.clone(), RefKind::Weak),
    )
    .await;
    let error = match result {
        Ok(Ok(mut conn)) => {
            debug!(target: LOG_TARGET, "Connected to {} in {:.1?}", peer.node_id, started.elapsed());
            if let Err(err) = conn.disconnect(Minimized::Yes, "peer sync probe complete").await {
                debug!(target: LOG_TARGET, "Failed to disconnect from {}: {}", peer.node_id, err);
            }
            None
        },
        Ok(Err(err)) => Some(err.to_string()),
        Err(_) => Some(format!("Dial timed out after {dial_timeout:.0?}")),
    };
    DialOutcome {
        node_id: peer.node_id.to_hex(),
        address: peer
            .addresses
            .best()
            .map(|a| a.address().to_string())
            .unwrap_or_else(|| "no address".to_string()),
        is_seed: peer.is_seed(),
        latency: started.elapsed(),
        error,
    }
}

/// Dial failures usually name the peer or address that failed, which would make every failure look unique in the
/// summary. Those parts are replaced with placeholders so that failures group by their actual cause.
fn generalize_failure(reason: &str) -> String {
    reason
        .split_whitespace()
        .map(|token| {
            let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '/');
            if trimmed.starts_with('/') {
                "<address>"
            } else if trimmed.len() >= 20 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                "<peer>"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// -------------------------------------------- Reporting ---------------------------------------------------------//

#[derive(Debug, Default)]
struct SyncOutcome {
    completed: bool,
    timed_out: bool,
    rounds: usize,
    num_new_peers: usize,
    num_duplicate_peers: usize,
    num_seeds_contacted: usize,
}

#[derive(Debug)]
struct DialOutcome {
    node_id: String,
    address: String,
    is_seed: bool,
    latency: Duration,
    error: Option<String>,
}

struct Report {
    network: String,
    transport: String,
    node_id: String,
    ephemeral_identity: bool,
    show_peers: bool,
    num_seed_peers: usize,
    num_peers_in_db: usize,
    num_downloaded: usize,
    num_skipped: usize,
    sync: SyncOutcome,
    sync_time: Duration,
    dial_time: Duration,
    dialed: Vec<DialOutcome>,
}

impl Report {
    fn new(cli: &Cli, config: &PeerSyncConfig, node_identity: &NodeIdentity) -> Self {
        Self {
            network: config.base_node.network.to_string(),
            transport: format!("{:?}", config.base_node.p2p.transport.transport_type).to_lowercase(),
            node_id: node_identity.node_id().to_hex(),
            ephemeral_identity: !cli.use_node_identity,
            show_peers: cli.show_peers,
            num_seed_peers: 0,
            num_peers_in_db: 0,
            num_downloaded: 0,
            num_skipped: 0,
            sync: SyncOutcome::default(),
            sync_time: Duration::ZERO,
            dial_time: Duration::ZERO,
            dialed: Vec::new(),
        }
    }

    fn add_dial_results(&mut self, mut results: Vec<DialOutcome>) {
        results.sort_by_key(|r| (r.error.is_some(), r.latency));
        self.dialed = results;
    }

    fn num_connected(&self) -> usize {
        self.dialed.iter().filter(|r| r.error.is_none()).count()
    }
}

impl Display for Report {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let num_dialed = self.dialed.len();
        let num_connected = self.num_connected();
        let num_failed = num_dialed.saturating_sub(num_connected);
        let pct = if num_dialed == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            {
                num_connected as f64 * 100.0 / num_dialed as f64
            }
        };

        writeln!(
            f,
            "\n================================ Peer sync report ================================"
        )?;
        writeln!(f, "Network                       : {}", self.network)?;
        writeln!(
            f,
            "Node id                       : {} ({})",
            self.node_id,
            if self.ephemeral_identity {
                "throw-away identity"
            } else {
                "base node identity"
            }
        )?;
        writeln!(f, "Transport                     : {}", self.transport)?;
        writeln!(
            f,
            "---------------------------------- Peer sync -----------------------------------"
        )?;
        writeln!(f, "Seed peers (config + DNS)     : {}", self.num_seed_peers)?;
        writeln!(f, "Seed peers synced from        : {}", self.sync.num_seeds_contacted)?;
        writeln!(f, "Peers downloaded              : {}", self.num_downloaded)?;
        writeln!(
            f,
            "  new / duplicate this run    : {} / {}",
            self.sync.num_new_peers, self.sync.num_duplicate_peers
        )?;
        writeln!(f, "Peers in peer database        : {}", self.num_peers_in_db)?;
        writeln!(
            f,
            "Peer sync status              : {} after {:.1?} ({} round(s))",
            if self.sync.completed {
                "completed"
            } else if self.sync.timed_out {
                "TIMED OUT"
            } else {
                "did not complete"
            },
            self.sync_time,
            self.sync.rounds
        )?;
        writeln!(
            f,
            "----------------------------------- Dialing ------------------------------------"
        )?;
        writeln!(f, "Peers dialled                 : {num_dialed}")?;
        if self.num_skipped > 0 {
            writeln!(f, "  skipped (banned / deleted)  : {}", self.num_skipped)?;
        }
        writeln!(f, "Connected                     : {num_connected} ({pct:.1}%)")?;
        writeln!(f, "Failed                        : {num_failed}")?;
        writeln!(f, "Time to dial all peers        : {:.1?}", self.dial_time)?;

        if num_failed > 0 {
            let mut reasons: HashMap<String, usize> = HashMap::new();
            for outcome in self.dialed.iter().filter_map(|r| r.error.as_deref()) {
                *reasons.entry(generalize_failure(outcome)).or_default() += 1;
            }
            let mut reasons: Vec<_> = reasons.into_iter().collect();
            reasons.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            writeln!(f, "Failure reasons:")?;
            for (reason, count) in reasons.iter().take(10) {
                writeln!(f, "  {count:>5} x {reason}")?;
            }
        }

        if self.show_peers {
            writeln!(
                f,
                "------------------------------------ Peers -------------------------------------"
            )?;
            for outcome in &self.dialed {
                let seed = if outcome.is_seed { " [seed]" } else { "" };
                match &outcome.error {
                    None => writeln!(
                        f,
                        "  OK   {} {} ({:.1?}){}",
                        outcome.node_id, outcome.address, outcome.latency, seed
                    )?,
                    Some(err) => writeln!(f, "  FAIL {} {} - {}{}", outcome.node_id, outcome.address, err, seed)?,
                }
            }
        }
        writeln!(
            f,
            "================================================================================"
        )
    }
}
