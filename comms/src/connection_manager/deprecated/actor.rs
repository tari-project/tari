// Copyright 2019, The Tari Project
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

use super::{dialer::Dialer, ConnectionManagerError, Connectivity};
use crate::{connection::PeerConnection, peer_manager::NodeId};
use futures::{
    channel::{mpsc, oneshot},
    future::{self, BoxFuture, Either, FutureExt},
    sink::SinkExt,
    stream::{FusedStream, FuturesUnordered, StreamExt},
    Stream,
};
use log::*;
use std::{collections::HashMap, sync::Arc};
use tari_shutdown::ShutdownSignal;

const LOG_TARGET: &str = "comms::dialer::actor";

/// Create a connected ConnectionManagerRequester and ConnectionManagerService pair. The ConnectionManagerService
/// should be started using an executor `e.g. pool.spawn(service.start());`. The requester is used to
/// make requests to the started ConnectionManagerService.
pub fn create<TConnectionManager>(
    buffer_size: usize,
    dialer: TConnectionManager,
    shutdown_signal: ShutdownSignal,
) -> (
    ConnectionManagerRequester,
    ConnectionManagerActor<TConnectionManager, mpsc::Receiver<ConnectionManagerRequest>>,
)
{
    let (sender, receiver) = mpsc::channel(buffer_size);
    let actor = ConnectionManagerActor::new(dialer, receiver, shutdown_signal);
    let requester = ConnectionManagerRequester::new(sender);
    (requester, actor)
}

/// Requests which are handled by the ConnectionManagerService
pub enum ConnectionManagerRequest {
    DialPeer(
        Box<(
            NodeId,
            oneshot::Sender<Result<Arc<PeerConnection>, ConnectionManagerError>>,
        )>,
    ),
    GetConnection(Box<(NodeId, oneshot::Sender<Option<Arc<PeerConnection>>>)>),
    GetActiveConnectionCount(oneshot::Sender<usize>),
    SetLastConnectionSucceeded(NodeId),
    SetLastConnectionFailed(NodeId),
}

/// Responsible for constructing requests to the ConnectionManagerService
#[derive(Clone)]
pub struct ConnectionManagerRequester {
    sender: mpsc::Sender<ConnectionManagerRequest>,
}

impl ConnectionManagerRequester {
    /// Create a new ConnectionManagerRequester
    pub fn new(sender: mpsc::Sender<ConnectionManagerRequest>) -> Self {
        Self { sender }
    }
}

impl ConnectionManagerRequester {
    /// Attempt to connect to a remote peer
    pub async fn dial_node(&mut self, node_id: NodeId) -> Result<Arc<PeerConnection>, ConnectionManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectionManagerRequest::DialPeer(Box::new((node_id, reply_tx))))
            .await
            .map_err(|_| ConnectionManagerError::SendToActorFailed)?;
        reply_rx
            .await
            .map_err(|_| ConnectionManagerError::ActorRequestCanceled)?
    }

    /// Get number of active connections
    pub async fn get_active_connection_count(&mut self) -> Result<usize, ConnectionManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectionManagerRequest::GetActiveConnectionCount(reply_tx))
            .await
            .map_err(|_| ConnectionManagerError::SendToActorFailed)?;
        reply_rx.await.map_err(|_| ConnectionManagerError::ActorRequestCanceled)
    }

    /// Set the last connection state to `Succeeded` for a given `NodeId`
    pub async fn set_last_connection_succeeded(&mut self, node_id: NodeId) -> Result<(), ConnectionManagerError> {
        self.sender
            .send(ConnectionManagerRequest::SetLastConnectionSucceeded(node_id))
            .await
            .map_err(|_| ConnectionManagerError::SendToActorFailed)
    }

    /// Set the last connection state to `Failed` for a given `NodeId`
    pub async fn set_last_connection_failed(&mut self, node_id: NodeId) -> Result<(), ConnectionManagerError> {
        self.sender
            .send(ConnectionManagerRequest::SetLastConnectionFailed(node_id))
            .await
            .map_err(|_| ConnectionManagerError::SendToActorFailed)
    }

    /// Attempt to connect to a remote peer
    pub async fn get_connection(
        &mut self,
        node_id: NodeId,
    ) -> Result<Option<Arc<PeerConnection>>, ConnectionManagerError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectionManagerRequest::GetConnection(Box::new((node_id, reply_tx))))
            .await
            .map_err(|_| ConnectionManagerError::SendToActorFailed)?;
        reply_rx.await.map_err(|_| ConnectionManagerError::ActorRequestCanceled)
    }
}

