// Copyright 2020. The Tari Project
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
    fmt,
    fmt::{Display, Write},
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};

use futures::{future, future::Either};
use log::*;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId, NodeIdentity, PeerManager};
use tari_shutdown::ShutdownSignal;
use tokio::{
    sync::{broadcast, RwLock},
    task,
};

use crate::{
    event::DhtEvent,
    network_discovery::{
        discovering::Discovering,
        initializing::Initializing,
        on_connect::OnConnect,
        ready::DiscoveryReady,
        seed_strap::SeedStrap,
        waiting::Waiting,
        NetworkDiscoveryError,
    },
    DhtConfig,
};

const LOG_TARGET: &str = "comms::dht::network_discovery";

#[derive(Debug, Clone, PartialEq)]
pub enum BootstrapMethod {
    None,          // No bootstrap needed
    SeedStrap,     // Traditional seed bootstrap
    ExistingPeers, // Skipped due to sufficient existing peers
}

impl Display for BootstrapMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootstrapMethod::None => write!(f, "None"),
            BootstrapMethod::SeedStrap => write!(f, "SeedStrap"),
            BootstrapMethod::ExistingPeers => write!(f, "ExistingPeers"),
        }
    }
}

#[derive(Debug)]
enum State {
    Initializing,
    SeedStrap(SeedStrap),
    Ready(DiscoveryReady),
    Discovering(Discovering),
    Waiting(Waiting),
    OnConnect(OnConnect),
    Shutdown,
}

impl Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use State::{Discovering, Initializing, OnConnect, Ready, SeedStrap, Shutdown, Waiting};
        match self {
            Initializing => write!(f, "Initializing"),
            SeedStrap(_) => write!(f, "SeedStrap"),
            Ready(_) => write!(f, "Ready"),
            Discovering(_) => write!(f, "Discovering"),
            Waiting(w) => write!(f, "Waiting({:.0?})", w.duration()),
            OnConnect(_) => write!(f, "OnConnect"),
            Shutdown => write!(f, "Shutdown"),
        }
    }
}

impl State {
    pub fn is_shutdown(&self) -> bool {
        matches!(self, State::Shutdown)
    }

    pub fn is_seed_strap(&self) -> bool {
        matches!(self, State::SeedStrap(_))
    }
}

#[derive(Debug)]
pub enum StateEvent {
    Initialized,
    InitialPeersSufficient,
    BeginDiscovery(DiscoveryParams),
    Ready,
    Idle,
    OnConnectMode,
    DiscoveryComplete(DhtNetworkDiscoveryRoundInfo),
    Errored(NetworkDiscoveryError),
    Shutdown,
}

impl Display for StateEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use StateEvent::*;
        match self {
            Initialized => write!(f, "Initialized"),
            InitialPeersSufficient => write!(f, "InitialPeersSufficient (skipping seed bootstrap)"),
            BeginDiscovery(params) => write!(f, "BeginDiscovery({})", params),
            Ready => write!(f, "Ready"),
            Idle => write!(f, "Idle"),
            DiscoveryComplete(stats) => write!(f, "DiscoveryComplete({})", stats),
            Errored(err) => write!(f, "Errored({})", err),
            OnConnectMode => write!(f, "OnConnectMode"),
            Shutdown => write!(f, "Shutdown"),
        }
    }
}

impl PartialEq for StateEvent {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (StateEvent::Initialized, StateEvent::Initialized) => true,
            (StateEvent::InitialPeersSufficient, StateEvent::InitialPeersSufficient) => true,
            (StateEvent::Ready, StateEvent::Ready) => true,
            (StateEvent::Idle, StateEvent::Idle) => true,
            (StateEvent::OnConnectMode, StateEvent::OnConnectMode) => true,
            (StateEvent::Shutdown, StateEvent::Shutdown) => true,
            // For complex variants, we only check the variant type, not the data
            (StateEvent::BeginDiscovery(_), StateEvent::BeginDiscovery(_)) => true,
            (StateEvent::DiscoveryComplete(_), StateEvent::DiscoveryComplete(_)) => true,
            (StateEvent::Errored(_), StateEvent::Errored(_)) => true,
            _ => false,
        }
    }
}

impl<E: Into<NetworkDiscoveryError>> From<E> for StateEvent {
    fn from(err: E) -> Self {
        Self::Errored(err.into())
    }
}

