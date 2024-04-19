//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fmt,
    fmt::{Display, Write},
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
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
        waiting::Waiting,
        NetworkDiscoveryError,
    },
    DhtConfig,
};

const LOG_TARGET: &str = "comms::dht::network_discovery";

#[derive(Debug)]
enum State {
    Initializing,
    Ready(DiscoveryReady),
    Discovering(Discovering),
    Waiting(Waiting),
    OnConnect(OnConnect),
    Shutdown,
}

impl Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use State::{Discovering, Initializing, OnConnect, Ready, Shutdown, Waiting};
        match self {
            Initializing => write!(f, "Initializing"),
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
}

#[derive(Debug)]
pub enum StateEvent {
    Initialized,
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

    pub(super) fn publish_event(&self, event: DhtEvent) {
        let _result = self.event_tx.send(Arc::new(event));
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
            },
            shutdown_signal,
        }
    }

    async fn get_next_event(&mut self, state: &mut State) -> StateEvent {
        use State::{Discovering, Initializing, OnConnect, Ready, Waiting};
        match state {
            Initializing => self::Initializing::new(&mut self.context).next_event().await,
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
        match (current_state, next_event) {
            (State::Initializing, StateEvent::Initialized) => State::Ready(DiscoveryReady::new(self.context.clone())),
            (_, StateEvent::Ready) => State::Ready(DiscoveryReady::new(self.context.clone())),
            (State::Ready(_), StateEvent::BeginDiscovery(params)) => {
                State::Discovering(Discovering::new(params, self.context.clone()))
            },
            (State::Ready(_), StateEvent::OnConnectMode) => State::OnConnect(OnConnect::new(self.context.clone())),
            (State::Discovering(_), StateEvent::DiscoveryComplete(stats)) => {
                if stats.has_new_peers() {
                    self.context
                        .publish_event(DhtEvent::NetworkDiscoveryPeersAdded(stats.clone()));
                }
                let is_success = stats.is_success();
                self.context.set_last_round(stats).await;
                if !is_success {
                    return State::Waiting(self.config().network_discovery.on_failure_idle_period.into());
                }

                State::Ready(DiscoveryReady::new(self.context.clone()))
            },
            (State::Ready(_), StateEvent::Idle) => State::Waiting(config.idle_period.into()),
            (_, StateEvent::Shutdown) => State::Shutdown,
            (_, StateEvent::Errored(err)) => {
                error!(
                    target: LOG_TARGET,
                    "Network discovery errored: {}. Waiting for {:.0?}", err, config.on_failure_idle_period
                );
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
        loop {
            let shutdown_signal = self.shutdown_signal.clone();
            let next_event = {
                let fut = self.get_next_event(&mut state);
                futures::pin_mut!(fut);
                or_shutdown(shutdown_signal, fut).await
            };
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

#[derive(Debug, Clone)]
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
            self.num_peers_to_request
        )
    }
}

#[derive(Debug, Default, Clone)]
pub struct DhtNetworkDiscoveryRoundInfo {
    pub num_new_peers: usize,
    pub num_duplicate_peers: usize,
    pub num_succeeded: usize,
    pub sync_peers: Vec<NodeId>,
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