/// # Connection Manager Actor
///
/// Responsible for executing connection requests.
pub struct ConnectionManagerActor<TConnectionManager, TStream> {
    connection_manager: TConnectionManager,
    pending_dial_tasks: FuturesUnordered<BoxFuture<'static, ()>>,
    request_rx: TStream,
    shutdown_signal: Option<ShutdownSignal>,
    dial_cancel_signals: HashMap<NodeId, oneshot::Sender<()>>,
}

impl<TConnectionManager, TStream> ConnectionManagerActor<TConnectionManager, TStream> {
    /// Create a new ConnectionManagerActor
    pub fn new(dialer: TConnectionManager, request_rx: TStream, shutdown_signal: ShutdownSignal) -> Self {
        Self {
            connection_manager: dialer,
            request_rx,
            pending_dial_tasks: FuturesUnordered::new(),
            shutdown_signal: Some(shutdown_signal),
            dial_cancel_signals: HashMap::new(),
        }
    }
}

impl<TConnectionManager, TStream> ConnectionManagerActor<TConnectionManager, TStream>
where
    TStream: Stream<Item = ConnectionManagerRequest> + FusedStream + Unpin,
    TConnectionManager: Dialer<NodeId, Output = Arc<PeerConnection>, Error = ConnectionManagerError> + Connectivity,
    TConnectionManager: Clone + Send + Sync + 'static,
    TConnectionManager::Future: Send + Unpin,
{
    /// Start the connection manager actor
    pub async fn run(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("ConnectionManagerActor initialized without shutdown signal");

        loop {
            ::futures::select! {
                // Handle requests to the ConnectionManagerActor
                request = self.request_rx.select_next_some() => self.handle_request(request),

                // Make progress pending connection tasks
                _ = self.pending_dial_tasks.select_next_some() => { },
                _ = shutdown_signal => {
                    info!(
                        target: LOG_TARGET,
                        "Shutting down connection manager actor because the shutdown signal was received",
                    );
                    self.cancel_pending_connection_attempts();
                    break;
                },
                complete => {
                    info!(
                        target: LOG_TARGET,
                        "Shutting down connection manager actor because the request stream and all tasks completed",
                    );
                    break;
                },
            }
        }
    }

    fn cancel_pending_connection_attempts(&mut self) {
        self.dial_cancel_signals
            .drain()
            .filter_map(|(_, cancel_tx)| if cancel_tx.is_canceled() { None } else { Some(cancel_tx) })
            .for_each(|cancel_tx| {
                let _ = cancel_tx.send(());
            });
    }

    fn handle_request(&mut self, request: ConnectionManagerRequest) {
        match request {
            ConnectionManagerRequest::DialPeer(boxed) => {
                let (node_id, reply_tx) = *boxed;
                let dialer = self.connection_manager.clone();
                let (cancel_tx, cancel_rx) = oneshot::channel();
                self.dial_cancel_signals.insert(node_id.clone(), cancel_tx);

                let connect_future = async move {
                    let either = future::select(dialer.dial(&node_id), cancel_rx).await;
                    match either {
                        Either::Left((result, _)) => match reply_tx.send(result) {
                            Ok(_) => {},
                            Err(_msg) => {
                                error!(
                                    target: LOG_TARGET,
                                    "Unable to send connection result back to requester. Request was cancelled.",
                                );
                            },
                        },
                        // Cancel resolved first
                        Either::Right((_, _)) => {
                            trace!(target: LOG_TARGET, "Pending dial request cancelled",);
                        },
                    }
                };

                self.pending_dial_tasks.push(connect_future.boxed());
            },
            ConnectionManagerRequest::GetConnection(boxed) => {
                let (node_id, reply_tx) = *boxed;
                log_if_error_fmt!(
                    target: LOG_TARGET,
                    reply_tx.send(self.connection_manager.get_connection(&node_id)),
                    "Error sending on reply oneshot for peer '{}'",
                    node_id.short_str()
                );
            },
            ConnectionManagerRequest::GetActiveConnectionCount(reply_tx) => {
                if let Err(err) = reply_tx.send(self.connection_manager.get_active_connection_count()) {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to reply to ConnectedPeersCount request: {}", err
                    );
                }
            },
            ConnectionManagerRequest::SetLastConnectionFailed(node_id) => {
                if let Err(err) = self.connection_manager.set_last_connection_failed(&node_id) {
                    error!(
                        target: LOG_TARGET,
                        "Error when setting last connection state to failed: {}", err
                    );
                }
            },
            ConnectionManagerRequest::SetLastConnectionSucceeded(node_id) => {
                if let Err(err) = self.connection_manager.set_last_connection_succeeded(&node_id) {
                    error!(
                        target: LOG_TARGET,
                        "Error when setting last connection state to succeeded: {}", err
                    );
                }
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::dialers::CountDialer;
    use tari_shutdown::Shutdown;
    use tokio::runtime::Builder;

    #[test]
    fn requester_dial_peer() {
        let mut rt = Builder::new().basic_scheduler().build().unwrap();
        let (tx, mut rx) = mpsc::channel(1);
        let mut requester = ConnectionManagerRequester::new(tx);
        let node_id = NodeId::new();
        let node_id_clone = node_id.clone();

        let assert_request = async move {
            let msg = rx.next().await.unwrap();
            match msg {
                ConnectionManagerRequest::DialPeer(boxed) => {
                    let (req_node_id, reply_tx) = *boxed;
                    assert_eq!(req_node_id, node_id_clone);
                    drop(reply_tx);
                },
                _ => panic!("unexpected connection manager request"),
            }
        };
        rt.spawn(assert_request);

        match rt.block_on(requester.dial_node(node_id)) {
            Err(ConnectionManagerError::ActorRequestCanceled) => {},
            _ => panic!("unexpected result"),
        }
    }

    #[test]
    fn connection_manager_service_calls_dialer() {
        let mut rt = Builder::new().basic_scheduler().build().unwrap();

        let dialer = CountDialer::<NodeId>::new();
        let shutdown = Shutdown::new();
        let (mut requester, service) = create(1, dialer.clone(), shutdown.to_signal());

        rt.spawn(service.run());

        let node_id = NodeId::new();
        let _ = rt.block_on(requester.dial_node(node_id.clone())).unwrap();

        assert_eq!(dialer.count(), 1);
    }

    #[test]
    fn connection_manager_service_get_active_connection_count() {
        let mut rt = Builder::new().basic_scheduler().build().unwrap();

        let dialer = CountDialer::<NodeId>::new();
        let shutdown = Shutdown::new();
        let (mut requester, service) = create(1, dialer.clone(), shutdown.to_signal());

        rt.spawn(service.run());

        let n = rt.block_on(requester.get_active_connection_count()).unwrap();
        assert_eq!(n, 0);

        let node_id = NodeId::new();
        let _ = rt.block_on(requester.dial_node(node_id.clone())).unwrap();

        let n = rt.block_on(requester.get_active_connection_count()).unwrap();

        assert_eq!(n, 1);
    }
}