#[derive(Debug, Clone)]
pub(super) struct NetworkDiscoveryContext {
    pub config: Arc<DhtConfig>,
    pub peer_manager: Arc<PeerManager>,
    pub connectivity: ConnectivityRequester,
    pub node_identity: Arc<NodeIdentity>,
    pub num_rounds: Arc<AtomicUsize>,
    pub all_attempted_peers: Arc<RwLock<Vec<NodeId>>>,
    pub event_tx: broadcast::Sender<Arc<DhtEvent>>,
    pub last_round: Arc<RwLock<Option<DhtNetworkDiscoveryRoundInfo>>>,
    pub bootstrap_method: Arc<RwLock<BootstrapMethod>>,
    pub bootstrap_started_at: Arc<RwLock<Option<Instant>>>,
}

impl NetworkDiscoveryContext {
    /// Increment the number of rounds by 1
    pub(super) fn increment_num_rounds(&self) -> usize {
        self.num_rounds.fetch_add(1, Ordering::SeqCst)
    }

    /// Get the number of rounds
    pub fn num_rounds(&self) -> usize {
        self.num_rounds.load(Ordering::SeqCst)
    }

    /// Reset the number of rounds to 0
    pub(super) fn reset_num_rounds(&self) {
        self.num_rounds.store(0, Ordering::SeqCst);
    }

    /// Set the bootstrap method and notify the base node
    pub(super) async fn set_bootstrap_method(&self, method: BootstrapMethod) {
        *self.bootstrap_method.write().await = method.clone();

        info!(
            target: LOG_TARGET,
            "[DHT BOOTSTRAP] Bootstrap method determined: {}",
            method
        );

        // Publish event to inform base node of bootstrap method
        self.publish_event(DhtEvent::BootstrapMethodDetermined(method));
    }

    /// Mark bootstrap as started
    pub(super) async fn mark_bootstrap_started(&self) {
        *self.bootstrap_started_at.write().await = Some(Instant::now());
    }

    /// Complete bootstrap and publish event
    pub(super) async fn complete_bootstrap(&self, method: BootstrapMethod) {
        let started_at = *self.bootstrap_started_at.read().await;
        let duration = started_at.map(|start| start.elapsed());

        info!(
            target: LOG_TARGET,
            "[DHT BOOTSTRAP] Bootstrap completed via {} in {:?}",
            method,
            duration.unwrap_or_default()
        );

        self.publish_event(DhtEvent::PrimaryBootstrapComplete);
    }

    pub(super) fn publish_event(&self, event: DhtEvent) {
        let num_receivers = self.event_tx.receiver_count();
        let event_name = match &event {
            DhtEvent::PrimaryBootstrapComplete => "PrimaryBootstrapComplete",
            DhtEvent::NetworkDiscoveryPeersAdded(_) => "NetworkDiscoveryPeersAdded",
            DhtEvent::BootstrapMethodDetermined(_) => "BootstrapMethodDetermined",
            _ => "Other",
        };

        match self.event_tx.send(Arc::new(event)) {
            Ok(_) => {
                info!(
                    target: LOG_TARGET,
                    "[DHT EVENT PUBLISH] Successfully published DhtEvent::{} to {} receiver(s)",
                    event_name,
                    num_receivers
                );
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "[DHT EVENT PUBLISH] Failed to publish DhtEvent::{}: {}. Receivers: {}",
                    event_name,
                    e,
                    num_receivers
                );
            },
        }
    }

    pub(super) async fn set_last_round(&self, last_round: DhtNetworkDiscoveryRoundInfo) {
        self.all_attempted_peers
            .write()
            .await
            .append(&mut last_round.sync_peers.clone());
        *self.last_round.write().await = Some(last_round);
    }

    pub async fn last_round(&self) -> Option<DhtNetworkDiscoveryRoundInfo> {
        self.last_round.read().await.as_ref().cloned()
    }
}

pub struct DhtNetworkDiscovery {
    context: NetworkDiscoveryContext,
    shutdown_signal: ShutdownSignal,
}

