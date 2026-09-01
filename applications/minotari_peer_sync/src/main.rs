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
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter},
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use futures::{StreamExt, future, stream};
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
    PeerConnection,
    PeerManager,
    RefKind,
    UnspawnedCommsNode,
    connectivity::ConnectivityRequester,
    multiaddr::{Multiaddr, Protocol},
    peer_manager::{NodeId, Peer, PeerFeatures},
    utils::multiaddr::multiaddr_to_socketaddr,
};
use tari_comms_dht::{
    Dht,
    DhtClient,
    DhtConfig,
    GetPeersRequest,
    GetPeersResponse,
    NetworkDiscoveryConfig,
    PeerInfo,
    PeerValidator,
    UnvalidatedPeerInfo,
    event::DhtEvent,
};
use tari_p2p::{
    TransportType,
    comms_connector::InboundDomainConnector,
    initialization::{P2pInitializer, spawn_comms_using_transport},
};
use tari_service_framework::StackBuilder;
use tari_shutdown::Shutdown;
use tari_utilities::hex::Hex;
use tokio::{
    sync::{broadcast::error::RecvError, mpsc},
    time,
};

use crate::{cli::Cli, config::PeerSyncConfig};

const LOG_TARGET: &str = "minotari::peer_sync";
/// The buffer for inbound domain messages. Nothing consumes them here, they are dropped.
const MESSAGE_BUFFER_SIZE: usize = 100;
/// How long to wait for the listener to bind before giving up on it. Creating a tor hidden service is the slow case.
const LISTENER_TIMEOUT: Duration = Duration::from_secs(120);
/// How long to wait for an already-running tor's control port during the pre-flight check.
const TOR_CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
/// How long to wait for the bundled tor (libtor) to open its control port. Starting tor takes a few seconds.
const LIBTOR_STARTUP_TIMEOUT: Duration = Duration::from_secs(120);

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

    let libtor_dir = start_libtor(&cli, &mut config, &base_path)?;
    check_tor_is_reachable(&config, libtor_dir.as_deref())?;

    let node_identity = build_node_identity(&cli, &config)?;
    info!(
        target: LOG_TARGET,
        "Peer sync node identity: {} ({})",
        node_identity.node_id().to_hex(),
        if cli.use_node_identity { "base node identity" } else { "throw-away identity" }
    );

    let runtime = setup_runtime()?;
    let result = runtime.block_on(run(cli, config, node_identity, libtor_dir.clone()));

    // A per-run tor data directory holds a consensus cache of tens of megabytes and is of no use to the next run, so
    // only the shared directory is kept.
    if let Some(dir) = libtor_dir.filter(|d| d.ends_with(format!("peer_sync-{}", process::id()))) {
        debug!(target: LOG_TARGET, "Removing the per-run tor data directory {}", dir.display());
        if let Err(err) = fs::remove_dir_all(&dir) {
            debug!(target: LOG_TARGET, "Could not remove {}: {}", dir.display(), err);
        }
    }
    result
}