impl DhtNetworkDiscovery {
    pub fn new(
        config: Arc<DhtConfig>,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        connectivity: ConnectivityRequester,
        event_tx: broadcast::Sender<Arc<DhtEvent>>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            context: NetworkDiscoveryContext {
                config,
                peer_manager,
                connectivity,
                node_identity,
                all_attempted_peers: Default::default(),
                num_rounds: Default::default(),
                last_round: Default::default(),
                event_tx,
                bootstrap_method: Arc::new(RwLock::new(BootstrapMethod::None)),
                bootstrap_started_at: Arc::new(RwLock::new(None)),
            },
            shutdown_signal,
        }
    }

    async fn get_next_event(&mut self, state: &mut State) -> StateEvent {
        use State::{Discovering, Initializing, OnConnect, Ready, SeedStrap, Waiting};
        match state {
            Initializing => self::Initializing::new(&mut self.context).next_event().await,
            SeedStrap(seed_strap) => seed_strap.next_event().await,
            Ready(ready) => ready.next_event().await,
            Discovering(discovering) => discovering.next_event().await,
            OnConnect(on_connect) => on_connect.next_event().await,
            Waiting(idling) => idling.next_event().await,
            _ => StateEvent::Shutdown,
        }
    }

    async fn transition(&mut self, current_state: State, next_event: StateEvent) -> State {
        let config = &self.config().network_discovery;
        debug!(
            target: LOG_TARGET,
            "Transition triggered from current state `{}` by event `{}`", current_state, next_event
        );

        // Remember if current state is SeedStrap for error handling
        let was_seed_strap = current_state.is_seed_strap();

        match (current_state, next_event) {
            (State::Initializing, StateEvent::Initialized) => {
                self.context.mark_bootstrap_started().await;
                self.context.set_bootstrap_method(BootstrapMethod::SeedStrap).await;
                State::SeedStrap(SeedStrap::new(self.context.clone()))
            },
            (State::Initializing, StateEvent::InitialPeersSufficient) => {
                info!(target: LOG_TARGET, "BOOTSTRAP DECISION: Sufficient peers found in DB. Bypassing SeedStrap and considering primary bootstrap complete.");
                self.context.set_bootstrap_method(BootstrapMethod::ExistingPeers).await;
                self.context.complete_bootstrap(BootstrapMethod::ExistingPeers).await;
                State::Ready(DiscoveryReady::new(self.context.clone()))
            },
            (State::SeedStrap(_), StateEvent::DiscoveryComplete(stats)) => {
                if stats.has_new_peers() {
                    self.context
                        .publish_event(DhtEvent::NetworkDiscoveryPeersAdded(stats.clone()));
                }
                let is_success = stats.is_success();
                self.context.set_last_round(stats).await;
                if !is_success {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap round was not successful. Waiting before next attempt."
                    );
                    return State::Waiting(config.on_failure_idle_period.into());
                }

                // SeedStrap completed successfully, mark bootstrap complete
                self.context.complete_bootstrap(BootstrapMethod::SeedStrap).await;
                self.context.increment_num_rounds();
                State::Ready(DiscoveryReady::new(self.context.clone()))
            },
            (State::Discovering(_), StateEvent::DiscoveryComplete(stats)) => {
                if stats.has_new_peers() {
                    self.context
                        .publish_event(DhtEvent::NetworkDiscoveryPeersAdded(stats.clone()));
                }
                let is_success = stats.is_success();
                self.context.set_last_round(stats).await;
                if !is_success {
                    return State::Waiting(config.on_failure_idle_period.into());
                }
                self.context.increment_num_rounds();
                self.context.set_bootstrap_method(BootstrapMethod::SeedStrap).await;
                State::Ready(DiscoveryReady::new(self.context.clone()))
            },
            (State::Ready(_), StateEvent::BeginDiscovery(params)) => {
                State::Discovering(Discovering::new(params, self.context.clone()))
            },
            (State::Ready(_), StateEvent::OnConnectMode) => State::OnConnect(OnConnect::new(self.context.clone())),
            (State::Ready(_), StateEvent::Idle) => State::Waiting(config.idle_period.into()),
            (State::OnConnect(_), StateEvent::Ready) => State::Ready(DiscoveryReady::new(self.context.clone())),
            (_, StateEvent::Shutdown) => State::Shutdown,
            (_state, StateEvent::Errored(err)) => {
                error!(
                    target: LOG_TARGET,
                    "Network discovery errored: {}. Waiting for {:.0?}", err, config.on_failure_idle_period
                );

                // If we're in SeedStrap and get an error, still mark bootstrap complete to prevent UI stuck state
                if was_seed_strap {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap failed with error: {}. Marking bootstrap complete anyway to prevent UI deadlock.", err
                    );
                    self.context.complete_bootstrap(BootstrapMethod::SeedStrap).await;
                }

                State::Waiting(config.on_failure_idle_period.into())
            },
            (state, event) => {
                debug!(
                    target: LOG_TARGET,
                    "No state transition for event `{}`. The current state is `{}`", event, state
                );
                state
            },
        }
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }

    pub fn spawn(self) -> task::JoinHandle<()> {
        task::spawn(self.run())
    }

    pub async fn run(mut self) {
        if !self.config().network_discovery.enabled {
            warn!(
                target: LOG_TARGET,
                "Network discovery is disabled. This node may fail to participate in the network."
            );
            return;
        }

        let mut state = State::Initializing;
        let mut bootstrap_completed = false;

        loop {
            let shutdown_signal = self.shutdown_signal.clone();

            let next_event = if bootstrap_completed {
                let fut = self.get_next_event(&mut state);
                futures::pin_mut!(fut);
                or_shutdown(shutdown_signal, fut).await
            } else {
                // Create a separate context to avoid borrow issues
                let context_clone = self.context.clone();
                let bootstrap_timeout_duration = self.config().network_discovery.bootstrap_timeout;

                let fut = self.get_next_event(&mut state);
                futures::pin_mut!(fut);

                tokio::select! {
                    event = or_shutdown(shutdown_signal, fut) => event,
                    _ = tokio::time::sleep(bootstrap_timeout_duration) => {
                        warn!(target: LOG_TARGET, "Bootstrap timeout reached - forcing completion");
                        context_clone.complete_bootstrap(BootstrapMethod::SeedStrap).await;
                        bootstrap_completed = true;
                        StateEvent::Ready
                    }
                }
            };

            // Check if bootstrap completed with this event
            if matches!(next_event, StateEvent::DiscoveryComplete(_)) && state.is_seed_strap() {
                bootstrap_completed = true;
            }
            if matches!(next_event, StateEvent::InitialPeersSufficient) {
                bootstrap_completed = true;
            }

            state = self.transition(state, next_event).await;
            if state.is_shutdown() {
                break;
            }
        }
    }
}

async fn or_shutdown<Fut>(shutdown_signal: ShutdownSignal, fut: Fut) -> StateEvent
where Fut: Future<Output = StateEvent> + Unpin {
    match future::select(shutdown_signal, fut).await {
        Either::Left(_) => StateEvent::Shutdown,
        Either::Right((event, _)) => event,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryParams {
    pub peers: Vec<NodeId>,
    pub num_peers_to_request: u32,
}

impl Display for DiscoveryParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DiscoveryParams({} peer(s) ({}), num_peers_to_request = {})",
            self.peers.len(),
            self.peers.iter().fold(String::new(), |mut peers, p| {
                let _ = write!(peers, "{p}, ");
                peers
            }),
            self.num_peers_to_request,
        )
    }
}

// Add this enum to describe the current discovery phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscoveryPhase {
    SeedStrap,
    #[default]
    General, // For regular, ongoing discovery after initial bootstrap
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct DhtNetworkDiscoveryRoundInfo {
    pub num_new_peers: usize,
    pub num_duplicate_peers: usize,
    pub num_succeeded: usize,
    pub sync_peers: Vec<NodeId>,
    // New fields:
    pub phase: DiscoveryPhase,
}

impl DhtNetworkDiscoveryRoundInfo {
    pub fn has_new_peers(&self) -> bool {
        self.num_new_peers > 0
    }

    /// Returns true if the round succeeded (i.e. at least one sync peer was contacted and succeeded in the protocol),
    /// otherwise false
    pub fn is_success(&self) -> bool {
        self.num_succeeded > 0
    }
}

impl Display for DhtNetworkDiscoveryRoundInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Synced {}/{}, num_new_peers = {}, num_duplicate_peers = {}",
            self.num_succeeded,
            self.sync_peers.len(),
            self.num_new_peers,
            self.num_duplicate_peers,
        )
    }
}