/// Applies the peer-sync specific overrides on top of the base node's config. These exist so that a run does not
/// disturb - or get disturbed by - a base node that is running against the same base directory.
fn apply_overrides(cli: &Cli, config: &mut PeerSyncConfig) -> Result<(), ExitError> {
    let p2p = &mut config.base_node.p2p;

    if let Some(transport) = cli.transport {
        p2p.transport.transport_type = transport.into();
    }

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

    // The DHT's network discovery state machine returns immediately when this is off, which would leave the run
    // waiting out --sync-timeout with an empty peer database and no indication of why.
    if !p2p.dht.network_discovery.enabled {
        return Err(ExitError::new(
            ExitCode::ConfigError,
            "`base_node.p2p.dht.network_discovery.enabled` is false, so the DHT downloads no peers and there would be \
             nothing to dial. Enable it in config.toml, or override it for this run with `-p \
             base_node.p2p.dht.network_discovery.enabled=true`.",
        ));
    }

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

/// Starts the bundled tor instance when the transport needs tor, pointing the transport at its control port. This is
/// the same thing the base node does with `use_libtor`, so a run needs no externally managed tor.
///
/// Returns the tor data directory when tor was started here.
#[cfg(all(unix, feature = "libtor"))]
fn start_libtor(cli: &Cli, config: &mut PeerSyncConfig, base_path: &Path) -> Result<Option<PathBuf>, ExitError> {
    use tari_libtor::tor::Tor;

    let wanted = !cli.no_libtor && (cli.libtor || config.base_node.use_libtor);
    if !wanted || !config.base_node.p2p.transport.is_tor() {
        return Ok(None);
    }
    // Its own data directory: a base node's libtor instance owns (and locks) the one under its own directory.
    let mut data_dir = cli
        .libtor_data_dir
        .clone()
        .unwrap_or_else(|| base_path.to_path_buf())
        .join("libtor")
        .join("peer_sync");
    // Tor refuses to run two instances against one data directory: it waits five seconds for the lock and then exits,
    // which surfaces later as an unexplained listener failure. Give a concurrent run its own directory instead. The
    // shared one is kept as the default because it caches the consensus, which is what makes startup quick.
    if is_tor_data_dir_in_use(&data_dir) {
        let fallback = data_dir.with_file_name(format!("peer_sync-{}", process::id()));
        println!(
            "The tor data directory {} is in use by another instance, using {} for this run",
            data_dir.display(),
            fallback.display()
        );
        data_dir = fallback;
    }
    println!("Starting the bundled tor instance in {}...", data_dir.display());
    let tor = Tor::initialize(data_dir.clone())?;
    tor.update_comms_transport(&mut config.base_node.p2p.transport)?;
    tor.run_background();
    debug!(target: LOG_TARGET, "Bundled tor started: {:?}", config.base_node.p2p.transport.tor.control_address);
    Ok(Some(data_dir))
}

/// Whether another process is using this tor data directory. Tor takes a `flock` on `data/lock` for as long as it
/// runs, so probing that lock is the same test tor itself makes. The control port it writes to `data/control_port` is
/// not a usable signal: every run overwrites that file, so it can name a port belonging to a run that has since ended.
#[cfg(all(unix, feature = "libtor"))]
fn is_tor_data_dir_in_use(data_dir: &Path) -> bool {
    let Ok(lock) = fs::File::open(data_dir.join("data").join("lock")) else {
        // No lock file, so tor has never run here
        return false;
    };
    match lock.try_lock() {
        // Taken here, so nothing else holds it. Dropping the file releases it again.
        Ok(()) => false,
        Err(fs::TryLockError::WouldBlock) => true,
        Err(err) => {
            debug!(target: LOG_TARGET, "Could not probe the tor data directory lock: {err}");
            false
        },
    }
}

#[cfg(not(all(unix, feature = "libtor")))]
fn start_libtor(_cli: &Cli, _config: &mut PeerSyncConfig, _base_path: &Path) -> Result<Option<PathBuf>, ExitError> {
    Ok(None)
}

/// The tor transports cannot bind a listener without tor, and when the listener fails the whole connection manager
/// quits - leaving every dial to fail with an unhelpful "channel closed". Check up front so that the reason, and the
/// way around it, is the first thing reported.
fn check_tor_is_reachable(config: &PeerSyncConfig, libtor_dir: Option<&Path>) -> Result<(), ExitError> {
    let transport = &config.base_node.p2p.transport;
    if !transport.transport_type.uses_tor_hidden_service() {
        return Ok(());
    }
    // A non-ip control address (e.g. a dns one) cannot be probed here; leave it to the listener.
    let Ok(control_addr) = multiaddr_to_socketaddr(&transport.tor.control_address) else {
        return Ok(());
    };
    let timeout = if libtor_dir.is_some() {
        LIBTOR_STARTUP_TIMEOUT
    } else {
        TOR_CONTROL_TIMEOUT
    };
    let Err(err) = wait_for_tor_control_port(control_addr, timeout) else {
        return Ok(());
    };
    let detail = if let Some(dir) = libtor_dir {
        format!(
            "the bundled tor did not open its control port at {control_addr} within {timeout:.0?}: {err}.\nCheck {}",
            dir.join("data").join("tor.log").display()
        )
    } else {
        format!(
            "the tor control port at {control_addr} could not be reached: {err}.\nEither start tor, or run with \
             `--libtor` to have a bundled tor started for you (unix builds with the `libtor` feature)"
        )
    };
    Err(ExitError::new(
        ExitCode::TorOffline,
        format!(
            "The `{:?}` transport needs tor, but {detail}, or run with `--transport tcp` to test over TCP only (peers \
             that advertise only onion addresses will then be unreachable).",
            transport.transport_type
        ),
    ))
}

/// Polls the tor control port until it accepts a connection or `timeout` expires.
fn wait_for_tor_control_port(control_addr: SocketAddr, timeout: Duration) -> Result<(), std::io::Error> {
    const POLL_INTERVAL: Duration = Duration::from_millis(500);
    let deadline = Instant::now().checked_add(timeout).unwrap_or_else(Instant::now);
    loop {
        match std::net::TcpStream::connect_timeout(&control_addr, TOR_CONTROL_TIMEOUT) {
            Ok(_) => return Ok(()),
            Err(err) => {
                if Instant::now() >= deadline {
                    return Err(err);
                }
                std::thread::sleep(POLL_INTERVAL);
            },
        }
    }
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

/// Runs the dial/ask rounds: round 1 dials what the seed sync found, and each round after that asks the peers that
/// answered for their peer lists and dials only the peers that have not been dialled yet.
async fn dial_rounds(
    cli: &Cli,
    config: &PeerSyncConfig,
    node_identity: &NodeIdentity,
    peer_manager: &PeerManager,
    connectivity: &ConnectivityRequester,
    report: &mut Report,
) -> Result<(), ExitError> {
    // --- Dial, then ask whoever answered for more peers, round after round -----------------------------------------
    let dial_timeout = Duration::from_secs(cli.dial_timeout);
    let mut dialled: HashSet<NodeId> = HashSet::new();
    let mut connected_last_round: Vec<Peer> = Vec::new();
    let mut all_results: Vec<DialOutcome> = Vec::new();
    let started = Instant::now();

    for round in 1..=cli.rounds.max(1) {
        // Round 1 works off the seed strap above; later rounds have to go and ask for more peers first.
        let peers_before = peer_manager.count().await;
        let (asked_ok, asked_total) = if round == 1 {
            (0, 0)
        } else {
            println!(
                "Round {round}: asking {} peer(s) that answered last round for their peer lists...",
                connected_last_round.len()
            );
            ask_peers_for_peers(
                connectivity,
                peer_manager,
                config,
                node_identity,
                &connected_last_round,
                cli,
            )
            .await
        };
        // Round 1 inherits everything the seed sync found; later rounds only count what asking added.
        let num_discovered = if round == 1 {
            peers_before
        } else {
            peer_manager.count().await.saturating_sub(peers_before)
        };

        let (candidates, num_undialled) = undialled_candidates(cli, peer_manager, &dialled).await?;
        if candidates.is_empty() {
            report.stopped_after = Some(format!("round {round}: no peers left that have not been dialled"));
            break;
        }

        println!("Round {round}: dialing {} new peer(s)...", candidates.len());
        let results: Vec<DialOutcome> = stream::iter(candidates.iter())
            .map(|peer| dial_peer(connectivity, peer, dial_timeout))
            .buffer_unordered(cli.concurrency.max(1))
            .collect()
            .await;

        dialled.extend(candidates.iter().map(|p| p.node_id.clone()));
        let connected_node_ids: HashSet<NodeId> = results
            .iter()
            .filter(|r| r.error.is_none())
            .map(|r| r.node_id.clone())
            .collect();
        connected_last_round = candidates
            .into_iter()
            .filter(|p| connected_node_ids.contains(&p.node_id))
            .collect();

        report.round_stats.push(RoundStats {
            round,
            asked_ok,
            asked_total,
            num_discovered,
            num_undialled,
            num_dialled: results.len(),
            num_connected: connected_node_ids.len(),
            num_failed: results.len().saturating_sub(connected_node_ids.len()),
        });
        all_results.extend(results);

        if connected_last_round.is_empty() && round < cli.rounds.max(1) {
            report.stopped_after = Some(format!(
                "round {round}: no peer answered, so there is nobody to ask for more"
            ));
            break;
        }
    }

    report.dial_time = started.elapsed();
    report.address_failures = collect_address_failures(peer_manager, &all_results).await;
    let connected: HashSet<NodeId> = all_results
        .iter()
        .filter(|r| r.error.is_none())
        .map(|r| r.node_id.clone())
        .collect();
    summarize_peer_db(peer_manager, &dialled, &connected, report).await?;
    report.add_dial_results(all_results);
    Ok(())
}

/// The peers that are worth dialling this round: everything in the peer database that has not been dialled yet and
/// is not banned or deleted. Banned and deleted peers are recorded in `skipped` rather than counted, because they are
/// never added to `dialled` and so come back around on every round.
///
/// Returns (the peers to dial, how many were undialled before `--max-peers` capped the round).
async fn undialled_candidates(
    cli: &Cli,
    peer_manager: &PeerManager,
    dialled: &HashSet<NodeId>,
) -> Result<(Vec<Peer>, usize), ExitError> {
    let mut candidates: Vec<Peer> = peer_manager
        .all(None)
        .await
        .map_err(|e| ExitError::new(ExitCode::NetworkError, e))?
        .into_iter()
        .filter(|p| {
            if dialled.contains(&p.node_id) || (cli.skip_seeds && p.is_seed()) {
                return false;
            }
            if p.deleted_at.is_some() || p.is_banned() {
                return false;
            }
            true
        })
        .collect();
    let num_undialled = candidates.len();
    if let Some(max) = cli.max_peers {
        candidates.truncate(max);
    }
    Ok((candidates, num_undialled))
}

/// Reads the final state of the peer database into the report. Every count the Result section prints is derived
/// here, from this one snapshot, so the section reconciles by construction: each peer falls into exactly one of
/// dialled / banned or deleted / never reached, and the three add up to the peer's population by definition.
///
/// Deriving "banned or deleted" here rather than accumulating it as the rounds go is what makes it truthful.
/// [`Peer::is_banned`] is wall-clock bound (`peer.rs:287` filters `banned_until` against `Utc::now()`), and the DHT's
/// short ban is ten minutes, so a peer banned in an early round can legitimately be dialled in a later one. A running
/// tally would have counted such a peer in both buckets at once.
async fn summarize_peer_db(
    peer_manager: &PeerManager,
    dialled: &HashSet<NodeId>,
    connected: &HashSet<NodeId>,
    report: &mut Report,
) -> Result<(), ExitError> {
    let now = now_epoch_secs();
    report.claim_ages.clear();
    report.reset_peer_db_counts();
    for peer in peer_manager
        .all(None)
        .await
        .map_err(|e| ExitError::new(ExitCode::NetworkError, e))?
    {
        let counter = report.claim_ages.entry(ClaimAge::of(&peer, now)).or_default();
        *counter = counter.saturating_add(1);

        report.count_peer(
            &peer,
            dialled.contains(&peer.node_id),
            connected.contains(&peer.node_id),
        );
    }
    report.num_peers_in_db_final = report.claim_ages.values().sum();
    Ok(())
}

async fn run(
    cli: Cli,
    config: PeerSyncConfig,
    node_identity: Arc<NodeIdentity>,
    libtor_dir: Option<PathBuf>,
) -> Result<Report, ExitError> {
    let mut shutdown = Shutdown::new();
    let mut report = Report::new(&cli, &config, &node_identity, libtor_dir.is_some());

    let (mut comms, dht_bootstrap) = start_comms(&cli, &config, node_identity, shutdown.to_signal()).await?;

    // From here on comms is spawned. It owns a peer database connection pool backed by blocking threads, and dropping
    // the runtime out from under those can hang, so every path out of this function goes through the one shutdown
    // block below - including a listener that never bound.
    let listener = wait_for_listener(&mut comms, libtor_dir.as_deref()).await;
    let peer_manager = comms.peer_manager();
    let connectivity = comms.connectivity();
    let identity = comms.node_identity();

    let result = match listener {
        Ok(()) => {
            sync_and_dial(
                &cli,
                &config,
                &identity,
                &peer_manager,
                &connectivity,
                dht_bootstrap,
                &mut report,
            )
            .await
        },
        Err(err) => Err(err),
    };

    shutdown.trigger();
    if time::timeout(Duration::from_secs(10), comms.wait_until_shutdown())
        .await
        .is_err()
    {
        debug!(target: LOG_TARGET, "Comms did not shut down cleanly within 10s");
    }

    result?;
    Ok(report)
}

/// Waits for the DHT's peer sync to finish, then dials what it downloaded.
async fn sync_and_dial(
    cli: &Cli,
    config: &PeerSyncConfig,
    node_identity: &NodeIdentity,
    peer_manager: &PeerManager,
    connectivity: &ConnectivityRequester,
    dht_bootstrap: BootstrapWaiter,
    report: &mut Report,
) -> Result<(), ExitError> {
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

    dial_rounds(cli, config, node_identity, peer_manager, connectivity, report).await
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
    // The base node hands inbound domain messages to a pubsub connector for its services to subscribe to. There is
    // no service here to subscribe, and a pubsub connector with no subscribers logs a warning for every message it
    // forwards, so the messages are drained and dropped instead.
    let (sink, mut inbound_messages) = mpsc::channel(MESSAGE_BUFFER_SIZE);
    tokio::spawn(async move { while inbound_messages.recv().await.is_some() {} });
    let publisher = InboundDomainConnector::new(sink);

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

    let comms = handles.take_handle::<UnspawnedCommsNode>().ok_or_else(|| {
        ExitError::new(
            ExitCode::NetworkError,
            "P2pInitializer did not register an UnspawnedCommsNode, so comms cannot be started",
        )
    })?;
    let comms = spawn_comms_using_transport(comms, p2p_config.transport.clone(), |_identity| {})
        .await
        .map_err(|e| e.to_exit_error())?;

    Ok((comms, bootstrap))
}

/// The listener binds asynchronously, and if it fails the connection manager exits without dialing anything. Waiting
/// for it means the failure is reported as itself, instead of showing up as every peer failing to dial.
///
/// This runs after [`start_comms`] rather than inside it because by then comms is spawned: its caller has to shut it
/// down on the way out, and a failure here is not special enough to deserve a second way of doing that.
async fn wait_for_listener(comms: &mut CommsNode, libtor_dir: Option<&Path>) -> Result<(), ExitError> {
    match time::timeout(
        LISTENER_TIMEOUT,
        comms.connection_manager_requester().wait_until_listening(),
    )
    .await
    {
        Ok(Ok(info)) => info!(target: LOG_TARGET, "Listening on {}", info.bind_address()),
        Ok(Err(err)) => {
            let where_to_look = match libtor_dir {
                Some(dir) => format!(
                    "See the network log, and {}, for the underlying error - a second tor instance on the same data \
                     directory is one cause.",
                    dir.join("data").join("tor.log").display()
                ),
                None => "See the network log for the underlying error.".to_string(),
            };
            return Err(ExitError::new(
                ExitCode::NetworkError,
                format!("The comms listener failed to start ({err}), so no peer can be dialled. {where_to_look}"),
            ));
        },
        Err(_) => warn!(
            target: LOG_TARGET,
            "The listener did not report as bound within {LISTENER_TIMEOUT:.0?}, continuing anyway"
        ),
    }
    Ok(())
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
        let deadline = time::Instant::now()
            .checked_add(timeout)
            .unwrap_or_else(time::Instant::now);
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
                        outcome.rounds = outcome.rounds.saturating_add(1);
                        outcome.num_new_peers = outcome.num_new_peers.saturating_add(info.num_new_peers);
                        outcome.num_duplicate_peers =
                            outcome.num_duplicate_peers.saturating_add(info.num_duplicate_peers);
                        outcome.num_seeds_contacted = outcome.num_seeds_contacted.saturating_add(info.num_succeeded);
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

/// Asks each of `sources` for its peer list over the DHT `get_peers` RPC and adds whatever passes validation to the
/// peer database - the same exchange the DHT's own network discovery makes with its sync peers, driven here from the
/// peers that answered in the previous round.
///
/// Returns (peers that answered, peers asked).
async fn ask_peers_for_peers(
    connectivity: &ConnectivityRequester,
    peer_manager: &PeerManager,
    config: &PeerSyncConfig,
    node_identity: &NodeIdentity,
    sources: &[Peer],
    cli: &Cli,
) -> (usize, usize) {
    let answered = stream::iter(sources.iter())
        .map(|peer| ask_peer_for_peers(connectivity, peer_manager, config, node_identity, peer, cli))
        .buffer_unordered(cli.concurrency.max(1))
        .filter(|answered| future::ready(*answered))
        .count()
        .await;
    (answered, sources.len())
}

/// Returns whether this peer answered with a peer list.
async fn ask_peer_for_peers(
    connectivity: &ConnectivityRequester,
    peer_manager: &PeerManager,
    config: &PeerSyncConfig,
    node_identity: &NodeIdentity,
    peer: &Peer,
    cli: &Cli,
) -> bool {
    let dht = &config.base_node.p2p.dht;
    let discovery = &dht.network_discovery;

    let dial = time::timeout(
        Duration::from_secs(cli.dial_timeout),
        connectivity.dial_peer(peer.node_id.clone(), RefKind::Weak),
    );
    let mut conn = match dial.await {
        Ok(Ok(conn)) => conn,
        Ok(Err(err)) => {
            debug!(target: LOG_TARGET, "Could not dial {} to ask for peers: {}", peer.node_id, err);
            return false;
        },
        Err(_) => {
            debug!(target: LOG_TARGET, "Dial to {} timed out before it could be asked for peers", peer.node_id);
            return false;
        },
    };

    let peers = match fetch_peers(&mut conn, discovery, dht).await {
        Ok(peers) => peers,
        Err(err) => {
            debug!(target: LOG_TARGET, "{} did not give us its peer list: {}", peer.node_id, err);
            let _ignore = conn.disconnect(Minimized::Yes, "peer sync round done").await;
            return false;
        },
    };
    let _ignore = conn.disconnect(Minimized::Yes, "peer sync round done").await;

    let validator = PeerValidator::new(dht);
    for peer_info in peers {
        let candidate: UnvalidatedPeerInfo = match peer_info.try_into() {
            Ok(candidate) => candidate,
            Err(err) => {
                debug!(target: LOG_TARGET, "Skipping an invalid peer from {}: {}", peer.node_id, err);
                continue;
            },
        };
        // The DHT's own seed strap drops ourselves out of the peer lists it is handed; without this we would store
        // ourselves as a peer and then spend a round trying to dial ourselves.
        if candidate.public_key == *node_identity.public_key() {
            continue;
        }
        let existing = match peer_manager.find_by_public_key(&candidate.public_key).await {
            Ok(existing) => existing,
            Err(err) => {
                debug!(target: LOG_TARGET, "Could not look up a peer offered by {}: {}", peer.node_id, err);
                continue;
            },
        };
        match validator.validate_peer(candidate, existing) {
            Ok(valid) => {
                if let Err(err) = peer_manager.add_or_update_peer(valid).await {
                    debug!(target: LOG_TARGET, "Could not store a peer offered by {}: {}", peer.node_id, err);
                }
            },
            Err(err) => debug!(target: LOG_TARGET, "Rejected a peer offered by {}: {}", peer.node_id, err),
        }
    }
    true
}

/// Runs the `get_peers` RPC over an established connection, bounded the same way the DHT bounds it: a peer is under no
/// obligation to respect the count we ask for, or to end the stream at all.
async fn fetch_peers(
    conn: &mut PeerConnection,
    discovery: &NetworkDiscoveryConfig,
    dht: &DhtConfig,
) -> Result<Vec<PeerInfo>, String> {
    let mut client = time::timeout(discovery.bootstrap_rpc_connect_timeout, conn.connect_rpc::<DhtClient>())
        .await
        .map_err(|_| "rpc connect timed out".to_string())?
        .map_err(|e| e.to_string())?;

    let num_peers = discovery.max_peers_to_sync_per_round.max(1);
    let req = GetPeersRequest {
        n: num_peers,
        include_clients: false,
        max_claims: dht.max_permitted_peer_claims.try_into().unwrap_or(u32::MAX),
        max_addresses_per_claim: dht
            .peer_validator_config
            .max_permitted_peer_addresses_per_claim
            .try_into()
            .unwrap_or(u32::MAX),
    };
    let mut stream = time::timeout(discovery.bootstrap_rpc_get_peers_stream_timeout, client.get_peers(req))
        .await
        .map_err(|_| "get_peers timed out".to_string())?
        .map_err(|e| e.to_string())?;

    let max_peers = usize::try_from(num_peers).unwrap_or(usize::MAX);
    // Allow some slack for empty responses interleaved with real ones, as the DHT does
    let max_items = max_peers.saturating_mul(2);
    let mut peers = Vec::with_capacity(max_peers);
    let mut items = 0usize;
    while peers.len() < max_peers && items < max_items {
        match time::timeout(discovery.bootstrap_rpc_streaming_timeout, stream.next()).await {
            Ok(Some(Ok(GetPeersResponse { peer }))) => {
                items = items.saturating_add(1);
                if let Some(peer) = peer {
                    peers.push(peer);
                }
            },
            // Stream ended, errored, or stalled: keep whatever it gave us
            Ok(Some(Err(err))) => {
                debug!(target: LOG_TARGET, "get_peers stream error from {}: {}", conn.peer_node_id(), err);
                break;
            },
            Ok(None) | Err(_) => break,
        }
    }
    Ok(peers)
}

/// `ConnectivityError` only reports that every address failed, not why each one did. The dialer records the reason
/// per address on the peer as it goes, so read those back for the peers that failed to give the real causes.
async fn collect_address_failures(peer_manager: &PeerManager, results: &[DialOutcome]) -> Vec<(Sanitized, usize)> {
    let failed: Vec<NodeId> = results
        .iter()
        .filter(|r| r.error.is_some())
        .map(|r| r.node_id.clone())
        .collect();
    if failed.is_empty() {
        return Vec::new();
    }
    let peers = match peer_manager.get_peers_by_node_ids(&failed).await {
        Ok(peers) => peers,
        Err(err) => {
            warn!(target: LOG_TARGET, "Could not read back the peers that failed to dial: {err}");
            return Vec::new();
        },
    };

    let mut reasons: HashMap<Sanitized, usize> = HashMap::new();
    for peer in &peers {
        for address in peer.addresses.addresses() {
            if let Some(reason) = address.last_failed_reason() {
                // The dialer recorded this against an address the peer supplied, so it is untrusted as well.
                let counter = reasons.entry(generalize_failure(&Sanitized::new(reason))).or_default();
                *counter = counter.saturating_add(1);
            }
        }
    }
    let mut reasons: Vec<_> = reasons.into_iter().collect();
    reasons.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    reasons
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
        // The connectivity error quotes the addresses it tried, which the peer chose, so it is sanitised too.
        Ok(Err(err)) => Some(Sanitized::new(&err.to_string())),
        Err(_) => Some(Sanitized::new(&format!("Dial timed out after {dial_timeout:.0?}"))),
    };
    DialOutcome {
        node_id: peer.node_id.clone(),
        address: peer
            .addresses
            .best()
            .map(|a| Sanitized::new(&a.address().to_string()))
            .unwrap_or_else(|| Sanitized::new("no address")),
        mix: AddressMix::of(peer),
        claim_age: ClaimAge::of(peer, now_epoch_secs()),
        is_seed: peer.is_seed(),
        latency: started.elapsed(),
        error,
    }
}

/// Seconds since the unix epoch, to compare against the epoch timestamp inside a peer's signed address claim.
fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// The config name of a transport, as it would be written in config.toml.
fn transport_label(transport: TransportType) -> &'static str {
    match transport {
        TransportType::Memory => "memory",
        TransportType::Tcp => "tcp",
        TransportType::Tor => "tor",
        TransportType::TorTcp => "tor_tcp",
        TransportType::TcpTor => "tcp_tor",
        TransportType::Socks5 => "socks5",
    }
}

/// A string that came from a remote peer, with everything that could drive the operator's terminal escaped.
///
/// Constructing one of these is the only way peer-supplied text gets into the report, so the escaping happens once on
/// the way in rather than at each of the places the report prints. A new print site cannot reintroduce the problem,
/// and a new *ingestion* site has to go through [`Sanitized::new`] to typecheck.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Sanitized(String);

impl Sanitized {
    fn new(raw: &str) -> Self {
        Self(escape_untrusted(raw))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Sanitized {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// Escapes every character a remote peer could use to drive a terminal instead of just being read as text.
///
/// A peer's multiaddr is echoed back in this report, and for `/dns4/`, `/dns6/` and `/dnsaddr/` components the peer
/// validator only checks the address's structure and that it is globally routable - never its character set (see
/// `validate_address` in `comms/core/src/peer_validator/helpers.rs`). The multiaddr wire decode is a plain
/// `str::from_utf8` and `Protocol`'s `Display` writes the string straight back out, so a peer that self-signs an
/// identity claim - which needs no reputation - can carry ESC, NUL or a newline all the way to stdout. That is enough
/// to clear the screen, recolour a failed dial so it reads as a success, or forge whole report lines. The point of
/// this tool is to produce a report an operator can trust, so a remote peer must not be able to write part of it.
///
/// Escapes rather than drops, so that a hostile address is conspicuous in the report instead of quietly shortened.
fn escape_untrusted(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        if c.is_control() || is_invisible(c) {
            let code = u32::from(c);
            if code <= 0xff {
                out.push_str(&format!("\\x{code:02x}"));
            } else {
                out.push_str(&format!("\\u{{{code:04x}}}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Characters that render as nothing, or that reorder the text around them. `char::is_control` does not cover these:
/// they are the bidirectional overrides and zero-width marks behind "Trojan Source" style spoofing, and a peer could
/// use them to make one address display as another.
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{00ad}' |               // soft hyphen
        '\u{200b}'..='\u{200f}' |  // zero width space/joiners, LTR and RTL marks
        '\u{2028}'..='\u{202e}' |  // line and paragraph separators, bidi embedding and overrides
        '\u{2060}'..='\u{2064}' |  // word joiner and invisible maths operators
        '\u{2066}'..='\u{206f}' |  // bidi isolates and deprecated formatting
        '\u{feff}'                 // zero width no-break space / BOM
    )
}

/// Dial failures usually name the peer or address that failed, which would make every failure look unique in the
/// summary. Those parts are replaced with placeholders so that failures group by their actual cause.
///
/// This is a grouping helper and deliberately not a sanitiser: it splits on whitespace, so an address carrying an
/// embedded space or newline would never match the `<address>` placeholder and would pass straight through. It takes
/// an already-[`Sanitized`] string for that reason, and only ever drops or keeps whole tokens, so its result is still
/// safe to print.
fn generalize_failure(reason: &Sanitized) -> Sanitized {
    let generalized = reason
        .as_str()
        .split_whitespace()
        .map(|token| {
            // Keep any trailing punctuation, so that `<address>:` still reads as a prefix of the message after it
            let core_len = token
                .trim_end_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '/' || c == '.'))
                .len();
            let trailing = token.get(core_len..).unwrap_or("");
            let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '/');
            if trimmed.starts_with('/') {
                format!("<address>{trailing}")
            } else if trimmed.len() >= 20 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                format!("<peer>{trailing}")
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    Sanitized(generalized)
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
    node_id: NodeId,
    /// The peer's own advertised address, so untrusted - see [`Sanitized`].
    address: Sanitized,
    mix: AddressMix,
    claim_age: ClaimAge,
    is_seed: bool,
    latency: Duration,
    /// The dial failure, which quotes the address that failed, so also untrusted.
    error: Option<Sanitized>,
}

/// The kinds of address a peer advertises. Which kinds a peer has decides which transports can reach it at all, so
/// this is the first thing to look at when a lot of peers fail to connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum AddressMix {
    OnionOnly,
    Mixed,
    IpOnly,
    None,
}

impl AddressMix {
    fn of(peer: &Peer) -> Self {
        let (onion, other) = peer
            .addresses
            .address_iter()
            .fold((0usize, 0usize), |(onion, other), addr| {
                if addr.iter().any(|p| matches!(p, Protocol::Onion3(_))) {
                    (onion.saturating_add(1), other)
                } else {
                    (onion, other.saturating_add(1))
                }
            });
        match (onion, other) {
            (0, 0) => AddressMix::None,
            (_, 0) => AddressMix::OnionOnly,
            (0, _) => AddressMix::IpOnly,
            _ => AddressMix::Mixed,
        }
    }

    fn label(self) -> &'static str {
        match self {
            AddressMix::OnionOnly => "onion-only",
            AddressMix::Mixed => "mixed",
            AddressMix::IpOnly => "ip-only",
            AddressMix::None => "no address",
        }
    }
}

/// How long ago the peer last signed its address claim, bucketed. The claim timestamp is the only temporal
/// information the peer sync carries, and a peer re-signs only when its addresses or features change - so this is
/// "when did this peer last change its addresses", not a liveness signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum ClaimAge {
    Days7,
    Days14,
    Days30,
    Days90,
    Days180,
    Older,
    Unknown,
}

impl ClaimAge {
    const ALL: [ClaimAge; 7] = [
        ClaimAge::Days7,
        ClaimAge::Days14,
        ClaimAge::Days30,
        ClaimAge::Days90,
        ClaimAge::Days180,
        ClaimAge::Older,
        ClaimAge::Unknown,
    ];

    fn of(peer: &Peer, now: i64) -> Self {
        let Some(claimed_at) = peer.addresses.newest_claim_updated_at() else {
            return ClaimAge::Unknown;
        };
        let age_days = now.saturating_sub(claimed_at.timestamp()).max(0) / 86_400;
        match age_days {
            0..=6 => ClaimAge::Days7,
            7..=13 => ClaimAge::Days14,
            14..=29 => ClaimAge::Days30,
            30..=89 => ClaimAge::Days90,
            90..=179 => ClaimAge::Days180,
            _ => ClaimAge::Older,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ClaimAge::Days7 => "under 7 days",
            ClaimAge::Days14 => "7 - 14 days",
            ClaimAge::Days30 => "14 - 30 days",
            ClaimAge::Days90 => "30 - 90 days",
            ClaimAge::Days180 => "90 - 180 days",
            ClaimAge::Older => "over 180 days",
            ClaimAge::Unknown => "no claim (seeds)",
        }
    }
}

/// A `count x label` summary of address mixes, e.g. `onion-only 37, mixed 2, ip-only 1`.
fn summarize_mixes<'a, I: Iterator<Item = &'a DialOutcome>>(outcomes: I) -> String {
    let mut counts: HashMap<AddressMix, usize> = HashMap::new();
    for outcome in outcomes {
        let counter = counts.entry(outcome.mix).or_default();
        *counter = counter.saturating_add(1);
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by_key(|(mix, _)| *mix);
    counts
        .into_iter()
        .map(|(mix, count)| format!("{} {}", mix.label(), count))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What one round did: who it asked, what it found, and how the dials went.
struct RoundStats {
    round: usize,
    asked_ok: usize,
    asked_total: usize,
    num_discovered: usize,
    num_undialled: usize,
    num_dialled: usize,
    num_connected: usize,
    num_failed: usize,
}

struct Report {
    network: String,
    transport: String,
    node_id: String,
    ephemeral_identity: bool,
    show_peers: bool,
    num_seed_peers: usize,
    /// Peers in the peer database when the seed sync finished. Printed in the peer sync section, which is a snapshot
    /// of that moment, so the later rounds must not touch it - see `num_peers_in_db_final`.
    num_peers_in_db: usize,
    /// Peers in the peer database at the end of the run.
    num_peers_in_db_final: usize,
    /// Non-seed peers in the peer database when the seed sync finished.
    num_downloaded: usize,
    /// Non-seed peers in the peer database at the end of the run, i.e. including everything the later rounds asked
    /// for. Equal to `num_downloaded` for a single round run.
    num_downloaded_total: usize,
    /// The three disjoint buckets `num_downloaded_total` partitions into, all counted in the same pass over the final
    /// peer database, so that they sum back to it exactly.
    num_downloaded_dialled: usize,
    /// Downloaded peers never dialled because they are banned or deleted *now*, at the end of the run.
    num_downloaded_skipped: usize,
    /// Downloaded peers never dialled because `--rounds` or `--max-peers` ran out first.
    num_downloaded_never_reached: usize,
    /// The subset of `num_downloaded_dialled` that answered.
    num_downloaded_connected: usize,
    num_seeds_dialled: usize,
    num_seeds_connected: usize,
    num_skipped: usize,
    sync: SyncOutcome,
    sync_time: Duration,
    dial_time: Duration,
    dialed: Vec<DialOutcome>,
    address_failures: Vec<(Sanitized, usize)>,
    claim_ages: HashMap<ClaimAge, usize>,
    round_stats: Vec<RoundStats>,
    stopped_after: Option<String>,
}

impl Report {
    fn new(cli: &Cli, config: &PeerSyncConfig, node_identity: &NodeIdentity, started_libtor: bool) -> Self {
        Self {
            network: config.base_node.network.to_string(),
            transport: format!(
                "{}{}",
                transport_label(config.base_node.p2p.transport.transport_type),
                if started_libtor { " (bundled tor)" } else { "" }
            ),
            node_id: node_identity.node_id().to_hex(),
            ephemeral_identity: !cli.use_node_identity,
            show_peers: cli.show_peers,
            num_seed_peers: 0,
            num_peers_in_db: 0,
            num_peers_in_db_final: 0,
            num_downloaded: 0,
            num_downloaded_total: 0,
            num_downloaded_dialled: 0,
            num_downloaded_skipped: 0,
            num_downloaded_never_reached: 0,
            num_downloaded_connected: 0,
            num_seeds_dialled: 0,
            num_seeds_connected: 0,
            num_skipped: 0,
            sync: SyncOutcome::default(),
            sync_time: Duration::ZERO,
            dial_time: Duration::ZERO,
            dialed: Vec::new(),
            address_failures: Vec::new(),
            claim_ages: HashMap::new(),
            round_stats: Vec::new(),
            stopped_after: None,
        }
    }

    /// Files one peer from the final peer database snapshot into exactly one bucket, so that the buckets sum back to
    /// the population they came from without any arithmetic at print time.
    ///
    /// `was_dialled` is tested before the ban, deliberately. [`Peer::is_banned`] is wall-clock bound - it filters
    /// `banned_until` against `Utc::now()` - and the DHT's short ban is ten minutes, so a peer banned in an early
    /// round can be dialled perfectly legitimately in a later one. Reading the ban first would count that peer as
    /// both dialled and skipped, inflating the "banned or deleted" figure this section is meant to be trusted on.
    fn count_peer(&mut self, peer: &Peer, was_dialled: bool, was_connected: bool) {
        // Read from the peer as it is now, not as it was when a round passed over it: a ban that has since expired is
        // not the reason this peer went undialled.
        let unusable = peer.deleted_at.is_some() || peer.is_banned();
        if !was_dialled && unusable {
            self.num_skipped = self.num_skipped.saturating_add(1);
        }
        if peer.is_seed() {
            if was_dialled {
                self.num_seeds_dialled = self.num_seeds_dialled.saturating_add(1);
                if was_connected {
                    self.num_seeds_connected = self.num_seeds_connected.saturating_add(1);
                }
            }
            return;
        }
        self.num_downloaded_total = self.num_downloaded_total.saturating_add(1);
        if was_dialled {
            self.num_downloaded_dialled = self.num_downloaded_dialled.saturating_add(1);
            if was_connected {
                self.num_downloaded_connected = self.num_downloaded_connected.saturating_add(1);
            }
        } else if unusable {
            self.num_downloaded_skipped = self.num_downloaded_skipped.saturating_add(1);
        } else {
            self.num_downloaded_never_reached = self.num_downloaded_never_reached.saturating_add(1);
        }
    }

    /// Zeroes everything [`summarize_peer_db`] counts, so that it always reports the snapshot it just read.
    fn reset_peer_db_counts(&mut self) {
        self.num_downloaded_total = 0;
        self.num_downloaded_dialled = 0;
        self.num_downloaded_skipped = 0;
        self.num_downloaded_never_reached = 0;
        self.num_downloaded_connected = 0;
        self.num_seeds_dialled = 0;
        self.num_seeds_connected = 0;
        self.num_skipped = 0;
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
        self.fmt_header(f)?;
        self.fmt_rounds(f)?;
        self.fmt_dialing(f)?;
        self.fmt_claim_ages(f)?;
        self.fmt_peer_list(f)?;
        self.fmt_result(f)?;
        writeln!(
            f,
            "================================================================================"
        )
    }
}

impl Report {
    fn fmt_header(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
        writeln!(f, "Peers downloaded by seed sync : {}", self.num_downloaded)?;
        writeln!(
            f,
            "  new / duplicate this run    : {} / {}",
            self.sync.num_new_peers, self.sync.num_duplicate_peers
        )?;
        writeln!(f, "Peers in database after sync  : {}", self.num_peers_in_db)?;
        writeln!(
            f,
            "Peer sync status              : {} after {:.1?} ({} seed sync round(s))",
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
        Ok(())
    }

    fn fmt_rounds(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if !self.round_stats.is_empty() {
            writeln!(
                f,
                "------------------------------------ Rounds ------------------------------------"
            )?;
            writeln!(
                f,
                "Round 1 dials what the seed sync found. Every later round asks the peers that answered for\ntheir \
                 peer lists and dials only the peers that are new."
            )?;
            writeln!(
                f,
                "  {:<8}{:>12}{:>12}{:>11}{:>10}{:>11}{:>8}",
                "round", "asked", "discovered", "undialled", "dialled", "connected", "failed"
            )?;
            for r in &self.round_stats {
                let asked = if r.round == 1 {
                    "seed sync".to_string()
                } else {
                    format!("{}/{}", r.asked_ok, r.asked_total)
                };
                writeln!(
                    f,
                    "  {:<8}{:>12}{:>12}{:>11}{:>10}{:>11}{:>8}",
                    r.round, asked, r.num_discovered, r.num_undialled, r.num_dialled, r.num_connected, r.num_failed
                )?;
            }
            if let Some(reason) = &self.stopped_after {
                writeln!(f, "  stopped early - {reason}")?;
            }
        }

        Ok(())
    }

    fn fmt_dialing(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
            "------------------------------ Dialing (all rounds) ----------------------------"
        )?;
        writeln!(
            f,
            "Peers dialled                 : {num_dialed}  ({})",
            summarize_mixes(self.dialed.iter())
        )?;
        if self.num_skipped > 0 {
            writeln!(f, "  skipped (banned / deleted)  : {}", self.num_skipped)?;
        }
        writeln!(
            f,
            "Connected                     : {num_connected} ({pct:.1}%)  ({})",
            summarize_mixes(self.dialed.iter().filter(|r| r.error.is_none()))
        )?;
        writeln!(
            f,
            "Failed                        : {num_failed}  ({})",
            summarize_mixes(self.dialed.iter().filter(|r| r.error.is_some()))
        )?;
        writeln!(f, "Time to dial all peers        : {:.1?}", self.dial_time)?;

        if num_failed > 0 {
            let mut reasons: HashMap<Sanitized, usize> = HashMap::new();
            for outcome in self.dialed.iter().filter_map(|r| r.error.as_ref()) {
                let counter = reasons.entry(generalize_failure(outcome)).or_default();
                *counter = counter.saturating_add(1);
            }
            let mut reasons: Vec<_> = reasons.into_iter().collect();
            reasons.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            writeln!(f, "Failure reasons:")?;
            for (reason, count) in reasons.iter().take(10) {
                writeln!(f, "  {count:>5} x {reason}")?;
            }
        }

        if !self.address_failures.is_empty() {
            writeln!(
                f,
                "Why each address failed (the dialer tries every address a peer advertises; a dial cut short by \
                 --dial-timeout records nothing):"
            )?;
            for (reason, count) in self.address_failures.iter().take(10) {
                writeln!(f, "  {count:>5} x {reason}")?;
            }
        }

        Ok(())
    }

    fn fmt_claim_ages(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if !self.claim_ages.is_empty() {
            writeln!(
                f,
                "------------------------- Age of the peers' address claims ---------------------"
            )?;
            writeln!(
                f,
                "A peer signs its addresses only when they change, so this is how long ago each peer last changed \
                 its\naddresses - not how long ago it was seen alive."
            )?;
            writeln!(
                f,
                "  {:<18}{:>8}{:>10}{:>11}{:>8}",
                "claim age", "peers", "dialled", "connected", "failed"
            )?;
            for age in ClaimAge::ALL {
                let peers = self.claim_ages.get(&age).copied().unwrap_or(0);
                if peers == 0 {
                    continue;
                }
                let dialed = self.dialed.iter().filter(|r| r.claim_age == age);
                let (connected, failed) = dialed.fold((0usize, 0usize), |(ok, bad), r| {
                    if r.error.is_none() {
                        (ok.saturating_add(1), bad)
                    } else {
                        (ok, bad.saturating_add(1))
                    }
                });
                writeln!(
                    f,
                    "  {:<18}{:>8}{:>10}{:>11}{:>8}",
                    age.label(),
                    peers,
                    connected.saturating_add(failed),
                    connected,
                    failed
                )?;
            }
        }

        Ok(())
    }

    fn fmt_peer_list(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
                        outcome.node_id.to_hex(),
                        outcome.address,
                        outcome.latency,
                        seed
                    )?,
                    Some(err) => writeln!(
                        f,
                        "  FAIL {} {} - {}{}",
                        outcome.node_id.to_hex(),
                        outcome.address,
                        err,
                        seed
                    )?,
                }
            }
        }
        Ok(())
    }

    /// The two numbers the tool exists to produce: how many peers were downloaded, and how many of *those* answered a
    /// dial. The seed peers are dialled too, but they were configured rather than downloaded, so counting them in
    /// either number would make the pair a rate over two different populations. They get their own line instead.
    fn fmt_result(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (seeds_dialled, seeds_connected) = (self.num_seeds_dialled, self.num_seeds_connected);
        let (downloaded_dialled, downloaded_connected) = (self.num_downloaded_dialled, self.num_downloaded_connected);
        // Not every downloaded peer gets dialled: `--max-peers` caps a round, `--rounds` can run out with candidates
        // left over, and banned or deleted peers are never candidates at all. Spell the remainder out, so that the
        // downloaded count and the dial count below it are visibly the same population rather than two adjacent
        // numbers that happen to differ.
        //
        // These are summed, not subtracted: `summarize_peer_db` puts every downloaded peer into exactly one of the
        // three buckets in a single pass over the final peer database, so they add back up to `num_downloaded_total`
        // by construction. Nothing here needs clamping to make the section reconcile.
        let never_reached = self.num_downloaded_never_reached;
        let banned = self.num_downloaded_skipped;
        let not_dialled = never_reached.saturating_add(banned);
        writeln!(
            f,
            "------------------------------------ Result ------------------------------------"
        )?;
        writeln!(f, "Peers in peer database        : {}", self.num_peers_in_db_final)?;
        writeln!(
            f,
            "Peers downloaded              : {} (not counting the {} seed peer(s))",
            self.num_downloaded_total, self.num_seed_peers
        )?;
        writeln!(f, "  dialled                     : {downloaded_dialled}")?;
        writeln!(
            f,
            "  not dialled                 : {not_dialled} ({never_reached} never reached before --rounds / \
             --max-peers ran out, {banned} banned or deleted)"
        )?;
        writeln!(
            f,
            "Peers connected to            : {downloaded_connected} of the {downloaded_dialled} downloaded peer(s) \
             dialled"
        )?;
        writeln!(
            f,
            "Seed peers connected to       : {seeds_connected} of the {seeds_dialled} seed peer(s) dialled"
        )
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration as StdDuration;

    use clap::Parser;
    use tari_comms::{
        NodeIdentity,
        net_address::MultiaddressesWithStats,
        peer_manager::{Peer, PeerFeatures, PeerFlags},
    };

    use super::{DialOutcome, Report, Sanitized, escape_untrusted, generalize_failure};
    use crate::{cli::Cli, config::PeerSyncConfig};

    fn empty_report() -> Report {
        let cli = Cli::parse_from(["minotari_peer_sync"]);
        let config = PeerSyncConfig::default();
        let identity =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        Report::new(&cli, &config, &identity, false)
    }

    fn peer(flags: PeerFlags) -> Peer {
        let identity =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        Peer::new(
            identity.public_key().clone(),
            identity.node_id().clone(),
            MultiaddressesWithStats::default(),
            flags,
            PeerFeatures::COMMUNICATION_NODE,
            Vec::new(),
            String::new(),
        )
    }

    fn downloaded_peer() -> Peer {
        peer(PeerFlags::default())
    }

    fn seed_peer() -> Peer {
        peer(PeerFlags::SEED)
    }

    fn banned(mut peer: Peer) -> Peer {
        peer.ban_for(StdDuration::from_secs(600), "test".to_string());
        assert!(peer.is_banned(), "the test peer should be banned");
        peer
    }

    fn outcome(error: Option<&str>) -> DialOutcome {
        outcome_at("/ip4/1.2.3.4/tcp/18189", error)
    }

    fn outcome_at(address: &str, error: Option<&str>) -> DialOutcome {
        DialOutcome {
            node_id: super::NodeId::default(),
            address: Sanitized::new(address),
            mix: super::AddressMix::IpOnly,
            claim_age: super::ClaimAge::Unknown,
            is_seed: false,
            latency: super::Duration::from_millis(1),
            error: error.map(Sanitized::new),
        }
    }

    /// The report's closing `Result` section, which is what a caller reads the headline numbers off.
    fn result_section(report: &Report) -> String {
        let rendered = report.to_string();
        let (_, result) = rendered
            .split_once("------------------------------------ Result ")
            .unwrap_or_else(|| panic!("the report has no Result section:\n{rendered}"));
        result.to_string()
    }

    #[test]
    fn report_ends_with_the_downloaded_and_connected_counts() {
        let mut report = empty_report();
        // Three downloaded peers dialled, two of which answered.
        report.count_peer(&downloaded_peer(), true, true);
        report.count_peer(&downloaded_peer(), true, true);
        report.count_peer(&downloaded_peer(), true, false);
        let result = result_section(&report);

        assert!(result.contains("Peers downloaded              : 3"), "{result}");
        assert!(
            result.contains("Peers connected to            : 2 of the 3 downloaded peer(s) dialled"),
            "{result}"
        );
    }

    #[test]
    fn report_of_a_run_that_found_nothing_reports_zeroes_rather_than_nothing() {
        let result = result_section(&empty_report());
        assert!(result.contains("Peers downloaded              : 0"), "{result}");
        assert!(
            result.contains("Peers connected to            : 0 of the 0 downloaded peer(s) dialled"),
            "{result}"
        );
        assert!(
            result.contains("Seed peers connected to       : 0 of the 0 seed peer(s) dialled"),
            "{result}"
        );
    }

    /// The headline pair has to be a rate over one population: the seeds are dialled too, but they were configured
    /// rather than downloaded, so they belong on their own line and out of both of the downloaded numbers.
    #[test]
    fn seed_peers_are_kept_out_of_the_downloaded_connect_rate() {
        let mut report = empty_report();
        report.num_seed_peers = 2;
        // Two downloaded peers, one of which connected, plus two seeds that both connected.
        report.count_peer(&downloaded_peer(), true, true);
        report.count_peer(&downloaded_peer(), true, false);
        report.count_peer(&seed_peer(), true, true);
        report.count_peer(&seed_peer(), true, true);
        let result = result_section(&report);

        assert!(
            result.contains("Peers downloaded              : 2 (not counting the 2 seed peer(s))"),
            "{result}"
        );
        // Not "1 of 4": the four dials include the two seeds, which are not downloaded peers.
        assert!(
            result.contains("Peers connected to            : 1 of the 2 downloaded peer(s) dialled"),
            "{result}"
        );
        assert!(
            result.contains("Seed peers connected to       : 2 of the 2 seed peer(s) dialled"),
            "{result}"
        );
    }

    /// Downloaded peers are not all dialled - `--max-peers` caps a round, `--rounds` can run out, and banned peers
    /// are never candidates. The Result section has to account for that gap itself, or the downloaded count and the
    /// dial count printed under it read as two measurements of one population when they are not.
    #[test]
    fn undialled_downloaded_peers_are_reconciled_in_the_result_section() {
        let mut report = empty_report();
        // Ten downloaded peers known: three dialled (one answered), one banned and never dialled, six that simply
        // never came up before the run ended.
        report.count_peer(&downloaded_peer(), true, true);
        report.count_peer(&downloaded_peer(), true, false);
        report.count_peer(&downloaded_peer(), true, false);
        report.count_peer(&banned(downloaded_peer()), false, false);
        for _ in 0..6 {
            report.count_peer(&downloaded_peer(), false, false);
        }
        let result = result_section(&report);

        assert!(result.contains("Peers downloaded              : 10"), "{result}");
        assert!(result.contains("  dialled                     : 3"), "{result}");
        // 3 dialled + 7 not dialled = the 10 downloaded, and 6 + 1 = the 7, all stated here rather than left to the
        // reader to find in the Rounds table.
        assert!(
            result.contains(
                "  not dialled                 : 7 (6 never reached before --rounds / --max-peers ran out, 1 banned \
                 or deleted)"
            ),
            "{result}"
        );
        assert!(
            result.contains("Peers connected to            : 1 of the 3 downloaded peer(s) dialled"),
            "{result}"
        );
    }

    /// A ban expires on the wall clock, so a peer passed over as banned in an early round can be dialled for real in
    /// a later one. It must then be counted as dialled and *only* as dialled: counting it in the banned bucket too
    /// would inflate that figure, and would previously have been papered over by clamping the split.
    #[test]
    fn a_peer_that_was_dialled_is_never_also_counted_as_banned() {
        let mut report = empty_report();
        // Still banned in the final snapshot, yet it was dialled earlier while the ban was not in force.
        report.count_peer(&banned(downloaded_peer()), true, true);
        let result = result_section(&report);

        assert!(result.contains("Peers downloaded              : 1"), "{result}");
        assert!(result.contains("  dialled                     : 1"), "{result}");
        assert!(
            result.contains(
                "  not dialled                 : 0 (0 never reached before --rounds / --max-peers ran out, 0 banned \
                 or deleted)"
            ),
            "{result}"
        );
        assert!(
            result.contains("Peers connected to            : 1 of the 1 downloaded peer(s) dialled"),
            "{result}"
        );
        // The Dialing section's skipped line is derived from the same snapshot, so it must agree.
        assert_eq!(report.num_skipped, 0);
    }

    /// The three downloaded buckets are counted in one pass over one snapshot, so they sum back to the population
    /// exactly - there is no arithmetic at print time that could disagree.
    #[test]
    fn the_downloaded_buckets_always_sum_to_the_downloaded_total() {
        let mut report = empty_report();
        report.count_peer(&downloaded_peer(), true, true);
        report.count_peer(&banned(downloaded_peer()), false, false);
        report.count_peer(&downloaded_peer(), false, false);
        report.count_peer(&seed_peer(), true, true);

        assert_eq!(
            report.num_downloaded_dialled + report.num_downloaded_skipped + report.num_downloaded_never_reached,
            report.num_downloaded_total
        );
        assert_eq!(report.num_downloaded_total, 3, "the seed must not be counted here");
        assert_eq!(report.num_seeds_dialled, 1);
    }

    /// The peer sync section is a snapshot of what the seed sync produced, so the rounds that run after it must not
    /// write back into it. Only the Result section carries the end-of-run peer database size.
    #[test]
    fn the_peer_sync_section_keeps_its_sync_time_database_count() {
        let mut report = empty_report();
        report.add_dial_results(vec![outcome(None)]);
        report.num_peers_in_db = 4;
        report.num_peers_in_db_final = 12;
        let rendered = report.to_string();
        let (header, result) = rendered
            .split_once("------------------------------------ Result ")
            .unwrap_or_else(|| panic!("the report has no Result section:\n{rendered}"));

        assert!(header.contains("Peers in database after sync  : 4"), "{header}");
        assert!(!header.contains(": 12"), "the sync snapshot was overwritten:\n{header}");
        assert!(result.contains("Peers in peer database        : 12"), "{result}");
    }

    fn generalize(reason: &str) -> String {
        generalize_failure(&Sanitized::new(reason)).as_str().to_string()
    }

    #[test]
    fn generalize_failure_replaces_addresses_and_peers_but_keeps_punctuation() {
        assert_eq!(
            generalize("Transport error for /onion3/abcdef123456:18141: Host unreachable"),
            "Transport error for <address>: Host unreachable"
        );
        assert_eq!(
            generalize("Dial timeout dialing /ip4/1.2.3.4/tcp/18189 after 60.00s"),
            "Dial timeout dialing <address> after 60.00s"
        );
        assert_eq!(
            generalize("All peer addresses are excluded for peer a1b68fcda89ce0366858fd9c66"),
            "All peer addresses are excluded for peer <peer>"
        );
    }

    #[test]
    fn generalize_failure_leaves_plain_messages_alone() {
        let msg = "Noise handshake error: peer closed the connection during the noise handshake";
        assert_eq!(generalize(msg), msg);
    }

    /// A peer chooses its own advertised address, and for `/dns4/` and friends the peer validator never constrains
    /// the character set, so the report must not echo whatever bytes it is handed. Anything that could move the
    /// cursor, recolour output, or start a new line has to be inert by the time it reaches the terminal.
    #[test]
    fn terminal_control_sequences_from_a_peer_never_reach_the_report() {
        // Clear the screen, home the cursor, turn the text green, forge a line, and slip in a NUL for good measure.
        let hostile = "/dns4/\u{1b}[2J\u{1b}[H\u{1b}[32mOK reachable\nSeed peers connected to       : 99\0/tcp/9000";

        let mut report = empty_report();
        report.show_peers = true;
        report.add_dial_results(vec![outcome_at(hostile, Some(hostile))]);
        report.address_failures = vec![(generalize_failure(&Sanitized::new(hostile)), 1)];
        let rendered = report.to_string();

        // No raw ESC or NUL survives anywhere in the output. Raw newlines are not checked this way - the report is
        // full of its own - so the peer's newline is pinned instead by the line it tried to forge never existing.
        assert!(
            !rendered.contains('\u{1b}'),
            "a raw ESC from a peer reached the report:\n{rendered:?}"
        );
        assert!(
            !rendered.contains('\0'),
            "a raw NUL from a peer reached the report:\n{rendered:?}"
        );
        assert!(
            !rendered
                .lines()
                .any(|l| l.trim() == "Seed peers connected to       : 99"),
            "a peer forged a report line:\n{rendered}"
        );
        // Escaped, not dropped: the operator can see something hostile was advertised.
        assert!(
            rendered.contains("\\x1b[2J"),
            "the ESC was not escaped visibly:\n{rendered}"
        );
        assert!(
            rendered.contains("\\x00"),
            "the NUL was not escaped visibly:\n{rendered}"
        );
        assert!(
            rendered.contains("\\x0a"),
            "the newline was not escaped visibly:\n{rendered}"
        );
    }

    #[test]
    fn escape_untrusted_leaves_ordinary_addresses_alone() {
        let addr = "/onion3/abcdefghijklmnop:18141";
        assert_eq!(escape_untrusted(addr), addr);
        assert_eq!(escape_untrusted("/ip4/1.2.3.4/tcp/18189"), "/ip4/1.2.3.4/tcp/18189");
    }

    /// Bidi overrides and zero-width characters are not `char::is_control`, but they reorder or hide what is printed
    /// around them, which is enough to make one address display as another.
    #[test]
    fn invisible_and_bidi_characters_are_escaped_too() {
        assert_eq!(escape_untrusted("a\u{202e}b"), "a\\u{202e}b");
        assert_eq!(escape_untrusted("a\u{200b}b"), "a\\u{200b}b");
        assert_eq!(escape_untrusted("a\u{feff}b"), "a\\u{feff}b");
    }
}
